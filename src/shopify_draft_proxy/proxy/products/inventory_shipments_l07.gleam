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
import shopify_draft_proxy/proxy/products/inventory_shipments_l00.{
  shipment_line_item_unreceived,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_l02.{
  inventory_shipment_delete_payload,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_l06.{
  apply_inventory_shipment_deltas,
}
import shopify_draft_proxy/proxy/products/shared_l01.{mutation_result}
import shopify_draft_proxy/proxy/products/types.{
  type InventoryShipmentDelta, type MutationFieldResult, type ProductUserError,
  InventoryShipmentDelta, MutationFieldResult, ProductUserError,
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
pub fn handle_inventory_shipment_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  let existing =
    option.then(id, fn(id) {
      store.get_effective_inventory_shipment_by_id(store, id)
    })
  case existing {
    None ->
      mutation_result(
        key,
        inventory_shipment_delete_payload(
          None,
          [
            ProductUserError(
              ["id"],
              "The specified inventory shipment could not be found.",
              Some("NOT_FOUND"),
            ),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(shipment) -> {
      let user_errors = case shipment.status == "RECEIVED" {
        True -> [
          ProductUserError(
            ["id"],
            "Received shipments cannot be deleted.",
            Some("INVALID_STATUS"),
          ),
        ]
        False -> []
      }
      case user_errors {
        [] -> {
          let deltas =
            shipment.line_items
            |> list.map(fn(line_item) {
              InventoryShipmentDelta(
                inventory_item_id: line_item.inventory_item_id,
                incoming: 0 - shipment_line_item_unreceived(line_item),
                available: None,
              )
            })
          let #(next_store, next_identity) =
            apply_inventory_shipment_deltas(store, identity, deltas)
          let deleted_store =
            store.delete_staged_inventory_shipment(next_store, shipment.id)
          mutation_result(
            key,
            inventory_shipment_delete_payload(
              Some(shipment.id),
              [],
              field,
              fragments,
            ),
            deleted_store,
            next_identity,
            [shipment.id],
          )
        }
        _ ->
          mutation_result(
            key,
            inventory_shipment_delete_payload(
              None,
              user_errors,
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    }
  }
}

@internal
pub fn stage_inventory_shipment_with_incoming(
  store: Store,
  identity: SyntheticIdentityRegistry,
  shipment: InventoryShipmentRecord,
) -> #(InventoryShipmentRecord, Store, SyntheticIdentityRegistry) {
  let previous =
    store.get_effective_inventory_shipment_by_id(store, shipment.id)
  let should_add_incoming = case previous {
    Some(previous) ->
      previous.status != "IN_TRANSIT" && shipment.status == "IN_TRANSIT"
    None -> shipment.status == "IN_TRANSIT"
  }
  let deltas = case should_add_incoming {
    True ->
      shipment.line_items
      |> list.map(fn(line_item) {
        InventoryShipmentDelta(
          inventory_item_id: line_item.inventory_item_id,
          incoming: shipment_line_item_unreceived(line_item),
          available: None,
        )
      })
    False -> []
  }
  let #(next_store, next_identity) =
    apply_inventory_shipment_deltas(store, identity, deltas)
  let #(staged, staged_store) =
    store.upsert_staged_inventory_shipment(next_store, shipment)
  #(staged, staged_store, next_identity)
}
