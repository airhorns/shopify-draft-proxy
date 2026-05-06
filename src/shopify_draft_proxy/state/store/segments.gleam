//// Store operations for segment records.

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
  type CustomerSegmentMembersQueryRecord, type SegmentRecord,
  type StorePropertyValue,
} as _

// ---------------------------------------------------------------------------
// Segment slice (Pass 20)
// ---------------------------------------------------------------------------

pub fn upsert_base_segments(
  store: Store,
  records: List(SegmentRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    Store(
      ..acc,
      base_state: BaseState(
        ..base,
        segments: dict.insert(base.segments, record.id, record),
        segment_order: append_unique_id(base.segment_order, record.id),
        deleted_segment_ids: dict.delete(base.deleted_segment_ids, record.id),
      ),
    )
  })
}

/// Stage a segment record. Mirrors `upsertStagedSegment`. Returns the
/// stored record alongside the new store so the caller can build a
/// mutation payload.
pub fn upsert_staged_segment(
  store: Store,
  record: SegmentRecord,
) -> #(SegmentRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.segment_order, record.id)
    || list.contains(staged.segment_order, record.id)
  let new_order = case already_known {
    True -> staged.segment_order
    False -> list.append(staged.segment_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      segments: dict.insert(staged.segments, record.id, record),
      segment_order: new_order,
      deleted_segment_ids: dict.delete(staged.deleted_segment_ids, record.id),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a segment id as deleted. Mirrors `deleteStagedSegment`.
pub fn delete_staged_segment(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      segments: dict.delete(staged.segments, id),
      deleted_segment_ids: dict.insert(staged.deleted_segment_ids, id, True),
    )
  Store(..store, staged_state: new_staged)
}

/// Look up the effective segment for an id. Staged wins over base; any
/// "deleted" marker on either side suppresses the record. Mirrors
/// `getEffectiveSegmentById`.
pub fn get_effective_segment_by_id(
  store: Store,
  id: String,
) -> Option(SegmentRecord) {
  let deleted =
    dict_has(store.base_state.deleted_segment_ids, id)
    || dict_has(store.staged_state.deleted_segment_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.segments, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.segments, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective segment the store knows about. Ordered records
/// (those tracked by `segmentOrder`) come first, followed by any
/// unordered staged/base records sorted by id. Mirrors
/// `listEffectiveSegments`.
pub fn list_effective_segments(store: Store) -> List(SegmentRecord) {
  let ordered_ids =
    list.append(
      store.base_state.segment_order,
      store.staged_state.segment_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_segment_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(store.base_state.segments, store.staged_state.segments)
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_segment_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn set_base_segment_root_payload(
  store: Store,
  root_name: String,
  payload: StorePropertyValue,
) -> Store {
  let base = store.base_state
  Store(
    ..store,
    base_state: BaseState(
      ..base,
      segment_root_payloads: dict.insert(
        base.segment_root_payloads,
        root_name,
        payload,
      ),
    ),
  )
}

pub fn get_base_segment_root_payload(
  store: Store,
  root_name: String,
) -> Option(StorePropertyValue) {
  case dict.get(store.base_state.segment_root_payloads, root_name) {
    Ok(payload) -> Some(payload)
    Error(_) -> None
  }
}

// ---------------------------------------------------------------------------
// Customer-segment-members-query slice (Pass 22j)
// ---------------------------------------------------------------------------

/// Stage a customer-segment-members-query record. Mirrors
/// `stageCustomerSegmentMembersQuery`.
pub fn stage_customer_segment_members_query(
  store: Store,
  record: CustomerSegmentMembersQueryRecord,
) -> Store {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.customer_segment_members_query_order, record.id)
    || list.contains(staged.customer_segment_members_query_order, record.id)
  let new_order = case already_known {
    True -> staged.customer_segment_members_query_order
    False ->
      list.append(staged.customer_segment_members_query_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      customer_segment_members_queries: dict.insert(
        staged.customer_segment_members_queries,
        record.id,
        record,
      ),
      customer_segment_members_query_order: new_order,
    )
  Store(..store, staged_state: new_staged)
}

/// Look up the effective customer-segment-members-query for an id.
/// Staged wins over base. Mirrors
/// `getEffectiveCustomerSegmentMembersQueryById`.
pub fn get_effective_customer_segment_members_query_by_id(
  store: Store,
  id: String,
) -> Option(CustomerSegmentMembersQueryRecord) {
  case dict.get(store.staged_state.customer_segment_members_queries, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.customer_segment_members_queries, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}
