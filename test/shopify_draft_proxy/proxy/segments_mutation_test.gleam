//// Mutation-path tests for `proxy/segments`.
////
//// Covers all 3 mutation roots (`segmentCreate`/`Update`/`Delete`),
//// the `process_mutation` `{"data": …}` envelope, the synthetic-id /
//// timestamp threading, the user-error path on blank/invalid input,
//// and the `resolveUniqueSegmentName` " (N)" suffix collision logic.

import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/proxy_state.{DraftProxy, Request, Response}
import shopify_draft_proxy/proxy/segments
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type CustomerRecord, type SegmentRecord, CustomerDefaultEmailAddressRecord,
  CustomerRecord, Money, SegmentRecord,
}

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
      empty_upstream_context(),
    )
  outcome
}

fn run_mutation(store_in: store.Store, document: String) -> String {
  json.to_string(run_mutation_outcome(store_in, document).data)
}

fn run_mutation_with_variables(
  store_in: store.Store,
  document: String,
  variables: dict.Dict(String, root_field.ResolvedValue),
) -> String {
  json.to_string(
    run_mutation_outcome_with_variables(store_in, document, variables).data,
  )
}

fn run_mutation_outcome_with_variables(
  store_in: store.Store,
  document: String,
  variables: dict.Dict(String, root_field.ResolvedValue),
) -> mutation_helpers.MutationOutcome {
  let identity = synthetic_identity.new()
  segments.process_mutation(
    store_in,
    identity,
    "/admin/api/2025-01/graphql.json",
    document,
    variables,
    empty_upstream_context(),
  )
}

fn graphql_request(body: String) -> draft_proxy.Request {
  Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: dict.new(),
    body: json.to_string(json.object([#("query", json.string(body))])),
  )
}

fn run_proxy_mutation(
  store_in: store.Store,
  document: String,
) -> #(Int, String, draft_proxy.DraftProxy) {
  let proxy = draft_proxy.new()
  let proxy = DraftProxy(..proxy, store: store_in)
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(document))
  #(status, json.to_string(body), next_proxy)
}

fn run_proxy_graphql(
  proxy: draft_proxy.DraftProxy,
  document: String,
) -> #(Int, String, draft_proxy.DraftProxy) {
  let #(Response(status: status, body: body, ..), next_proxy) =
    draft_proxy.process_request(proxy, graphql_request(document))
  #(status, json.to_string(body), next_proxy)
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

fn seed_segment_count(
  store_in: store.Store,
  next: Int,
  remaining: Int,
) -> store.Store {
  case remaining <= 0 {
    True -> store_in
    False -> {
      let suffix = int.to_string(next)
      let seeded =
        seed(
          store_in,
          segment_record(
            "gid://shopify/Segment/" <> suffix,
            "Seed " <> suffix,
            "number_of_orders >= 1",
          ),
        )
      seed_segment_count(seeded, next + 1, remaining - 1)
    }
  }
}

fn seed_duplicate_segment_names(
  store_in: store.Store,
  base_name: String,
  next: Int,
  last: Int,
) -> store.Store {
  case next > last {
    True -> store_in
    False -> {
      let suffix = int.to_string(next)
      let name = case next {
        1 -> base_name
        _ -> base_name <> " (" <> suffix <> ")"
      }
      let seeded =
        seed(
          store_in,
          segment_record(
            "gid://shopify/Segment/retry-" <> suffix,
            name,
            "number_of_orders >= 1",
          ),
        )
      seed_duplicate_segment_names(seeded, base_name, next + 1, last)
    }
  }
}

fn customer(
  id: String,
  first_name: String,
  number_of_orders: String,
) -> CustomerRecord {
  CustomerRecord(
    id: id,
    first_name: Some(first_name),
    last_name: None,
    display_name: Some(first_name),
    email: Some(string.lowercase(first_name) <> "@example.com"),
    legacy_resource_id: None,
    locale: Some("en"),
    note: None,
    can_delete: Some(True),
    verified_email: Some(True),
    data_sale_opt_out: False,
    tax_exempt: Some(False),
    tax_exemptions: [],
    state: Some("DISABLED"),
    tags: [],
    number_of_orders: Some(number_of_orders),
    amount_spent: Some(Money(amount: "0.0", currency_code: "USD")),
    default_email_address: Some(CustomerDefaultEmailAddressRecord(
      email_address: Some(string.lowercase(first_name) <> "@example.com"),
      marketing_state: None,
      marketing_opt_in_level: None,
      marketing_updated_at: None,
    )),
    default_phone_number: None,
    email_marketing_consent: None,
    sms_marketing_consent: None,
    default_address: None,
    account_activation_token: None,
    created_at: None,
    updated_at: None,
  )
}

