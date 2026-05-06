//// Products-domain submodule: products_records.
//// Combines layered files: products_l05.

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
import shopify_draft_proxy/proxy/products/products_core.{
  product_cursor, product_handle_should_dedupe, product_numeric_id,
  product_tags_validation_errors, read_explicit_product_handle,
  read_product_status_field, slugify_product_handle, updated_product_seo,
}
import shopify_draft_proxy/proxy/products/products_validation.{
  ensure_unique_product_handle, explicit_product_handle_collision_errors,
  filtered_products, normalize_product_tags, product_scalar_validation_errors,
  product_sort_cursor_int, product_sort_cursor_string,
  product_sort_cursor_timestamp, product_vendor_for_create,
}
import shopify_draft_proxy/proxy/products/publications_feeds.{
  product_source_with_relationships,
}
import shopify_draft_proxy/proxy/products/shared.{
  count_source, empty_connection_source, read_non_empty_string_field,
  read_string_argument, read_string_field, read_string_list_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type ProductUserError, ProductUserError,
} as product_types
import shopify_draft_proxy/proxy/products/variants_helpers.{
  has_only_default_variant,
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

// ===== from products_l05 =====
@internal
pub fn product_count_for_field(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Int {
  case read_string_argument(field, variables, "query") {
    Some(_) -> list.length(filtered_products(store, field, variables))
    None -> store.get_effective_product_count(store)
  }
}

@internal
pub fn product_cursor_for_field(
  product: ProductRecord,
  index: Int,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> String {
  case product.cursor {
    Some(_) -> product_cursor(product, index)
    None ->
      case read_string_argument(field, variables, "sortKey") {
        Some("TITLE") ->
          product_sort_cursor_string(product, string.lowercase(product.title))
        Some("VENDOR") ->
          product_sort_cursor_string(
            product,
            product.vendor |> option.unwrap("") |> string.lowercase,
          )
        Some("PRODUCT_TYPE") ->
          product_sort_cursor_string(
            product,
            product.product_type |> option.unwrap("") |> string.lowercase,
          )
        Some("ID") ->
          product_sort_cursor_int(product, product_numeric_id(product))
        Some("PUBLISHED_AT") ->
          product_sort_cursor_timestamp(product, product.published_at)
        Some("UPDATED_AT") ->
          product_sort_cursor_timestamp(product, product.updated_at)
        _ -> product_cursor(product, index)
      }
  }
}

@internal
pub fn product_source(product: ProductRecord) -> SourceValue {
  product_source_with_relationships(
    product,
    empty_connection_source(),
    empty_connection_source(),
    empty_connection_source(),
    SrcList([]),
    empty_connection_source(),
    count_source(0),
    "USD",
    None,
  )
}

@internal
pub fn product_set_product_field_errors(
  store: Store,
  input: Dict(String, ResolvedValue),
  existing: Option(ProductRecord),
) -> List(ProductOperationUserErrorRecord) {
  let scalar_errors = case existing {
    Some(_) ->
      product_scalar_validation_errors(input, ["input"], require_title: False)
    None ->
      product_scalar_validation_errors(input, ["input"], require_title: True)
  }
  let tag_errors =
    product_tags_validation_errors(input)
    |> list.map(fn(error) {
      let ProductUserError(field: path, message: message, code: code) = error
      ProductUserError(field: ["input", ..path], message: message, code: code)
    })
  let existing_id = option.map(existing, fn(product) { product.id })
  let handle_errors =
    explicit_product_handle_collision_errors(store, input, existing_id)
    |> list.map(fn(error) {
      let ProductUserError(field: path, message: message, code: code) = error
      ProductUserError(field: ["input", ..path], message: message, code: code)
    })
  list.append(scalar_errors, list.append(tag_errors, handle_errors))
  |> list.map(fn(error) {
    let ProductUserError(field: path, message: message, code: code) = error
    ProductOperationUserErrorRecord(
      field: Some(path),
      message: message,
      code: code,
    )
  })
}

@internal
pub fn product_update_validation_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  list.append(
    product_scalar_validation_errors(input, [], require_title: False),
    product_tags_validation_errors(input),
  )
}

@internal
pub fn updated_product_record(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
  input: Dict(String, ResolvedValue),
) -> #(ProductRecord, SyntheticIdentityRegistry) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  #(
    ProductRecord(
      ..product,
      title: read_non_empty_string_field(input, "title")
        |> option.unwrap(product.title),
      handle: read_non_empty_string_field(input, "handle")
        |> option.unwrap(product.handle),
      status: read_product_status_field(input) |> option.unwrap(product.status),
      vendor: read_string_field(input, "vendor") |> option.or(product.vendor),
      product_type: read_string_field(input, "productType")
        |> option.or(product.product_type),
      tags: read_string_list_field(input, "tags")
        |> option.map(normalize_product_tags)
        |> option.unwrap(product.tags),
      description_html: read_string_field(input, "descriptionHtml")
        |> option.unwrap(product.description_html),
      template_suffix: read_string_field(input, "templateSuffix")
        |> option.or(product.template_suffix),
      seo: updated_product_seo(product.seo, input),
      updated_at: Some(updated_at),
    ),
    next_identity,
  )
}

@internal
pub fn created_product_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  shopify_admin_origin: String,
  input: Dict(String, ResolvedValue),
) -> #(ProductRecord, SyntheticIdentityRegistry) {
  let assert Some(title) = read_non_empty_string_field(input, "title")
  let #(created_at, identity_after_timestamp) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(id, next_identity) =
    synthetic_identity.make_proxy_synthetic_gid(
      identity_after_timestamp,
      "Product",
    )
  let base_handle = case read_explicit_product_handle(input) {
    Some(handle) -> handle
    None -> slugify_product_handle(title)
  }
  let handle = case product_handle_should_dedupe(input) {
    True -> ensure_unique_product_handle(store, base_handle)
    False -> base_handle
  }
  #(
    ProductRecord(
      id: id,
      legacy_resource_id: None,
      title: title,
      handle: handle,
      status: read_product_status_field(input) |> option.unwrap("ACTIVE"),
      vendor: product_vendor_for_create(store, shopify_admin_origin, input),
      product_type: read_string_field(input, "productType"),
      tags: read_string_list_field(input, "tags")
        |> option.map(normalize_product_tags)
        |> option.unwrap([]),
      price_range_min: None,
      price_range_max: None,
      total_variants: None,
      has_only_default_variant: None,
      has_out_of_stock_variants: None,
      total_inventory: Some(0),
      tracks_inventory: Some(False),
      created_at: Some(created_at),
      updated_at: Some(created_at),
      published_at: None,
      description_html: read_string_field(input, "descriptionHtml")
        |> option.unwrap(""),
      online_store_preview_url: None,
      template_suffix: read_string_field(input, "templateSuffix"),
      seo: updated_product_seo(
        ProductSeoRecord(title: None, description: None),
        input,
      ),
      category: None,
      publication_ids: [],
      contextual_pricing: None,
      cursor: None,
    ),
    next_identity,
  )
}
