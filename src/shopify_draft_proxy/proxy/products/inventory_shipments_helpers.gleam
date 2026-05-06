//// Products-domain submodule: inventory_shipments_helpers.
//// Combines layered files: inventory_shipments_l00, inventory_shipments_l01, inventory_shipments_l02.

import gleam/dict.{type Dict}

import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string

import shopify_draft_proxy/graphql/ast.{type Selection}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue, ObjectVal}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcNull, SrcString,
  default_selected_field_options, get_selected_child_fields,
  project_graphql_value, src_object,
}

import shopify_draft_proxy/proxy/products/inventory_core.{
  inventory_item_legacy_id,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type InventoryShipmentDelta, type ProductUserError, InventoryShipmentDelta,
  ProductUserError,
}
import shopify_draft_proxy/proxy/products/products_core.{enumerate_items}
import shopify_draft_proxy/proxy/products/shared.{
  read_int_field, read_string_field, user_errors_source,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type InventoryLevelRecord, type InventoryShipmentLineItemRecord,
  type InventoryShipmentRecord, type InventoryShipmentTrackingRecord,
  type InventoryTransferLineItemRecord, type InventoryTransferRecord,
  type ProductVariantRecord, InventoryLevelRecord, InventoryLocationRecord,
  InventoryQuantityRecord, InventoryShipmentLineItemRecord,
  InventoryShipmentTrackingRecord,
}

// ===== from inventory_shipments_l00 =====
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

// ===== from inventory_shipments_l01 =====
@internal
pub fn shipment_total_received(shipment: InventoryShipmentRecord) -> Int {
  shipment_total_accepted(shipment) + shipment_total_rejected(shipment)
}

@internal
pub fn make_inventory_shipment_line_items(
  identity: SyntheticIdentityRegistry,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(InventoryShipmentLineItemRecord), SyntheticIdentityRegistry) {
  let #(reversed, next_identity) =
    list.fold(inputs, #([], identity), fn(acc, input) {
      let #(records, current_identity) = acc
      let #(id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(
          current_identity,
          "InventoryShipmentLineItem",
        )
      let assert Some(inventory_item_id) =
        read_string_field(input, "inventoryItemId")
      let assert Some(quantity) = read_int_field(input, "quantity")
      #(
        [
          InventoryShipmentLineItemRecord(
            id: id,
            inventory_item_id: inventory_item_id,
            quantity: quantity,
            accepted_quantity: 0,
            rejected_quantity: 0,
          ),
          ..records
        ],
        identity_after_id,
      )
    })
  #(list.reverse(reversed), next_identity)
}

@internal
pub fn inventory_shipment_tracking_from_fields(
  tracking: Dict(String, ResolvedValue),
) -> Option(InventoryShipmentTrackingRecord) {
  let company = case read_string_field(tracking, "company") {
    Some(company) -> Some(company)
    None -> read_string_field(tracking, "carrier")
  }
  let tracking_url = case read_string_field(tracking, "trackingUrl") {
    Some(url) -> Some(url)
    None -> read_string_field(tracking, "url")
  }
  Some(InventoryShipmentTrackingRecord(
    tracking_number: read_string_field(tracking, "trackingNumber"),
    company: company,
    tracking_url: tracking_url,
    arrives_at: read_string_field(tracking, "arrivesAt"),
  ))
}

@internal
pub fn inventory_shipment_not_found_error() -> ProductUserError {
  ProductUserError(
    ["id"],
    "The specified inventory shipment could not be found.",
    Some("NOT_FOUND"),
  )
}

@internal
pub fn inventory_shipment_invalid_state_error(
  field: List(String),
  message: String,
) -> ProductUserError {
  ProductUserError(field, message, Some("INVALID_STATE"))
}

@internal
pub fn inventory_shipment_transfer_not_found_error() -> ProductUserError {
  ProductUserError(
    ["transferId"],
    "The specified inventory transfer could not be found.",
    Some("NOT_FOUND"),
  )
}

@internal
pub fn inventory_shipment_transfer_invalid_state_error() -> ProductUserError {
  inventory_shipment_invalid_state_error(
    ["transferId"],
    "Inventory shipments can only be created for open or ready to ship transfers.",
  )
}

