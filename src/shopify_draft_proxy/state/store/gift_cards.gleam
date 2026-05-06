//// Store operations for gift card records.

import gleam/dict
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, dedupe_strings, dict_has, list_to_set, string_compare,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type GiftCardConfigurationRecord, type GiftCardRecord,
} as types_mod

// ---------------------------------------------------------------------------
// Gift card slice (Pass 19)
// ---------------------------------------------------------------------------

/// Upsert one or more gift-card records into the base state.
/// Mirrors `upsertBaseGiftCards`.
pub fn upsert_base_gift_cards(
  store: Store,
  records: List(GiftCardRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let new_base =
      BaseState(
        ..base,
        gift_cards: dict.insert(base.gift_cards, record.id, record),
        gift_card_order: append_unique_id(base.gift_card_order, record.id),
      )
    Store(..acc, base_state: new_base)
  })
}

/// Upsert the singleton base gift-card configuration.
/// Mirrors `upsertBaseGiftCardConfiguration`.
pub fn upsert_base_gift_card_configuration(
  store: Store,
  record: GiftCardConfigurationRecord,
) -> Store {
  let base = store.base_state
  let new_base = BaseState(..base, gift_card_configuration: Some(record))
  Store(..store, base_state: new_base)
}

/// Stage a freshly minted `GiftCardRecord`. Mirrors
/// `stageCreateGiftCard` — appends the id to staged order on first
/// sight, otherwise leaves the order alone (idempotent re-stage).
pub fn stage_create_gift_card(
  store: Store,
  record: GiftCardRecord,
) -> #(GiftCardRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.gift_card_order, record.id)
    || list.contains(staged.gift_card_order, record.id)
  let new_order = case already_known {
    True -> staged.gift_card_order
    False -> list.append(staged.gift_card_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      gift_cards: dict.insert(staged.gift_cards, record.id, record),
      gift_card_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Stage an updated `GiftCardRecord`. Mirrors `stageUpdateGiftCard`.
/// Same semantics as `stage_create_gift_card` since gift cards are
/// never deleted (deactivation flips a flag instead).
pub fn stage_update_gift_card(
  store: Store,
  record: GiftCardRecord,
) -> #(GiftCardRecord, Store) {
  stage_create_gift_card(store, record)
}

/// Look up the effective gift card for an id (staged-over-base).
/// Mirrors `getEffectiveGiftCardById`.
pub fn get_effective_gift_card_by_id(
  store: Store,
  id: String,
) -> Option(GiftCardRecord) {
  case dict.get(store.staged_state.gift_cards, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.gift_cards, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

/// List every effective gift card. Mirrors `listEffectiveGiftCards`.
/// Ordered records first (`giftCardOrder`), then any unordered records
/// sorted by id.
pub fn list_effective_gift_cards(store: Store) -> List(GiftCardRecord) {
  let ordered_ids =
    list.append(
      store.base_state.gift_card_order,
      store.staged_state.gift_card_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_gift_card_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.gift_cards, store.staged_state.gift_cards)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_gift_card_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

/// Stage the singleton gift-card configuration. Mirrors
/// `setStagedGiftCardConfiguration`.
pub fn set_staged_gift_card_configuration(
  store: Store,
  record: GiftCardConfigurationRecord,
) -> Store {
  let staged = store.staged_state
  let new_staged = StagedState(..staged, gift_card_configuration: Some(record))
  Store(..store, staged_state: new_staged)
}

/// Read the effective gift-card configuration (staged-over-base).
/// Mirrors `getEffectiveGiftCardConfiguration`. Returns the proxy's
/// default (CAD 0.0 limits) when neither side has staged a
/// configuration — matches the TS fallback.
pub fn get_effective_gift_card_configuration(
  store: Store,
) -> GiftCardConfigurationRecord {
  case store.staged_state.gift_card_configuration {
    Some(record) -> record
    None ->
      case store.base_state.gift_card_configuration {
        Some(record) -> record
        None -> default_gift_card_configuration()
      }
  }
}

fn default_gift_card_configuration() -> GiftCardConfigurationRecord {
  types_mod.GiftCardConfigurationRecord(
    issue_limit: types_mod.Money(amount: "0.0", currency_code: "CAD"),
    purchase_limit: types_mod.Money(amount: "0.0", currency_code: "CAD"),
  )
}