fn seed_customer(store_in: store.Store, record: CustomerRecord) -> store.Store {
  let #(_, s) = store.stage_create_customer(store_in, record)
  s
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

// ----------- customerSegmentMembersQueryCreate -----------

pub fn member_query_create_rejects_both_query_and_segment_id_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { customerSegmentMembersQueryCreate(input: { segmentId: \"gid://shopify/Segment/1\", query: \"number_of_orders > 0\" }) { customerSegmentMembersQuery { id status currentCount done } userErrors { __typename field code message } } }",
    )
  assert body
    == "{\"data\":{\"customerSegmentMembersQueryCreate\":{\"customerSegmentMembersQuery\":null,\"userErrors\":[{\"__typename\":\"CustomerSegmentMembersQueryUserError\",\"field\":[\"input\"],\"code\":\"INVALID\",\"message\":\"Providing both segment_id and query is not supported.\"}]}}}"
}

pub fn member_query_create_rejects_neither_query_nor_segment_id_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { customerSegmentMembersQueryCreate(input: {}) { customerSegmentMembersQuery { id status currentCount done } userErrors { __typename field code message } } }",
    )
  assert body
    == "{\"data\":{\"customerSegmentMembersQueryCreate\":{\"customerSegmentMembersQuery\":null,\"userErrors\":[{\"__typename\":\"CustomerSegmentMembersQueryUserError\",\"field\":[\"input\"],\"code\":\"INVALID\",\"message\":\"You must provide one of segment_id or query.\"}]}}}"
}

