//// Products-domain submodule: inventory_shipments_handlers.
//// Combines layered files: inventory_shipments_l06, inventory_shipments_l07, inventory_shipments_l09, inventory_shipments_l10, inventory_shipments_l11, inventory_shipments_l12, inventory_shipments_l13, inventory_shipments_l14.

import gleam/dict.{type Dict}

import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcInt, SrcList, SrcNull, SrcString,
  default_selected_field_options, get_field_response_key,
  get_selected_child_fields, project_graphql_value, src_object,
}

import shopify_draft_proxy/proxy/products/inventory_apply.{
  adjust_inventory_item_quantities,
}
import shopify_draft_proxy/proxy/products/inventory_core.{
  active_inventory_levels,
}
import shopify_draft_proxy/proxy/products/inventory_handlers.{
  product_variant_source_without_inventory,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_helpers.{
  apply_inventory_shipment_quantity_updates,
  apply_inventory_shipment_receive_inputs, find_inventory_shipment_line_item,
  inventory_shipment_delete_payload, inventory_shipment_not_found_error,
  inventory_shipment_status_after_receive,
  inventory_shipment_tracking_from_argument,
  inventory_shipment_tracking_from_input, inventory_shipment_tracking_source,
  make_inventory_shipment_line_items, shipment_has_unreceived_incoming,
  shipment_line_item_total, shipment_line_item_unreceived,
  shipment_total_accepted, shipment_total_received, shipment_total_rejected,
  validate_inventory_shipment_line_item_inputs,
}
import shopify_draft_proxy/proxy/products/inventory_validation.{
  inventory_levels_connection_source, optional_measurement_source,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type InventoryShipmentDelta, type MutationFieldResult, type ProductUserError,
  InventoryShipmentDelta, ProductUserError,
}
import shopify_draft_proxy/proxy/products/products_core.{enumerate_items}
import shopify_draft_proxy/proxy/products/shared.{
  connection_page_info_source, count_source, mutation_result,
  read_arg_object_list, read_arg_string_list, read_object_list_field,
  read_string_argument, read_string_field, user_errors_source,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type InventoryShipmentLineItemRecord, type InventoryShipmentRecord,
  type ProductVariantRecord, InventoryShipmentRecord,
}

// ===== from inventory_shipments_l06 =====
@internal
pub fn apply_inventory_shipment_deltas(
  store: Store,
  identity: SyntheticIdentityRegistry,
  deltas: List(InventoryShipmentDelta),
) -> #(Store, SyntheticIdentityRegistry) {
  list.fold(deltas, #(store, identity), fn(acc, delta) {
    let #(current_store, current_identity) = acc
    adjust_inventory_item_quantities(
      current_store,
      current_identity,
      delta.inventory_item_id,
      delta.incoming,
      delta.available,
    )
  })
}

// ===== from inventory_shipments_l07 =====
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

// ===== from inventory_shipments_l09 =====
@internal
pub fn shipment_inventory_item_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  case variant.inventory_item {
    Some(item) ->
      src_object([
        #("__typename", SrcString("InventoryItem")),
        #("id", SrcString(item.id)),
        #("sku", graphql_helpers.option_string_source(variant.sku)),
        #("tracked", graphql_helpers.option_bool_source(item.tracked)),
        #(
          "requiresShipping",
          graphql_helpers.option_bool_source(item.requires_shipping),
        ),
        #("measurement", optional_measurement_source(item.measurement)),
        #(
          "countryCodeOfOrigin",
          graphql_helpers.option_string_source(item.country_code_of_origin),
        ),
        #(
          "provinceCodeOfOrigin",
          graphql_helpers.option_string_source(item.province_code_of_origin),
        ),
        #(
          "harmonizedSystemCode",
          graphql_helpers.option_string_source(item.harmonized_system_code),
        ),
        #(
          "inventoryLevels",
          inventory_levels_connection_source(active_inventory_levels(
            item.inventory_levels,
          )),
        ),
        #("variant", product_variant_source_without_inventory(store, variant)),
      ])
    None -> SrcNull
  }
}

