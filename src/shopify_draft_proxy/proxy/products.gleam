//// Public entrypoint for products, collections, inventory, publications, and selling plans.
////
//// Implementation lives under proxy/products/* so this file can preserve the
//// original public API surface without keeping the whole domain in one module.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import shopify_draft_proxy/graphql/ast.{type Selection}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue,
}
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/products/collections_core as collections
import shopify_draft_proxy/proxy/products/hydration
import shopify_draft_proxy/proxy/products/mutations
import shopify_draft_proxy/proxy/products/product_types
import shopify_draft_proxy/proxy/products/products_handlers as product_serializers
import shopify_draft_proxy/proxy/products/products_records as product_sources
import shopify_draft_proxy/proxy/products/queries
import shopify_draft_proxy/proxy/products/selling_plans_core as selling_plans
import shopify_draft_proxy/proxy/products/variants_options as options
import shopify_draft_proxy/proxy/products/variants_options_core as option_values
import shopify_draft_proxy/proxy/products/variants_sources as variants
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type ProductRecord, type ProductVariantRecord,
}

pub type ProductsError =
  product_types.ProductsError

pub const product_user_error_code_blank = "BLANK"

pub const product_user_error_code_invalid = "INVALID"

pub const product_user_error_code_taken = "TAKEN"

pub const product_user_error_code_greater_than = "GREATER_THAN"

pub const product_user_error_code_less_than = "LESS_THAN"

pub const product_user_error_code_inclusion = "INCLUSION"

pub const product_user_error_code_not_a_number = "NOT_A_NUMBER"

pub const product_user_error_code_product_does_not_exist = "PRODUCT_DOES_NOT_EXIST"

pub const product_user_error_code_product_not_found = "PRODUCT_NOT_FOUND"

pub const product_user_error_code_product_variant_does_not_exist = "PRODUCT_VARIANT_DOES_NOT_EXIST"

pub const product_user_error_code_invalid_inventory_item = "INVALID_INVENTORY_ITEM"

pub const product_user_error_code_invalid_location = "INVALID_LOCATION"

pub const product_user_error_code_invalid_name = "INVALID_NAME"

pub const product_user_error_code_invalid_quantity_negative = "INVALID_QUANTITY_NEGATIVE"

pub const product_user_error_code_invalid_quantity_too_high = "INVALID_QUANTITY_TOO_HIGH"

pub const product_user_error_code_invalid_quantity_too_low = "INVALID_QUANTITY_TOO_LOW"

pub fn is_products_query_root(name: String) -> Bool {
  queries.is_products_query_root(name)
}

pub fn is_products_mutation_root(name: String) -> Bool {
  mutations.is_products_mutation_root(name)
}

pub fn local_has_product_id(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  queries.local_has_product_id(proxy, document, variables)
}

pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  queries.handle_query_request(
    proxy,
    request,
    parsed,
    primary_root_field,
    document,
    variables,
  )
}

pub fn handle_products_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, ProductsError) {
  queries.handle_products_query(store, document, variables)
}

pub fn serialize_product_node_by_id(
  store: Store,
  id: String,
  selection: List(Selection),
  fragments: FragmentMap,
) -> Json {
  product_serializers.serialize_product_node_by_id(
    store,
    id,
    selection,
    fragments,
  )
}

pub fn serialize_collection_node_by_id(
  store: Store,
  id: String,
  selection: List(Selection),
  fragments: FragmentMap,
) -> Json {
  collections.serialize_collection_node_by_id(store, id, selection, fragments)
}

pub fn serialize_product_option_node_by_id(
  store: Store,
  id: String,
  selection: List(Selection),
  fragments: FragmentMap,
) -> Json {
  options.serialize_product_option_node_by_id(store, id, selection, fragments)
}

pub fn serialize_product_option_value_node_by_id(
  store: Store,
  id: String,
  selection: List(Selection),
  fragments: FragmentMap,
) -> Json {
  option_values.serialize_product_option_value_node_by_id(
    store,
    id,
    selection,
    fragments,
  )
}

pub fn serialize_product_operation_node_by_id(
  store: Store,
  id: String,
  selection: List(Selection),
  fragments: FragmentMap,
) -> Json {
  product_serializers.serialize_product_operation_node_by_id(
    store,
    id,
    selection,
    fragments,
  )
}

pub fn serialize_selling_plan_node_by_id(
  store: Store,
  id: String,
  selection: List(Selection),
  fragments: FragmentMap,
) -> Json {
  selling_plans.serialize_selling_plan_node_by_id(
    store,
    id,
    selection,
    fragments,
  )
}

pub fn product_source(product: ProductRecord) -> SourceValue {
  product_sources.product_source(product)
}

pub fn product_variant_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  variants.product_variant_source(store, variant)
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  mutations.process_mutation(
    store,
    identity,
    request_path,
    document,
    variables,
    upstream,
  )
}

pub fn hydrate_products_for_live_hybrid_mutation(
  store: Store,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  hydration.hydrate_products_for_live_hybrid_mutation(
    store,
    variables,
    upstream,
    [],
  )
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, ProductsError) {
  queries.process(store, document, variables)
}
