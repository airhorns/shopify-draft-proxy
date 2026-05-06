//// Products-domain submodule: inventory_transfers.
//// Combines layered files: inventory_transfers_l00, inventory_transfers_l01, inventory_transfers_l02, inventory_transfers_l03, inventory_transfers_l05, inventory_transfers_l06, inventory_transfers_l07, inventory_transfers_l10, inventory_transfers_l11, inventory_transfers_l12, inventory_transfers_l13, inventory_transfers_l14, inventory_transfers_l15.

import gleam/dict.{type Dict}

import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcInt, SrcList, SrcNull, SrcString,
  default_connection_window_options, default_selected_field_options,
  get_field_response_key, get_selected_child_fields, paginate_connection_items,
  project_graphql_value, serialize_connection, src_object,
}

import shopify_draft_proxy/proxy/products/inventory_core.{
  find_inventory_level, inventory_level_is_active, inventory_quantity_amount,
  location_source, replace_inventory_level, variant_inventory_levels,
  write_inventory_quantity_amount,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_handlers.{
  shipment_inventory_item_source,
}
import shopify_draft_proxy/proxy/products/inventory_validation.{
  stage_variant_inventory_levels,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type InventoryTransferLineItemInput, type InventoryTransferLineItemUpdate,
  type MutationFieldResult, type ProductUserError,
  InventoryTransferLineItemInput, InventoryTransferLineItemUpdate,
  ProductUserError,
}
import shopify_draft_proxy/proxy/products/products_core.{
  enumerate_items, pad_start_zero,
}
import shopify_draft_proxy/proxy/products/shared.{
  connection_page_info_source, count_source, empty_connection_source,
  mutation_error_result, mutation_rejected_result, mutation_result,
  read_int_field, read_object_list_field, read_string_argument,
  read_string_field, read_string_list_field, user_errors_source,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type InventoryLevelRecord, type InventoryTransferLineItemRecord,
  type InventoryTransferLocationSnapshotRecord, type InventoryTransferRecord,
  type ProductVariantRecord, type StorePropertyRecord, InventoryLevelRecord,
  InventoryTransferLineItemRecord, InventoryTransferLocationSnapshotRecord,
  InventoryTransferRecord, StorePropertyBool,
}

// ===== from inventory_transfers_l00 =====
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

// ===== from inventory_transfers_l01 =====
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
pub fn inventory_transfer_location_not_found_error(
  field: List(String),
) -> ProductUserError {
  ProductUserError(
    field,
    "The location selected can't be found.",
    Some("LOCATION_NOT_FOUND"),
  )
}

@internal
pub fn inventory_transfer_same_location_error(
  field: List(String),
) -> ProductUserError {
  ProductUserError(
    field,
    "The origin location cannot be the same as the destination location.",
    Some("TRANSFER_ORIGIN_CANNOT_BE_THE_SAME_AS_DESTINATION"),
  )
}

@internal
pub fn inventory_transfer_item_not_found_error(
  field: List(String),
) -> ProductUserError {
  ProductUserError(
    field,
    "The inventory item could not be found.",
    Some("ITEM_NOT_FOUND"),
  )
}

@internal
pub fn inventory_transfer_duplicate_item_error(
  field: List(String),
) -> ProductUserError {
  ProductUserError(
    field,
    "The inventory item is already present in the list. Each item must be unique.",
    Some("DUPLICATE_ITEM"),
  )
}

@internal
pub fn inventory_transfer_negative_quantity_error(
  field: List(String),
) -> ProductUserError {
  ProductUserError(
    field,
    "The quantity can't be negative.",
    Some("INVALID_QUANTITY"),
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

// ===== from inventory_transfers_l02 =====
@internal
pub fn validate_inventory_transfer_line_items(
  store: Store,
  inputs: List(InventoryTransferLineItemInput),
  origin_location_id: Option(String),
) -> List(ProductUserError) {
  inputs
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(input, index) = pair
    let path = ["input", "lineItems", int.to_string(index)]
    let item_field = list.append(path, ["inventoryItemId"])
    let item_errors = case input.inventory_item_id {
      Some(inventory_item_id) ->
        case
          store.find_effective_variant_by_inventory_item_id(
            store,
            inventory_item_id,
          )
        {
          Some(variant) -> {
            let tracked = case variant.inventory_item {
              Some(item) -> item.tracked == Some(True)
              None -> False
            }
            let active_errors = case origin_location_id {
              Some(origin_id) ->
                case
                  find_inventory_level(
                    variant_inventory_levels(variant),
                    origin_id,
                  )
                {
                  Some(level) ->
                    case inventory_level_is_active(level) {
                      True -> []
                      False -> [
                        inventory_transfer_item_not_found_error(item_field),
                      ]
                    }
                  None -> [inventory_transfer_item_not_found_error(item_field)]
                }
              None -> []
            }
            case tracked, active_errors {
              True, [] -> []
              True, errors -> errors
              False, _ -> [
                ProductUserError(
                  item_field,
                  "The inventory item does not track inventory.",
                  Some("UNTRACKED_ITEM"),
                ),
              ]
            }
          }
          None -> [inventory_transfer_item_not_found_error(item_field)]
        }
      None -> [inventory_transfer_item_not_found_error(item_field)]
    }
    let duplicate_errors = case input.inventory_item_id {
      Some(inventory_item_id) ->
        case
          inventory_transfer_input_item_count(inputs, inventory_item_id) > 1
        {
          True -> [inventory_transfer_duplicate_item_error(item_field)]
          False -> []
        }
      None -> []
    }
    let quantity_errors = case input.quantity {
      Some(quantity) if quantity < 0 -> [
        inventory_transfer_negative_quantity_error(
          list.append(path, [
            "quantity",
          ]),
        ),
      ]
      _ -> []
    }
    list.append(list.append(item_errors, duplicate_errors), quantity_errors)
  })
}

@internal
pub fn inventory_transfer_input_item_count(
  inputs: List(InventoryTransferLineItemInput),
  inventory_item_id: String,
) -> Int {
  inputs
  |> list.filter(fn(input) {
    input.inventory_item_id == Some(inventory_item_id)
  })
  |> list.length
}

@internal
pub fn validate_inventory_transfer_locations(
  store: Store,
  origin_location_id: Option(String),
  destination_location_id: Option(String),
  origin_field: List(String),
  destination_field: List(String),
) -> List(ProductUserError) {
  let same_location_errors = case origin_location_id, destination_location_id {
    Some(origin), Some(destination) if origin == destination -> [
      inventory_transfer_same_location_error(destination_field),
    ]
    _, _ -> []
  }
  let origin_errors = case origin_location_id {
    Some(id) ->
      case inventory_transfer_location_known(store, id) {
        True -> []
        False -> [inventory_transfer_location_not_found_error(origin_field)]
      }
    None -> []
  }
  let destination_errors = case destination_location_id {
    Some(id) ->
      case inventory_transfer_location_known(store, id) {
        True -> []
        False -> [
          inventory_transfer_location_not_found_error(destination_field),
        ]
      }
    None -> []
  }
  list.append(
    same_location_errors,
    list.append(origin_errors, destination_errors),
  )
}

@internal
pub fn inventory_transfer_location_known(
  store: Store,
  location_id: String,
) -> Bool {
  case store.get_effective_store_property_location_by_id(store, location_id) {
    Some(location) ->
      inventory_transfer_store_property_location_active(location)
    None ->
      case store.get_effective_location_by_id(store, location_id) {
        Some(location) -> option.unwrap(location.is_active, True)
        None ->
          store.list_effective_product_variants(store)
          |> list.any(fn(variant) {
            variant_inventory_levels(variant)
            |> list.any(fn(level) { level.location.id == location_id })
          })
      }
  }
}

@internal
pub fn inventory_transfer_store_property_location_active(
  location: StorePropertyRecord,
) -> Bool {
  case dict.get(location.data, "isActive") {
    Ok(StorePropertyBool(False)) -> False
    _ -> True
  }
}

@internal
pub fn inventory_transfer_line_origin_location_id(
  store: Store,
  origin_location_id: Option(String),
) -> Option(String) {
  case origin_location_id {
    Some(id) ->
      case inventory_transfer_location_known(store, id) {
        True -> Some(id)
        False -> None
      }
    None -> None
  }
}

@internal
pub fn make_inventory_transfer_line_items(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inputs: List(InventoryTransferLineItemInput),
) -> #(List(InventoryTransferLineItemRecord), SyntheticIdentityRegistry) {
  let #(reversed, final_identity) =
    list.fold(inputs, #([], identity), fn(acc, input) {
      let #(records, current_identity) = acc
      case make_inventory_transfer_line_item(store, current_identity, input) {
        #(Some(record), next_identity) -> #([record, ..records], next_identity)
        #(None, next_identity) -> #(records, next_identity)
      }
    })
  #(list.reverse(reversed), final_identity)
}