// ===== from inventory_shipments_l10 =====
@internal
pub fn inventory_shipment_line_item_source(
  store: Store,
  line_item: InventoryShipmentLineItemRecord,
) -> SourceValue {
  let inventory_item = case
    store.find_effective_variant_by_inventory_item_id(
      store,
      line_item.inventory_item_id,
    )
  {
    Some(variant) -> shipment_inventory_item_source(store, variant)
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("InventoryShipmentLineItem")),
    #("id", SrcString(line_item.id)),
    #("quantity", SrcInt(line_item.quantity)),
    #("acceptedQuantity", SrcInt(line_item.accepted_quantity)),
    #("rejectedQuantity", SrcInt(line_item.rejected_quantity)),
    #("unreceivedQuantity", SrcInt(shipment_line_item_unreceived(line_item))),
    #("inventoryItem", inventory_item),
  ])
}

// ===== from inventory_shipments_l11 =====
@internal
pub fn inventory_shipment_line_items_source(
  store: Store,
  shipment: InventoryShipmentRecord,
) -> SourceValue {
  let edges =
    shipment.line_items
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(line_item, _) = pair
      src_object([
        #("cursor", SrcString(line_item.id)),
        #("node", inventory_shipment_line_item_source(store, line_item)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(shipment.line_items, fn(line_item) {
          inventory_shipment_line_item_source(store, line_item)
        }),
      ),
    ),
    #(
      "pageInfo",
      connection_page_info_source(shipment.line_items, fn(line_item, _index) {
        line_item.id
      }),
    ),
  ])
}

// ===== from inventory_shipments_l12 =====
@internal
pub fn inventory_shipment_source(
  store: Store,
  shipment: InventoryShipmentRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("InventoryShipment")),
    #("id", SrcString(shipment.id)),
    #("movementId", SrcString(shipment.movement_id)),
    #("name", SrcString(shipment.name)),
    #("status", SrcString(shipment.status)),
    #("createdAt", SrcString(shipment.created_at)),
    #("updatedAt", SrcString(shipment.updated_at)),
    #("lineItemTotalQuantity", SrcInt(shipment_line_item_total(shipment))),
    #("totalAcceptedQuantity", SrcInt(shipment_total_accepted(shipment))),
    #("totalReceivedQuantity", SrcInt(shipment_total_received(shipment))),
    #("totalRejectedQuantity", SrcInt(shipment_total_rejected(shipment))),
    #("tracking", inventory_shipment_tracking_source(shipment.tracking)),
    #("lineItems", inventory_shipment_line_items_source(store, shipment)),
    #("lineItemsCount", count_source(list.length(shipment.line_items))),
  ])
}

