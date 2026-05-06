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
import shopify_draft_proxy/proxy/products/inventory_transfers_l00.{
  duplicate_inventory_transfer_line_items, find_inventory_transfer_line_item,
  find_inventory_transfer_line_item_by_item_id,
  get_inventory_transfer_by_optional_id,
  inventory_transfer_has_reserved_origin_inventory,
  inventory_transfer_staged_ids, make_inventory_transfer_location_snapshot,
}
import shopify_draft_proxy/proxy/products/inventory_transfers_l01.{
  inventory_transfer_not_found_error, inventory_transfer_set_item_deltas,
  read_inventory_transfer_line_item_inputs,
}
import shopify_draft_proxy/proxy/products/inventory_transfers_l02.{
  validate_inventory_transfer_line_items,
}
import shopify_draft_proxy/proxy/products/inventory_transfers_l03.{
  make_inventory_transfer_line_items_reusing_ids, make_inventory_transfer_record,
}
import shopify_draft_proxy/proxy/products/inventory_transfers_l06.{
  apply_inventory_transfer_reservation_deltas,
}
import shopify_draft_proxy/proxy/products/inventory_transfers_l07.{
  apply_inventory_transfer_reservation,
}
import shopify_draft_proxy/proxy/products/inventory_transfers_l13.{
  inventory_transfer_payload,
}
import shopify_draft_proxy/proxy/products/products_l00.{pad_start_zero}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_string_field, read_string_list_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{mutation_result}
import shopify_draft_proxy/proxy/products/types.{
  type InventoryTransferLineItemUpdate, type MutationFieldResult,
  type ProductUserError, InventoryTransferLineItemUpdate, MutationFieldResult,
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
          mutation_result(
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
            [],
          )
      }
    }
    _, errors ->
      mutation_result(
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
        [],
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
          origin: case read_string_field(input, "originId") {
            Some(id) ->
              make_inventory_transfer_location_snapshot(
                store,
                Some(id),
                identity,
              )
            None -> transfer.origin
          },
          destination: case read_string_field(input, "destinationId") {
            Some(id) ->
              make_inventory_transfer_location_snapshot(
                store,
                Some(id),
                identity,
              )
            None -> transfer.destination
          },
        )
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
      let user_errors =
        validate_inventory_transfer_line_items(store, line_item_inputs)
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
