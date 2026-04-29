import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/operation_registry.{
  type RegistryEntry, AdminPlatform, Mutation, OverlayRead, Products, Query,
  RegistryEntry, StageLocally,
}

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
