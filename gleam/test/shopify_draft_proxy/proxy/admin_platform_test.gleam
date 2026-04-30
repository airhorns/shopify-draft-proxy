import gleam/dict
import gleam/json
import gleam/list
import gleam/string
import shopify_draft_proxy/proxy/admin_platform
import shopify_draft_proxy/proxy/draft_proxy.{Request, Response}
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity

fn empty_vars() {
  dict.new()
}

/// Apply the dispatcher-level `record_log_drafts` to the outcome.
/// Tests that exercise `admin_platform.process_mutation` directly (no
/// `draft_proxy` round-trip) need this so log-buffer assertions still
/// see the drafts the module emitted; centralized recording is the
/// dispatcher's responsibility post-refactor.
fn record_drafts(
  outcome: admin_platform.MutationOutcome,
  request_path: String,
  document: String,
) -> admin_platform.MutationOutcome {
  let #(logged_store, logged_identity) =
    mutation_helpers.record_log_drafts(
      outcome.store,
      outcome.identity,
      request_path,
      document,
      outcome.log_drafts,
    )
  admin_platform.MutationOutcome(
    ..outcome,
    store: logged_store,
    identity: logged_identity,
  )
}

fn run_query(source: store.Store, query: String) -> String {
  let assert Ok(body) = admin_platform.process(source, query, empty_vars())
  json.to_string(body)
}

pub fn root_predicates_test() {
  assert admin_platform.is_admin_platform_query_root("publicApiVersions")
  assert admin_platform.is_admin_platform_query_root("node")
  assert admin_platform.is_admin_platform_query_root("nodes")
  assert admin_platform.is_admin_platform_query_root("job")
  assert admin_platform.is_admin_platform_query_root("domain")
  assert admin_platform.is_admin_platform_query_root("backupRegion")
  assert admin_platform.is_admin_platform_query_root("taxonomy")
  assert admin_platform.is_admin_platform_query_root("staffMember")
  assert admin_platform.is_admin_platform_query_root("staffMembers")
  assert admin_platform.is_admin_platform_mutation_root("flowGenerateSignature")
  assert admin_platform.is_admin_platform_mutation_root("flowTriggerReceive")
  assert admin_platform.is_admin_platform_mutation_root("backupRegionUpdate")
  assert !admin_platform.is_admin_platform_query_root("products")
}

pub fn utility_reads_return_local_no_data_shapes_test() {
  let body =
    run_query(
      store.new(),
      "query { publicApiVersions { handle displayName supported } node(id: \"gid://shopify/Product/0\") { id } nodes(ids: [\"gid://shopify/Product/0\", \"gid://shopify/Customer/0\"]) { id } job(id: \"gid://shopify/Job/0\") { __typename id done query { __typename } } domain(id: \"gid://shopify/Domain/0\") { id } backupRegion { __typename id name code } taxonomy { categories(first: 1) { nodes { id } edges { node { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }",
    )

  assert string.contains(
    body,
    "\"publicApiVersions\":[{\"handle\":\"2025-07\",\"displayName\":\"2025-07\",\"supported\":true}",
  )
  assert string.contains(body, "\"node\":null")
  assert string.contains(body, "\"nodes\":[null,null]")
  assert string.contains(
    body,
    "\"job\":{\"__typename\":\"Job\",\"id\":\"gid://shopify/Job/0\",\"done\":true,\"query\":{\"__typename\":\"QueryRoot\"}}",
  )
  assert string.contains(body, "\"domain\":null")
  assert string.contains(
    body,
    "\"backupRegion\":{\"__typename\":\"MarketRegionCountry\",\"id\":\"gid://shopify/MarketRegionCountry/4062110417202\",\"name\":\"Canada\",\"code\":\"CA\"}",
  )
  assert string.contains(
    body,
    "\"categories\":{\"nodes\":[],\"edges\":[],\"pageInfo\":{\"hasNextPage\":false,\"hasPreviousPage\":false,\"startCursor\":null,\"endCursor\":null}}",
  )
}

pub fn staff_roots_return_access_denied_errors_test() {
  let body =
    run_query(
      store.new(),
      "query { staffMember(id: \"gid://shopify/StaffMember/1\") { id } staffMembers(first: 1) { nodes { id } } }",
    )

  assert string.contains(body, "\"staffMember\":null")
  assert string.contains(body, "\"staffMembers\":null")
  assert string.contains(body, "Access denied for staffMember field.")
  assert string.contains(body, "Access denied for staffMembers field.")
  assert string.contains(body, "\"code\":\"ACCESS_DENIED\"")
}