@internal
pub fn default_shipment_inventory_level(
  variant: ProductVariantRecord,
  inventory_item_id: String,
) -> InventoryLevelRecord {
  let available = variant.inventory_quantity |> option.unwrap(0)
  InventoryLevelRecord(
    id: "gid://shopify/InventoryLevel/"
      <> inventory_item_legacy_id(inventory_item_id)
      <> "?inventory_item_id="
      <> inventory_item_id,
    location: InventoryLocationRecord(
      id: "gid://shopify/Location/1",
      name: "Default location",
    ),
    quantities: [
      InventoryQuantityRecord(
        name: "available",
        quantity: available,
        updated_at: None,
      ),
      InventoryQuantityRecord(
        name: "on_hand",
        quantity: available,
        updated_at: None,
      ),
      InventoryQuantityRecord(name: "incoming", quantity: 0, updated_at: None),
    ],
    is_active: Some(True),
    cursor: None,
  )
}

// ===== from inventory_shipments_l02 =====
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
pub fn read_inventory_shipment_transfer_id(
  input: Dict(String, ResolvedValue),
) -> Option(String) {
  case read_inventory_shipment_explicit_transfer_id(input) {
    Some(id) -> Some(id)
    None -> read_string_field(input, "movementId")
  }
}

@internal
pub fn read_inventory_shipment_explicit_transfer_id(
  input: Dict(String, ResolvedValue),
) -> Option(String) {
  case read_string_field(input, "transferId") {
    Some(id) -> Some(id)
    None -> read_string_field(input, "inventoryTransferId")
  }
}

