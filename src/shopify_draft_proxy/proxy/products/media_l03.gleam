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
import shopify_draft_proxy/proxy/products/inventory_l02.{
  find_variable_definition_location,
}
import shopify_draft_proxy/proxy/products/media_l00.{
  find_media_by_id, product_media_cursor,
}
import shopify_draft_proxy/proxy/products/media_l02.{
  product_media_source, product_update_media_payload_with_media_value,
}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/products_l02.{enumerate_strings}
import shopify_draft_proxy/proxy/products/shared_l00.{
  connection_end_cursor, connection_start_cursor, resolved_value_to_json,
}
import shopify_draft_proxy/proxy/products/types.{
  type ProductUserError, ProductUserError,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{option_to_result}
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
pub fn product_media_connection_source(
  store: Store,
  product: ProductRecord,
) -> SourceValue {
  let media = store.get_effective_media_by_product_id(store, product.id)
  src_object([
    #(
      "edges",
      SrcList(
        list.map(enumerate_items(media), fn(entry) {
          let #(record, index) = entry
          src_object([
            #("cursor", SrcString(product_media_cursor(record, index))),
            #("node", product_media_source(record)),
          ])
        }),
      ),
    ),
    #("nodes", SrcList(list.map(media, product_media_source))),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", connection_start_cursor(media, product_media_cursor)),
        #("endCursor", connection_end_cursor(media, product_media_cursor)),
      ]),
    ),
  ])
}

@internal
pub fn variant_media_connection_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  let product_media =
    store.get_effective_media_by_product_id(store, variant.product_id)
  let media =
    variant.media_ids
    |> list.filter_map(fn(media_id) {
      find_media_by_id(product_media, media_id) |> option_to_result
    })
  src_object([
    #(
      "edges",
      SrcList(
        list.map(enumerate_items(media), fn(entry) {
          let #(record, index) = entry
          src_object([
            #("cursor", SrcString(product_media_cursor(record, index))),
            #("node", product_media_source(record)),
          ])
        }),
      ),
    ),
    #("nodes", SrcList(list.map(media, product_media_source))),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", connection_start_cursor(media, product_media_cursor)),
        #("endCursor", connection_end_cursor(media, product_media_cursor)),
      ]),
    ),
  ])
}

@internal
pub fn first_unknown_media_index(
  media_ids: List(String),
  product_media_ids: List(String),
) -> Option(Int) {
  media_ids
  |> enumerate_strings()
  |> list.find(fn(entry) {
    let #(media_id, _) = entry
    !list.contains(product_media_ids, media_id)
  })
  |> result.map(fn(entry) {
    let #(_, index) = entry
    index
  })
  |> option.from_result
}

@internal
pub fn product_update_media_payload(
  media: List(ProductMediaRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  product_update_media_payload_with_media_value(
    SrcList(list.map(media, product_media_source)),
    user_errors,
    field,
    fragments,
  )
}

@internal
pub fn invalid_product_media_product_id_variable_error(
  product_id: String,
  document: String,
) -> Json {
  let message = "Invalid global id '" <> product_id <> "'"
  let base = [
    #(
      "message",
      json.string("Variable $productId of type ID! was provided invalid value"),
    ),
  ]
  let with_locations = case
    find_variable_definition_location(document, "productId")
  {
    Some(loc) ->
      list.append(base, [
        #("locations", graphql_helpers.locations_json(loc, document)),
      ])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "extensions",
        json.object([
          #("code", json.string("INVALID_VARIABLE")),
          #("value", json.string(product_id)),
          #(
            "problems",
            json.preprocessed_array([
              json.object([
                #("path", json.preprocessed_array([])),
                #("explanation", json.string(message)),
                #("message", json.string(message)),
              ]),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

@internal
pub fn invalid_product_media_content_type_variable_error(
  media_values: List(ResolvedValue),
  media_index: Int,
  media_content_type: String,
  document: String,
) -> Json {
  let explanation =
    "Expected \""
    <> media_content_type
    <> "\" to be one of: VIDEO, EXTERNAL_VIDEO, MODEL_3D, IMAGE"
  let base = [
    #(
      "message",
      json.string(
        "Variable $media of type [CreateMediaInput!]! was provided invalid value for "
        <> int.to_string(media_index)
        <> ".mediaContentType ("
        <> explanation
        <> ")",
      ),
    ),
  ]
  let with_locations = case
    find_variable_definition_location(document, "media")
  {
    Some(loc) ->
      list.append(base, [
        #("locations", graphql_helpers.locations_json(loc, document)),
      ])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "extensions",
        json.object([
          #("code", json.string("INVALID_VARIABLE")),
          #("value", json.array(media_values, resolved_value_to_json)),
          #(
            "problems",
            json.preprocessed_array([
              json.object([
                #(
                  "path",
                  json.preprocessed_array([
                    json.int(media_index),
                    json.string("mediaContentType"),
                  ]),
                ),
                #("explanation", json.string(explanation)),
              ]),
            ]),
          ),
        ]),
      ),
    ]),
  )
}
