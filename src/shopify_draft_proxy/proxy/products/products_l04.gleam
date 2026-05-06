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
  variant_tracks_inventory,
}
import shopify_draft_proxy/proxy/products/inventory_l02.{
  variant_available_quantity,
}
import shopify_draft_proxy/proxy/products/products_l01.{
  build_product_delete_invalid_variable_error,
  product_description_html_validation_errors,
}
import shopify_draft_proxy/proxy/products/products_l02.{
  product_delete_payload, product_string_length_validation_errors,
  product_tag_identity_key, slugify_product_handle,
}
import shopify_draft_proxy/proxy/products/products_l03.{
  compare_product_tags, default_product_vendor, ensure_unique_product_handle,
  product_delete_input_error, product_matches_positive_query_term,
  product_sort_cursor_int, product_title_validation_errors,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_string_argument, trimmed_non_empty,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  mutation_error_result, mutation_result, read_non_empty_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type ProductDerivedSummary, type ProductUserError,
  MutationFieldResult, ProductDerivedSummary, ProductUserError,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  has_only_default_variant, product_search_parse_options,
}
import shopify_draft_proxy/proxy/products/variants_l03.{
  max_variant_price_amount, min_variant_price_amount,
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
pub fn filtered_products(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductRecord) {
  search_query_parser.apply_search_query(
    store.list_effective_products(store),
    read_string_argument(field, variables, "query"),
    product_search_parse_options(),
    fn(product, term) {
      product_matches_positive_query_term(store, product, term)
    },
  )
}

@internal
pub fn product_sort_cursor_timestamp(
  product: ProductRecord,
  value: Option(String),
) -> String {
  let timestamp = case value {
    Some(raw) -> iso_timestamp.parse_iso(raw) |> result.unwrap(0)
    None -> 0
  }
  product_sort_cursor_int(product, timestamp)
}

@internal
pub fn handle_product_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  case product_delete_input_error(document, field, variables) {
    Some(error) -> mutation_error_result(key, store, identity, [error])
    None -> {
      let args = graphql_helpers.field_args(field, variables)
      let input = graphql_helpers.read_arg_object(args, "input")
      let id = case input {
        Some(input) -> graphql_helpers.read_arg_string(input, "id")
        None -> graphql_helpers.read_arg_string(args, "id")
      }
      case id {
        None ->
          mutation_error_result(key, store, identity, [
            build_product_delete_invalid_variable_error(
              "input",
              json.object([]),
              None,
              document,
            ),
          ])
        Some(product_id) ->
          case store.get_effective_product_by_id(store, product_id) {
            None ->
              mutation_result(
                key,
                product_delete_payload(
                  None,
                  [ProductUserError(["id"], "Product does not exist", None)],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(_) -> {
              let next_store = store.delete_staged_product(store, product_id)
              mutation_result(
                key,
                product_delete_payload(Some(product_id), [], field, fragments),
                next_store,
                identity,
                [product_id],
              )
            }
          }
      }
    }
  }
}

@internal
pub fn product_scalar_validation_errors(
  input: Dict(String, ResolvedValue),
  field_prefix: List(String),
  require_title require_title: Bool,
) -> List(ProductUserError) {
  list.append(
    product_title_validation_errors(
      input,
      field_prefix,
      require_missing: require_title,
    ),
    list.append(
      product_string_length_validation_errors(input, field_prefix),
      product_description_html_validation_errors(input, field_prefix),
    ),
  )
}

@internal
pub fn product_vendor_for_create(
  store: Store,
  shopify_admin_origin: String,
  input: Dict(String, ResolvedValue),
) -> Option(String) {
  read_non_empty_string_field(input, "vendor")
  |> option.or(default_product_vendor(store, shopify_admin_origin))
}

@internal
pub fn normalize_product_tags(tags: List(String)) -> List(String) {
  let #(reversed, _) =
    tags
    |> list.filter_map(trimmed_non_empty)
    |> list.fold(#([], dict.new()), fn(acc, tag) {
      let #(items, seen) = acc
      let key = product_tag_identity_key(tag)
      case dict.has_key(seen, key) {
        True -> #(items, seen)
        False -> #([tag, ..items], dict.insert(seen, key, True))
      }
    })

  reversed
  |> list.reverse
  |> list.sort(compare_product_tags)
}

@internal
pub fn duplicated_product_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  source_product: ProductRecord,
  new_title: Option(String),
) -> #(ProductRecord, SyntheticIdentityRegistry) {
  let title = new_title |> option.unwrap(source_product.title <> " Copy")
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "Product")
  let base_handle = slugify_product_handle(title)
  let handle = ensure_unique_product_handle(store, base_handle)
  #(
    ProductRecord(
      ..source_product,
      id: id,
      legacy_resource_id: None,
      title: title,
      handle: handle,
      status: "DRAFT",
      cursor: None,
    ),
    next_identity,
  )
}

@internal
pub fn product_derived_summary(
  variants: List(ProductVariantRecord),
) -> ProductDerivedSummary {
  let tracked_variants = list.filter(variants, variant_tracks_inventory)
  ProductDerivedSummary(
    price_range_min: min_variant_price_amount(variants),
    price_range_max: max_variant_price_amount(variants),
    total_variants: Some(list.length(variants)),
    has_only_default_variant: Some(has_only_default_variant(variants)),
    has_out_of_stock_variants: Some(
      list.any(tracked_variants, fn(variant) {
        variant_available_quantity(variant) <= 0
      }),
    ),
    total_inventory: Some(
      list.fold(tracked_variants, 0, fn(total, variant) {
        total + variant_available_quantity(variant)
      }),
    ),
    tracks_inventory: Some(!list.is_empty(tracked_variants)),
  )
}
