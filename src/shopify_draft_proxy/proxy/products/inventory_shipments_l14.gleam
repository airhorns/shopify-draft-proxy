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
  find_inventory_shipment_line_item, inventory_shipment_status_after_receive,
  shipment_has_unreceived_incoming, shipment_line_item_unreceived,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_l01.{
  inventory_shipment_not_found_error, make_inventory_shipment_line_items,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_l02.{
  apply_inventory_shipment_quantity_updates,
  apply_inventory_shipment_receive_inputs,
  inventory_shipment_tracking_from_argument,
  inventory_shipment_tracking_from_input,
  validate_inventory_shipment_line_item_inputs,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_l06.{
  apply_inventory_shipment_deltas,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_l07.{
  stage_inventory_shipment_with_incoming,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_l13.{
  inventory_shipment_add_items_payload, inventory_shipment_create_payload,
  inventory_shipment_payload, inventory_shipment_receive_payload,
  inventory_shipment_update_item_quantities_payload,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_arg_object_list, read_arg_string_list, read_object_list_field,
  read_string_field,
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
