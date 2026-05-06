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
pub fn product_media_cursor(media: ProductMediaRecord, _index: Int) -> String {
  "cursor:" <> media.key
}

@internal
pub fn product_media_typename(media: ProductMediaRecord) -> String {
  case media.media_content_type {
    Some("IMAGE") -> "MediaImage"
    Some("VIDEO") -> "Video"
    Some("EXTERNAL_VIDEO") -> "ExternalVideo"
    Some("MODEL_3D") -> "Model3d"
    _ -> "Media"
  }
}

@internal
pub fn product_media_image_source(url: Option(String)) -> SourceValue {
  case url {
    Some(url) -> src_object([#("url", SrcString(url))])
    None -> SrcNull
  }
}

@internal
pub fn insert_product_media_at_position(
  media: List(ProductMediaRecord),
  record: ProductMediaRecord,
  position: Int,
) -> List(ProductMediaRecord) {
  let insertion_index = int.min(position, list.length(media))
  let before = list.take(media, insertion_index)
  let after = list.drop(media, insertion_index)
  list.append(before, [record, ..after])
}

@internal
pub fn is_create_media_content_type(value: String) -> Bool {
  case value {
    "VIDEO" | "EXTERNAL_VIDEO" | "MODEL_3D" | "IMAGE" -> True
    _ -> False
  }
}

@internal
pub fn is_valid_media_source(value: Option(String)) -> Bool {
  case value {
    Some(value) -> {
      let trimmed = string.trim(value)
      string.length(trimmed) > 0
      && {
        string.starts_with(trimmed, "http://")
        || string.starts_with(trimmed, "https://")
      }
    }
    None -> False
  }
}

@internal
pub fn make_synthetic_media_id(
  identity: SyntheticIdentityRegistry,
  media_content_type: String,
) -> #(String, SyntheticIdentityRegistry) {
  case media_content_type {
    "IMAGE" -> synthetic_identity.make_synthetic_gid(identity, "MediaImage")
    _ -> synthetic_identity.make_synthetic_gid(identity, "Media")
  }
}

@internal
pub fn make_synthetic_product_image_id(
  identity: SyntheticIdentityRegistry,
  media_content_type: String,
) -> #(Option(String), SyntheticIdentityRegistry) {
  case media_content_type {
    "IMAGE" -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "ProductImage")
      #(Some(id), next_identity)
    }
    _ -> #(None, identity)
  }
}

@internal
pub fn transition_created_media_to_processing(
  media: ProductMediaRecord,
) -> ProductMediaRecord {
  ProductMediaRecord(
    ..media,
    status: Some("PROCESSING"),
    image_url: None,
    preview_image_url: None,
  )
}

@internal
pub fn transition_media_to_ready(
  media: ProductMediaRecord,
) -> ProductMediaRecord {
  let ready_url =
    media.source_url
    |> option.or(media.image_url)
    |> option.or(media.preview_image_url)
  ProductMediaRecord(
    ..media,
    status: Some("READY"),
    image_url: ready_url,
    preview_image_url: ready_url,
  )
}

@internal
pub fn find_media_by_id(
  media: List(ProductMediaRecord),
  id: String,
) -> Option(ProductMediaRecord) {
  media
  |> list.find(fn(record) { record.id == Some(id) })
  |> option.from_result
}

@internal
pub fn media_record_id_result(
  media: ProductMediaRecord,
) -> Result(String, Nil) {
  case media.id {
    Some(id) -> Ok(id)
    None -> Error(Nil)
  }
}

@internal
pub fn product_media_product_image_id_result(
  media: ProductMediaRecord,
) -> Result(String, Nil) {
  case media.product_image_id {
    Some(id) -> Ok(id)
    None -> Error(Nil)
  }
}
