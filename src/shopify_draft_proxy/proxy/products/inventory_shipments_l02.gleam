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
  find_inventory_shipment_line_item, shipment_has_unreceived_incoming,
  shipment_line_item_unreceived,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_l01.{
  inventory_shipment_tracking_from_fields,
}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_int_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{user_errors_source}
import shopify_draft_proxy/proxy/products/types.{
  type InventoryShipmentDelta, type ProductUserError, InventoryShipmentDelta,
  ProductUserError,
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
pub fn validate_inventory_shipment_line_item_inputs(
  store: Store,
  line_items: List(Dict(String, ResolvedValue)),
  field_prefix: List(String),
) -> List(ProductUserError) {
  let initial = case line_items {
    [] -> [
      ProductUserError(
        field_prefix,
        "At least one line item is required.",
        Some("BLANK"),
      ),
    ]
    _ -> []
  }
  list.fold(enumerate_items(line_items), initial, fn(errors, pair) {
    let #(line_item, index) = pair
    let inventory_item_id = read_string_field(line_item, "inventoryItemId")
    let quantity = read_int_field(line_item, "quantity")
    let errors = case inventory_item_id {
      Some(id) ->
        case store.find_effective_variant_by_inventory_item_id(store, id) {
          Some(_) -> errors
          None ->
            list.append(errors, [
              ProductUserError(
                list.append(field_prefix, [
                  int.to_string(index),
                  "inventoryItemId",
                ]),
                "The specified inventory item could not be found.",
                Some("NOT_FOUND"),
              ),
            ])
        }
      None ->
        list.append(errors, [
          ProductUserError(
            list.append(field_prefix, [int.to_string(index), "inventoryItemId"]),
            "The specified inventory item could not be found.",
            Some("NOT_FOUND"),
          ),
        ])
    }
    case quantity {
      Some(quantity) if quantity > 0 -> errors
      _ ->
        list.append(errors, [
          ProductUserError(
            list.append(field_prefix, [int.to_string(index), "quantity"]),
            "Quantity must be greater than 0.",
            Some("INVALID"),
          ),
        ])
    }
  })
}

@internal
pub fn inventory_shipment_tracking_from_input(
  input: Dict(String, ResolvedValue),
) -> Option(InventoryShipmentTrackingRecord) {
  case dict.get(input, "trackingInput") {
    Ok(ObjectVal(tracking)) -> inventory_shipment_tracking_from_fields(tracking)
    _ -> None
  }
}

@internal
pub fn inventory_shipment_tracking_from_argument(
  args: Dict(String, ResolvedValue),
) -> Option(InventoryShipmentTrackingRecord) {
  case dict.get(args, "tracking") {
    Ok(ObjectVal(tracking)) -> inventory_shipment_tracking_from_fields(tracking)
    _ -> None
  }
}

@internal
pub fn apply_inventory_shipment_receive_inputs(
  shipment: InventoryShipmentRecord,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(
  List(InventoryShipmentLineItemRecord),
  List(ProductUserError),
  List(InventoryShipmentDelta),
) {
  let initial = #(shipment.line_items, [], [])
  list.fold(enumerate_items(inputs), initial, fn(acc, pair) {
    let #(line_items, errors, deltas) = acc
    let #(input, index) = pair
    let line_item_id = read_string_field(input, "shipmentLineItemId")
    let quantity = read_int_field(input, "quantity")
    let reason = read_string_field(input, "reason")
    let current =
      option.then(line_item_id, fn(id) {
        find_inventory_shipment_line_item(shipment.line_items, id)
      })
    case current, line_item_id, quantity, reason {
      None, _, _, _ -> #(
        line_items,
        list.append(errors, [
          ProductUserError(
            ["lineItems", int.to_string(index), "shipmentLineItemId"],
            "Shipment line item could not be found.",
            Some("NOT_FOUND"),
          ),
        ]),
        deltas,
      )
      Some(current), Some(id), Some(quantity), Some(reason) -> {
        let valid_quantity =
          quantity > 0 && quantity <= shipment_line_item_unreceived(current)
        let valid_reason = reason == "ACCEPTED" || reason == "REJECTED"
        case valid_quantity, valid_reason {
          True, True -> {
            let next_line_items =
              line_items
              |> list.map(fn(line_item) {
                case line_item.id == id {
                  True ->
                    case reason {
                      "ACCEPTED" ->
                        InventoryShipmentLineItemRecord(
                          ..line_item,
                          accepted_quantity: line_item.accepted_quantity
                            + quantity,
                        )
                      _ ->
                        InventoryShipmentLineItemRecord(
                          ..line_item,
                          rejected_quantity: line_item.rejected_quantity
                            + quantity,
                        )
                    }
                  False -> line_item
                }
              })
            let delta =
              InventoryShipmentDelta(
                inventory_item_id: current.inventory_item_id,
                incoming: 0 - quantity,
                available: case reason {
                  "ACCEPTED" -> Some(quantity)
                  _ -> None
                },
              )
            #(next_line_items, errors, list.append(deltas, [delta]))
          }
          False, _ -> #(
            line_items,
            list.append(errors, [
              ProductUserError(
                ["lineItems", int.to_string(index), "quantity"],
                "Quantity must be greater than 0 and no more than the unreceived quantity.",
                Some("INVALID"),
              ),
            ]),
            deltas,
          )
          _, False -> #(
            line_items,
            list.append(errors, [
              ProductUserError(
                ["lineItems", int.to_string(index), "reason"],
                "Receive reason is required.",
                Some("BLANK"),
              ),
            ]),
            deltas,
          )
        }
      }
      Some(_), _, _, None -> #(
        line_items,
        list.append(errors, [
          ProductUserError(
            ["lineItems", int.to_string(index), "reason"],
            "Receive reason is required.",
            Some("BLANK"),
          ),
        ]),
        deltas,
      )
      _, _, _, _ -> #(
        line_items,
        list.append(errors, [
          ProductUserError(
            ["lineItems", int.to_string(index), "quantity"],
            "Quantity must be greater than 0 and no more than the unreceived quantity.",
            Some("INVALID"),
          ),
        ]),
        deltas,
      )
    }
  })
}

