//// Mirrors the slice of `src/state/store.ts` that backs the
//// saved-searches domain plus the mutation log. Only the saved-search
//// fields of `BaseState`/`StagedState` are modelled here; every other
//// resource will land slice-by-slice as its domain handler ports.
////
//// The TS class mutates state in place. This Gleam port returns updated
//// `Store` records from every mutator so callers thread state through
//// their own pipeline (matching the pattern already established for
//// `SyntheticIdentityRegistry`).

import gleam/dict.{type Dict}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/string
import shopify_draft_proxy/state/types.{type SavedSearchRecord}

/// Server-authoritative state. Mirrors the saved-search slice of
/// `StateSnapshot` for `baseState`. Every field besides
/// `saved_searches` / `saved_search_order` /
/// `deleted_saved_search_ids` is stubbed out as an empty Dict because
/// no other domain currently writes to base state in the Gleam port.
pub type BaseState {
  BaseState(
    saved_searches: Dict(String, SavedSearchRecord),
    saved_search_order: List(String),
    deleted_saved_search_ids: Dict(String, Bool),
  )
}

/// Mutations the proxy has staged but not yet committed upstream.
/// Mirrors the saved-search slice of `StateSnapshot` for `stagedState`.
pub type StagedState {
  StagedState(
    saved_searches: Dict(String, SavedSearchRecord),
    saved_search_order: List(String),
    deleted_saved_search_ids: Dict(String, Bool),
  )
}

/// Operation type a mutation log entry was recorded for. Mirrors the
/// `'query' | 'mutation'` union in TS.
pub type OperationType {
  Query
  Mutation
}

/// Status the mutation log records each entry under. Mirrors
/// `'staged' | 'proxied' | 'committed' | 'failed'`.
pub type EntryStatus {
  Staged
  Proxied
  Committed
  Failed
}

/// Capability metadata recorded alongside each mutation log entry.
/// Mirrors `MutationLogInterpretedMetadata['capability']`.
pub type Capability {
  Capability(
    operation_name: Option(String),
    domain: String,
    execution: String,
  )
}

/// Slim port of `MutationLogInterpretedMetadata`. Only the fields the
/// Gleam port currently writes are modelled. The optional pieces
/// (`registeredOperation`, `safety`, `bulkOperationImport`) are deferred
/// until their producers port.
pub type InterpretedMetadata {
  InterpretedMetadata(
    operation_type: OperationType,
    operation_name: Option(String),
    root_fields: List(String),
    primary_root_field: Option(String),
    capability: Capability,
  )
}

/// Slim port of `MutationLogEntry`. `requestBody` and the optional
/// fields are deferred to the next pass that produces them.
pub type MutationLogEntry {
  MutationLogEntry(
    id: String,
    received_at: String,
    operation_name: Option(String),
    path: String,
    query: String,
    variables: Dict(String, String),
    staged_resource_ids: List(String),
    status: EntryStatus,
    interpreted: InterpretedMetadata,
    notes: Option(String),
  )
}

/// Long-lived runtime store. The TS class also tracks lagged search
/// caches and a handful of cross-domain side tables; those will land
/// when their domains do.
pub type Store {
  Store(
    base_state: BaseState,
    staged_state: StagedState,
    mutation_log: List(MutationLogEntry),
  )
}

/// An empty `BaseState`. Equivalent to `cloneSnapshot(EMPTY_SNAPSHOT)`
/// projected onto the saved-search slice.
pub fn empty_base_state() -> BaseState {
  BaseState(
    saved_searches: dict.new(),
    saved_search_order: [],
    deleted_saved_search_ids: dict.new(),
  )
}

/// An empty `StagedState`.
pub fn empty_staged_state() -> StagedState {
  StagedState(
    saved_searches: dict.new(),
    saved_search_order: [],
    deleted_saved_search_ids: dict.new(),
  )
}

/// Fresh store, equivalent to `new InMemoryStore()`.
pub fn new() -> Store {
  Store(
    base_state: empty_base_state(),
    staged_state: empty_staged_state(),
    mutation_log: [],
  )
}

/// Reset both base and staged state plus the mutation log. Mirrors
/// `reset()` (which calls `restoreInitialState()` against an empty
/// snapshot — equivalent to a fresh store for the slices we ship).
pub fn reset(_store: Store) -> Store {
  new()
}

// ---------------------------------------------------------------------------
// Saved-search slice
// ---------------------------------------------------------------------------

/// Upsert one or more saved-search records into the base state.
/// Mirrors `upsertBaseSavedSearches`. Removes any existing
/// "deleted" markers (in either base or staged) for the same id, since
/// the upstream answer wins.
pub fn upsert_base_saved_searches(
  store: Store,
  records: List(SavedSearchRecord),
) -> Store {
  list.fold(records, store, fn(acc, record) {
    let base = acc.base_state
    let staged = acc.staged_state
    let new_base =
      BaseState(
        saved_searches: dict.insert(base.saved_searches, record.id, record),
        saved_search_order: append_unique_id(
          base.saved_search_order,
          record.id,
        ),
        deleted_saved_search_ids: dict.delete(
          base.deleted_saved_search_ids,
          record.id,
        ),
      )
    let new_staged =
      StagedState(
        ..staged,
        deleted_saved_search_ids: dict.delete(
          staged.deleted_saved_search_ids,
          record.id,
        ),
      )
    Store(..acc, base_state: new_base, staged_state: new_staged)
  })
}

