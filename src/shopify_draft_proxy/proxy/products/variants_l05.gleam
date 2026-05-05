//// Internal products-domain implementation split from proxy/products.gleam.

import gleam/bit_array
import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Definition, type Location, type ObjectField, type Selection,
  type VariableDefinition, Argument, Directive, Field, InlineFragment, NullValue,
  ObjectField, ObjectValue, OperationDefinition, SelectionSet, StringValue,
  VariableDefinition, VariableValue,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/parser
import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, type RootFieldError, BoolVal, FloatVal, IntVal, ListVal,
  NullVal, ObjectVal, StringVal, get_field_arguments, get_root_fields,
}
import shopify_draft_proxy/graphql/source as graphql_source
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  paginate_connection_items, project_graphql_field_value, project_graphql_value,
  serialize_connection, serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, RequiredArgument,
  build_null_argument_error, find_argument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/products/inventory_l00.{
  clone_default_inventory_item,
}
import shopify_draft_proxy/proxy/products/inventory_l02.{
  read_variant_inventory_quantity, variant_weight_problems,
}
import shopify_draft_proxy/proxy/products/inventory_l03.{
  read_variant_inventory_item,
}
import shopify_draft_proxy/proxy/products/inventory_l04.{
  variant_quantity_problems,
}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_bool_field, read_object_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  read_non_empty_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type BulkVariantUserError, type ProductUserError,
  type VariantValidationProblem, BulkVariantUserError, ProductUserError,
  VariantValidationProblem,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  bulk_variant_option_field_name, has_variant_option_input,
}
import shopify_draft_proxy/proxy/products/variants_l01.{
  read_variant_sku, validate_bulk_variant_required_options,
  validate_bulk_variant_selected_options, variant_title_with_fallback,
}
import shopify_draft_proxy/proxy/products/variants_l02.{
  duplicate_option_input_name_errors, variant_compare_at_price_problems,
  variant_price_problems, variant_text_length_problems,
}
import shopify_draft_proxy/proxy/products/variants_l03.{
  variant_option_value_length_problems,
}
import shopify_draft_proxy/proxy/products/variants_l04.{
  create_single_option_input_errors, read_variant_selected_options,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type ChannelRecord, type CollectionImageRecord,
  type CollectionRecord, type CollectionRuleRecord, type CollectionRuleSetRecord,
  type InventoryItemRecord, type InventoryLevelRecord,
  type InventoryLocationRecord, type InventoryMeasurementRecord,
  type InventoryQuantityRecord, type InventoryShipmentLineItemRecord,
  type InventoryShipmentRecord, type InventoryShipmentTrackingRecord,
  type InventoryTransferLineItemRecord,
  type InventoryTransferLocationSnapshotRecord, type InventoryTransferRecord,
  type InventoryWeightRecord, type InventoryWeightValue, type LocationRecord,
  type ProductCategoryRecord, type ProductCollectionRecord,
  type ProductFeedRecord, type ProductMediaRecord, type ProductMetafieldRecord,
  type ProductOperationRecord, type ProductOperationUserErrorRecord,
  type ProductOptionRecord, type ProductOptionValueRecord, type ProductRecord,
  type ProductResourceFeedbackRecord, type ProductSeoRecord,
  type ProductVariantRecord, type ProductVariantSelectedOptionRecord,
  type PublicationRecord, type SellingPlanGroupRecord, type SellingPlanRecord,
  type ShopResourceFeedbackRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString, CollectionRecord,
  CollectionRuleRecord, CollectionRuleSetRecord, InventoryItemRecord,
  InventoryLevelRecord, InventoryLocationRecord, InventoryMeasurementRecord,
  InventoryQuantityRecord, InventoryShipmentLineItemRecord,
  InventoryShipmentRecord, InventoryShipmentTrackingRecord,
  InventoryTransferLineItemRecord, InventoryTransferLocationSnapshotRecord,
  InventoryTransferRecord, InventoryWeightFloat, InventoryWeightInt,
  InventoryWeightRecord, LocationRecord, ProductCollectionRecord,
  ProductFeedRecord, ProductMediaRecord, ProductMetafieldRecord,
  ProductOperationRecord, ProductOperationUserErrorRecord, ProductOptionRecord,
  ProductOptionValueRecord, ProductRecord, ProductResourceFeedbackRecord,
  ProductSeoRecord, ProductVariantRecord, ProductVariantSelectedOptionRecord,
  PublicationRecord, SellingPlanGroupRecord, SellingPlanRecord,
  ShopResourceFeedbackRecord,
}

