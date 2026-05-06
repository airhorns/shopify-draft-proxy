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
pub fn inventory_transfer_cursor(
  transfer: InventoryTransferRecord,
  _index: Int,
) -> String {
  "cursor:" <> transfer.id
}

@internal
pub fn inventory_transfer_total_quantity(
  transfer: InventoryTransferRecord,
) -> Int {
  list.fold(transfer.line_items, 0, fn(total, line_item) {
    total + line_item.total_quantity
  })
}

@internal
pub fn make_inventory_transfer_location_snapshot(
  store: Store,
  location_id: Option(String),
  identity: SyntheticIdentityRegistry,
) -> Option(InventoryTransferLocationSnapshotRecord) {
  case location_id {
    Some(id) -> {
      let name = case store.get_effective_location_by_id(store, id) {
        Some(location) -> location.name
        None -> id
      }
      let #(snapshotted_at, _) =
        synthetic_identity.make_synthetic_timestamp(identity)
      Some(InventoryTransferLocationSnapshotRecord(
        id: Some(id),
        name: name,
        snapshotted_at: snapshotted_at,
      ))
    }
    None -> None
  }
}

@internal
pub fn duplicate_inventory_transfer_line_items(
  line_items: List(InventoryTransferLineItemRecord),
  identity: SyntheticIdentityRegistry,
) -> #(List(InventoryTransferLineItemRecord), SyntheticIdentityRegistry) {
  let #(reversed, next_identity) =
    list.fold(line_items, #([], identity), fn(acc, line_item) {
      let #(records, current_identity) = acc
      let #(id, identity_after_id) =
        synthetic_identity.make_proxy_synthetic_gid(
          current_identity,
          "InventoryTransferLineItem",
        )
      #(
        [InventoryTransferLineItemRecord(..line_item, id: id), ..records],
        identity_after_id,
      )
    })
  #(list.reverse(reversed), next_identity)
}

@internal
pub fn find_inventory_transfer_line_item(
  line_items: List(InventoryTransferLineItemRecord),
  id: String,
) -> Option(InventoryTransferLineItemRecord) {
  line_items
  |> list.find(fn(line_item) { line_item.id == id })
  |> option.from_result
}

@internal
pub fn get_inventory_transfer_by_optional_id(
  store: Store,
  transfer_id: Option(String),
) -> Option(InventoryTransferRecord) {
  case transfer_id {
    Some(id) -> store.get_effective_inventory_transfer_by_id(store, id)
    None -> None
  }
}

@internal
pub fn find_inventory_transfer_line_item_by_item_id(
  line_items: List(InventoryTransferLineItemRecord),
  inventory_item_id: String,
) -> Option(InventoryTransferLineItemRecord) {
  line_items
  |> list.find(fn(line_item) {
    line_item.inventory_item_id == inventory_item_id
  })
  |> option.from_result
}

@internal
pub fn inventory_transfer_has_reserved_origin_inventory(
  transfer: InventoryTransferRecord,
) -> Bool {
  transfer.status == "READY_TO_SHIP" || transfer.status == "IN_PROGRESS"
}

@internal
pub fn inventory_transfer_staged_ids(
  transfer: InventoryTransferRecord,
) -> List(String) {
  [transfer.id, ..list.map(transfer.line_items, fn(line_item) { line_item.id })]
}