/// Stage a saved-search record. Mirrors `upsertStagedSavedSearch`. The
/// TS version returns a fresh clone — Gleam values are already
/// immutable, so we return the record unchanged.
pub fn upsert_staged_saved_search(
  store: Store,
  record: SavedSearchRecord,
) -> #(SavedSearchRecord, Store) {
  let staged = store.staged_state
  let base = store.base_state
  let already_known =
    list.contains(base.saved_search_order, record.id)
    || list.contains(staged.saved_search_order, record.id)
  let new_order = case already_known {
    True -> staged.saved_search_order
    False -> list.append(staged.saved_search_order, [record.id])
  }
  let new_staged =
    StagedState(
      saved_searches: dict.insert(staged.saved_searches, record.id, record),
      saved_search_order: new_order,
      deleted_saved_search_ids: dict.delete(
        staged.deleted_saved_search_ids,
        record.id,
      ),
    )
  #(record, Store(..store, staged_state: new_staged))
}

/// Mark a saved-search id as deleted. Mirrors
/// `deleteStagedSavedSearch`.
pub fn delete_staged_saved_search(store: Store, id: String) -> Store {
  let staged = store.staged_state
  let new_staged =
    StagedState(
      ..staged,
      saved_searches: dict.delete(staged.saved_searches, id),
      deleted_saved_search_ids: dict.insert(
        staged.deleted_saved_search_ids,
        id,
        True,
      ),
    )
  Store(..store, staged_state: new_staged)
}

/// Look up the effective saved search for an id. Staged wins over base;
/// any "deleted" marker on either side suppresses the record.
/// Mirrors `getEffectiveSavedSearchById`.
pub fn get_effective_saved_search_by_id(
  store: Store,
  id: String,
) -> Option(SavedSearchRecord) {
  let deleted =
    dict_has(store.base_state.deleted_saved_search_ids, id)
    || dict_has(store.staged_state.deleted_saved_search_ids, id)
  case deleted {
    True -> None
    False ->
      case dict.get(store.staged_state.saved_searches, id) {
        Ok(record) -> Some(record)
        Error(_) ->
          case dict.get(store.base_state.saved_searches, id) {
            Ok(record) -> Some(record)
            Error(_) -> None
          }
      }
  }
}

/// List every effective saved search the store knows about. Mirrors
/// `listEffectiveSavedSearches`. Ordered records (those tracked by the
/// `savedSearchOrder` arrays) come first, followed by any unordered
/// staged/base records sorted by id.
pub fn list_effective_saved_searches(store: Store) -> List(SavedSearchRecord) {
  let ordered_ids =
    list.append(
      store.base_state.saved_search_order,
      store.staged_state.saved_search_order,
    )
    |> dedupe_strings()
  let ordered_records =
    list.filter_map(ordered_ids, fn(id) {
      case get_effective_saved_search_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_ids)
  let merged =
    dict.merge(
      store.base_state.saved_searches,
      store.staged_state.saved_searches,
    )
  let unordered_ids =
    dict.keys(merged)
    |> list.filter(fn(id) { !dict_has(ordered_set, id) })
    |> list.sort(string_compare)
  let unordered_records =
    list.filter_map(unordered_ids, fn(id) {
      case get_effective_saved_search_by_id(store, id) {
        Some(record) -> Ok(record)
        None -> Error(Nil)
      }
    })
  list.append(ordered_records, unordered_records)
}

// ---------------------------------------------------------------------------
// Mutation log
// ---------------------------------------------------------------------------

/// Append a mutation log entry. Mirrors `recordMutationLogEntry`.
pub fn record_mutation_log_entry(
  store: Store,
  entry: MutationLogEntry,
) -> Store {
  Store(..store, mutation_log: list.append(store.mutation_log, [entry]))
}

/// Read the mutation log in insertion order. Mirrors `getLog`.
pub fn get_log(store: Store) -> List(MutationLogEntry) {
  store.mutation_log
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn append_unique_id(order: List(String), id: String) -> List(String) {
  case list.contains(order, id) {
    True -> order
    False -> list.append(order, [id])
  }
}

fn dict_has(d: Dict(String, a), key: String) -> Bool {
  case dict.get(d, key) {
    Ok(_) -> True
    Error(_) -> False
  }
}

fn dedupe_strings(items: List(String)) -> List(String) {
  do_dedupe(items, dict.new(), [])
}

fn do_dedupe(
  remaining: List(String),
  seen: Dict(String, Bool),
  acc: List(String),
) -> List(String) {
  case remaining {
    [] -> list.reverse(acc)
    [first, ..rest] ->
      case dict.get(seen, first) {
        Ok(_) -> do_dedupe(rest, seen, acc)
        Error(_) ->
          do_dedupe(rest, dict.insert(seen, first, True), [first, ..acc])
      }
  }
}

fn list_to_set(items: List(String)) -> Dict(String, Bool) {
  list.fold(items, dict.new(), fn(acc, item) { dict.insert(acc, item, True) })
}

fn string_compare(a: String, b: String) -> order.Order {
  string.compare(a, b)
}
