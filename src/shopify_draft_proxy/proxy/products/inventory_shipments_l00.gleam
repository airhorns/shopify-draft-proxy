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
pub fn inventory_shipment_tracking_source(
  tracking: Option(InventoryShipmentTrackingRecord),
) -> SourceValue {
  case tracking {
    Some(tracking) ->
      src_object([
        #(
          "trackingNumber",
          graphql_helpers.option_string_source(tracking.tracking_number),
        ),
        #("company", graphql_helpers.option_string_source(tracking.company)),
        #(
          "trackingUrl",
          graphql_helpers.option_string_source(tracking.tracking_url),
        ),
        #(
          "arrivesAt",
          graphql_helpers.option_string_source(tracking.arrives_at),
        ),
      ])
    None -> SrcNull
  }
}

@internal
pub fn shipment_line_item_unreceived(
  line_item: InventoryShipmentLineItemRecord,
) -> Int {
  int.max(
    0,
    line_item.quantity
      - line_item.accepted_quantity
      - line_item.rejected_quantity,
  )
}

@internal
pub fn shipment_line_item_total(shipment: InventoryShipmentRecord) -> Int {
  list.fold(shipment.line_items, 0, fn(total, line_item) {
    total + line_item.quantity
  })
}

@internal
pub fn shipment_total_accepted(shipment: InventoryShipmentRecord) -> Int {
  list.fold(shipment.line_items, 0, fn(total, line_item) {
    total + line_item.accepted_quantity
  })
}

@internal
pub fn shipment_total_rejected(shipment: InventoryShipmentRecord) -> Int {
  list.fold(shipment.line_items, 0, fn(total, line_item) {
    total + line_item.rejected_quantity
  })
}

@internal
pub fn find_inventory_shipment_line_item(
  line_items: List(InventoryShipmentLineItemRecord),
  id: String,
) -> Option(InventoryShipmentLineItemRecord) {
  line_items
  |> list.find(fn(line_item) { line_item.id == id })
  |> option.from_result
}

@internal
pub fn inventory_shipment_status_after_receive(
  line_items: List(InventoryShipmentLineItemRecord),
) -> String {
  let total =
    list.fold(line_items, 0, fn(sum, line_item) { sum + line_item.quantity })
  let received =
    list.fold(line_items, 0, fn(sum, line_item) {
      sum + line_item.accepted_quantity + line_item.rejected_quantity
    })
  case received <= 0 {
    True -> "IN_TRANSIT"
    False ->
      case received >= total {
        True -> "RECEIVED"
        False -> "PARTIALLY_RECEIVED"
      }
  }
}

@internal
pub fn shipment_has_unreceived_incoming(
  shipment: InventoryShipmentRecord,
) -> Bool {
  shipment.status == "IN_TRANSIT" || shipment.status == "PARTIALLY_RECEIVED"
}