pub fn backup_region_update_stages_and_reads_back_test() {
  let source = store.new()
  let identity = synthetic_identity.new()
  let request_path = "/admin/api/2026-04/graphql.json"
  let document =
    "mutation { backupRegionUpdate(region: { countryCode: CA }) { backupRegion { id name code } userErrors { field message code } } }"
  let assert Ok(outcome) =
    admin_platform.process_mutation(
      source,
      identity,
      request_path,
      document,
      empty_vars(),
    )
  let outcome = record_drafts(outcome, request_path, document)

  let mutation_body = json.to_string(outcome.data)
  assert string.contains(
    mutation_body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/4062110417202\",\"name\":\"Canada\",\"code\":\"CA\"}",
  )
  assert string.contains(mutation_body, "\"userErrors\":[]")

  let read_body = run_query(outcome.store, "{ backupRegion { id name code } }")
  assert string.contains(
    read_body,
    "\"backupRegion\":{\"id\":\"gid://shopify/MarketRegionCountry/4062110417202\",\"name\":\"Canada\",\"code\":\"CA\"}",
  )
  assert list.length(store.get_log(outcome.store)) == 1
  let assert [entry] = store.get_log(outcome.store)
  assert entry.staged_resource_ids
    == ["gid://shopify/MarketRegionCountry/4062110417202"]
}

pub fn backup_region_update_validation_does_not_log_test() {
  let assert Ok(outcome) =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { backupRegionUpdate(region: { countryCode: ZZ }) { backupRegion { id } userErrors { field message code } } }",
      empty_vars(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(body, "\"backupRegion\":null")
  assert string.contains(body, "\"message\":\"Region not found.\"")
  assert string.contains(body, "\"code\":\"REGION_NOT_FOUND\"")
  assert store.get_log(outcome.store) == []
}

pub fn flow_utility_mutations_stage_without_sensitive_state_test() {
  let request_path = "/admin/api/2026-04/graphql.json"
  let document =
    "mutation { sig: flowGenerateSignature(id: \"gid://shopify/FlowTrigger/374\", payload: \"{\\\"id\\\":1}\") { payload signature userErrors { field message } } receive: flowTriggerReceive(handle: \"local-order-created\", payload: \"{\\\"id\\\":1}\") { userErrors { field message } } }"
  let assert Ok(outcome) =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      request_path,
      document,
      empty_vars(),
    )
  let outcome = record_drafts(outcome, request_path, document)

  let body = json.to_string(outcome.data)
  assert string.contains(
    body,
    "\"sig\":{\"payload\":\"{\\\"id\\\":1}\",\"signature\":\"",
  )
  assert string.contains(body, "\"userErrors\":[]")
  assert list.length(outcome.staged_resource_ids) == 2
  assert list.length(store.get_log(outcome.store)) == 1
  let staged = outcome.store.staged_state
  assert list.length(staged.admin_platform_flow_signature_order) == 1
  assert list.length(staged.admin_platform_flow_trigger_order) == 1
  assert !string.contains(
    json.to_string(outcome.data),
    "shopify-draft-proxy-flow-signature-local-secret-v1",
  )
}

pub fn flow_validation_branches_do_not_stage_test() {
  let assert Ok(outcome) =
    admin_platform.process_mutation(
      store.new(),
      synthetic_identity.new(),
      "/admin/api/2026-04/graphql.json",
      "mutation { badSig: flowGenerateSignature(id: \"gid://shopify/FlowTrigger/0\", payload: \"{}\") { signature userErrors { field message } } badReceive: flowTriggerReceive(handle: \"remote-handle\", payload: \"{}\") { userErrors { field message } } }",
      empty_vars(),
    )

  let body = json.to_string(outcome.data)
  assert string.contains(body, "\"badSig\":null")
  assert string.contains(body, "\"Invalid id: gid://shopify/FlowTrigger/0\"")
  assert string.contains(body, "Invalid handle 'remote-handle'.")
  assert outcome.staged_resource_ids == []
  assert store.get_log(outcome.store) == []
}

pub fn draft_proxy_routes_admin_platform_reads_and_mutations_test() {
  let proxy = draft_proxy.new()
  let read_request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"{ publicApiVersions { handle supported } backupRegion { code } }\"}",
    )
  let #(Response(status: read_status, body: read_body, ..), proxy) =
    draft_proxy.process_request(proxy, read_request)
  assert read_status == 200
  assert string.contains(
    json.to_string(read_body),
    "\"backupRegion\":{\"code\":\"CA\"}",
  )

  let mutation_request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"mutation { backupRegionUpdate(region: { countryCode: CA }) { backupRegion { code } userErrors { message } } }\"}",
    )
  let #(Response(status: mutation_status, body: mutation_body, ..), proxy) =
    draft_proxy.process_request(proxy, mutation_request)
  assert mutation_status == 200
  assert string.contains(
    json.to_string(mutation_body),
    "\"backupRegionUpdate\":{\"backupRegion\":{\"code\":\"CA\"},\"userErrors\":[]}",
  )
  assert list.length(store.get_log(proxy.store)) == 1
}
