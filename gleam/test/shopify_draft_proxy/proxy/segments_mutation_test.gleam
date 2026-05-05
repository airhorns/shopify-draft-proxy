//// Mutation-path tests for `proxy/segments`.
////
//// Covers all 3 mutation roots (`segmentCreate`/`Update`/`Delete`),
//// the `process_mutation` `{"data": …}` envelope, the synthetic-id /
//// timestamp threading, the user-error path on blank/invalid input,
//// and the `resolveUniqueSegmentName` " (N)" suffix collision logic.

import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/segments
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{type SegmentRecord, SegmentRecord}

// ----------- Helpers -----------

fn run_mutation_outcome(
  store_in: store.Store,
  document: String,
) -> mutation_helpers.MutationOutcome {
  let identity = synthetic_identity.new()
  let outcome =
    segments.process_mutation(
      store_in,
      identity,
      "/admin/api/2025-01/graphql.json",
      document,
      dict.new(),
    )
  outcome
}

fn run_mutation(store_in: store.Store, document: String) -> String {
  json.to_string(run_mutation_outcome(store_in, document).data)
}

fn seed(store_in: store.Store, record: SegmentRecord) -> store.Store {
  let #(_, s) = store.upsert_staged_segment(store_in, record)
  s
}

fn segment_record(id: String, name: String, query: String) -> SegmentRecord {
  SegmentRecord(
    id: id,
    name: Some(name),
    query: Some(query),
    creation_date: Some("2024-01-01T00:00:00.000Z"),
    last_edit_date: Some("2024-01-01T00:00:00.000Z"),
  )
}

// ----------- envelope -----------

pub fn process_mutation_returns_data_envelope_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { segmentCreate(name: \"VIPs\", query: \"number_of_orders >= 5\") { segment { id name } userErrors { field } } }",
    )
  // Always wraps in `{"data": {...}}`.
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":{\"id\":\"gid://shopify/Segment/1\",\"name\":\"VIPs\"},\"userErrors\":[]}}}"
}

// ----------- segmentCreate -----------

