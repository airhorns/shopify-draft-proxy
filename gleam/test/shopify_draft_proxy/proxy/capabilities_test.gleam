import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/parse_operation.{
  MutationOperation, ParsedOperation, QueryOperation,
}
import shopify_draft_proxy/proxy/capabilities
import shopify_draft_proxy/proxy/operation_registry.{
  type RegistryEntry, Mutation, OverlayRead, Passthrough, Products, Query,
  RegistryEntry, StageLocally, Unknown,
}

fn registry() -> List(RegistryEntry) {
  [
    RegistryEntry(
      name: "product",
      type_: Query,
      domain: Products,
      execution: OverlayRead,
      implemented: True,
      match_names: ["product", "Product"],
      runtime_tests: [],
      support_notes: None,
    ),
    RegistryEntry(
      name: "productCreate",
      type_: Mutation,
      domain: Products,
      execution: StageLocally,
      implemented: True,
      match_names: ["productCreate"],
      runtime_tests: [],
      support_notes: None,
    ),
    // Implemented entry with name === operation-name path so we can
    // exercise the "prefer op.name when both resolve to same entry" rule
    RegistryEntry(
      name: "products",
      type_: Query,
      domain: Products,
      execution: OverlayRead,
      implemented: True,
      match_names: ["products", "Products"],
      runtime_tests: [],
      support_notes: None,
    ),
    // Unimplemented entry: should not be selectable for capability lookup
    RegistryEntry(
      name: "shop",
      type_: Query,
      domain: Products,
      execution: OverlayRead,
      implemented: False,
      match_names: ["shop"],
      runtime_tests: [],
      support_notes: None,
    ),
  ]
}

pub fn matches_root_field_for_query_test() {
  let op =
    ParsedOperation(
      type_: QueryOperation,
      name: None,
      root_fields: ["product"],
    )
  let cap = capabilities.get_operation_capability(op, registry())
  assert cap.domain == Products
  assert cap.execution == OverlayRead
  assert cap.operation_name == Some("product")
}

pub fn matches_root_field_for_mutation_test() {
  let op =
    ParsedOperation(
      type_: MutationOperation,
      name: Some("CreateThing"),
      root_fields: ["productCreate"],
    )
  let cap = capabilities.get_operation_capability(op, registry())
  assert cap.domain == Products
  assert cap.execution == StageLocally
  // CreateThing is not in any matchNames, so the matched candidate
  // (productCreate) wins.
  assert cap.operation_name == Some("productCreate")
}

pub fn prefers_root_field_over_operation_name_test() {
  // Both root field and operation name match, but a root-field hit
  // resolves first.
  let op =
    ParsedOperation(
      type_: QueryOperation,
      name: Some("Product"),
      root_fields: ["product"],
    )
  let cap = capabilities.get_operation_capability(op, registry())
  // Both root_fields[0]="product" and name="Product" resolve to the
  // same registry entry ("product"), so the operation's declared name
  // wins ("Product"), per the TS rule.
  assert cap.operation_name == Some("Product")
  assert cap.domain == Products
}

pub fn falls_back_to_operation_name_when_root_field_misses_test() {
  // No root field matches, but the operation name "products" does.
  let op =
    ParsedOperation(
      type_: QueryOperation,
      name: Some("products"),
      root_fields: ["__nope__"],
    )
  let cap = capabilities.get_operation_capability(op, registry())
  assert cap.operation_name == Some("products")
  assert cap.domain == Products
}

pub fn ignores_unimplemented_entries_test() {
  // "shop" is unimplemented in the registry — should fall back to
  // unknown/passthrough.
  let op =
    ParsedOperation(
      type_: QueryOperation,
      name: None,
      root_fields: ["shop"],
    )
  let cap = capabilities.get_operation_capability(op, registry())
  assert cap.domain == Unknown
  assert cap.execution == Passthrough
  assert cap.operation_name == Some("shop")
}

pub fn unknown_root_field_with_operation_name_falls_back_to_name_test() {
  let op =
    ParsedOperation(
      type_: QueryOperation,
      name: Some("Mystery"),
      root_fields: ["__missing__"],
    )
  let cap = capabilities.get_operation_capability(op, registry())
  assert cap.domain == Unknown
  assert cap.execution == Passthrough
  // Fallback prefers operation.name over first root field.
  assert cap.operation_name == Some("Mystery")
}

pub fn unknown_with_no_operation_name_uses_first_root_field_test() {
  let op =
    ParsedOperation(
      type_: QueryOperation,
      name: None,
      root_fields: ["__missing__"],
    )
  let cap = capabilities.get_operation_capability(op, registry())
  assert cap.operation_name == Some("__missing__")
}

pub fn unknown_with_no_operation_name_and_no_root_fields_yields_none_test() {
  let op =
    ParsedOperation(type_: QueryOperation, name: None, root_fields: [])
  let cap = capabilities.get_operation_capability(op, registry())
  assert cap.operation_name == None
}

pub fn type_mismatched_match_falls_back_test() {
  // "product" is a query in the registry; running it as a mutation
  // should miss and fall back.
  let op =
    ParsedOperation(
      type_: MutationOperation,
      name: None,
      root_fields: ["product"],
    )
  let cap = capabilities.get_operation_capability(op, registry())
  assert cap.domain == Unknown
  assert cap.execution == Passthrough
}
