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
  find_inventory_level, location_source, variant_inventory_levels,
}
import shopify_draft_proxy/proxy/products/inventory_transfers_l00.{
  find_inventory_transfer_line_item_by_item_id,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_int_field, read_object_list_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type InventoryTransferLineItemInput, type InventoryTransferLineItemUpdate,
  type ProductUserError, InventoryTransferLineItemInput,
  InventoryTransferLineItemUpdate, ProductUserError,
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
pub fn inventory_transfer_location_source(
  store: Store,
  snapshot: Option(InventoryTransferLocationSnapshotRecord),
) -> SourceValue {
  case snapshot {
    Some(snapshot) -> {
      let location = case snapshot.id {
        Some(id) -> {
          case store.get_effective_location_by_id(store, id) {
            Some(location) -> location_source(location)
            None -> SrcNull
          }
        }
        None -> SrcNull
      }
      src_object([
        #("__typename", SrcString("InventoryTransferLocationSnapshot")),
        #("name", SrcString(snapshot.name)),
        #("snapshottedAt", SrcString(snapshot.snapshotted_at)),
        #("location", location),
        #("address", src_object([])),
      ])
    }
    None -> SrcNull
  }
}

@internal
pub fn read_inventory_transfer_line_item_inputs(
  input: Dict(String, ResolvedValue),
) -> List(InventoryTransferLineItemInput) {
  read_object_list_field(input, "lineItems")
  |> list.map(fn(fields) {
    InventoryTransferLineItemInput(
      inventory_item_id: read_string_field(fields, "inventoryItemId"),
      quantity: read_int_field(fields, "quantity"),
    )
  })
}

@internal
pub fn make_inventory_transfer_line_item(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: InventoryTransferLineItemInput,
) -> #(Option(InventoryTransferLineItemRecord), SyntheticIdentityRegistry) {
  case input.inventory_item_id, input.quantity {
    Some(inventory_item_id), Some(quantity) -> {
      let #(id, next_identity) =
        synthetic_identity.make_proxy_synthetic_gid(
          identity,
          "InventoryTransferLineItem",
        )
      let variant =
        store.find_effective_variant_by_inventory_item_id(
          store,
          inventory_item_id,
        )
      let title = case variant {
        Some(variant) ->
          case store.get_effective_product_by_id(store, variant.product_id) {
            Some(product) -> Some(product.title)
            None -> Some(variant.title)
          }
        None -> None
      }
      #(
        Some(InventoryTransferLineItemRecord(
          id: id,
          inventory_item_id: inventory_item_id,
          title: title,
          total_quantity: quantity,
          shipped_quantity: 0,
          picked_for_shipment_quantity: 0,
        )),
        next_identity,
      )
    }
    _, _ -> #(None, identity)
  }
}

@internal
pub fn inventory_transfer_set_item_deltas(
  prior_items: List(InventoryTransferLineItemRecord),
  updated_items: List(InventoryTransferLineItemRecord),
) -> List(#(InventoryTransferLineItemRecord, Int)) {
  let updated_deltas =
    list.map(updated_items, fn(line_item) {
      let prior_quantity =
        find_inventory_transfer_line_item_by_item_id(
          prior_items,
          line_item.inventory_item_id,
        )
        |> option.map(fn(prior) { prior.total_quantity })
        |> option.unwrap(0)
      #(line_item, line_item.total_quantity - prior_quantity)
    })
  let removed_deltas =
    prior_items
    |> list.filter(fn(line_item) {
      find_inventory_transfer_line_item_by_item_id(
        updated_items,
        line_item.inventory_item_id,
      )
      == None
    })
    |> list.map(fn(line_item) { #(line_item, 0 - line_item.total_quantity) })
  list.append(updated_deltas, removed_deltas)
}

@internal
pub fn find_inventory_transfer_origin_level(
  store: Store,
  transfer: InventoryTransferRecord,
  line_item: InventoryTransferLineItemRecord,
) -> Option(#(ProductVariantRecord, InventoryLevelRecord)) {
  case
    store.find_effective_variant_by_inventory_item_id(
      store,
      line_item.inventory_item_id,
    ),
    transfer.origin
  {
    Some(variant), Some(origin) ->
      case origin.id {
        Some(location_id) ->
          case
            find_inventory_level(variant_inventory_levels(variant), location_id)
          {
            Some(level) -> Some(#(variant, level))
            None -> None
          }
        None -> None
      }
    _, _ -> None
  }
}

@internal
pub fn inventory_transfer_origin_state_error() -> ProductUserError {
  ProductUserError(
    ["id"],
    "Cannot mark the transfer as ready to ship as the line items contain following errors: The item is not stocked at the origin location.",
    Some("INVENTORY_STATE_NOT_ACTIVE"),
  )
}

@internal
pub fn inventory_transfer_not_found_error() -> ProductUserError {
  ProductUserError(
    ["id"],
    "The inventory transfer can't be found.",
    Some("TRANSFER_NOT_FOUND"),
  )
}

@internal
pub fn inventory_transfer_line_item_update_source(
  update: InventoryTransferLineItemUpdate,
) -> SourceValue {
  src_object([
    #("inventoryItemId", SrcString(update.inventory_item_id)),
    #("newQuantity", SrcInt(update.new_quantity)),
    #("deltaQuantity", SrcInt(update.delta_quantity)),
  ])
}