pub fn segment_create_mints_record_test() {
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { segmentCreate(name: \"VIPs\", query: \"number_of_orders >= 5\") { segment { id name query creationDate lastEditDate } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":{\"id\":\"gid://shopify/Segment/1\",\"name\":\"VIPs\",\"query\":\"number_of_orders >= 5\",\"creationDate\":\"2024-01-01T00:00:00.000Z\",\"lastEditDate\":\"2024-01-01T00:00:00.000Z\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == ["gid://shopify/Segment/1"]
  let assert Some(_) =
    store.get_effective_segment_by_id(outcome.store, "gid://shopify/Segment/1")
}

pub fn segment_create_customer_tags_contains_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { segmentCreate(name: \"Tagged\", query: \"customer_tags CONTAINS 'gold'\") { segment { id name query } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":{\"id\":\"gid://shopify/Segment/1\",\"name\":\"Tagged\",\"query\":\"customer_tags CONTAINS 'gold'\"},\"userErrors\":[]}}}"
}

pub fn segment_create_blank_name_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { segmentCreate(name: \"\", query: \"number_of_orders >= 5\") { segment { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"name\"],\"message\":\"Name can't be blank\"}]}}}"
}

pub fn segment_create_missing_query_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { segmentCreate(name: \"VIPs\") { segment { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"Query can't be blank\"}]}}}"
}

pub fn segment_create_invalid_query_emits_filter_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { segmentCreate(name: \"Bad\", query: \"foo bar\") { segment { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"Query Line 1 Column 1: 'foo' filter cannot be found.\"}]}}}"
}

pub fn segment_create_customer_tags_equals_emits_operator_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { segmentCreate(name: \"Bad\", query: \"customer_tags = 'x'\") { segment { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"Query Line 1 Column 14: customer_tags does not support operator '='\"}]}}}"
}

// ----------- segmentUpdate -----------

pub fn segment_update_happy_path_test() {
  let existing =
    segment_record("gid://shopify/Segment/100", "VIPs", "number_of_orders >= 5")
  let s = seed(store.new(), existing)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/100\", name: \"Top VIPs\", query: \"number_of_orders >= 10\") { segment { id name query lastEditDate } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  // First synthetic timestamp consumed during update.
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":{\"id\":\"gid://shopify/Segment/100\",\"name\":\"Top VIPs\",\"query\":\"number_of_orders >= 10\",\"lastEditDate\":\"2024-01-01T00:00:00.000Z\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == ["gid://shopify/Segment/100"]
}

pub fn segment_update_name_only_preserves_query_test() {
  let existing =
    segment_record("gid://shopify/Segment/101", "Old", "number_of_orders >= 1")
  let s = seed(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/101\", name: \"Renamed\") { segment { id name query } userErrors { field } } }",
    )
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":{\"id\":\"gid://shopify/Segment/101\",\"name\":\"Renamed\",\"query\":\"number_of_orders >= 1\"},\"userErrors\":[]}}}"
}

pub fn segment_update_missing_id_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/missing\", name: \"X\") { segment { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Segment does not exist\"}]}}}"
}

pub fn segment_update_blank_name_emits_user_error_test() {
  let existing =
    segment_record("gid://shopify/Segment/102", "Keep", "number_of_orders >= 2")
  let s = seed(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/102\", name: \"\") { segment { id } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"name\"],\"message\":\"Name can't be blank\"}]}}}"
}

// ----------- segmentDelete -----------

pub fn segment_delete_returns_deleted_id_test() {
  let existing =
    segment_record(
      "gid://shopify/Segment/200",
      "Doomed",
      "number_of_orders >= 3",
    )
  let s = seed(store.new(), existing)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { segmentDelete(id: \"gid://shopify/Segment/200\") { deletedSegmentId userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentDelete\":{\"deletedSegmentId\":\"gid://shopify/Segment/200\",\"userErrors\":[]}}}"
  // Deletion marker suppresses the record in the effective getter.
  assert store.get_effective_segment_by_id(
      outcome.store,
      "gid://shopify/Segment/200",
    )
    == None
}

pub fn segment_delete_missing_id_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { segmentDelete(id: \"gid://shopify/Segment/missing\") { deletedSegmentId userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentDelete\":{\"deletedSegmentId\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Segment does not exist\"}]}}}"
}

// ----------- unique-name resolution -----------

pub fn segment_create_resolves_collision_with_suffix_test() {
  let existing =
    segment_record("gid://shopify/Segment/300", "VIPs", "number_of_orders >= 5")
  let s = seed(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { segmentCreate(name: \"VIPs\", query: \"number_of_orders >= 10\") { segment { name } userErrors { field } } }",
    )
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":{\"name\":\"VIPs (2)\"},\"userErrors\":[]}}}"
}

pub fn segment_create_resolves_double_collision_test() {
  let existing1 =
    segment_record("gid://shopify/Segment/301", "VIPs", "number_of_orders >= 5")
  let existing2 =
    segment_record(
      "gid://shopify/Segment/302",
      "VIPs (2)",
      "number_of_orders >= 6",
    )
  let s =
    store.new()
    |> seed(existing1)
    |> seed(existing2)
  let body =
    run_mutation(
      s,
      "mutation { segmentCreate(name: \"VIPs\", query: \"number_of_orders >= 10\") { segment { name } userErrors { field } } }",
    )
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":{\"name\":\"VIPs (3)\"},\"userErrors\":[]}}}"
}

pub fn segment_update_self_rename_does_not_collide_test() {
  // An update keeping its own existing name shouldn't suffix-bump itself.
  let existing =
    segment_record("gid://shopify/Segment/400", "Solo", "number_of_orders >= 1")
  let s = seed(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/400\", name: \"Solo\") { segment { name } userErrors { field } } }",
    )
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":{\"name\":\"Solo\"},\"userErrors\":[]}}}"
}

// ----------- predicate -----------

pub fn is_segment_mutation_root_predicate_test() {
  assert segments.is_segment_mutation_root("segmentCreate")
  assert !segments.is_segment_mutation_root("segment")
}
