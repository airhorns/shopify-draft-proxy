import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/operation_registry.{
  type RegistryEntry, AdminPlatform, Mutation, OverlayRead, Products, Query,
  RegistryEntry, StageLocally,
}
import shopify_draft_proxy/proxy/operation_registry_data

const sample_json = "[
  {\"name\":\"product\",\"type\":\"query\",\"domain\":\"products\",\"execution\":\"overlay-read\",\"implemented\":true,\"matchNames\":[\"product\",\"Product\"],\"runtimeTests\":[\"a.test.ts\"]},
  {\"name\":\"productCreate\",\"type\":\"mutation\",\"domain\":\"products\",\"execution\":\"stage-locally\",\"implemented\":true,\"matchNames\":[\"productCreate\"],\"runtimeTests\":[\"b.test.ts\"],\"supportNotes\":\"creates a draft\"},
  {\"name\":\"shop\",\"type\":\"query\",\"domain\":\"admin-platform\",\"execution\":\"overlay-read\",\"implemented\":false,\"matchNames\":[\"shop\"],\"runtimeTests\":[\"c.test.ts\"]}
]"

pub fn parse_round_trips_three_entries_test() {
  let assert Ok(entries) = operation_registry.parse(sample_json)
  case entries {
    [
      RegistryEntry(
        name: "product",
        type_: Query,
        domain: Products,
        execution: OverlayRead,
        implemented: True,
        match_names: ["product", "Product"],
        runtime_tests: ["a.test.ts"],
        support_notes: None,
      ),
      RegistryEntry(
        name: "productCreate",
        type_: Mutation,
        domain: Products,
        execution: StageLocally,
        implemented: True,
        match_names: ["productCreate"],
        runtime_tests: ["b.test.ts"],
        support_notes: Some("creates a draft"),
      ),
      RegistryEntry(
        name: "shop",
        type_: Query,
        domain: AdminPlatform,
        implemented: False,
        ..,
      ),
    ] -> Nil
    _ -> panic as "registry parse shape mismatch"
  }
}

pub fn list_implemented_drops_unimplemented_test() {
  let assert Ok(entries) = operation_registry.parse(sample_json)
  let implemented = operation_registry.list_implemented(entries)
  // shop has implemented:false and should be dropped
  let names =
    implemented
    |> list_map_to_name()
  assert names == ["product", "productCreate"]
}

pub fn find_entry_matches_first_candidate_test() {
  let assert Ok(entries) = operation_registry.parse(sample_json)
  let assert Some(entry) =
    operation_registry.find_entry(entries, Query, [Some("product")])
  assert entry.name == "product"
}

pub fn find_entry_matches_via_match_names_alt_casing_test() {
  // matchNames includes "Product" (uppercase, the operation name form)
  let assert Ok(entries) = operation_registry.parse(sample_json)
  let assert Some(entry) =
    operation_registry.find_entry(entries, Query, [Some("Product")])
  assert entry.name == "product"
}

pub fn find_entry_returns_none_when_type_mismatches_test() {
  let assert Ok(entries) = operation_registry.parse(sample_json)
  // "product" is a query, not a mutation, so a mutation lookup should miss.
  assert operation_registry.find_entry(entries, Mutation, [Some("product")])
    == None
}

pub fn find_entry_skips_empty_and_none_candidates_test() {
  let assert Ok(entries) = operation_registry.parse(sample_json)
  let assert Some(entry) =
    operation_registry.find_entry(entries, Mutation, [
      None,
      Some(""),
      Some("productCreate"),
    ])
  assert entry.name == "productCreate"
}

pub fn find_entry_returns_none_when_no_candidates_match_test() {
  let assert Ok(entries) = operation_registry.parse(sample_json)
  assert operation_registry.find_entry(entries, Query, [Some("__missing__")])
    == None
}

pub fn parse_rejects_unknown_domain_test() {
  let bad =
    "[{\"name\":\"x\",\"type\":\"query\",\"domain\":\"nope\",\"execution\":\"overlay-read\",\"implemented\":true,\"matchNames\":[\"x\"],\"runtimeTests\":[\"y\"]}]"
  let assert Error(_) = operation_registry.parse(bad)
}

pub fn parse_rejects_unknown_execution_test() {
  let bad =
    "[{\"name\":\"x\",\"type\":\"query\",\"domain\":\"products\",\"execution\":\"laser-mode\",\"implemented\":true,\"matchNames\":[\"x\"],\"runtimeTests\":[\"y\"]}]"
  let assert Error(_) = operation_registry.parse(bad)
}

pub fn parse_rejects_missing_required_field_test() {
  // missing matchNames
  let bad =
    "[{\"name\":\"x\",\"type\":\"query\",\"domain\":\"products\",\"execution\":\"overlay-read\",\"implemented\":true,\"runtimeTests\":[\"y\"]}]"
  let assert Error(_) = operation_registry.parse(bad)
}

fn list_map_to_name(entries: List(RegistryEntry)) -> List(String) {
  case entries {
    [] -> []
    [e, ..rest] -> [e.name, ..list_map_to_name(rest)]
  }
}

// --- Codegen-backed default_registry / with_default_registry coverage. ---

pub fn default_registry_has_many_entries_test() {
  // The TS-side JSON has hundreds of entries; this is a sanity floor so
  // a future codegen regression that produces a near-empty list fails
  // loudly. Exact count drifts as the TS side adds operations; assert a
  // generous lower bound only.
  let entries = operation_registry_data.default_registry()
  assert list.length(entries) >= 60
}

pub fn default_registry_includes_known_query_test() {
  let entries = operation_registry_data.default_registry()
  let assert Some(entry) =
    operation_registry.find_entry(entries, Query, [Some("product")])
  assert entry.domain == Products
  assert entry.execution == OverlayRead
}

pub fn default_registry_includes_known_mutation_test() {
  let entries = operation_registry_data.default_registry()
  let assert Some(entry) =
    operation_registry.find_entry(entries, Mutation, [Some("savedSearchCreate")])
  assert entry.implemented == True
}

pub fn default_registry_implemented_subset_is_nonempty_test() {
  let entries = operation_registry_data.default_registry()
  let implemented = operation_registry.list_implemented(entries)
  assert list.length(implemented) >= 1
}

pub fn with_default_registry_attaches_registry_test() {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry()
  assert list.length(proxy.registry) >= 60
}

pub fn new_proxy_has_empty_registry_test() {
  // `new()` must not implicitly attach the default registry — Pass 1–7
  // tests rely on the empty-registry fallback path.
  let proxy = draft_proxy.new()
  assert proxy.registry == []
}