pub fn member_query_create_invalid_query_uses_member_error_type_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { customerSegmentMembersQueryCreate(input: { query: \"not a valid segment query ???\" }) { customerSegmentMembersQuery { id } userErrors { __typename field code message } } }",
    )
  assert body
    == "{\"data\":{\"customerSegmentMembersQueryCreate\":{\"customerSegmentMembersQuery\":null,\"userErrors\":[{\"__typename\":\"CustomerSegmentMembersQueryUserError\",\"field\":null,\"code\":\"INVALID\",\"message\":\"Line 1 Column 6: 'valid' is unexpected.\"}]}}}"
}

pub fn member_query_create_stages_initialized_query_job_test() {
  let s =
    store.new()
    |> seed_customer(customer("gid://shopify/Customer/1", "Buyer", "3"))
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { customerSegmentMembersQueryCreate(input: { query: \"number_of_orders > 0\" }) { customerSegmentMembersQuery { id status currentCount done } userErrors { field code message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"customerSegmentMembersQueryCreate\":{\"customerSegmentMembersQuery\":{\"id\":\"gid://shopify/CustomerSegmentMembersQuery/1\",\"status\":\"INITIALIZED\",\"currentCount\":0,\"done\":false},\"userErrors\":[]}}}"
  let assert Some(record) =
    store.get_effective_customer_segment_members_query_by_id(
      outcome.store,
      "gid://shopify/CustomerSegmentMembersQuery/1",
    )
  assert record.status == "INITIALIZED"
  assert record.current_count == 0
  assert record.done == False

  let assert Ok(lookup) =
    segments.handle_segments_query(
      outcome.store,
      "{ customerSegmentMembersQuery(id: \"gid://shopify/CustomerSegmentMembersQuery/1\") { id status currentCount done } }",
      dict.new(),
    )
  assert json.to_string(lookup)
    == "{\"customerSegmentMembersQuery\":{\"id\":\"gid://shopify/CustomerSegmentMembersQuery/1\",\"status\":\"INITIALIZED\",\"currentCount\":0,\"done\":false}}"
}

pub fn member_query_create_accepts_broad_save_time_grammar_test() {
  let s =
    store.new()
    |> seed_customer(customer("gid://shopify/Customer/1", "Buyer", "3"))
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { customerSegmentMembersQueryCreate(input: { query: \"country = 'CA'\" }) { customerSegmentMembersQuery { id status currentCount done } userErrors { field code message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"customerSegmentMembersQueryCreate\":{\"customerSegmentMembersQuery\":{\"id\":\"gid://shopify/CustomerSegmentMembersQuery/1\",\"status\":\"INITIALIZED\",\"currentCount\":0,\"done\":false},\"userErrors\":[]}}}"

  let assert Ok(members) =
    segments.handle_segments_query(
      outcome.store,
      "{ customerSegmentMembers(queryId: \"gid://shopify/CustomerSegmentMembersQuery/1\", first: 10) { nodes { id } } }",
      dict.new(),
    )
  assert json.to_string(members)
    == "{\"customerSegmentMembers\":{\"nodes\":[]}}"
}

pub fn member_query_create_from_segment_id_stages_initialized_query_job_test() {
  let s =
    store.new()
    |> seed(segment_record(
      "gid://shopify/Segment/55",
      "Buyers",
      "number_of_orders > 0",
    ))
  let body =
    run_mutation(
      s,
      "mutation { customerSegmentMembersQueryCreate(input: { segmentId: \"gid://shopify/Segment/55\" }) { customerSegmentMembersQuery { id status currentCount done } userErrors { field code message } } }",
    )
  assert body
    == "{\"data\":{\"customerSegmentMembersQueryCreate\":{\"customerSegmentMembersQuery\":{\"id\":\"gid://shopify/CustomerSegmentMembersQuery/1\",\"status\":\"INITIALIZED\",\"currentCount\":0,\"done\":false},\"userErrors\":[]}}}"
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

pub fn segment_create_returns_payload_defaults_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { segmentCreate(name: \"VIPs\", query: \"number_of_orders >= 5\") { segment { id tagMigrated valid percentageSnapshot percentageSnapshotUpdatedAt translation author { name } } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":{\"id\":\"gid://shopify/Segment/1\",\"tagMigrated\":false,\"valid\":true,\"percentageSnapshot\":null,\"percentageSnapshotUpdatedAt\":null,\"translation\":null,\"author\":null},\"userErrors\":[]}}}"
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

pub fn segment_create_accepts_broad_query_grammar_test() {
  let cases = [
    #("Country", "country = 'CA'"),
    #("Total spent", "total_spent > 100"),
    #("Companies", "companies IS NULL"),
    #("And", "number_of_orders >= 1 AND country = 'CA'"),
    #("Or", "(number_of_orders >= 1) OR (number_of_orders = 0)"),
    #("Relative date", "last_order_date >= -30d"),
    #("Not null", "companies IS NOT NULL"),
    #("Contains", "customer_cities NOT CONTAINS 'US-CA-LosAngeles'"),
  ]
  list.each(cases, fn(test_case) {
    let #(name, query) = test_case
    let body =
      run_mutation(
        store.new(),
        "mutation { segmentCreate(name: \""
          <> name
          <> "\", query: \""
          <> query
          <> "\") { segment { id name query } userErrors { field message } } }",
      )
    assert body
      == "{\"data\":{\"segmentCreate\":{\"segment\":{\"id\":\"gid://shopify/Segment/1\",\"name\":\""
      <> name
      <> "\",\"query\":\""
      <> query
      <> "\"},\"userErrors\":[]}}}"
  })
}

pub fn segment_create_blank_name_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { segmentCreate(name: \"\", query: \"number_of_orders >= 5\") { segment { id } userErrors { __typename field code message } } }",
    )
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":null,\"userErrors\":[{\"__typename\":\"UserError\",\"field\":[\"name\"],\"code\":null,\"message\":\"Name can't be blank\"}]}}}"
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

pub fn segment_create_overlong_name_emits_user_error_test() {
  let long_name = string.repeat("N", times: 256)
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { segmentCreate(name: \""
        <> long_name
        <> "\", query: \"number_of_orders >= 5\") { segment { id } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"name\"],\"message\":\"Name is too long (maximum is 255 characters)\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert store.list_effective_segments(outcome.store) == []
}

pub fn segment_create_accepts_name_when_stripped_length_is_within_limit_test() {
  let padded_name =
    string.repeat(" ", times: 250) <> "abc" <> string.repeat(" ", times: 10)
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { segmentCreate(name: \""
        <> padded_name
        <> "\", query: \"number_of_orders = 0\") { segment { id name query } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":{\"id\":\"gid://shopify/Segment/1\",\"name\":\"abc\",\"query\":\"number_of_orders = 0\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == ["gid://shopify/Segment/1"]
  let assert Some(record) =
    store.get_effective_segment_by_id(outcome.store, "gid://shopify/Segment/1")
  assert record.name == Some("abc")
}

pub fn segment_create_padded_name_reads_back_stripped_name_test() {
  let padded_name =
    string.repeat(" ", times: 250) <> "abc" <> string.repeat(" ", times: 10)
  let create_document =
    "mutation { segmentCreate(name: \""
    <> padded_name
    <> "\", query: \"number_of_orders = 0\") { segment { id name } userErrors { field message } } }"
  let #(create_status, create_body, after_create) =
    run_proxy_mutation(store.new(), create_document)
  assert create_status == 200
  assert create_body
    == "{\"data\":{\"segmentCreate\":{\"segment\":{\"id\":\"gid://shopify/Segment/1\",\"name\":\"abc\"},\"userErrors\":[]}}}"

  let #(segment_status, segment_body, after_segment) =
    run_proxy_graphql(
      after_create,
      "{ segment(id: \"gid://shopify/Segment/1\") { id name } }",
    )
  assert segment_status == 200
  assert segment_body
    == "{\"data\":{\"segment\":{\"id\":\"gid://shopify/Segment/1\",\"name\":\"abc\"}}}"

  let #(segments_status, segments_body, after_segments) =
    run_proxy_graphql(
      after_segment,
      "{ segments(first: 5) { nodes { id name } } }",
    )
  assert segments_status == 200
  assert segments_body
    == "{\"data\":{\"segments\":{\"nodes\":[{\"id\":\"gid://shopify/Segment/1\",\"name\":\"abc\"}]}}}"

  let #(count_status, count_body, _) =
    run_proxy_graphql(after_segments, "{ segmentsCount { count precision } }")
  assert count_status == 200
  assert count_body
    == "{\"data\":{\"segmentsCount\":{\"count\":1,\"precision\":\"EXACT\"}}}"
}

pub fn segment_create_overlong_query_emits_length_error_before_grammar_test() {
  let long_query = "number_of_orders >= 5 " <> string.repeat("x", times: 5000)
  let outcome =
    run_mutation_outcome(
      store.new(),
      "mutation { segmentCreate(name: \"Big\", query: \""
        <> long_query
        <> "\") { segment { id } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"Query is too long (maximum is 5000 characters)\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert store.list_effective_segments(outcome.store) == []
}

pub fn segment_create_rejects_query_when_raw_length_exceeds_limit_test() {
  let raw_over_limit_query =
    string.repeat(" ", times: 4000)
    <> "number_of_orders = 0"
    <> string.repeat(" ", times: 1500)
  let outcome =
    run_mutation_outcome_with_variables(
      store.new(),
      "mutation SegmentCreateRawQueryLimit($query: String!) { segmentCreate(name: \"Big\", query: $query) { segment { id } userErrors { field message } } }",
      dict.from_list([#("query", root_field.StringVal(raw_over_limit_query))]),
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"Query is too long (maximum is 5000 characters)\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert store.list_effective_segments(outcome.store) == []
}

pub fn segment_create_at_shop_segment_limit_emits_user_error_test() {
  let full_store = seed_segment_count(store.new(), 1, 6000)
  let outcome =
    run_mutation_outcome(
      full_store,
      "mutation { segmentCreate(name: \"extra\", query: \"number_of_orders >= 1\") { segment { id } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":null,\"userErrors\":[{\"field\":null,\"message\":\"Segment limit reached. Delete an existing segment to create more.\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert list.length(store.list_effective_segments(outcome.store)) == 6000
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

pub fn segment_update_returns_payload_defaults_test() {
  let existing =
    segment_record("gid://shopify/Segment/100", "VIPs", "number_of_orders >= 5")
  let s = seed(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/100\", name: \"Top VIPs\") { segment { id tagMigrated valid percentageSnapshot percentageSnapshotUpdatedAt translation author { name } } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":{\"id\":\"gid://shopify/Segment/100\",\"tagMigrated\":false,\"valid\":true,\"percentageSnapshot\":null,\"percentageSnapshotUpdatedAt\":null,\"translation\":null,\"author\":null},\"userErrors\":[]}}}"
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

pub fn segment_update_accepts_broad_query_grammar_test() {
  let existing =
    segment_record("gid://shopify/Segment/106", "Old", "number_of_orders >= 1")
  let s = seed(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/106\", query: \"(number_of_orders >= 1) OR (number_of_orders = 0)\") { segment { id name query } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":{\"id\":\"gid://shopify/Segment/106\",\"name\":\"Old\",\"query\":\"(number_of_orders >= 1) OR (number_of_orders = 0)\"},\"userErrors\":[]}}}"
}

pub fn segment_update_missing_id_emits_user_error_test() {
  let body =
    run_mutation(
      store.new(),
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/missing\", name: \"X\") { segment { id } userErrors { __typename field code message } } }",
    )
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":null,\"userErrors\":[{\"__typename\":\"UserError\",\"field\":[\"id\"],\"code\":null,\"message\":\"Segment does not exist\"}]}}}"
}

pub fn segment_update_malformed_gid_variable_is_top_level_error_test() {
  let body =
    run_mutation_with_variables(
      store.new(),
      "mutation SegmentUpdateMalformedGid($id: ID!, $name: String!) { segmentUpdate(id: $id, name: $name) { segment { id } userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal("not-a-gid")),
        #("name", root_field.StringVal("x")),
      ]),
    )
  assert body
    == "{\"errors\":[{\"message\":\"Variable $id of type ID! was provided invalid value\",\"extensions\":{\"code\":\"INVALID_VARIABLE\",\"value\":\"not-a-gid\",\"problems\":[{\"path\":[],\"explanation\":\"Invalid global id 'not-a-gid'\",\"message\":\"Invalid global id 'not-a-gid'\"}]}}]}"
}

pub fn segment_update_empty_gid_variable_is_top_level_error_test() {
  let body =
    run_mutation_with_variables(
      store.new(),
      "mutation SegmentUpdateMalformedGid($id: ID!, $name: String!) { segmentUpdate(id: $id, name: $name) { segment { id } userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal("")),
        #("name", root_field.StringVal("x")),
      ]),
    )
  assert body
    == "{\"errors\":[{\"message\":\"Variable $id of type ID! was provided invalid value\",\"extensions\":{\"code\":\"INVALID_VARIABLE\",\"value\":\"\",\"problems\":[{\"path\":[],\"explanation\":\"Invalid global id ''\",\"message\":\"Invalid global id ''\"}]}}]}"
}

pub fn segment_update_wrong_gid_type_is_top_level_error_with_null_data_test() {
  let body =
    run_mutation_with_variables(
      store.new(),
      "mutation SegmentUpdateMalformedGid($id: ID!, $name: String!) { segmentUpdate(id: $id, name: $name) { segment { id } userErrors { field message } } }",
      dict.from_list([
        #("id", root_field.StringVal("gid://shopify/Order/1")),
        #("name", root_field.StringVal("x")),
      ]),
    )
  assert body
    == "{\"errors\":[{\"message\":\"invalid id\",\"path\":[\"segmentUpdate\"],\"extensions\":{\"code\":\"RESOURCE_NOT_FOUND\"}}],\"data\":{\"segmentUpdate\":null}}"
}

pub fn segment_update_without_changes_emits_user_error_test() {
  let existing =
    segment_record("gid://shopify/Segment/105", "Keep", "number_of_orders >= 2")
  let s = seed(store.new(), existing)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/105\") { segment { id } userErrors { __typename field code message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":null,\"userErrors\":[{\"__typename\":\"UserError\",\"field\":null,\"code\":null,\"message\":\"At least one attribute to change must be present\"}]}}}"
  assert outcome.staged_resource_ids == []
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

pub fn segment_update_overlong_name_emits_user_error_test() {
  let existing =
    segment_record("gid://shopify/Segment/103", "Keep", "number_of_orders >= 2")
  let s = seed(store.new(), existing)
  let long_name = string.repeat("N", times: 256)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/103\", name: \""
        <> long_name
        <> "\") { segment { id } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"name\"],\"message\":\"Name is too long (maximum is 255 characters)\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn segment_update_accepts_name_when_stripped_length_is_within_limit_test() {
  let existing =
    segment_record("gid://shopify/Segment/107", "Keep", "number_of_orders >= 2")
  let s = seed(store.new(), existing)
  let padded_name = string.repeat(" ", times: 256) <> "x"
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/107\", name: \""
        <> padded_name
        <> "\") { segment { id name query } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":{\"id\":\"gid://shopify/Segment/107\",\"name\":\"x\",\"query\":\"number_of_orders >= 2\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == ["gid://shopify/Segment/107"]
  let assert Some(record) =
    store.get_effective_segment_by_id(
      outcome.store,
      "gid://shopify/Segment/107",
    )
  assert record.name == Some("x")
}

pub fn segment_update_overlong_query_emits_length_error_before_grammar_test() {
  let existing =
    segment_record("gid://shopify/Segment/104", "Keep", "number_of_orders >= 2")
  let s = seed(store.new(), existing)
  let long_query = "number_of_orders >= 5 " <> string.repeat("x", times: 5000)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/104\", query: \""
        <> long_query
        <> "\") { segment { id } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"Query is too long (maximum is 5000 characters)\"}]}}}"
  assert outcome.staged_resource_ids == []
}

pub fn segment_update_rejects_query_when_raw_length_exceeds_limit_test() {
  let existing =
    segment_record("gid://shopify/Segment/108", "Keep", "number_of_orders >= 2")
  let s = seed(store.new(), existing)
  let raw_over_limit_query =
    string.repeat(" ", times: 4000)
    <> "number_of_orders = 0"
    <> string.repeat(" ", times: 1500)
  let outcome =
    run_mutation_outcome_with_variables(
      s,
      "mutation SegmentUpdateRawQueryLimit($query: String!) { segmentUpdate(id: \"gid://shopify/Segment/108\", query: $query) { segment { id } userErrors { field message } } }",
      dict.from_list([#("query", root_field.StringVal(raw_over_limit_query))]),
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"query\"],\"message\":\"Query is too long (maximum is 5000 characters)\"}]}}}"
  assert outcome.staged_resource_ids == []
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
      "mutation { segmentDelete(id: \"gid://shopify/Segment/missing\") { deletedSegmentId userErrors { __typename field code message } } }",
    )
  assert body
    == "{\"data\":{\"segmentDelete\":{\"deletedSegmentId\":null,\"userErrors\":[{\"__typename\":\"UserError\",\"field\":[\"id\"],\"code\":null,\"message\":\"Segment does not exist\"}]}}}"
}

pub fn segment_delete_malformed_gid_variable_is_top_level_error_test() {
  let body =
    run_mutation_with_variables(
      store.new(),
      "mutation SegmentDeleteMalformedGid($id: ID!) { segmentDelete(id: $id) { deletedSegmentId userErrors { field message } } }",
      dict.from_list([#("id", root_field.StringVal("not-a-gid"))]),
    )
  assert body
    == "{\"errors\":[{\"message\":\"Variable $id of type ID! was provided invalid value\",\"extensions\":{\"code\":\"INVALID_VARIABLE\",\"value\":\"not-a-gid\",\"problems\":[{\"path\":[],\"explanation\":\"Invalid global id 'not-a-gid'\",\"message\":\"Invalid global id 'not-a-gid'\"}]}}]}"
}

pub fn segment_delete_empty_gid_variable_is_top_level_error_test() {
  let body =
    run_mutation_with_variables(
      store.new(),
      "mutation SegmentDeleteMalformedGid($id: ID!) { segmentDelete(id: $id) { deletedSegmentId userErrors { field message } } }",
      dict.from_list([#("id", root_field.StringVal(""))]),
    )
  assert body
    == "{\"errors\":[{\"message\":\"Variable $id of type ID! was provided invalid value\",\"extensions\":{\"code\":\"INVALID_VARIABLE\",\"value\":\"\",\"problems\":[{\"path\":[],\"explanation\":\"Invalid global id ''\",\"message\":\"Invalid global id ''\"}]}}]}"
}

pub fn segment_delete_wrong_gid_type_is_top_level_error_with_null_data_test() {
  let body =
    run_mutation_with_variables(
      store.new(),
      "mutation SegmentDeleteMalformedGid($id: ID!) { segmentDelete(id: $id) { deletedSegmentId userErrors { field message } } }",
      dict.from_list([#("id", root_field.StringVal("gid://shopify/Order/1"))]),
    )
  assert body
    == "{\"errors\":[{\"message\":\"invalid id\",\"path\":[\"segmentDelete\"],\"extensions\":{\"code\":\"RESOURCE_NOT_FOUND\"}}],\"data\":{\"segmentDelete\":null}}"
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

pub fn segment_create_bumps_requested_trailing_numeric_suffix_test() {
  let existing =
    segment_record(
      "gid://shopify/Segment/303",
      "Foo (5)",
      "number_of_orders >= 5",
    )
  let s = seed(store.new(), existing)
  let body =
    run_mutation(
      s,
      "mutation { segmentCreate(name: \"Foo (5)\", query: \"number_of_orders >= 10\") { segment { name } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":{\"name\":\"Foo (6)\"},\"userErrors\":[]}}}"
}

pub fn segment_create_duplicate_retry_cap_returns_taken_test() {
  let s = seed_duplicate_segment_names(store.new(), "Bar", 1, 11)
  let document =
    "mutation { segmentCreate(name: \"Bar\", query: \"number_of_orders >= 10\") { segment { name } userErrors { field message } } }"
  let outcome = run_mutation_outcome(s, document)
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentCreate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"name\"],\"message\":\"Name has already been taken\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert list.length(store.list_effective_segments(outcome.store)) == 11

  let #(status, proxy_body, proxy_after) = run_proxy_mutation(s, document)
  assert status == 200
  assert proxy_body == body
  let assert [entry] = store.get_log(proxy_after.store)
  assert entry.query == document
  assert entry.status == store_types.Failed
  assert entry.staged_resource_ids == []
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

pub fn segment_update_bumps_requested_trailing_numeric_suffix_test() {
  let current =
    segment_record(
      "gid://shopify/Segment/401",
      "Current",
      "number_of_orders >= 1",
    )
  let colliding =
    segment_record(
      "gid://shopify/Segment/402",
      "Foo (5)",
      "number_of_orders >= 1",
    )
  let s =
    store.new()
    |> seed(current)
    |> seed(colliding)
  let body =
    run_mutation(
      s,
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/401\", name: \"Foo (5)\") { segment { name } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":{\"name\":\"Foo (6)\"},\"userErrors\":[]}}}"
}

pub fn segment_update_duplicate_retry_cap_returns_taken_test() {
  let current =
    segment_record(
      "gid://shopify/Segment/403",
      "Original",
      "number_of_orders >= 1",
    )
  let s =
    store.new()
    |> seed(current)
    |> seed_duplicate_segment_names("Bar", 1, 11)
  let outcome =
    run_mutation_outcome(
      s,
      "mutation { segmentUpdate(id: \"gid://shopify/Segment/403\", name: \"Bar\") { segment { name } userErrors { field message } } }",
    )
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"segmentUpdate\":{\"segment\":null,\"userErrors\":[{\"field\":[\"name\"],\"message\":\"Name has already been taken\"}]}}}"
  assert outcome.staged_resource_ids == []
  let assert Some(still_current) =
    store.get_effective_segment_by_id(
      outcome.store,
      "gid://shopify/Segment/403",
    )
  assert still_current.name == Some("Original")
}

// ----------- predicate -----------

pub fn is_segment_mutation_root_predicate_test() {
  assert segments.is_segment_mutation_root("segmentCreate")
  assert !segments.is_segment_mutation_root("segment")
}