// ===== from inventory_shipments_l13 =====
@internal
pub fn serialize_inventory_shipment_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_inventory_shipment_by_id(store, id) {
        Some(shipment) ->
          project_graphql_value(
            inventory_shipment_source(store, shipment),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn inventory_shipment_create_payload(
  store: Store,
  typename: String,
  shipment: Option(InventoryShipmentRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let shipment_value = case shipment {
    Some(record) -> inventory_shipment_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("inventoryShipment", shipment_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn inventory_shipment_receive_payload(
  store: Store,
  shipment: Option(InventoryShipmentRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let shipment_value = case shipment {
    Some(record) -> inventory_shipment_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryShipmentReceivePayload")),
      #("inventoryShipment", shipment_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn inventory_shipment_payload(
  store: Store,
  typename: String,
  shipment: Option(InventoryShipmentRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let shipment_value = case shipment {
    Some(record) -> inventory_shipment_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("inventoryShipment", shipment_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn inventory_shipment_add_items_payload(
  store: Store,
  shipment: Option(InventoryShipmentRecord),
  added_items: List(InventoryShipmentLineItemRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let shipment_value = case shipment {
    Some(record) -> inventory_shipment_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryShipmentAddItemsPayload")),
      #(
        "addedItems",
        SrcList(
          list.map(added_items, fn(line_item) {
            inventory_shipment_line_item_source(store, line_item)
          }),
        ),
      ),
      #("inventoryShipment", shipment_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn inventory_shipment_update_item_quantities_payload(
  store: Store,
  shipment: Option(InventoryShipmentRecord),
  updated_line_items: List(InventoryShipmentLineItemRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let shipment_value = case shipment {
    Some(record) -> inventory_shipment_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryShipmentUpdateItemQuantitiesPayload")),
      #("shipment", shipment_value),
      #(
        "updatedLineItems",
        SrcList(
          list.map(updated_line_items, fn(line_item) {
            inventory_shipment_line_item_source(store, line_item)
          }),
        ),
      ),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

// ===== from inventory_shipments_l14 =====
@internal
pub fn handle_inventory_shipment_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let status = case root_name {
    "inventoryShipmentCreateInTransit" -> "IN_TRANSIT"
    _ -> "DRAFT"
  }
  let typename = case root_name {
    "inventoryShipmentCreateInTransit" ->
      "InventoryShipmentCreateInTransitPayload"
    _ -> "InventoryShipmentCreatePayload"
  }
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let movement_id = read_string_field(input, "movementId")
  let line_item_inputs = read_object_list_field(input, "lineItems")
  let user_errors =
    validate_inventory_shipment_line_item_inputs(store, line_item_inputs, [
      "input",
      "lineItems",
    ])
  let user_errors = case movement_id {
    Some(_) -> user_errors
    None ->
      list.append(user_errors, [
        ProductUserError(
          ["input", "movementId"],
          "Movement id is required.",
          Some("BLANK"),
        ),
      ])
  }
  case user_errors, movement_id {
    [], Some(movement_id) -> {
      let #(now, identity_after_timestamp) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let #(shipment_id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(
          identity_after_timestamp,
          "InventoryShipment",
        )
      let #(line_items, identity_after_line_items) =
        make_inventory_shipment_line_items(identity_after_id, line_item_inputs)
      let shipment =
        InventoryShipmentRecord(
          id: shipment_id,
          movement_id: movement_id,
          name: "#S"
            <> int.to_string(
            list.length(store.list_effective_inventory_shipments(store)) + 1,
          ),
          status: status,
          created_at: now,
          updated_at: now,
          tracking: inventory_shipment_tracking_from_input(input),
          line_items: line_items,
        )
      let #(staged_shipment, staged_store, next_identity) = case status {
        "IN_TRANSIT" ->
          stage_inventory_shipment_with_incoming(
            store,
            identity_after_line_items,
            shipment,
          )
        _ -> {
          let #(staged, staged_store) =
            store.upsert_staged_inventory_shipment(store, shipment)
          #(staged, staged_store, identity_after_line_items)
        }
      }
      mutation_result(
        key,
        inventory_shipment_create_payload(
          staged_store,
          typename,
          Some(staged_shipment),
          [],
          field,
          fragments,
        ),
        staged_store,
        next_identity,
        [
          staged_shipment.id,
          ..list.map(staged_shipment.line_items, fn(item) { item.id })
        ],
      )
    }
    _, _ ->
      mutation_result(
        key,
        inventory_shipment_create_payload(
          store,
          typename,
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

@internal
pub fn handle_inventory_shipment_set_tracking(
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
        inventory_shipment_payload(
          store,
          "InventoryShipmentSetTrackingPayload",
          None,
          [inventory_shipment_not_found_error()],
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
            "Received shipments cannot be updated.",
            Some("INVALID_STATUS"),
          ),
        ]
        False -> []
      }
      case user_errors {
        [] -> {
          let #(now, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let next_shipment =
            InventoryShipmentRecord(
              ..shipment,
              tracking: inventory_shipment_tracking_from_argument(args),
              updated_at: now,
            )
          let #(staged, staged_store) =
            store.upsert_staged_inventory_shipment(store, next_shipment)
          mutation_result(
            key,
            inventory_shipment_payload(
              staged_store,
              "InventoryShipmentSetTrackingPayload",
              Some(staged),
              [],
              field,
              fragments,
            ),
            staged_store,
            next_identity,
            [staged.id],
          )
        }
        _ ->
          mutation_result(
            key,
            inventory_shipment_payload(
              store,
              "InventoryShipmentSetTrackingPayload",
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
pub fn handle_inventory_shipment_mark_in_transit(
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
        inventory_shipment_payload(
          store,
          "InventoryShipmentMarkInTransitPayload",
          None,
          [inventory_shipment_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(shipment) ->
      case shipment.status == "DRAFT" {
        False ->
          mutation_result(
            key,
            inventory_shipment_payload(
              store,
              "InventoryShipmentMarkInTransitPayload",
              None,
              [
                ProductUserError(
                  ["id"],
                  "Only draft shipments can be marked in transit.",
                  Some("INVALID_STATUS"),
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        True -> {
          let #(now, identity_after_timestamp) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let next_shipment =
            InventoryShipmentRecord(
              ..shipment,
              status: "IN_TRANSIT",
              updated_at: now,
            )
          let #(staged, staged_store, next_identity) =
            stage_inventory_shipment_with_incoming(
              store,
              identity_after_timestamp,
              next_shipment,
            )
          mutation_result(
            key,
            inventory_shipment_payload(
              staged_store,
              "InventoryShipmentMarkInTransitPayload",
              Some(staged),
              [],
              field,
              fragments,
            ),
            staged_store,
            next_identity,
            [staged.id],
          )
        }
      }
  }
}

@internal
pub fn handle_inventory_shipment_add_items(
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
        inventory_shipment_add_items_payload(
          store,
          None,
          [],
          [inventory_shipment_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(shipment) -> {
      let line_item_inputs = read_arg_object_list(args, "lineItems")
      let user_errors = case shipment.status == "RECEIVED" {
        True -> [
          ProductUserError(
            ["id"],
            "Received shipments cannot be updated.",
            Some("INVALID_STATUS"),
          ),
        ]
        False ->
          validate_inventory_shipment_line_item_inputs(store, line_item_inputs, [
            "lineItems",
          ])
      }
      case user_errors {
        [] -> {
          let #(now, identity_after_timestamp) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let #(added_items, identity_after_items) =
            make_inventory_shipment_line_items(
              identity_after_timestamp,
              line_item_inputs,
            )
          let next_shipment =
            InventoryShipmentRecord(
              ..shipment,
              updated_at: now,
              line_items: list.append(shipment.line_items, added_items),
            )
          let deltas = case shipment_has_unreceived_incoming(shipment) {
            True ->
              added_items
              |> list.map(fn(line_item) {
                InventoryShipmentDelta(
                  inventory_item_id: line_item.inventory_item_id,
                  incoming: line_item.quantity,
                  available: None,
                )
              })
            False -> []
          }
          let #(next_store, next_identity) =
            apply_inventory_shipment_deltas(store, identity_after_items, deltas)
          let #(staged, staged_store) =
            store.upsert_staged_inventory_shipment(next_store, next_shipment)
          mutation_result(
            key,
            inventory_shipment_add_items_payload(
              staged_store,
              Some(staged),
              added_items,
              [],
              field,
              fragments,
            ),
            staged_store,
            next_identity,
            [staged.id, ..list.map(added_items, fn(item) { item.id })],
          )
        }
        _ ->
          mutation_result(
            key,
            inventory_shipment_add_items_payload(
              store,
              None,
              [],
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
pub fn handle_inventory_shipment_remove_items(
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
        inventory_shipment_payload(
          store,
          "InventoryShipmentRemoveItemsPayload",
          None,
          [inventory_shipment_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(shipment) -> {
      let ids = read_arg_string_list(args, "lineItems")
      let has_unknown =
        list.any(ids, fn(id) {
          find_inventory_shipment_line_item(shipment.line_items, id) == None
        })
      let user_errors = case has_unknown, shipment.status == "RECEIVED" {
        True, _ -> [
          ProductUserError(
            ["lineItems"],
            "One or more shipment line items could not be found.",
            Some("NOT_FOUND"),
          ),
        ]
        _, True -> [
          ProductUserError(
            ["id"],
            "Received shipments cannot be updated.",
            Some("INVALID_STATUS"),
          ),
        ]
        _, _ -> []
      }
      case user_errors {
        [] -> {
          let #(now, identity_after_timestamp) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let removed =
            shipment.line_items
            |> list.filter(fn(line_item) { list.contains(ids, line_item.id) })
          let remaining =
            shipment.line_items
            |> list.filter(fn(line_item) { !list.contains(ids, line_item.id) })
          let next_shipment =
            InventoryShipmentRecord(
              ..shipment,
              updated_at: now,
              line_items: remaining,
            )
          let deltas = case shipment_has_unreceived_incoming(shipment) {
            True ->
              removed
              |> list.map(fn(line_item) {
                InventoryShipmentDelta(
                  inventory_item_id: line_item.inventory_item_id,
                  incoming: 0 - shipment_line_item_unreceived(line_item),
                  available: None,
                )
              })
            False -> []
          }
          let #(next_store, next_identity) =
            apply_inventory_shipment_deltas(
              store,
              identity_after_timestamp,
              deltas,
            )
          let #(staged, staged_store) =
            store.upsert_staged_inventory_shipment(next_store, next_shipment)
          mutation_result(
            key,
            inventory_shipment_payload(
              staged_store,
              "InventoryShipmentRemoveItemsPayload",
              Some(staged),
              [],
              field,
              fragments,
            ),
            staged_store,
            next_identity,
            [staged.id],
          )
        }
        _ ->
          mutation_result(
            key,
            inventory_shipment_payload(
              store,
              "InventoryShipmentRemoveItemsPayload",
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
pub fn handle_inventory_shipment_receive(
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
        inventory_shipment_receive_payload(
          store,
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
      let receive_inputs = read_arg_object_list(args, "lineItems")
      let #(next_line_items, user_errors, inventory_deltas) =
        apply_inventory_shipment_receive_inputs(shipment, receive_inputs)
      case user_errors {
        [] -> {
          let #(now, identity_after_timestamp) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let next_shipment =
            InventoryShipmentRecord(
              ..shipment,
              status: inventory_shipment_status_after_receive(next_line_items),
              updated_at: now,
              line_items: next_line_items,
            )
          let #(next_store, next_identity) =
            apply_inventory_shipment_deltas(
              store,
              identity_after_timestamp,
              inventory_deltas,
            )
          let #(staged, staged_store) =
            store.upsert_staged_inventory_shipment(next_store, next_shipment)
          mutation_result(
            key,
            inventory_shipment_receive_payload(
              staged_store,
              Some(staged),
              [],
              field,
              fragments,
            ),
            staged_store,
            next_identity,
            [staged.id],
          )
        }
        _ ->
          mutation_result(
            key,
            inventory_shipment_receive_payload(
              store,
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
pub fn handle_inventory_shipment_update_item_quantities(
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
        inventory_shipment_update_item_quantities_payload(
          store,
          None,
          [],
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
      let updates = read_arg_object_list(args, "items")
      let #(next_line_items, updated_line_items, user_errors, deltas) =
        apply_inventory_shipment_quantity_updates(shipment, updates)
      case user_errors {
        [] -> {
          let #(now, identity_after_timestamp) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let next_shipment =
            InventoryShipmentRecord(
              ..shipment,
              updated_at: now,
              line_items: next_line_items,
            )
          let #(next_store, next_identity) =
            apply_inventory_shipment_deltas(
              store,
              identity_after_timestamp,
              deltas,
            )
          let #(staged, staged_store) =
            store.upsert_staged_inventory_shipment(next_store, next_shipment)
          mutation_result(
            key,
            inventory_shipment_update_item_quantities_payload(
              staged_store,
              Some(staged),
              updated_line_items,
              [],
              field,
              fragments,
            ),
            staged_store,
            next_identity,
            [staged.id],
          )
        }
        _ ->
          mutation_result(
            key,
            inventory_shipment_update_item_quantities_payload(
              store,
              None,
              [],
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