@internal
pub fn inventory_shipment_transfer_allows_shipping(
  transfer: InventoryTransferRecord,
) -> Bool {
  transfer.status == "OPEN" || transfer.status == "READY_TO_SHIP"
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
pub fn find_inventory_transfer_line_item_by_inventory_item_id(
  line_items: List(InventoryTransferLineItemRecord),
  inventory_item_id: String,
) -> Option(InventoryTransferLineItemRecord) {
  line_items
  |> list.find(fn(line_item) {
    line_item.inventory_item_id == inventory_item_id
  })
  |> option.from_result
}

fn find_transfer_line_for_shipment_input(
  transfer: InventoryTransferRecord,
  input: Dict(String, ResolvedValue),
) -> Option(InventoryTransferLineItemRecord) {
  case read_string_field(input, "inventoryTransferLineItemId") {
    Some(id) -> find_inventory_transfer_line_item(transfer.line_items, id)
    None ->
      case read_string_field(input, "inventoryItemId") {
        Some(inventory_item_id) ->
          find_inventory_transfer_line_item_by_inventory_item_id(
            transfer.line_items,
            inventory_item_id,
          )
        None -> None
      }
  }
}

fn shipment_line_quantity_for_transfer_line(
  line_item: InventoryShipmentLineItemRecord,
  transfer_line: InventoryTransferLineItemRecord,
  excluded_shipment_line_item_id: Option(String),
) -> Int {
  case
    line_item.inventory_item_id == transfer_line.inventory_item_id,
    excluded_shipment_line_item_id
  {
    False, _ -> 0
    True, Some(excluded_id) if line_item.id == excluded_id -> 0
    True, _ -> line_item.quantity
  }
}

@internal
pub fn inventory_shipment_shipped_quantity_for_transfer_line(
  store: Store,
  transfer: InventoryTransferRecord,
  transfer_line: InventoryTransferLineItemRecord,
  excluded_shipment_line_item_id: Option(String),
) -> Int {
  store.list_effective_inventory_shipments(store)
  |> list.filter(fn(shipment) { shipment.movement_id == transfer.id })
  |> list.fold(0, fn(total, shipment) {
    total
    + list.fold(shipment.line_items, 0, fn(line_total, line_item) {
      line_total
      + shipment_line_quantity_for_transfer_line(
        line_item,
        transfer_line,
        excluded_shipment_line_item_id,
      )
    })
  })
}

@internal
pub fn inventory_shipment_remaining_quantity_for_transfer_line(
  store: Store,
  transfer: InventoryTransferRecord,
  transfer_line: InventoryTransferLineItemRecord,
  excluded_shipment_line_item_id: Option(String),
) -> Int {
  int.max(
    0,
    transfer_line.total_quantity
      - inventory_shipment_shipped_quantity_for_transfer_line(
      store,
      transfer,
      transfer_line,
      excluded_shipment_line_item_id,
    ),
  )
}

@internal
pub fn validate_inventory_shipment_transfer_line_item_inputs(
  store: Store,
  transfer: InventoryTransferRecord,
  line_items: List(Dict(String, ResolvedValue)),
  field_prefix: List(String),
) -> List(ProductUserError) {
  list.fold(enumerate_items(line_items), [], fn(errors, pair) {
    let #(input, index) = pair
    let path = list.append(field_prefix, [int.to_string(index)])
    let quantity = read_int_field(input, "quantity")
    case find_transfer_line_for_shipment_input(transfer, input) {
      None ->
        list.append(errors, [
          ProductUserError(
            list.append(path, ["inventoryTransferLineItemId"]),
            "The specified inventory transfer line item could not be found.",
            Some("NOT_FOUND"),
          ),
        ])
      Some(transfer_line) -> {
        let remaining =
          inventory_shipment_remaining_quantity_for_transfer_line(
            store,
            transfer,
            transfer_line,
            None,
          )
        case quantity {
          Some(quantity) if quantity <= remaining -> errors
          _ ->
            list.append(errors, [
              ProductUserError(
                list.append(path, ["quantity"]),
                "Quantity exceeds the remaining quantity for the inventory transfer line item.",
                Some("QUANTITY_EXCEEDS_REMAINING"),
              ),
            ])
        }
      }
    }
  })
}

@internal
pub fn validate_inventory_shipment_quantity_updates_against_transfer(
  store: Store,
  transfer: InventoryTransferRecord,
  shipment: InventoryShipmentRecord,
  updates: List(Dict(String, ResolvedValue)),
) -> List(ProductUserError) {
  list.fold(enumerate_items(updates), [], fn(errors, pair) {
    let #(input, index) = pair
    let shipment_line_item_id = read_string_field(input, "shipmentLineItemId")
    let quantity = read_int_field(input, "quantity")
    let current =
      option.then(shipment_line_item_id, fn(id) {
        find_inventory_shipment_line_item(shipment.line_items, id)
      })
    case current, quantity {
      Some(line_item), Some(quantity) -> {
        case
          find_inventory_transfer_line_item_by_inventory_item_id(
            transfer.line_items,
            line_item.inventory_item_id,
          )
        {
          None ->
            list.append(errors, [
              ProductUserError(
                ["items", int.to_string(index), "shipmentLineItemId"],
                "The shipment line item does not belong to the inventory transfer.",
                Some("NOT_FOUND"),
              ),
            ])
          Some(transfer_line) -> {
            let remaining =
              inventory_shipment_remaining_quantity_for_transfer_line(
                store,
                transfer,
                transfer_line,
                Some(line_item.id),
              )
            case quantity <= remaining {
              True -> errors
              False ->
                list.append(errors, [
                  ProductUserError(
                    ["items", int.to_string(index), "quantity"],
                    "Quantity exceeds the remaining quantity for the inventory transfer line item.",
                    Some("QUANTITY_EXCEEDS_REMAINING"),
                  ),
                ])
            }
          }
        }
      }
      _, _ -> errors
    }
  })
}

fn is_allowed_tracking_carrier(carrier: String) -> Bool {
  list.contains(
    [
      "UPS",
      "USPS",
      "FEDEX",
      "DHL",
      "DHL_EXPRESS",
      "CANADA_POST",
      "OTHER",
    ],
    carrier,
  )
}

fn is_valid_tracking_url(url: String) -> Bool {
  string.starts_with(url, "https://") || string.starts_with(url, "http://")
}

@internal
pub fn validate_inventory_shipment_tracking_fields(
  tracking: Dict(String, ResolvedValue),
  field_prefix: List(String),
) -> List(ProductUserError) {
  let carrier_errors = case read_string_field(tracking, "carrier") {
    Some(carrier) ->
      case is_allowed_tracking_carrier(carrier) {
        True -> []
        False -> [
          ProductUserError(
            list.append(field_prefix, ["carrier"]),
            "Carrier is not included in the list.",
            Some("INVALID"),
          ),
        ]
      }
    None -> []
  }
  let tracking_url = case read_string_field(tracking, "url") {
    Some(url) -> Some(url)
    None -> read_string_field(tracking, "trackingUrl")
  }
  let url_errors = case tracking_url {
    Some(url) ->
      case is_valid_tracking_url(url) {
        True -> []
        False -> [
          ProductUserError(
            list.append(field_prefix, ["url"]),
            "Tracking URL is invalid.",
            Some("INVALID"),
          ),
        ]
      }
    None -> []
  }
  list.append(carrier_errors, url_errors)
}

@internal
pub fn validate_inventory_shipment_tracking_from_input(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  case dict.get(input, "trackingInput") {
    Ok(ObjectVal(tracking)) ->
      validate_inventory_shipment_tracking_fields(tracking, [
        "input",
        "trackingInput",
      ])
    _ -> []
  }
}

@internal
pub fn validate_inventory_shipment_tracking_from_argument(
  args: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  case dict.get(args, "tracking") {
    Ok(ObjectVal(tracking)) ->
      validate_inventory_shipment_tracking_fields(tracking, ["tracking"])
    _ -> []
  }
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
