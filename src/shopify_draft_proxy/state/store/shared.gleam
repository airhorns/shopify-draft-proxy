//// Shared store helpers used by entity bucket modules.

import gleam/dict.{type Dict}
import gleam/int
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/string
import shopify_draft_proxy/state/store/types.{
  type EntryStatus, type MutationLogEntry, type Store, MutationLogEntry, Store,
}
import shopify_draft_proxy/state/types.{
  type AppRecord, type ChannelRecord, type CollectionRecord,
  type DelegatedAccessTokenRecord, type ProductCollectionRecord,
  type PublicationRecord, ChannelRecord,
} as _

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

/// Update the status and notes of a single log entry, looked up by id.
/// Mirrors `InMemoryStore.updateLogEntry` — used by the commit path to
/// flip entries from `Staged` to `Committed` or `Failed` and stamp the
/// reason. A no-op when no entry matches the id.
pub fn update_log_entry(
  store: Store,
  id: String,
  status: EntryStatus,
  notes: Option(String),
) -> Store {
  let updated =
    list.map(store.mutation_log, fn(entry) {
      case entry.id == id {
        True -> MutationLogEntry(..entry, status: status, notes: notes)
        False -> entry
      }
    })
  Store(..store, mutation_log: updated)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

@internal
pub fn append_unique_id(order: List(String), id: String) -> List(String) {
  case list.contains(order, id) {
    True -> order
    False -> list.append(order, [id])
  }
}

@internal
pub fn append_unique_ids(
  left: List(String),
  right: List(String),
) -> List(String) {
  list.fold(right, left, append_unique_id)
}

@internal
pub fn product_collection_storage_key(
  record: ProductCollectionRecord,
) -> String {
  record.product_id <> "::" <> record.collection_id
}

@internal
pub fn compare_product_collection_records(
  left: ProductCollectionRecord,
  right: ProductCollectionRecord,
) -> order.Order {
  case int.compare(left.position, right.position) {
    order.Eq -> string.compare(left.product_id, right.product_id)
    other -> other
  }
}

@internal
pub fn compare_collection_membership_entries(
  left: #(CollectionRecord, ProductCollectionRecord),
  right: #(CollectionRecord, ProductCollectionRecord),
) -> order.Order {
  let #(left_collection, _) = left
  let #(right_collection, _) = right
  case string.compare(left_collection.title, right_collection.title) {
    order.Eq -> string.compare(left_collection.id, right_collection.id)
    other -> other
  }
}

@internal
pub fn channel_from_publication(
  publication: PublicationRecord,
) -> Option(ChannelRecord) {
  case publication.channel_id {
    Some(id) ->
      Some(ChannelRecord(
        id: id,
        name: publication.name,
        handle: None,
        publication_id: Some(publication.id),
        cursor: None,
      ))
    None -> {
      let tail = resource_tail(publication.id)
      case tail == "" {
        True -> None
        False ->
          Some(ChannelRecord(
            id: "gid://shopify/Channel/" <> tail,
            name: publication.name,
            handle: None,
            publication_id: Some(publication.id),
            cursor: None,
          ))
      }
    }
  }
}

@internal
pub fn resource_tail(id: String) -> String {
  case list.last(string.split(id, "/")) {
    Ok(tail_with_query) ->
      case string.split(tail_with_query, "?") {
        [tail, ..] -> tail
        [] -> tail_with_query
      }
    Error(_) -> ""
  }
}

@internal
pub fn dict_has(d: Dict(String, a), key: String) -> Bool {
  case dict.get(d, key) {
    Ok(_) -> True
    Error(_) -> False
  }
}

@internal
pub fn option_to_result(value: Option(a)) -> Result(a, Nil) {
  case value {
    Some(value) -> Ok(value)
    None -> Error(Nil)
  }
}

@internal
pub fn dedupe_strings(items: List(String)) -> List(String) {
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

@internal
pub fn list_to_set(items: List(String)) -> Dict(String, Bool) {
  list.fold(items, dict.new(), fn(acc, item) { dict.insert(acc, item, True) })
}

@internal
pub fn string_compare(a: String, b: String) -> order.Order {
  string.compare(a, b)
}

@internal
pub fn bool_compare(a: Bool, b: Bool) -> order.Order {
  case a, b {
    True, False -> order.Gt
    False, True -> order.Lt
    _, _ -> order.Eq
  }
}

@internal
pub fn find_app_in_dict(
  d: Dict(String, AppRecord),
  predicate: fn(AppRecord) -> Bool,
) -> Option(AppRecord) {
  list.find(dict.values(d), predicate)
  |> option.from_result
}

@internal
pub fn find_token_in_dict(
  d: Dict(String, DelegatedAccessTokenRecord),
  predicate: fn(DelegatedAccessTokenRecord) -> Bool,
) -> Option(DelegatedAccessTokenRecord) {
  list.find(dict.values(d), predicate)
  |> option.from_result
}