@internal
pub fn inventory_transfer_delete_payload(
  deleted_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryTransferDeletePayload")),
      #("deletedId", graphql_helpers.option_string_source(deleted_id)),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

// ===== from inventory_transfers_l03 =====
@internal
pub fn handle_inventory_transfer_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let transfer_id =
    graphql_helpers.read_arg_string(
      graphql_helpers.field_args(field, variables),
      "id",
    )
  case get_inventory_transfer_by_optional_id(store, transfer_id) {
    None ->
      mutation_result(
        key,
        inventory_transfer_delete_payload(
          None,
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) ->
      case transfer.status == "DRAFT" {
        False ->
          mutation_result(
            key,
            inventory_transfer_delete_payload(
              None,
              [
                ProductUserError(
                  ["id"],
                  "Can't delete the transfer if it's not in the draft status.",
                  None,
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
          let next_store =
            store.delete_staged_inventory_transfer(store, transfer.id)
          mutation_result(
            key,
            inventory_transfer_delete_payload(
              Some(transfer.id),
              [],
              field,
              fragments,
            ),
            next_store,
            identity,
            [transfer.id],
          )
        }
      }
  }
}

@internal
pub fn make_inventory_transfer_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  status: String,
) -> #(
  Option(InventoryTransferRecord),
  List(ProductUserError),
  SyntheticIdentityRegistry,
) {
  let line_item_inputs = read_inventory_transfer_line_item_inputs(input)
  let origin_location_id = read_string_field(input, "originLocationId")
  let destination_location_id =
    read_string_field(input, "destinationLocationId")
  let line_origin_location_id =
    inventory_transfer_line_origin_location_id(store, origin_location_id)
  let user_errors =
    list.append(
      validate_inventory_transfer_locations(
        store,
        origin_location_id,
        destination_location_id,
        ["input", "originLocationId"],
        ["input", "destinationLocationId"],
      ),
      validate_inventory_transfer_line_items(
        store,
        line_item_inputs,
        line_origin_location_id,
      ),
    )
  case user_errors {
    [] -> {
      let #(id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryTransfer")
      let #(line_items, identity_after_items) =
        make_inventory_transfer_line_items(
          store,
          identity_after_id,
          line_item_inputs,
        )
      let #(date_created, next_identity) = case
        read_string_field(input, "dateCreated")
      {
        Some(value) -> #(value, identity_after_items)
        None ->
          synthetic_identity.make_synthetic_timestamp(identity_after_items)
      }
      let transfer_index =
        list.length(store.list_effective_inventory_transfers(store)) + 1
      let transfer =
        InventoryTransferRecord(
          id: id,
          name: "#T" <> pad_start_zero(int.to_string(transfer_index), 4),
          reference_name: read_string_field(input, "referenceName"),
          status: status,
          note: read_string_field(input, "note"),
          tags: read_string_list_field(input, "tags") |> option.unwrap([]),
          date_created: date_created,
          origin: make_inventory_transfer_location_snapshot(
            store,
            origin_location_id,
            next_identity,
          ),
          destination: make_inventory_transfer_location_snapshot(
            store,
            destination_location_id,
            next_identity,
          ),
          line_items: line_items,
        )
      #(Some(transfer), [], next_identity)
    }
    errors -> #(None, errors, identity)
  }
}

@internal
pub fn make_inventory_transfer_line_items_reusing_ids(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inputs: List(InventoryTransferLineItemInput),
  prior_items: List(InventoryTransferLineItemRecord),
) -> #(List(InventoryTransferLineItemRecord), SyntheticIdentityRegistry) {
  let #(items, next_identity) =
    make_inventory_transfer_line_items(store, identity, inputs)
  let items =
    list.map(items, fn(item) {
      case
        find_inventory_transfer_line_item_by_item_id(
          prior_items,
          item.inventory_item_id,
        )
      {
        Some(prior) -> InventoryTransferLineItemRecord(..item, id: prior.id)
        None -> item
      }
    })
  #(items, next_identity)
}

// ===== from inventory_transfers_l05 =====
@internal
pub fn apply_inventory_transfer_reservation_delta(
  store: Store,
  identity: SyntheticIdentityRegistry,
  transfer: InventoryTransferRecord,
  line_item: InventoryTransferLineItemRecord,
  delta_quantity: Int,
) -> Result(#(Store, SyntheticIdentityRegistry), List(ProductUserError)) {
  case find_inventory_transfer_origin_level(store, transfer, line_item) {
    None -> Error([inventory_transfer_origin_state_error()])
    Some(target) -> {
      let #(variant, level) = target
      let available = inventory_quantity_amount(level.quantities, "available")
      let reserved = inventory_quantity_amount(level.quantities, "reserved")
      case delta_quantity > 0 && available < delta_quantity {
        True -> Error([inventory_transfer_origin_state_error()])
        False -> {
          let quantities =
            level.quantities
            |> write_inventory_quantity_amount(
              "available",
              available - delta_quantity,
            )
            |> write_inventory_quantity_amount(
              "reserved",
              int.max(0, reserved + delta_quantity),
            )
          let next_level = InventoryLevelRecord(..level, quantities: quantities)
          let next_levels =
            replace_inventory_level(
              variant_inventory_levels(variant),
              level.location.id,
              next_level,
            )
          Ok(#(
            stage_variant_inventory_levels(store, variant, next_levels),
            identity,
          ))
        }
      }
    }
  }
}

