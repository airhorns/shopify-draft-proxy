import gleam/dict
import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{SavedSearchRecord}

fn record(id: String, name: String) -> types.SavedSearchRecord {
  SavedSearchRecord(
    id: id,
    legacy_resource_id: id,
    name: name,
    query: "status:open",
    resource_type: "ORDER",
    search_terms: "",
    filters: [],
    cursor: None,
  )
}

pub fn new_store_is_empty_test() {
  let s = store.new()
  assert store.list_effective_saved_searches(s) == []
  assert store.get_log(s) == []
}

pub fn upsert_base_saved_searches_orders_inserts_test() {
  let s =
    store.upsert_base_saved_searches(store.new(), [
      record("a", "A"),
      record("b", "B"),
    ])
  let names =
    store.list_effective_saved_searches(s)
    |> list.map(fn(r) { r.name })
  assert names == ["A", "B"]
}

pub fn staged_overrides_base_test() {
  let base = record("a", "Base A")
  let staged = record("a", "Staged A")
  let s =
    store.new()
    |> store.upsert_base_saved_searches([base])
  let #(_, s) = store.upsert_staged_saved_search(s, staged)
  let assert Some(found) = store.get_effective_saved_search_by_id(s, "a")
  assert found.name == "Staged A"
}

pub fn delete_staged_hides_record_test() {
  let s =
    store.new()
    |> store.upsert_base_saved_searches([record("a", "A")])
  let s = store.delete_staged_saved_search(s, "a")
  assert store.get_effective_saved_search_by_id(s, "a") == None
  assert store.list_effective_saved_searches(s) == []
}

pub fn upsert_base_clears_deleted_marker_test() {
  let s =
    store.new()
    |> store.upsert_base_saved_searches([record("a", "A")])
    |> store.delete_staged_saved_search("a")
  let s = store.upsert_base_saved_searches(s, [record("a", "A again")])
  let assert Some(found) = store.get_effective_saved_search_by_id(s, "a")
  assert found.name == "A again"
}

pub fn list_returns_ordered_then_unordered_test() {
  let s =
    store.new()
    |> store.upsert_base_saved_searches([record("z", "Z"), record("a", "A")])
  let names =
    store.list_effective_saved_searches(s)
    |> list.map(fn(r) { r.name })
  assert names == ["Z", "A"]
}

pub fn get_log_preserves_insertion_order_test() {
  let entry1 =
    store.MutationLogEntry(
      id: "log-1",
      received_at: "2024-01-01T00:00:00.000Z",
      operation_name: None,
      path: "/admin/api/2025-01/graphql.json",
      query: "mutation { ... }",
      variables: dict.new(),
      staged_resource_ids: [],
      status: store.Staged,
      interpreted: store.InterpretedMetadata(
        operation_type: store.Mutation,
        operation_name: None,
        root_fields: ["x"],
        primary_root_field: Some("x"),
        capability: store.Capability(
          operation_name: Some("x"),
          domain: "saved-searches",
          execution: "stage-locally",
        ),
      ),
      notes: None,
    )
  let entry2 = store.MutationLogEntry(..entry1, id: "log-2")
  let s =
    store.new()
    |> store.record_mutation_log_entry(entry1)
    |> store.record_mutation_log_entry(entry2)
  let ids = list.map(store.get_log(s), fn(e) { e.id })
  assert ids == ["log-1", "log-2"]
}

pub fn reset_clears_state_test() {
  let s =
    store.new()
    |> store.upsert_base_saved_searches([record("a", "A")])
  assert list.length(store.list_effective_saved_searches(s)) == 1
  let s = store.reset(s)
  assert store.list_effective_saved_searches(s) == []
}
