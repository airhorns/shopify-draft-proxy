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
import shopify_draft_proxy/proxy/products/media_l00.{
  find_media_by_id, product_media_image_source, product_media_typename,
}
import shopify_draft_proxy/proxy/products/media_l01.{
  has_media_id, product_media_preview_source,
}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/shared_l00.{
  job_source, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{user_errors_source}
import shopify_draft_proxy/proxy/products/types.{
  type ProductUserError, ProductUserError,
} as product_types
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
pub fn product_media_source(media: ProductMediaRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString(product_media_typename(media))),
    #("id", graphql_helpers.option_string_source(media.id)),
    #("alt", graphql_helpers.option_string_source(media.alt)),
    #(
      "mediaContentType",
      graphql_helpers.option_string_source(media.media_content_type),
    ),
    #("status", graphql_helpers.option_string_source(media.status)),
    #("preview", product_media_preview_source(media)),
    #(
      "image",
      product_media_image_source(
        media.image_url |> option.or(media.preview_image_url),
      ),
    ),
    #("mediaErrors", SrcList([])),
    #("mediaWarnings", SrcList([])),
  ])
}

@internal
pub fn first_missing_media_update(
  updates: List(Dict(String, ResolvedValue)),
  media: List(ProductMediaRecord),
) -> Option(Dict(String, ResolvedValue)) {
  updates
  |> list.find(fn(update) {
    case read_string_field(update, "id") {
      Some(id) -> !has_media_id(media, id)
      None -> True
    }
  })
  |> option.from_result
}

@internal
pub fn first_non_ready_media_update(
  updates: List(Dict(String, ResolvedValue)),
  media: List(ProductMediaRecord),
) -> Option(Int) {
  updates
  |> enumerate_items()
  |> list.find_map(fn(entry) {
    let #(update, index) = entry
    case read_string_field(update, "id") {
      Some(id) ->
        case find_media_by_id(media, id) {
          Some(record) ->
            case record.status {
              Some("READY") -> Error(Nil)
              _ -> Ok(index)
            }
          None -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn first_unknown_media_id(
  media_ids: List(String),
  media: List(ProductMediaRecord),
) -> Option(String) {
  media_ids
  |> list.find(fn(id) { !has_media_id(media, id) })
  |> option.from_result
}

@internal
pub fn product_update_media_payload_with_media_value(
  media: SourceValue,
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductUpdateMediaPayload")),
      #("media", media),
      #("mediaUserErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_reorder_media_payload(
  job_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let job_value = case job_id {
    Some(id) -> job_source(id, False)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductReorderMediaPayload")),
      #("job", job_value),
      #("mediaUserErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}