// ===== from inventory_transfers_l06 =====
@internal
pub fn apply_inventory_transfer_reservation_deltas(
  store: Store,
  identity: SyntheticIdentityRegistry,
  transfer: InventoryTransferRecord,
  deltas: List(#(InventoryTransferLineItemRecord, Int)),
) -> #(Store, SyntheticIdentityRegistry, List(ProductUserError)) {
  let result =
    list.fold(deltas, Ok(#(store, identity)), fn(acc, delta) {
      case acc {
        Error(errors) -> Error(errors)
        Ok(state) -> {
          let #(current_store, current_identity) = state
          let #(line_item, delta_quantity) = delta
          case delta_quantity {
            0 -> Ok(#(current_store, current_identity))
            _ ->
              apply_inventory_transfer_reservation_delta(
                current_store,
                current_identity,
                transfer,
                line_item,
                delta_quantity,
              )
          }
        }
      }
    })
  case result {
    Ok(state) -> {
      let #(next_store, next_identity) = state
      #(next_store, next_identity, [])
    }
    Error(errors) -> #(store, identity, errors)
  }
}

// ===== from inventory_transfers_l07 =====
@internal
pub fn apply_inventory_transfer_reservation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  transfer: InventoryTransferRecord,
  direction: String,
) -> #(Store, SyntheticIdentityRegistry, List(ProductUserError)) {
  let deltas =
    list.map(transfer.line_items, fn(line_item) {
      let quantity = case direction {
        "release" -> 0 - line_item.total_quantity
        _ -> line_item.total_quantity
      }
      #(line_item, quantity)
    })
  apply_inventory_transfer_reservation_deltas(store, identity, transfer, deltas)
}

