//// Store operations for discount records.

import gleam/dict
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, dedupe_strings, dict_has, list_to_set, string_compare,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type DiscountBulkOperationRecord, type DiscountRecord,
} as _

// ---------------------------------------------------------------------------
// Discounts slice
// ---------------------------------------------------------------------------

pub fn upsert_base_discounts(
  store: Store,
  records: List(DiscountRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        ..base,
        discounts: dict.insert(base.discounts, record.id, record),
        discount_order: append_unique_id(base.discount_order, record.id),
        deleted_discount_ids: dict.delete(base.deleted_discount_ids, record.id),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_discount_ids: dict.delete(
          staged.deleted_discount_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

pub fn stage_discount(
  store: Store,
  record: DiscountRecord,
) -> #(DiscountRecord, Store) {
  let base = store.base_state
  let staged = store.staged_state
  let already_known =
    list.contains(base.discount_order, record.id)
    || list.contains(staged.discount_order, record.id)
  let new_order = case already_known {
    True -> staged.discount_order
    False -> list.append(staged.discount_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      discounts: dict.insert(staged.discounts, record.id, record),
      discount_order: new_order,
      deleted_discount_ids: dict.delete(staged.deleted_discount_ids, record.id),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn delete_staged_discount(store: Store, id: String) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      discounts: dict.delete(staged.discounts, id),
      deleted_discount_ids: dict.insert(staged.deleted_discount_ids, id, True),
    ),
  )
}

pub fn stage_discount_bulk_operation(
  store: Store,
  record: DiscountBulkOperationRecord,
) -> #(DiscountBulkOperationRecord, Store) {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      discount_bulk_operations: dict.insert(
        staged.discount_bulk_operations,
        record.id,
        record,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

pub fn get_effective_discount_by_id(
  store: Store,
  id: String,
) -> Option(DiscountRecord) {
  let deleted =
    dict_has(store.base_state.deleted_discount_ids, id)
    || dict_has(store.staged_state.deleted_discount_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.discounts, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.discounts, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

pub fn list_effective_discounts(store: Store) -> List(DiscountRecord) {
  let ordered_ids =
    list.append(
      store.base_state.discount_order,
      store.staged_state.discount_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_discount_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.discounts, store.staged_state.discounts)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_discount_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn find_effective_discount_by_code(
  store: Store,
  code: String,
) -> Option(DiscountRecord) {
  let wanted = string.lowercase(code)
  case
    list.find(list_effective_discounts(store), fn(record) {
      case record.code {
        Some(record_code) -> string.lowercase(record_code) == wanted
        None -> False
      }
    })
  {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}
/// Upsert one or more gift-card records into the base state.
/// Mirrors `upsertBaseGiftCards`.
// ---------------------------------------------------------------------------
// Gift card slice (Pass 19)
// ---------------------------------------------------------------------------
