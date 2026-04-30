import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/parse_operation.{
  type ParsedOperation, MutationOperation, ParsedOperation, QueryOperation,
}
import shopify_draft_proxy/proxy/capabilities
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/operation_registry.{
  type OperationType, type RegistryEntry, AdminPlatform, Mutation, OverlayRead,
  Passthrough, Products, Query, RegistryEntry, StageLocally, Unknown,
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

pub fn default_registry_classifies_every_implemented_match_name_test() {
  let entries = operation_registry_data.default_registry()
  assert_implemented_entries_classify(entries)
}

pub fn default_registry_unimplemented_match_names_stay_unsupported_test() {
  let entries = operation_registry_data.default_registry()
  assert_unimplemented_entries_do_not_classify(entries, entries)
}

pub fn default_registry_marks_only_ported_roots_as_locally_dispatched_test() {
  let entries = operation_registry_data.default_registry()

  let assert Some(saved_search_create) =
    operation_registry.find_entry(entries, Mutation, [Some("savedSearchCreate")])
  assert draft_proxy.registry_entry_has_local_dispatch(saved_search_create)

  let assert Some(shop) =
    operation_registry.find_entry(entries, Query, [Some("shop")])
  assert draft_proxy.registry_entry_has_local_dispatch(shop)

  let assert Some(product) =
    operation_registry.find_entry(entries, Query, [Some("product")])
  assert !draft_proxy.registry_entry_has_local_dispatch(product)

  let assert Some(product_create) =
    operation_registry.find_entry(entries, Mutation, [Some("productCreate")])
  assert !draft_proxy.registry_entry_has_local_dispatch(product_create)

  assert_no_unimplemented_entries_are_local(entries)
}

pub fn default_registry_unimplemented_root_blocks_legacy_fallback_test() {
  let proxy = draft_proxy.new() |> draft_proxy.with_default_registry()
  let request =
    draft_proxy.Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"{ app(id: \\\"gid://shopify/App/1\\\") { id } }\"}",
    )

  let #(draft_proxy.Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, request)

  assert status == 400
  assert json.to_string(body)
    == "{\"errors\":[{\"message\":\"No domain dispatcher implemented for root field: app\"}]}"
}

fn assert_implemented_entries_classify(entries: List(RegistryEntry)) -> Nil {
  case entries {
    [] -> Nil
    [entry, ..rest] -> {
      case entry.implemented {
        True -> assert_match_names_classify(entry.match_names, entry)
        False -> Nil
      }
      assert_implemented_entries_classify(rest)
    }
  }
}

fn assert_match_names_classify(
  names: List(String),
  entry: RegistryEntry,
) -> Nil {
  case names {
    [] -> Nil
    [name, ..rest] -> {
      let cap =
        capabilities.get_operation_capability(
          parsed_operation_for(entry.type_, name),
          operation_registry_data.default_registry(),
        )
      assert cap.domain == entry.domain
      assert cap.execution == entry.execution
      assert cap.operation_name == Some(name)
      assert_match_names_classify(rest, entry)
    }
  }
}

fn assert_unimplemented_entries_do_not_classify(
  entries: List(RegistryEntry),
  all_entries: List(RegistryEntry),
) -> Nil {
  case entries {
    [] -> Nil
    [entry, ..rest] -> {
      case entry.implemented {
        True -> Nil
        False ->
          assert_unimplemented_match_names(
            entry.match_names,
            entry.type_,
            all_entries,
          )
      }
      assert_unimplemented_entries_do_not_classify(rest, all_entries)
    }
  }
}

fn assert_unimplemented_match_names(
  names: List(String),
  type_: OperationType,
  all_entries: List(RegistryEntry),
) -> Nil {
  case names {
    [] -> Nil
    [name, ..rest] -> {
      case implemented_match_exists(all_entries, type_, name) {
        True -> Nil
        False -> {
          let cap =
            capabilities.get_operation_capability(
              parsed_operation_for(type_, name),
              all_entries,
            )
          assert cap.domain == Unknown
          assert cap.execution == Passthrough
          assert cap.operation_name == Some(name)
        }
      }
      assert_unimplemented_match_names(rest, type_, all_entries)
    }
  }
}

fn implemented_match_exists(
  entries: List(RegistryEntry),
  type_: OperationType,
  name: String,
) -> Bool {
  case entries {
    [] -> False
    [entry, ..rest] ->
      case
        entry.implemented
        && entry.type_ == type_
        && list.contains(entry.match_names, name)
      {
        True -> True
        False -> implemented_match_exists(rest, type_, name)
      }
  }
}

fn assert_no_unimplemented_entries_are_local(
  entries: List(RegistryEntry),
) -> Nil {
  case entries {
    [] -> Nil
    [entry, ..rest] -> {
      case entry.implemented {
        True -> Nil
        False -> {
          assert !draft_proxy.registry_entry_has_local_dispatch(entry)
        }
      }
      assert_no_unimplemented_entries_are_local(rest)
    }
  }
}

fn parsed_operation_for(type_: OperationType, root: String) -> ParsedOperation {
  let parsed_type = case type_ {
    Query -> QueryOperation
    Mutation -> MutationOperation
  }
  ParsedOperation(type_: parsed_type, name: None, root_fields: [root])
}