// ===== from inventory_transfers_l10 =====
@internal
pub fn inventory_transfer_line_item_source(
  store: Store,
  transfer: InventoryTransferRecord,
  line_item: InventoryTransferLineItemRecord,
) -> SourceValue {
  let is_ready =
    transfer.status == "READY_TO_SHIP" || transfer.status == "IN_PROGRESS"
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
    #("__typename", SrcString("InventoryTransferLineItem")),
    #("id", SrcString(line_item.id)),
    #("title", graphql_helpers.option_string_source(line_item.title)),
    #("totalQuantity", SrcInt(line_item.total_quantity)),
    #("shippedQuantity", SrcInt(line_item.shipped_quantity)),
    #(
      "pickedForShipmentQuantity",
      SrcInt(line_item.picked_for_shipment_quantity),
    ),
    #(
      "processableQuantity",
      SrcInt(line_item.total_quantity - line_item.shipped_quantity),
    ),
    #(
      "shippableQuantity",
      SrcInt(case is_ready {
        True -> line_item.total_quantity - line_item.shipped_quantity
        False -> 0
      }),
    ),
    #("inventoryItem", inventory_item),
  ])
}

// ===== from inventory_transfers_l11 =====
@internal
pub fn inventory_transfer_line_items_source(
  store: Store,
  transfer: InventoryTransferRecord,
) -> SourceValue {
  let edges =
    transfer.line_items
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(line_item, _) = pair
      src_object([
        #("cursor", SrcString("cursor:" <> line_item.id)),
        #(
          "node",
          inventory_transfer_line_item_source(store, transfer, line_item),
        ),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(transfer.line_items, fn(line_item) {
          inventory_transfer_line_item_source(store, transfer, line_item)
        }),
      ),
    ),
    #(
      "pageInfo",
      connection_page_info_source(transfer.line_items, fn(line_item, _index) {
        "cursor:" <> line_item.id
      }),
    ),
  ])
}

