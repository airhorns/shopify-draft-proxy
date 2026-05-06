//// Store operations for bulk operation records.

import gleam/dict
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/string
import shopify_draft_proxy/state/store/shared.{
  append_unique_id, dedupe_strings, dict_has, list_to_set, string_compare,
}
import shopify_draft_proxy/state/store/types.{
  type Store, BaseState, StagedState, Store,
}
import shopify_draft_proxy/state/types.{
  type BulkOperationRecord, BulkOperationRecord,
} as _

// ---------------------------------------------------------------------------
// Bulk-operations slice
// ---------------------------------------------------------------------------

/// Upsert BulkOperation records into base state. Mirrors
/// `upsertBaseBulkOperations`.
pub fn upsert_base_bulk_operations(
  store: Store,
  records: List(BulkOperationRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let new_base =
      BaseState(
        ..base,
        bulk_operations: dict.insert(base.bulk_operations, record.id, record),
        bulk_operation_order: append_unique_id(
          base.bulk_operation_order,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base)
  })
}

/// Stage a BulkOperation record. Mirrors `stageBulkOperation`.
pub fn stage_bulk_operation(
  store: Store,
  record: BulkOperationRecord,
) -> #(BulkOperationRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.bulk_operation_order, record.id)
    || list.contains(staged.bulk_operation_order, record.id)
  let new_order = case already_known {
    True -> staged.bulk_operation_order
    False -> list.append(staged.bulk_operation_order, [record.id])
  }
  let new_staged =
    StagedState(
      ..staged,
      bulk_operations: dict.insert(staged.bulk_operations, record.id, record),
      bulk_operation_order: new_order,
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Stage a BulkOperation and its generated result JSONL. The TS store
/// keeps result payloads in a sibling `bulkOperationResults` map; in
/// Gleam the not-yet-exposed result payload lives on the record.
pub fn stage_bulk_operation_result(
  store: Store,
  record: BulkOperationRecord,
  jsonl: String,
) -> #(BulkOperationRecord, Store) {
  stage_bulk_operation(
    store,
    BulkOperationRecord(..record, result_jsonl: Some(jsonl)),
  )
}

pub fn get_effective_bulk_operation_by_id(
  store: Store,
  id: String,
) -> Option(BulkOperationRecord) {
  case dict.get(store.staged_state.bulk_operations, id) {
    Ok(record) -> Some(record)
    Error(_) ->
      case dict.get(store.base_state.bulk_operations, id) {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

pub fn get_staged_bulk_operation_by_id(
  store: Store,
  id: String,
) -> Option(BulkOperationRecord) {
  case dict.get(store.staged_state.bulk_operations, id) {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}

/// List effective BulkOperations. Ordered ids from base+staged come
/// first, then unordered ids sorted by createdAt descending / id
/// ascending, matching the TS store helper.
pub fn list_effective_bulk_operations(
  store: Store,
) -> List(BulkOperationRecord) {
  let ordered_ids =
    list.append(
      store.base_state.bulk_operation_order,
      store.staged_state.bulk_operation_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_bulk_operation_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.bulk_operations,
      store.staged_state.bulk_operations,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(fn(left, right) {
      case dict.get(merged, left), dict.get(merged, right) {
        Ok(l), Ok(r) -> {
          let date_order = string.compare(r.created_at, l.created_at)
          case date_order {
            order.Eq -> string_compare(l.id, r.id)
            _ -> date_order
          }
        }
        _, _ -> string_compare(left, right)
      }
    })
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_bulk_operation_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

pub fn get_effective_bulk_operation_result_jsonl(
  store: Store,
  id: String,
) -> Option(String) {
  case get_effective_bulk_operation_by_id(store, id) {
    Some(BulkOperationRecord(result_jsonl: Some(jsonl), ..)) -> Some(jsonl)
    _ -> None
  }
}

/// Cancel only a staged operation, matching TS
/// `cancelStagedBulkOperation`.
pub fn cancel_staged_bulk_operation(
  store: Store,
  id: String,
) -> #(Option(BulkOperationRecord), Store) {
  case get_staged_bulk_operation_by_id(store, id) {
    None -> #(None, store)
    Some(record) -> {
      let canceled =
        BulkOperationRecord(..record, status: "CANCELING", completed_at: None)
      let staged = store.staged_state
      let new_staged =
        StagedState(
          ..staged,
          bulk_operations: dict.insert(staged.bulk_operations, id, canceled),
        )
      #(Some(canceled), Store(..store, staged_state: new_staged))
    }
  }
}

pub fn has_bulk_operations(store: Store) -> Bool {
  !list.is_empty(dict.keys(store.base_state.bulk_operations))
  || !list.is_empty(dict.keys(store.staged_state.bulk_operations))
}

pub fn has_staged_bulk_operations(store: Store) -> Bool {
  !list.is_empty(dict.keys(store.staged_state.bulk_operations))
}

/// Record staged-upload content for local bulk mutation imports. The JS HTTP
/// adapter calls this through the DraftProxy shim; Gleam tests can seed it
/// directly when they do not need to exercise HTTP routing.
pub fn stage_staged_upload_content(
  store: Store,
  staged_upload_path: String,
  content: String,
) -> Store {
  let staged = store.staged_state
  Store(
    ..store,
    staged_state: StagedState(
      ..staged,
      staged_upload_contents: dict.insert(
        staged.staged_upload_contents,
        staged_upload_path,
        content,
      ),
    ),
  )
}

pub fn get_staged_upload_content(
  store: Store,
  staged_upload_path: String,
) -> Option(String) {
  case dict.get(store.staged_state.staged_upload_contents, staged_upload_path) {
    Ok(content) -> Some(content)
    Error(_) -> None
  }
}