@internal
pub fn apply_inventory_shipment_quantity_updates(
  shipment: InventoryShipmentRecord,
  updates: List(Dict(String, ResolvedValue)),
) -> #(
  List(InventoryShipmentLineItemRecord),
  List(InventoryShipmentLineItemRecord),
  List(ProductUserError),
  List(InventoryShipmentDelta),
) {
  let initial = #(shipment.line_items, [], [], [])
  list.fold(enumerate_items(updates), initial, fn(acc, pair) {
    let #(line_items, updated, errors, deltas) = acc
    let #(input, index) = pair
    let line_item_id = read_string_field(input, "shipmentLineItemId")
    let quantity = read_int_field(input, "quantity")
    let current =
      option.then(line_item_id, fn(id) {
        find_inventory_shipment_line_item(line_items, id)
      })
    case current, line_item_id, quantity {
      Some(current), Some(id), Some(quantity)
        if quantity >= current.accepted_quantity + current.rejected_quantity
      -> {
        let incoming_delta = case shipment_has_unreceived_incoming(shipment) {
          True -> quantity - current.quantity
          False -> 0
        }
        let next_line_item =
          InventoryShipmentLineItemRecord(..current, quantity: quantity)
        let next_line_items =
          line_items
          |> list.map(fn(line_item) {
            case line_item.id == id {
              True -> next_line_item
              False -> line_item
            }
          })
        let next_deltas = case incoming_delta == 0 {
          True -> deltas
          False ->
            list.append(deltas, [
              InventoryShipmentDelta(
                inventory_item_id: current.inventory_item_id,
                incoming: incoming_delta,
                available: None,
              ),
            ])
        }
        #(
          next_line_items,
          list.append(updated, [next_line_item]),
          errors,
          next_deltas,
        )
      }
      None, _, _ -> #(
        line_items,
        updated,
        list.append(errors, [
          ProductUserError(
            ["items", int.to_string(index), "shipmentLineItemId"],
            "Shipment line item could not be found.",
            Some("NOT_FOUND"),
          ),
        ]),
        deltas,
      )
      _, _, _ -> #(
        line_items,
        updated,
        list.append(errors, [
          ProductUserError(
            ["items", int.to_string(index), "quantity"],
            "Quantity cannot be less than received quantity.",
            Some("INVALID"),
          ),
        ]),
        deltas,
      )
    }
  })
}

@internal
pub fn inventory_shipment_delete_payload(
  id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryShipmentDeletePayload")),
      #("id", graphql_helpers.option_string_source(id)),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}
