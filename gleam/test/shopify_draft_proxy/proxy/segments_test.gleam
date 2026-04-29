//// Read-path tests for `proxy/segments`.
////
//// Covers:
////   - the three query roots (`segment`, `segments`, `segmentsCount`)
////   - field projection (id / name / query / creationDate / lastEditDate)
////   - default null handling for nullable fields
////   - effective listing through staged store records
////   - mutation/query root-name predicates

import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/segments
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{type SegmentRecord, SegmentRecord}

// ----------- Helpers -----------

fn segment(id: String, name: String, query: String) -> SegmentRecord {
  SegmentRecord(
    id: id,
    name: Some(name),
    query: Some(query),
    creation_date: Some("2024-01-01T00:00:00.000Z"),
    last_edit_date: Some("2024-01-02T00:00:00.000Z"),
  )
}

fn run(store_in: store.Store, query: String) -> String {
  let assert Ok(data) =
    segments.handle_segments_query(store_in, query, dict.new())
  json.to_string(data)
}

fn seed(store_in: store.Store, record: SegmentRecord) -> store.Store {
  let #(_, s) = store.upsert_staged_segment(store_in, record)
  s
}

// ----------- Predicates -----------

pub fn is_segment_query_root_test() {
  assert segments.is_segment_query_root("segment")
  assert segments.is_segment_query_root("segments")
  assert segments.is_segment_query_root("segmentsCount")
  assert segments.is_segment_query_root("customerSegmentMembers")
  assert segments.is_segment_query_root("customerSegmentMembersQuery")
  assert segments.is_segment_query_root("customerSegmentMembership")
  assert !segments.is_segment_query_root("segmentCreate")
  assert !segments.is_segment_query_root("segmentFilters")
}

pub fn is_segment_mutation_root_test() {
  assert segments.is_segment_mutation_root("segmentCreate")
  assert segments.is_segment_mutation_root("segmentUpdate")
  assert segments.is_segment_mutation_root("segmentDelete")
  assert segments.is_segment_mutation_root("customerSegmentMembersQueryCreate")
  assert !segments.is_segment_mutation_root("segment")
}

// ----------- segment(id:) -----------

pub fn segment_by_id_returns_record_test() {
  let record =
    segment(
      "gid://shopify/Segment/1",
      "VIPs",
      "number_of_orders >= 5",
    )
  let s = seed(store.new(), record)
  let result =
    run(
      s,
      "{ segment(id: \"gid://shopify/Segment/1\") { __typename id name query creationDate lastEditDate } }",
    )
  assert result
    == "{\"segment\":{\"__typename\":\"Segment\",\"id\":\"gid://shopify/Segment/1\",\"name\":\"VIPs\",\"query\":\"number_of_orders >= 5\",\"creationDate\":\"2024-01-01T00:00:00.000Z\",\"lastEditDate\":\"2024-01-02T00:00:00.000Z\"}}"
}

pub fn segment_by_id_missing_returns_null_test() {
  let result =
    run(
      store.new(),
      "{ segment(id: \"gid://shopify/Segment/missing\") { id } }",
    )
  assert result == "{\"segment\":null}"
}

pub fn segment_by_id_missing_argument_returns_null_test() {
  let result = run(store.new(), "{ segment { id } }")
  assert result == "{\"segment\":null}"
}

pub fn segment_nullable_fields_test() {
  let record =
    SegmentRecord(
      id: "gid://shopify/Segment/2",
      name: None,
      query: None,
      creation_date: None,
      last_edit_date: None,
    )
  let s = seed(store.new(), record)
  let result =
    run(
      s,
      "{ segment(id: \"gid://shopify/Segment/2\") { id name query creationDate lastEditDate } }",
    )
  assert result
    == "{\"segment\":{\"id\":\"gid://shopify/Segment/2\",\"name\":null,\"query\":null,\"creationDate\":null,\"lastEditDate\":null}}"
}

// ----------- segments connection -----------

pub fn segments_connection_empty_test() {
  let result = run(store.new(), "{ segments(first: 5) { nodes { id } } }")
  assert result == "{\"segments\":{\"nodes\":[]}}"
}

pub fn segments_connection_returns_seeded_test() {
  let r1 = segment("gid://shopify/Segment/10", "VIPs", "number_of_orders >= 5")
  let r2 =
    segment(
      "gid://shopify/Segment/11",
      "Tagged",
      "customer_tags CONTAINS 'gold'",
    )
  let s =
    store.new()
    |> seed(r1)
    |> seed(r2)
  let result =
    run(s, "{ segments(first: 5) { nodes { id name } } }")
  assert result
    == "{\"segments\":{\"nodes\":[{\"id\":\"gid://shopify/Segment/10\",\"name\":\"VIPs\"},{\"id\":\"gid://shopify/Segment/11\",\"name\":\"Tagged\"}]}}"
}

// ----------- segmentsCount -----------

pub fn segments_count_zero_test() {
  let result = run(store.new(), "{ segmentsCount { count } }")
  assert result == "{\"segmentsCount\":{\"count\":0}}"
}

pub fn segments_count_seeded_test() {
  let r1 = segment("gid://shopify/Segment/20", "A", "number_of_orders >= 1")
  let r2 = segment("gid://shopify/Segment/21", "B", "number_of_orders >= 2")
  let s =
    store.new()
    |> seed(r1)
    |> seed(r2)
  let result = run(s, "{ segmentsCount { count precision } }")
  assert result == "{\"segmentsCount\":{\"count\":2,\"precision\":\"EXACT\"}}"
}