@internal
pub fn make_created_variant_record(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  input: Dict(String, ResolvedValue),
  defaults: Option(ProductVariantRecord),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let selected_options = read_variant_selected_options(input, [])
  let #(variant_id, identity_after_variant) =
    synthetic_identity.make_synthetic_gid(identity, "ProductVariant")
  let #(inventory_item, final_identity) = case
    read_object_field(input, "inventoryItem")
  {
    Some(inventory_item_input) ->
      read_variant_inventory_item(
        identity_after_variant,
        Some(inventory_item_input),
        None,
      )
    None ->
      clone_default_inventory_item(
        identity_after_variant,
        option.then(defaults, fn(variant) { variant.inventory_item }),
      )
  }
  #(
    ProductVariantRecord(
      id: variant_id,
      product_id: product_id,
      title: read_non_empty_string_field(input, "title")
        |> option.unwrap(variant_title_with_fallback(
          selected_options,
          "Default Title",
        )),
      sku: read_variant_sku(input, None),
      barcode: read_string_field(input, "barcode"),
      price: read_string_field(input, "price")
        |> option.or(option.then(defaults, fn(variant) { variant.price })),
      compare_at_price: read_string_field(input, "compareAtPrice")
        |> option.or(
          option.then(defaults, fn(variant) { variant.compare_at_price }),
        ),
      taxable: read_bool_field(input, "taxable")
        |> option.or(option.then(defaults, fn(variant) { variant.taxable })),
      inventory_policy: read_string_field(input, "inventoryPolicy")
        |> option.or(
          option.then(defaults, fn(variant) { variant.inventory_policy }),
        ),
      inventory_quantity: read_variant_inventory_quantity(input, Some(0)),
      selected_options: selected_options,
      media_ids: [],
      inventory_item: inventory_item,
      contextual_pricing: None,
      cursor: None,
    ),
    final_identity,
  )
}

@internal
pub fn variant_validation_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  []
  |> list.append(variant_price_problems(input))
  |> list.append(variant_compare_at_price_problems(input))
  |> list.append(variant_weight_problems(input))
  |> list.append(variant_quantity_problems(input))
  |> list.append(variant_text_length_problems(input))
  |> list.append(variant_option_value_length_problems(input))
}

@internal
pub fn validate_bulk_variant_option_input(
  store: Store,
  product_id: String,
  input: Dict(String, ResolvedValue),
  variant_index: Int,
  mode: String,
) -> #(List(ProductVariantSelectedOptionRecord), List(BulkVariantUserError)) {
  let selected_options = read_variant_selected_options(input, [])
  let product_options =
    store.get_effective_options_by_product_id(store, product_id)
  let option_field_name = bulk_variant_option_field_name(input)
  let user_errors =
    validate_bulk_variant_selected_options(
      selected_options,
      product_options,
      dict.new(),
      variant_index,
      0,
      option_field_name,
      mode,
    )
  let user_errors = case user_errors {
    [] ->
      validate_bulk_variant_required_options(
        selected_options,
        product_options,
        variant_index,
        mode,
        has_variant_option_input(input),
      )
    _ -> user_errors
  }
  #(selected_options, user_errors)
}

@internal
pub fn update_variant_record(
  identity: SyntheticIdentityRegistry,
  existing: ProductVariantRecord,
  input: Dict(String, ResolvedValue),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let selected_options =
    read_variant_selected_options(input, existing.selected_options)
  let #(inventory_item, next_identity) =
    read_variant_inventory_item(
      identity,
      read_object_field(input, "inventoryItem"),
      existing.inventory_item,
    )
  #(
    ProductVariantRecord(
      ..existing,
      title: read_non_empty_string_field(input, "title")
        |> option.unwrap(variant_title_with_fallback(
          selected_options,
          existing.title,
        )),
      sku: read_variant_sku(input, existing.sku),
      barcode: read_string_field(input, "barcode")
        |> option.or(existing.barcode),
      price: read_string_field(input, "price") |> option.or(existing.price),
      compare_at_price: read_string_field(input, "compareAtPrice")
        |> option.or(existing.compare_at_price),
      taxable: read_bool_field(input, "taxable") |> option.or(existing.taxable),
      inventory_policy: read_string_field(input, "inventoryPolicy")
        |> option.or(existing.inventory_policy),
      inventory_quantity: read_variant_inventory_quantity(
        input,
        existing.inventory_quantity,
      ),
      selected_options: selected_options,
      inventory_item: inventory_item,
    ),
    next_identity,
  )
}

@internal
pub fn create_option_input_errors(
  existing_options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
  inputs: List(Dict(String, ResolvedValue)),
  replacing_default: Bool,
) -> List(ProductUserError) {
  let field_errors =
    inputs
    |> enumerate_items()
    |> list.flat_map(fn(pair) {
      let #(input, index) = pair
      create_single_option_input_errors(
        input,
        index,
        existing_options,
        existing_variants,
        replacing_default,
      )
    })
  list.append(field_errors, duplicate_option_input_name_errors(inputs))
}
