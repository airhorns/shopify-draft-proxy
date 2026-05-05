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
  find_media_by_id, insert_product_media_at_position, make_synthetic_media_id,
  make_synthetic_product_image_id, product_media_image_source,
  transition_media_to_ready,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  non_empty_string, read_arg_string_list, read_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type CollectionProductMove, type VariantMediaInput, CollectionProductMove,
  VariantMediaInput,
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
pub fn product_media_preview_source(media: ProductMediaRecord) -> SourceValue {
  src_object([
    #("image", product_media_image_source(media.preview_image_url)),
  ])
}

@internal
pub fn apply_product_media_moves(
  media: List(ProductMediaRecord),
  moves: List(CollectionProductMove),
) -> List(ProductMediaRecord) {
  list.fold(moves, media, fn(current_media, move) {
    let CollectionProductMove(id: media_id, new_position: new_position) = move
    case find_media_by_id(current_media, media_id) {
      None -> current_media
      Some(record) -> {
        let without_record =
          current_media
          |> list.filter(fn(candidate) { candidate.id != Some(media_id) })
        insert_product_media_at_position(without_record, record, new_position)
      }
    }
  })
}

@internal
pub fn read_variant_media_inputs(
  raw_inputs: List(Dict(String, ResolvedValue)),
) -> List(VariantMediaInput) {
  raw_inputs
  |> list.filter_map(fn(input) {
    case read_string_field(input, "variantId") {
      Some(variant_id) ->
        Ok(VariantMediaInput(
          variant_id: variant_id,
          media_ids: read_arg_string_list(input, "mediaIds"),
        ))
      None -> Error(Nil)
    }
  })
}

@internal
pub fn make_created_media_record(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  input: Dict(String, ResolvedValue),
  position: Int,
) -> #(ProductMediaRecord, SyntheticIdentityRegistry) {
  let media_content_type =
    read_string_field(input, "mediaContentType") |> option.unwrap("IMAGE")
  let #(media_id, identity_after_media) =
    make_synthetic_media_id(identity, media_content_type)
  let #(product_image_id, next_identity) =
    make_synthetic_product_image_id(identity_after_media, media_content_type)
  let source_url =
    option.then(read_string_field(input, "originalSource"), non_empty_string)
  #(
    ProductMediaRecord(
      key: product_id <> ":media:" <> int.to_string(position),
      product_id: product_id,
      position: position,
      id: Some(media_id),
      media_content_type: Some(media_content_type),
      alt: read_string_field(input, "alt"),
      status: Some("UPLOADED"),
      product_image_id: product_image_id,
      image_url: None,
      image_width: None,
      image_height: None,
      preview_image_url: None,
      source_url: source_url,
    ),
    next_identity,
  )
}

@internal
pub fn settle_media_to_ready(media: ProductMediaRecord) -> ProductMediaRecord {
  case media.status {
    Some("PROCESSING") -> transition_media_to_ready(media)
    _ -> media
  }
}

@internal
pub fn update_media_record(
  media: ProductMediaRecord,
  input: Dict(String, ResolvedValue),
) -> ProductMediaRecord {
  let next_image_url =
    option.then(
      read_string_field(input, "previewImageSource"),
      non_empty_string,
    )
    |> option.or(option.then(
      read_string_field(input, "originalSource"),
      non_empty_string,
    ))
    |> option.or(media.image_url)
    |> option.or(media.preview_image_url)
    |> option.or(media.source_url)
  ProductMediaRecord(
    ..media,
    alt: read_string_field(input, "alt") |> option.or(media.alt),
    status: Some("READY"),
    image_url: next_image_url,
    preview_image_url: next_image_url,
    source_url: media.source_url |> option.or(next_image_url),
  )
}

@internal
pub fn find_media_update(
  updates: List(Dict(String, ResolvedValue)),
  media_id: Option(String),
) -> Option(Dict(String, ResolvedValue)) {
  case media_id {
    None -> None
    Some(id) ->
      updates
      |> list.find(fn(update) { read_string_field(update, "id") == Some(id) })
      |> option.from_result
  }
}

@internal
pub fn has_media_id(media: List(ProductMediaRecord), id: String) -> Bool {
  case find_media_by_id(media, id) {
    Some(_) -> True
    None -> False
  }
}