// ===== from inventory_transfers_l12 =====
@internal
pub fn inventory_transfer_source(
  store: Store,
  transfer: InventoryTransferRecord,
) -> SourceValue {
  let total_quantity = inventory_transfer_total_quantity(transfer)
  src_object([
    #("__typename", SrcString("InventoryTransfer")),
    #("id", SrcString(transfer.id)),
    #("name", SrcString(transfer.name)),
    #(
      "referenceName",
      graphql_helpers.option_string_source(transfer.reference_name),
    ),
    #("status", SrcString(transfer.status)),
    #("note", graphql_helpers.option_string_source(transfer.note)),
    #("tags", SrcList(list.map(transfer.tags, SrcString))),
    #("dateCreated", SrcString(transfer.date_created)),
    #("totalQuantity", SrcInt(total_quantity)),
    #("receivedQuantity", SrcInt(0)),
    #("origin", inventory_transfer_location_source(store, transfer.origin)),
    #(
      "destination",
      inventory_transfer_location_source(store, transfer.destination),
    ),
    #("lineItems", inventory_transfer_line_items_source(store, transfer)),
    #("lineItemsCount", count_source(total_quantity)),
    #("events", empty_connection_source()),
    #("shipments", empty_connection_source()),
    #("metafields", empty_connection_source()),
    #("metafield", SrcNull),
    #("hasTimelineComment", SrcBool(False)),
  ])
}

