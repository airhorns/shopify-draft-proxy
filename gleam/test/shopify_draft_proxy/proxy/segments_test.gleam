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
import shopify_draft_proxy/state/types.{
  type CustomerRecord, type SegmentRecord, CustomerDefaultEmailAddressRecord,
  CustomerRecord, Money, SegmentRecord,
}

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

fn customer(
  id: String,
  first_name: String,
  tags: List(String),
  number_of_orders: String,
) -> CustomerRecord {
  CustomerRecord(
    id: id,
    first_name: Some(first_name),
    last_name: None,
    display_name: Some(first_name),
    email: Some(first_name <> "@example.com"),
    legacy_resource_id: None,
    locale: Some("en"),
    note: None,
    can_delete: Some(True),
    verified_email: Some(True),
    data_sale_opt_out: False,
    tax_exempt: Some(False),
    tax_exemptions: [],
    state: Some("DISABLED"),
    tags: tags,
    number_of_orders: Some(number_of_orders),
    amount_spent: Some(Money(amount: "0.0", currency_code: "USD")),
    default_email_address: Some(CustomerDefaultEmailAddressRecord(
      email_address: Some(first_name <> "@example.com"),
      marketing_state: None,
      marketing_opt_in_level: None,
      marketing_updated_at: None,
    )),
    default_phone_number: None,
    email_marketing_consent: None,
    sms_marketing_consent: None,
    default_address: None,
    created_at: None,
    updated_at: None,
  )
}

fn seed_customer(store_in: store.Store, record: CustomerRecord) -> store.Store {
  let #(_, s) = store.stage_create_customer(store_in, record)
  s
}

// ----------- Predicates -----------

pub fn is_segment_query_root_test() {
  assert segments.is_segment_query_root("segment")
  assert segments.is_segment_query_root("segments")
  assert segments.is_segment_query_root("segmentsCount")
  assert segments.is_segment_query_root("segmentFilters")
  assert segments.is_segment_query_root("segmentFilterSuggestions")
  assert segments.is_segment_query_root("segmentValueSuggestions")
  assert segments.is_segment_query_root("segmentMigrations")
  assert segments.is_segment_query_root("customerSegmentMembers")
  assert segments.is_segment_query_root("customerSegmentMembersQuery")
  assert segments.is_segment_query_root("customerSegmentMembership")
  assert !segments.is_segment_query_root("segmentCreate")
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
    segment("gid://shopify/Segment/1", "VIPs", "number_of_orders >= 5")
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
  let result = run(s, "{ segments(first: 5) { nodes { id name } } }")
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

// ----------- customerSegmentMembers / customerSegmentMembership -----------

pub fn customer_segment_members_filters_staged_customers_test() {
  let s =
    store.new()
    |> seed_customer(customer(
      "gid://shopify/Customer/1",
      "Tagged",
      ["vip"],
      "0",
    ))
    |> seed_customer(customer("gid://shopify/Customer/2", "Untagged", [], "3"))

  let contains =
    run(
      s,
      "{ customerSegmentMembers(query: \"customer_tags CONTAINS 'vip'\", first: 5) { totalCount edges { node { firstName numberOfOrders defaultEmailAddress { emailAddress } amountSpent { amount currencyCode } } } } }",
    )
  assert contains
    == "{\"customerSegmentMembers\":{\"totalCount\":1,\"edges\":[{\"node\":{\"firstName\":\"Tagged\",\"numberOfOrders\":\"0\",\"defaultEmailAddress\":{\"emailAddress\":\"Tagged@example.com\"},\"amountSpent\":{\"amount\":\"0.0\",\"currencyCode\":\"USD\"}}}]}}"

  let not_contains =
    run(
      s,
      "{ customerSegmentMembers(query: \"customer_tags NOT CONTAINS 'vip'\", first: 5) { totalCount edges { node { firstName } } } }",
    )
  assert not_contains
    == "{\"customerSegmentMembers\":{\"totalCount\":1,\"edges\":[{\"node\":{\"firstName\":\"Untagged\"}}]}}"
}

pub fn customer_segment_membership_evaluates_known_segments_test() {
  let segment_id = "gid://shopify/Segment/30"
  let s =
    store.new()
    |> seed(segment(segment_id, "Zero orders", "number_of_orders = 0"))
    |> seed_customer(customer("gid://shopify/Customer/1", "Zero", [], "0"))
    |> seed_customer(customer("gid://shopify/Customer/2", "Three", [], "3"))

  let result =
    run(
      s,
      "{ yes: customerSegmentMembership(customerId: \"gid://shopify/Customer/1\", segmentIds: [\"gid://shopify/Segment/30\"]) { memberships { segmentId isMember } } no: customerSegmentMembership(customerId: \"gid://shopify/Customer/2\", segmentIds: [\"gid://shopify/Segment/30\", \"gid://shopify/Segment/missing\"]) { memberships { segmentId isMember } } }",
    )
  assert result
    == "{\"yes\":{\"memberships\":[{\"segmentId\":\"gid://shopify/Segment/30\",\"isMember\":true}]},\"no\":{\"memberships\":[{\"segmentId\":\"gid://shopify/Segment/30\",\"isMember\":false}]}}"
}