// ===== from inventory_transfers_l13 =====
@internal
pub fn serialize_inventory_transfer_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_inventory_transfer_by_id(store, id) {
        Some(transfer) ->
          project_graphql_value(
            inventory_transfer_source(store, transfer),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_inventory_transfers_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let transfers = store.list_effective_inventory_transfers(store)
  let window =
    paginate_connection_items(
      transfers,
      field,
      variables,
      inventory_transfer_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: inventory_transfer_cursor,
      serialize_node: fn(transfer, node_field, _index) {
        project_graphql_value(
          inventory_transfer_source(store, transfer),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: ConnectionPageInfoOptions(
        include_inline_fragments: False,
        prefix_cursors: False,
        include_cursors: True,
        fallback_start_cursor: None,
        fallback_end_cursor: None,
      ),
    ),
  )
}

@internal
pub fn inventory_transfer_payload(
  store: Store,
  typename: String,
  transfer_field: String,
  transfer: Option(InventoryTransferRecord),
  line_item_updates: List(InventoryTransferLineItemUpdate),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let transfer_value = case transfer {
    Some(record) -> inventory_transfer_source(store, record)
    None -> SrcNull
  }
  let updates =
    SrcList(list.map(
      line_item_updates,
      inventory_transfer_line_item_update_source,
    ))
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #(transfer_field, transfer_value),
      #("updatedLineItems", updates),
      #("removedQuantities", updates),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

// ===== from inventory_transfers_l14 =====
@internal
pub fn handle_inventory_transfer_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _root_name: String,
  payload_typename: String,
  status: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let #(transfer, user_errors, identity_after_transfer) =
    make_inventory_transfer_record(store, identity, input, status)
  case transfer, user_errors {
    Some(transfer), [] -> {
      let #(next_store, next_identity, reserve_errors) = case status {
        "READY_TO_SHIP" ->
          apply_inventory_transfer_reservation(
            store,
            identity_after_transfer,
            transfer,
            "reserve",
          )
        _ -> #(store, identity_after_transfer, [])
      }
      case reserve_errors {
        [] -> {
          let #(_, next_store) =
            store.upsert_staged_inventory_transfer(next_store, transfer)
          mutation_result(
            key,
            inventory_transfer_payload(
              next_store,
              payload_typename,
              "inventoryTransfer",
              Some(transfer),
              [],
              [],
              field,
              fragments,
            ),
            next_store,
            next_identity,
            inventory_transfer_staged_ids(transfer),
          )
        }
        errors ->
          mutation_rejected_result(
            key,
            inventory_transfer_payload(
              store,
              payload_typename,
              "inventoryTransfer",
              None,
              [],
              errors,
              field,
              fragments,
            ),
            store,
            identity_after_transfer,
          )
      }
    }
    _, errors ->
      mutation_rejected_result(
        key,
        inventory_transfer_payload(
          store,
          payload_typename,
          "inventoryTransfer",
          None,
          [],
          errors,
          field,
          fragments,
        ),
        store,
        identity_after_transfer,
      )
  }
}

@internal
pub fn handle_inventory_transfer_edit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let transfer_id = graphql_helpers.read_arg_string(args, "id")
  case get_inventory_transfer_by_optional_id(store, transfer_id) {
    None ->
      mutation_result(
        key,
        inventory_transfer_payload(
          store,
          "InventoryTransferEditPayload",
          "inventoryTransfer",
          None,
          [],
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) -> {
      let input =
        graphql_helpers.read_arg_object(args, "input")
        |> option.unwrap(dict.new())
      let origin_location_id = case read_string_field(input, "originId") {
        Some(id) -> Some(id)
        None ->
          transfer.origin
          |> option.map(fn(origin) { origin.id })
          |> option.flatten
      }
      let destination_location_id = case
        read_string_field(input, "destinationId")
      {
        Some(id) -> Some(id)
        None ->
          transfer.destination
          |> option.map(fn(destination) { destination.id })
          |> option.flatten
      }
      let user_errors =
        validate_inventory_transfer_locations(
          store,
          origin_location_id,
          destination_location_id,
          ["input", "originId"],
          ["input", "destinationId"],
        )
      let next_transfer =
        InventoryTransferRecord(
          ..transfer,
          reference_name: case read_string_field(input, "referenceName") {
            Some(value) -> Some(value)
            None -> transfer.reference_name
          },
          note: case read_string_field(input, "note") {
            Some(value) -> Some(value)
            None -> transfer.note
          },
          tags: read_string_list_field(input, "tags")
            |> option.unwrap(transfer.tags),
          date_created: read_string_field(input, "dateCreated")
            |> option.unwrap(transfer.date_created),
          origin: case origin_location_id {
            Some(_) ->
              make_inventory_transfer_location_snapshot(
                store,
                origin_location_id,
                identity,
              )
            None -> transfer.origin
          },
          destination: case destination_location_id {
            Some(_) ->
              make_inventory_transfer_location_snapshot(
                store,
                destination_location_id,
                identity,
              )
            None -> transfer.destination
          },
        )
      case user_errors {
        [] -> {
          let #(_, next_store) =
            store.upsert_staged_inventory_transfer(store, next_transfer)
          mutation_result(
            key,
            inventory_transfer_payload(
              next_store,
              "InventoryTransferEditPayload",
              "inventoryTransfer",
              Some(next_transfer),
              [],
              [],
              field,
              fragments,
            ),
            next_store,
            identity,
            inventory_transfer_staged_ids(next_transfer),
          )
        }
        errors ->
          mutation_rejected_result(
            key,
            inventory_transfer_payload(
              store,
              "InventoryTransferEditPayload",
              "inventoryTransfer",
              Some(transfer),
              [],
              errors,
              field,
              fragments,
            ),
            store,
            identity,
          )
      }
    }
  }
}

@internal
pub fn handle_inventory_transfer_set_items(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let transfer_id = read_string_field(input, "id")
  case get_inventory_transfer_by_optional_id(store, transfer_id) {
    None ->
      mutation_result(
        key,
        inventory_transfer_payload(
          store,
          "InventoryTransferSetItemsPayload",
          "inventoryTransfer",
          None,
          [],
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) -> {
      let line_item_inputs = read_inventory_transfer_line_item_inputs(input)
      let origin_location_id =
        transfer.origin
        |> option.map(fn(origin) { origin.id })
        |> option.flatten
      let line_origin_location_id =
        inventory_transfer_line_origin_location_id(store, origin_location_id)
      let user_errors =
        validate_inventory_transfer_line_items(
          store,
          line_item_inputs,
          line_origin_location_id,
        )
      let prior_items = transfer.line_items
      let #(updated_line_items, identity_after_items) =
        make_inventory_transfer_line_items_reusing_ids(
          store,
          identity,
          line_item_inputs,
          prior_items,
        )
      let updates =
        list.map(updated_line_items, fn(line_item) {
          let prior_quantity =
            find_inventory_transfer_line_item_by_item_id(
              prior_items,
              line_item.inventory_item_id,
            )
            |> option.map(fn(prior) { prior.total_quantity })
            |> option.unwrap(0)
          InventoryTransferLineItemUpdate(
            inventory_item_id: line_item.inventory_item_id,
            new_quantity: line_item.total_quantity,
            delta_quantity: line_item.total_quantity - prior_quantity,
          )
        })
      let deltas =
        inventory_transfer_set_item_deltas(prior_items, updated_line_items)
      let next_transfer =
        InventoryTransferRecord(..transfer, line_items: updated_line_items)
      case user_errors {
        [] -> {
          let #(next_store, next_identity, reserve_errors) = case
            inventory_transfer_has_reserved_origin_inventory(transfer)
          {
            True ->
              apply_inventory_transfer_reservation_deltas(
                store,
                identity_after_items,
                transfer,
                deltas,
              )
            False -> #(store, identity_after_items, [])
          }
          case reserve_errors {
            [] -> {
              let #(_, next_store) =
                store.upsert_staged_inventory_transfer(
                  next_store,
                  next_transfer,
                )
              mutation_result(
                key,
                inventory_transfer_payload(
                  next_store,
                  "InventoryTransferSetItemsPayload",
                  "inventoryTransfer",
                  Some(next_transfer),
                  updates,
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                inventory_transfer_staged_ids(next_transfer),
              )
            }
            errors ->
              mutation_result(
                key,
                inventory_transfer_payload(
                  store,
                  "InventoryTransferSetItemsPayload",
                  "inventoryTransfer",
                  Some(transfer),
                  [],
                  errors,
                  field,
                  fragments,
                ),
                store,
                identity_after_items,
                [],
              )
          }
        }
        errors ->
          mutation_rejected_result(
            key,
            inventory_transfer_payload(
              store,
              "InventoryTransferSetItemsPayload",
              "inventoryTransfer",
              Some(transfer),
              [],
              errors,
              field,
              fragments,
            ),
            store,
            identity_after_items,
          )
      }
    }
  }
}

@internal
pub fn handle_inventory_transfer_remove_items(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let transfer_id = read_string_field(input, "id")
  case get_inventory_transfer_by_optional_id(store, transfer_id) {
    None ->
      mutation_result(
        key,
        inventory_transfer_payload(
          store,
          "InventoryTransferRemoveItemsPayload",
          "inventoryTransfer",
          None,
          [],
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) -> {
      let remove_ids =
        read_string_list_field(input, "transferLineItemIds")
        |> option.unwrap([])
      let unknown =
        list.any(remove_ids, fn(id) {
          find_inventory_transfer_line_item(transfer.line_items, id) == None
        })
      let user_errors = case unknown {
        True -> [
          ProductUserError(
            ["input", "transferLineItemIds"],
            "The inventory transfer line item can't be found.",
            Some("LINE_ITEM_NOT_FOUND"),
          ),
        ]
        False -> []
      }
      let removed_items =
        list.filter(transfer.line_items, fn(line_item) {
          list.contains(remove_ids, line_item.id)
        })
      let next_items =
        list.filter(transfer.line_items, fn(line_item) {
          !list.contains(remove_ids, line_item.id)
        })
      let updates =
        list.map(removed_items, fn(line_item) {
          InventoryTransferLineItemUpdate(
            inventory_item_id: line_item.inventory_item_id,
            new_quantity: 0,
            delta_quantity: 0 - line_item.total_quantity,
          )
        })
      let next_transfer =
        InventoryTransferRecord(..transfer, line_items: next_items)
      case user_errors {
        [] -> {
          let #(next_store, next_identity, reserve_errors) = case
            inventory_transfer_has_reserved_origin_inventory(transfer)
          {
            True ->
              apply_inventory_transfer_reservation_deltas(
                store,
                identity,
                transfer,
                list.map(removed_items, fn(line_item) {
                  #(line_item, 0 - line_item.total_quantity)
                }),
              )
            False -> #(store, identity, [])
          }
          case reserve_errors {
            [] -> {
              let #(_, next_store) =
                store.upsert_staged_inventory_transfer(
                  next_store,
                  next_transfer,
                )
              mutation_result(
                key,
                inventory_transfer_payload(
                  next_store,
                  "InventoryTransferRemoveItemsPayload",
                  "inventoryTransfer",
                  Some(next_transfer),
                  updates,
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                inventory_transfer_staged_ids(next_transfer),
              )
            }
            errors ->
              mutation_result(
                key,
                inventory_transfer_payload(
                  store,
                  "InventoryTransferRemoveItemsPayload",
                  "inventoryTransfer",
                  Some(transfer),
                  [],
                  errors,
                  field,
                  fragments,
                ),
                store,
                next_identity,
                [],
              )
          }
        }
        errors ->
          mutation_result(
            key,
            inventory_transfer_payload(
              store,
              "InventoryTransferRemoveItemsPayload",
              "inventoryTransfer",
              Some(transfer),
              [],
              errors,
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
pub fn handle_inventory_transfer_mark_ready(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let transfer_id =
    graphql_helpers.read_arg_string(
      graphql_helpers.field_args(field, variables),
      "id",
    )
  case get_inventory_transfer_by_optional_id(store, transfer_id) {
    None ->
      mutation_result(
        key,
        inventory_transfer_payload(
          store,
          "InventoryTransferMarkAsReadyToShipPayload",
          "inventoryTransfer",
          None,
          [],
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) -> {
      let #(next_store, next_identity, user_errors) = case
        transfer.status == "DRAFT"
      {
        True ->
          apply_inventory_transfer_reservation(
            store,
            identity,
            transfer,
            "reserve",
          )
        False -> #(store, identity, [])
      }
      case user_errors {
        [] -> {
          let next_transfer =
            InventoryTransferRecord(..transfer, status: "READY_TO_SHIP")
          let #(_, next_store) =
            store.upsert_staged_inventory_transfer(next_store, next_transfer)
          mutation_result(
            key,
            inventory_transfer_payload(
              next_store,
              "InventoryTransferMarkAsReadyToShipPayload",
              "inventoryTransfer",
              Some(next_transfer),
              [],
              [],
              field,
              fragments,
            ),
            next_store,
            next_identity,
            inventory_transfer_staged_ids(next_transfer),
          )
        }
        errors ->
          mutation_result(
            key,
            inventory_transfer_payload(
              store,
              "InventoryTransferMarkAsReadyToShipPayload",
              "inventoryTransfer",
              None,
              [],
              errors,
              field,
              fragments,
            ),
            store,
            next_identity,
            [],
          )
      }
    }
  }
}

@internal
pub fn handle_inventory_transfer_cancel(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let transfer_id =
    graphql_helpers.read_arg_string(
      graphql_helpers.field_args(field, variables),
      "id",
    )
  case get_inventory_transfer_by_optional_id(store, transfer_id) {
    None ->
      mutation_result(
        key,
        inventory_transfer_payload(
          store,
          "InventoryTransferCancelPayload",
          "inventoryTransfer",
          None,
          [],
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) -> {
      let #(next_store, next_identity, _) = case
        transfer.status == "READY_TO_SHIP"
      {
        True ->
          apply_inventory_transfer_reservation(
            store,
            identity,
            transfer,
            "release",
          )
        False -> #(store, identity, [])
      }
      let next_transfer =
        InventoryTransferRecord(..transfer, status: "CANCELED")
      let #(_, next_store) =
        store.upsert_staged_inventory_transfer(next_store, next_transfer)
      mutation_result(
        key,
        inventory_transfer_payload(
          next_store,
          "InventoryTransferCancelPayload",
          "inventoryTransfer",
          Some(next_transfer),
          [],
          [],
          field,
          fragments,
        ),
        next_store,
        next_identity,
        inventory_transfer_staged_ids(next_transfer),
      )
    }
  }
}

@internal
pub fn handle_inventory_transfer_duplicate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let transfer_id =
    graphql_helpers.read_arg_string(
      graphql_helpers.field_args(field, variables),
      "id",
    )
  case get_inventory_transfer_by_optional_id(store, transfer_id) {
    None ->
      mutation_result(
        key,
        inventory_transfer_payload(
          store,
          "InventoryTransferDuplicatePayload",
          "inventoryTransfer",
          None,
          [],
          [inventory_transfer_not_found_error()],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(transfer) -> {
      let #(id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryTransfer")
      let #(line_items, next_identity) =
        duplicate_inventory_transfer_line_items(
          transfer.line_items,
          identity_after_id,
        )
      let transfer_index =
        list.length(store.list_effective_inventory_transfers(store)) + 1
      let duplicated =
        InventoryTransferRecord(
          ..transfer,
          id: id,
          name: "#T" <> pad_start_zero(int.to_string(transfer_index), 4),
          status: "DRAFT",
          line_items: line_items,
        )
      let #(_, next_store) =
        store.upsert_staged_inventory_transfer(store, duplicated)
      mutation_result(
        key,
        inventory_transfer_payload(
          next_store,
          "InventoryTransferDuplicatePayload",
          "inventoryTransfer",
          Some(duplicated),
          [],
          [],
          field,
          fragments,
        ),
        next_store,
        next_identity,
        inventory_transfer_staged_ids(duplicated),
      )
    }
  }
}

// ===== from inventory_transfers_l15 =====
@internal
pub fn handle_inventory_transfer_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  case root_name {
    "inventoryTransferCreate" ->
      handle_inventory_transfer_create(
        store,
        identity,
        root_name,
        "InventoryTransferCreatePayload",
        "DRAFT",
        field,
        fragments,
        variables,
      )
    "inventoryTransferCreateAsReadyToShip" ->
      handle_inventory_transfer_create(
        store,
        identity,
        root_name,
        "InventoryTransferCreateAsReadyToShipPayload",
        "READY_TO_SHIP",
        field,
        fragments,
        variables,
      )
    "inventoryTransferEdit" ->
      handle_inventory_transfer_edit(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "inventoryTransferSetItems" ->
      handle_inventory_transfer_set_items(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "inventoryTransferRemoveItems" ->
      handle_inventory_transfer_remove_items(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "inventoryTransferMarkAsReadyToShip" ->
      handle_inventory_transfer_mark_ready(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "inventoryTransferDuplicate" ->
      handle_inventory_transfer_duplicate(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "inventoryTransferCancel" ->
      handle_inventory_transfer_cancel(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "inventoryTransferDelete" ->
      handle_inventory_transfer_delete(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    _ ->
      mutation_error_result(
        field |> get_field_response_key,
        store,
        identity,
        [],
      )
  }
}
