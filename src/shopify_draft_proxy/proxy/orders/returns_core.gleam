//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type ObjectField, type Selection, Field, NullValue, ObjectField, ObjectValue,
  SelectionSet, VariableValue,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SelectedFieldOptions, SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt,
  SrcList, SrcNull, SrcObject, SrcString, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, resolved_value_to_source,
  serialize_connection, source_to_json, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type MutationOutcome, MutationOutcome, RequiredArgument,
  find_argument, single_root_log_draft, validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/orders/common.{
  captured_field_or_null, captured_int_field, captured_string_field,
  field_arguments, inferred_user_error, optional_captured_string, read_int,
  read_object, read_object_list, read_string, read_string_argument,
  replace_captured_object_fields,
}
import shopify_draft_proxy/proxy/orders/draft_order_builders.{total_quantity}
import shopify_draft_proxy/proxy/orders/hydration.{maybe_hydrate_order_by_id}
import shopify_draft_proxy/proxy/orders/order_types.{
  type DisposeMutationResult, type ReturnMutationResult,
  type ReverseDeliveryMutationResult, DisposeMutationResult,
  ReturnMutationResult, ReverseDeliveryMutationResult,
}
import shopify_draft_proxy/proxy/orders/serializers.{
  build_all_reverse_delivery_line_items, build_order_return,
  build_return_line_items, build_reverse_delivery_line_items,
  ensure_return_reverse_fulfillment_orders, find_order_return,
  find_order_reverse_delivery, find_order_reverse_fulfillment_order,
  find_order_reverse_fulfillment_order_line_item,
  normalize_reverse_delivery_label, normalize_reverse_delivery_tracking,
  order_return_line_items, order_returns,
  replace_return_reverse_fulfillment_order, return_log_draft,
  reverse_fulfillment_order_line_items,
  reverse_fulfillment_order_reverse_deliveries,
  serialize_return_mutation_payload, serialize_reverse_delivery_mutation_payload,
  serialize_reverse_fulfillment_order_dispose_payload, stage_order_with_return,
  stage_order_with_returns, stage_reverse_delivery_update,
  sync_reverse_fulfillment_line_items, total_return_quantity,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/proxy/user_error_codes
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type AbandonedCheckoutRecord, type AbandonmentRecord, type CapturedJsonValue,
  type CustomerRecord, type DraftOrderRecord,
  type DraftOrderVariantCatalogRecord, type OrderRecord,
  type ProductMetafieldRecord, type ProductRecord, type ProductVariantRecord,
  AbandonmentDeliveryActivityRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString,
  CustomerOrderSummaryRecord, CustomerRecord, DraftOrderRecord,
  DraftOrderVariantCatalogRecord, OrderRecord, ProductVariantRecord,
}

@internal
pub fn handle_return_lifecycle_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  case root_name {
    "returnCreate" | "returnRequest" -> {
      let args = field_arguments(field, variables)
      let input_key = case root_name {
        "returnCreate" -> "returnInput"
        _ -> "input"
      }
      let status = case root_name {
        "returnCreate" -> "OPEN"
        _ -> "REQUESTED"
      }
      // Pattern 2: returnCreate/returnRequest derive return line items from
      // the source order, so hydrate that order before local return staging.
      let hydrated_store =
        read_object(args, input_key)
        |> option.then(fn(input) { read_string(input, "orderId") })
        |> option.map(fn(order_id) {
          maybe_hydrate_order_by_id(store, order_id, upstream)
        })
        |> option.unwrap(store)
      let result =
        apply_return_create(
          hydrated_store,
          identity,
          read_object(args, input_key),
          status,
        )
      let ReturnMutationResult(
        order,
        order_return,
        next_store,
        next_identity,
        user_errors,
      ) = result
      let payload =
        serialize_return_mutation_payload(
          field,
          order_return,
          order,
          user_errors,
          fragments,
        )
      let staged_ids = case order_return {
        Some(value) ->
          captured_string_field(value, "id")
          |> option.map(fn(id) { [id] })
          |> option.unwrap([])
        None -> []
      }
      #(key, payload, next_store, next_identity, staged_ids, [
        return_log_draft(root_name, staged_ids, user_errors),
      ])
    }
    "returnCancel" | "returnClose" | "returnReopen" -> {
      let status = case root_name {
        "returnCancel" -> "CANCELED"
        "returnClose" -> "CLOSED"
        _ -> "OPEN"
      }
      let result =
        apply_return_status_update(
          store,
          identity,
          read_string_argument(field, "id", variables),
          status,
        )
      let ReturnMutationResult(
        order,
        order_return,
        next_store,
        next_identity,
        user_errors,
      ) = result
      let payload =
        serialize_return_mutation_payload(
          field,
          order_return,
          order,
          user_errors,
          fragments,
        )
      let staged_ids = case order_return {
        Some(value) ->
          captured_string_field(value, "id")
          |> option.map(fn(id) { [id] })
          |> option.unwrap([])
        None -> []
      }
      #(key, payload, next_store, next_identity, staged_ids, [
        return_log_draft(root_name, staged_ids, user_errors),
      ])
    }
    "removeFromReturn" -> {
      let args = field_arguments(field, variables)
      let result =
        apply_remove_from_return(
          store,
          identity,
          read_string(args, "returnId"),
          read_object_list(args, "returnLineItems"),
          read_object_list(args, "exchangeLineItems"),
        )
      let ReturnMutationResult(
        order,
        order_return,
        next_store,
        next_identity,
        user_errors,
      ) = result
      let payload =
        serialize_return_mutation_payload(
          field,
          order_return,
          order,
          user_errors,
          fragments,
        )
      let staged_ids = case order_return {
        Some(value) ->
          captured_string_field(value, "id")
          |> option.map(fn(id) { [id] })
          |> option.unwrap([])
        None -> []
      }
      #(key, payload, next_store, next_identity, staged_ids, [
        return_log_draft(root_name, staged_ids, user_errors),
      ])
    }
    "returnDeclineRequest" -> {
      let args = field_arguments(field, variables)
      let result =
        apply_return_decline_request(
          store,
          identity,
          read_object(args, "input"),
        )
      let ReturnMutationResult(
        order,
        order_return,
        next_store,
        next_identity,
        user_errors,
      ) = result
      let payload =
        serialize_return_mutation_payload(
          field,
          order_return,
          order,
          user_errors,
          fragments,
        )
      let staged_ids = case order_return {
        Some(value) ->
          captured_string_field(value, "id")
          |> option.map(fn(id) { [id] })
          |> option.unwrap([])
        None -> []
      }
      #(key, payload, next_store, next_identity, staged_ids, [
        return_log_draft(root_name, staged_ids, user_errors),
      ])
    }
    "returnApproveRequest" -> {
      let args = field_arguments(field, variables)
      let result =
        apply_return_approve_request(
          store,
          identity,
          read_object(args, "input"),
        )
      let ReturnMutationResult(
        order,
        order_return,
        next_store,
        next_identity,
        user_errors,
      ) = result
      let payload =
        serialize_return_mutation_payload(
          field,
          order_return,
          order,
          user_errors,
          fragments,
        )
      let staged_ids = case order_return {
        Some(value) ->
          captured_string_field(value, "id")
          |> option.map(fn(id) { [id] })
          |> option.unwrap([])
        None -> []
      }
      #(key, payload, next_store, next_identity, staged_ids, [
        return_log_draft(root_name, staged_ids, user_errors),
      ])
    }
    "returnProcess" -> {
      let args = field_arguments(field, variables)
      let result =
        apply_return_process(store, identity, read_object(args, "input"))
      let ReturnMutationResult(
        order,
        order_return,
        next_store,
        next_identity,
        user_errors,
      ) = result
      let payload =
        serialize_return_mutation_payload(
          field,
          order_return,
          order,
          user_errors,
          fragments,
        )
      let staged_ids = case order_return {
        Some(value) ->
          captured_string_field(value, "id")
          |> option.map(fn(id) { [id] })
          |> option.unwrap([])
        None -> []
      }
      #(key, payload, next_store, next_identity, staged_ids, [
        return_log_draft(root_name, staged_ids, user_errors),
      ])
    }
    "reverseDeliveryCreateWithShipping" | "reverseDeliveryShippingUpdate" -> {
      let args = field_arguments(field, variables)
      let result = case root_name {
        "reverseDeliveryCreateWithShipping" ->
          apply_reverse_delivery_create_with_shipping(store, identity, args)
        _ -> apply_reverse_delivery_shipping_update(store, identity, args)
      }
      let ReverseDeliveryMutationResult(
        order,
        order_return,
        reverse_order,
        reverse_delivery,
        next_store,
        next_identity,
        user_errors,
      ) = result
      let payload =
        serialize_reverse_delivery_mutation_payload(
          field,
          reverse_delivery,
          reverse_order,
          order_return,
          order,
          user_errors,
          fragments,
        )
      let staged_ids = case reverse_delivery {
        Some(value) ->
          captured_string_field(value, "id")
          |> option.map(fn(id) { [id] })
          |> option.unwrap([])
        None -> []
      }
      #(key, payload, next_store, next_identity, staged_ids, [
        return_log_draft(root_name, staged_ids, user_errors),
      ])
    }
    "reverseFulfillmentOrderDispose" -> {
      let args = field_arguments(field, variables)
      let result =
        apply_reverse_fulfillment_order_dispose(store, identity, args)
      let DisposeMutationResult(
        line_items,
        next_store,
        next_identity,
        user_errors,
      ) = result
      let payload =
        serialize_reverse_fulfillment_order_dispose_payload(
          field,
          line_items,
          user_errors,
          fragments,
        )
      #(key, payload, next_store, next_identity, [], [
        return_log_draft(root_name, [], user_errors),
      ])
    }
    _ -> #(key, json.null(), store, identity, [], [])
  }
}

@internal
pub fn apply_return_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Option(Dict(String, root_field.ResolvedValue)),
  status: String,
) -> ReturnMutationResult {
  case input {
    None ->
      ReturnMutationResult(None, None, store, identity, [
        inferred_user_error(["input"], "Input is required."),
      ])
    Some(input) -> {
      case read_string(input, "orderId") {
        None ->
          ReturnMutationResult(None, None, store, identity, [
            inferred_user_error(["orderId"], "Order does not exist."),
          ])
        Some(order_id) ->
          case store.get_order_by_id(store, order_id) {
            None ->
              ReturnMutationResult(None, None, store, identity, [
                inferred_user_error(["orderId"], "Order does not exist."),
              ])
            Some(order) -> {
              let line_item_result =
                build_return_line_items(identity, order, input)
              case line_item_result {
                Error(user_errors) ->
                  ReturnMutationResult(
                    Some(order),
                    None,
                    store,
                    identity,
                    user_errors,
                  )
                Ok(line_item_pack) -> {
                  let #(line_items, identity_after_line_items) = line_item_pack
                  let #(order_return, identity_after_return) =
                    build_order_return(
                      identity_after_line_items,
                      order,
                      line_items,
                      input,
                      status,
                    )
                  let #(next_store, next_identity, updated_order) =
                    stage_order_with_returns(
                      store,
                      identity_after_return,
                      order,
                      [order_return, ..order_returns(order.data)],
                    )
                  ReturnMutationResult(
                    Some(updated_order),
                    Some(order_return),
                    next_store,
                    next_identity,
                    [],
                  )
                }
              }
            }
          }
      }
    }
  }
}

@internal
pub fn apply_return_status_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  return_id: Option(String),
  status: String,
) -> ReturnMutationResult {
  case return_id {
    None ->
      ReturnMutationResult(None, None, store, identity, [
        inferred_user_error(["id"], "Return does not exist."),
      ])
    Some(return_id) ->
      case find_order_return(store, return_id) {
        None ->
          ReturnMutationResult(None, None, store, identity, [
            inferred_user_error(["id"], "Return does not exist."),
          ])
        Some(match) -> {
          let #(order, order_return) = match
          let #(closed_at, identity_after_closed) = case status {
            "CLOSED" -> {
              let #(timestamp, after_closed) =
                synthetic_identity.make_synthetic_timestamp(identity)
              #(CapturedString(timestamp), after_closed)
            }
            _ -> #(CapturedNull, identity)
          }
          let #(updated_at, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity_after_closed)
          let updated_return =
            replace_captured_object_fields(order_return, [
              #("status", CapturedString(status)),
              #("closedAt", closed_at),
            ])
          let returns =
            order_returns(order.data)
            |> list.map(fn(candidate) {
              case captured_string_field(candidate, "id") == Some(return_id) {
                True -> updated_return
                False -> candidate
              }
            })
          let updated_order =
            OrderRecord(
              ..order,
              data: replace_captured_object_fields(order.data, [
                #("updatedAt", CapturedString(updated_at)),
                #("returns", CapturedArray(returns)),
              ]),
            )
          ReturnMutationResult(
            Some(updated_order),
            Some(updated_return),
            store.stage_order(store, updated_order),
            next_identity,
            [],
          )
        }
      }
  }
}

@internal
pub fn apply_remove_from_return(
  store: Store,
  identity: SyntheticIdentityRegistry,
  return_id: Option(String),
  raw_return_line_items: List(Dict(String, root_field.ResolvedValue)),
  raw_exchange_line_items: List(Dict(String, root_field.ResolvedValue)),
) -> ReturnMutationResult {
  case return_id {
    None ->
      ReturnMutationResult(None, None, store, identity, [
        inferred_user_error(["returnId"], "Return does not exist."),
      ])
    Some(return_id) ->
      case find_order_return(store, return_id) {
        None ->
          ReturnMutationResult(None, None, store, identity, [
            inferred_user_error(["returnId"], "Return does not exist."),
          ])
        Some(match) -> {
          let #(order, order_return) = match
          case raw_return_line_items, raw_exchange_line_items {
            [], [] ->
              ReturnMutationResult(Some(order), None, store, identity, [
                inferred_user_error(
                  ["returnLineItems"],
                  "Return line items or exchange line items are required.",
                ),
              ])
            _, [_, ..] ->
              ReturnMutationResult(Some(order), None, store, identity, [
                inferred_user_error(
                  ["exchangeLineItems"],
                  "Exchange line item removal is not supported by the local return model yet.",
                ),
              ])
            _, _ -> {
              let #(next_line_items, user_errors) =
                remove_return_line_items(
                  order_return_line_items(order_return),
                  raw_return_line_items,
                )
              case user_errors {
                [_, ..] ->
                  ReturnMutationResult(
                    Some(order),
                    None,
                    store,
                    identity,
                    user_errors,
                  )
                [] -> {
                  let updated_return =
                    replace_captured_object_fields(order_return, [
                      #(
                        "totalQuantity",
                        CapturedInt(total_return_quantity(next_line_items)),
                      ),
                      #("returnLineItems", CapturedArray(next_line_items)),
                    ])
                    |> sync_reverse_fulfillment_line_items(identity)
                  let #(synced_return, next_identity) = updated_return
                  let returns =
                    order_returns(order.data)
                    |> list.map(fn(candidate) {
                      case
                        captured_string_field(candidate, "id")
                        == Some(return_id)
                      {
                        True -> synced_return
                        False -> candidate
                      }
                    })
                  let #(next_store, staged_identity, updated_order) =
                    stage_order_with_returns(
                      store,
                      next_identity,
                      order,
                      returns,
                    )
                  ReturnMutationResult(
                    Some(updated_order),
                    Some(synced_return),
                    next_store,
                    staged_identity,
                    [],
                  )
                }
              }
            }
          }
        }
      }
  }
}

@internal
pub fn remove_return_line_items(
  existing_line_items: List(CapturedJsonValue),
  raw_return_line_items: List(Dict(String, root_field.ResolvedValue)),
) -> #(List(CapturedJsonValue), List(#(List(String), String, Option(String)))) {
  raw_return_line_items
  |> list.index_fold(#(existing_line_items, []), fn(acc, input, index) {
    let #(line_items, user_errors) = acc
    let line_item_id = read_string(input, "returnLineItemId")
    let quantity = read_int(input, "quantity", 0)
    let line_item =
      line_item_id
      |> option.then(fn(id) { find_return_line_item(line_items, id) })
    case line_item_id, line_item {
      None, _ -> #(
        line_items,
        list.append(user_errors, [
          inferred_user_error(
            ["returnLineItems", int.to_string(index), "returnLineItemId"],
            "Return line item does not exist.",
          ),
        ]),
      )
      Some(_), None -> #(
        line_items,
        list.append(user_errors, [
          inferred_user_error(
            ["returnLineItems", int.to_string(index), "returnLineItemId"],
            "Return line item does not exist.",
          ),
        ]),
      )
      Some(id), Some(line_item) -> {
        let current_quantity =
          captured_int_field(line_item, "quantity") |> option.unwrap(0)
        let processed_quantity =
          captured_int_field(line_item, "processedQuantity") |> option.unwrap(0)
        let removable_quantity = current_quantity - processed_quantity
        case quantity <= 0 || quantity > removable_quantity {
          True -> #(
            line_items,
            list.append(user_errors, [
              inferred_user_error(
                ["returnLineItems", int.to_string(index), "quantity"],
                "Quantity is not removable from return.",
              ),
            ]),
          )
          False -> #(
            apply_return_line_item_removal(line_items, id, quantity),
            user_errors,
          )
        }
      }
    }
  })
}

@internal
pub fn find_return_line_item(
  line_items: List(CapturedJsonValue),
  id: String,
) -> Option(CapturedJsonValue) {
  line_items
  |> list.find(fn(line_item) {
    captured_string_field(line_item, "id") == Some(id)
  })
  |> option.from_result
}

@internal
pub fn apply_return_line_item_removal(
  line_items: List(CapturedJsonValue),
  id: String,
  quantity: Int,
) -> List(CapturedJsonValue) {
  line_items
  |> list.filter_map(fn(line_item) {
    case captured_string_field(line_item, "id") == Some(id) {
      False -> Ok(line_item)
      True -> {
        let current_quantity =
          captured_int_field(line_item, "quantity") |> option.unwrap(0)
        let next_quantity = current_quantity - quantity
        case next_quantity <= 0 {
          True -> Error(Nil)
          False ->
            Ok(
              replace_captured_object_fields(line_item, [
                #("quantity", CapturedInt(next_quantity)),
              ]),
            )
        }
      }
    }
  })
}

@internal
pub fn apply_return_decline_request(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> ReturnMutationResult {
  let return_id = case input {
    Some(input) -> read_string(input, "id")
    None -> None
  }
  case return_id {
    None ->
      ReturnMutationResult(None, None, store, identity, [
        inferred_user_error(["input", "id"], "Return does not exist."),
      ])
    Some(return_id) ->
      case find_order_return(store, return_id) {
        None ->
          ReturnMutationResult(None, None, store, identity, [
            inferred_user_error(["input", "id"], "Return does not exist."),
          ])
        Some(match) -> {
          let #(order, order_return) = match
          case captured_string_field(order_return, "status") {
            Some("REQUESTED") -> {
              let input_fields = input |> option.unwrap(dict.new())
              let declined_return =
                replace_captured_object_fields(order_return, [
                  #("status", CapturedString("DECLINED")),
                  #(
                    "decline",
                    CapturedObject([
                      #(
                        "reason",
                        optional_captured_string(read_string(
                          input_fields,
                          "declineReason",
                        )),
                      ),
                      #(
                        "note",
                        optional_captured_string(read_string(
                          input_fields,
                          "declineNote",
                        )),
                      ),
                    ]),
                  ),
                ])
              let returns =
                order_returns(order.data)
                |> list.map(fn(candidate) {
                  case
                    captured_string_field(candidate, "id") == Some(return_id)
                  {
                    True -> declined_return
                    False -> candidate
                  }
                })
              let #(next_store, next_identity, updated_order) =
                stage_order_with_returns(store, identity, order, returns)
              ReturnMutationResult(
                Some(updated_order),
                Some(declined_return),
                next_store,
                next_identity,
                [],
              )
            }
            _ ->
              ReturnMutationResult(Some(order), None, store, identity, [
                inferred_user_error(
                  ["input", "id"],
                  "Return is not declinable. Only non-refunded returns with status REQUESTED can be declined.",
                ),
              ])
          }
        }
      }
  }
}

@internal
pub fn apply_return_approve_request(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> ReturnMutationResult {
  let return_id = case input {
    Some(input) -> read_string(input, "id")
    None -> None
  }
  case return_id {
    None ->
      ReturnMutationResult(None, None, store, identity, [
        inferred_user_error(["input", "id"], "Return does not exist."),
      ])
    Some(return_id) ->
      case find_order_return(store, return_id) {
        None ->
          ReturnMutationResult(None, None, store, identity, [
            inferred_user_error(["input", "id"], "Return does not exist."),
          ])
        Some(match) -> {
          let #(order, order_return) = match
          case captured_string_field(order_return, "status") {
            Some("REQUESTED") -> {
              let open_return =
                replace_captured_object_fields(order_return, [
                  #("status", CapturedString("OPEN")),
                  #("decline", CapturedNull),
                ])
              let #(approved_return, next_identity) =
                ensure_return_reverse_fulfillment_orders(
                  identity,
                  order,
                  open_return,
                )
              let #(next_store, staged_identity, updated_order) =
                stage_order_with_return(
                  store,
                  next_identity,
                  order,
                  approved_return,
                )
              ReturnMutationResult(
                Some(updated_order),
                Some(approved_return),
                next_store,
                staged_identity,
                [],
              )
            }
            _ ->
              ReturnMutationResult(Some(order), None, store, identity, [
                inferred_user_error(
                  ["input", "id"],
                  "Return is not approvable. Only returns with status REQUESTED can be approved.",
                ),
              ])
          }
        }
      }
  }
}

@internal
pub fn apply_return_process(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> ReturnMutationResult {
  let input_fields = input |> option.unwrap(dict.new())
  case read_string(input_fields, "returnId") {
    None ->
      ReturnMutationResult(None, None, store, identity, [
        inferred_user_error(["input", "returnId"], "Return does not exist."),
      ])
    Some(return_id) ->
      case find_order_return(store, return_id) {
        None ->
          ReturnMutationResult(None, None, store, identity, [
            inferred_user_error(["input", "returnId"], "Return does not exist."),
          ])
        Some(match) -> {
          let #(order, order_return) = match
          case captured_string_field(order_return, "status") {
            Some("OPEN") -> {
              let raw_line_items =
                read_object_list(input_fields, "returnLineItems")
              case raw_line_items {
                [] ->
                  ReturnMutationResult(Some(order), None, store, identity, [
                    inferred_user_error(
                      ["input", "returnLineItems"],
                      "Return line items are required.",
                    ),
                  ])
                _ -> {
                  let #(next_line_items, user_errors) =
                    process_return_line_items(
                      order_return_line_items(order_return),
                      raw_line_items,
                    )
                  case user_errors {
                    [_, ..] ->
                      ReturnMutationResult(
                        Some(order),
                        None,
                        store,
                        identity,
                        user_errors,
                      )
                    [] -> {
                      let all_processed =
                        next_line_items
                        |> list.all(fn(line_item) {
                          let processed =
                            captured_int_field(line_item, "processedQuantity")
                            |> option.unwrap(0)
                          let quantity =
                            captured_int_field(line_item, "quantity")
                            |> option.unwrap(0)
                          processed >= quantity
                        })
                      let #(closed_at, identity_after_closed) = case
                        all_processed
                      {
                        True -> {
                          let #(timestamp, next_identity) =
                            synthetic_identity.make_synthetic_timestamp(
                              identity,
                            )
                          #(CapturedString(timestamp), next_identity)
                        }
                        False -> #(
                          captured_field_or_null(order_return, "closedAt"),
                          identity,
                        )
                      }
                      let staged_status = case all_processed {
                        True -> "CLOSED"
                        False ->
                          captured_string_field(order_return, "status")
                          |> option.unwrap("OPEN")
                      }
                      let base_return =
                        replace_captured_object_fields(order_return, [
                          #("status", CapturedString(staged_status)),
                          #("closedAt", closed_at),
                          #("returnLineItems", CapturedArray(next_line_items)),
                        ])
                      let #(return_with_reverse, identity_after_reverse) =
                        ensure_return_reverse_fulfillment_orders(
                          identity_after_closed,
                          order,
                          base_return,
                        )
                      let #(synced_return, next_identity) =
                        sync_reverse_fulfillment_line_items(
                          return_with_reverse,
                          identity_after_reverse,
                        )
                      let #(next_store, staged_identity, updated_order) =
                        stage_order_with_return(
                          store,
                          next_identity,
                          order,
                          synced_return,
                        )
                      let response_return = case all_processed {
                        True ->
                          replace_captured_object_fields(synced_return, [
                            #(
                              "status",
                              captured_field_or_null(order_return, "status"),
                            ),
                            #(
                              "closedAt",
                              captured_field_or_null(order_return, "closedAt"),
                            ),
                          ])
                        False -> synced_return
                      }
                      ReturnMutationResult(
                        Some(updated_order),
                        Some(response_return),
                        next_store,
                        staged_identity,
                        [],
                      )
                    }
                  }
                }
              }
            }
            _ ->
              ReturnMutationResult(Some(order), None, store, identity, [
                inferred_user_error(
                  ["input", "returnId"],
                  "Only OPEN returns can be processed.",
                ),
              ])
          }
        }
      }
  }
}

@internal
pub fn process_return_line_items(
  existing_line_items: List(CapturedJsonValue),
  raw_return_line_items: List(Dict(String, root_field.ResolvedValue)),
) -> #(List(CapturedJsonValue), List(#(List(String), String, Option(String)))) {
  raw_return_line_items
  |> list.index_fold(#(existing_line_items, []), fn(acc, input, index) {
    let #(line_items, user_errors) = acc
    let line_item_id = read_string(input, "id")
    let quantity = read_int(input, "quantity", 0)
    let line_item =
      line_item_id
      |> option.then(fn(id) { find_return_line_item(line_items, id) })
    case line_item_id, line_item {
      None, _ -> #(
        line_items,
        list.append(user_errors, [
          inferred_user_error(
            ["input", "returnLineItems", int.to_string(index), "id"],
            "Return line item does not exist.",
          ),
        ]),
      )
      Some(_), None -> #(
        line_items,
        list.append(user_errors, [
          inferred_user_error(
            ["input", "returnLineItems", int.to_string(index), "id"],
            "Return line item does not exist.",
          ),
        ]),
      )
      Some(id), Some(line_item) -> {
        let processed_quantity =
          captured_int_field(line_item, "processedQuantity") |> option.unwrap(0)
        let current_quantity =
          captured_int_field(line_item, "quantity") |> option.unwrap(0)
        let unprocessed_quantity = current_quantity - processed_quantity
        case quantity <= 0 || quantity > unprocessed_quantity {
          True -> #(
            line_items,
            list.append(user_errors, [
              inferred_user_error(
                ["input", "returnLineItems", int.to_string(index), "quantity"],
                "Quantity is not processable.",
              ),
            ]),
          )
          False -> #(
            line_items
              |> list.map(fn(candidate) {
                case captured_string_field(candidate, "id") == Some(id) {
                  True ->
                    replace_captured_object_fields(candidate, [
                      #(
                        "processedQuantity",
                        CapturedInt(processed_quantity + quantity),
                      ),
                    ])
                  False -> candidate
                }
              }),
            user_errors,
          )
        }
      }
    }
  })
}

@internal
pub fn apply_reverse_delivery_create_with_shipping(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args: Dict(String, root_field.ResolvedValue),
) -> ReverseDeliveryMutationResult {
  case read_string(args, "reverseFulfillmentOrderId") {
    None ->
      ReverseDeliveryMutationResult(None, None, None, None, store, identity, [
        inferred_user_error(
          ["reverseFulfillmentOrderId"],
          "Reverse fulfillment order does not exist.",
        ),
      ])
    Some(reverse_order_id) ->
      case find_order_reverse_fulfillment_order(store, reverse_order_id) {
        None ->
          ReverseDeliveryMutationResult(
            None,
            None,
            None,
            None,
            store,
            identity,
            [
              inferred_user_error(
                ["reverseFulfillmentOrderId"],
                "Reverse fulfillment order does not exist.",
              ),
            ],
          )
        Some(match) -> {
          let #(order, order_return, reverse_order) = match
          let raw_line_items =
            read_object_list(args, "reverseDeliveryLineItems")
          let line_item_result = case raw_line_items {
            [] ->
              Ok(build_all_reverse_delivery_line_items(identity, reverse_order))
            _ ->
              build_reverse_delivery_line_items(
                identity,
                reverse_order,
                raw_line_items,
              )
          }
          case line_item_result {
            Error(user_errors) ->
              ReverseDeliveryMutationResult(
                Some(order),
                Some(order_return),
                Some(reverse_order),
                None,
                store,
                identity,
                user_errors,
              )
            Ok(line_item_pack) -> {
              let #(line_items, identity_after_lines) = line_item_pack
              case line_items {
                [] ->
                  ReverseDeliveryMutationResult(
                    Some(order),
                    Some(order_return),
                    Some(reverse_order),
                    None,
                    store,
                    identity,
                    [
                      inferred_user_error(
                        ["reverseDeliveryLineItems"],
                        "Reverse delivery line items are required.",
                      ),
                    ],
                  )
                _ -> {
                  let #(reverse_delivery_id, identity_after_delivery) =
                    synthetic_identity.make_synthetic_gid(
                      identity_after_lines,
                      "ReverseDelivery",
                    )
                  let reverse_delivery =
                    CapturedObject([
                      #("id", CapturedString(reverse_delivery_id)),
                      #(
                        "reverseFulfillmentOrderId",
                        captured_field_or_null(reverse_order, "id"),
                      ),
                      #("reverseDeliveryLineItems", CapturedArray(line_items)),
                      #(
                        "tracking",
                        normalize_reverse_delivery_tracking(read_object(
                          args,
                          "trackingInput",
                        )),
                      ),
                      #(
                        "label",
                        normalize_reverse_delivery_label(read_object(
                          args,
                          "labelInput",
                        )),
                      ),
                    ])
                  let reverse_deliveries = [
                    reverse_delivery,
                    ..reverse_fulfillment_order_reverse_deliveries(
                      reverse_order,
                    )
                  ]
                  let updated_reverse_order =
                    replace_captured_object_fields(reverse_order, [
                      #("reverseDeliveries", CapturedArray(reverse_deliveries)),
                    ])
                  stage_reverse_delivery_update(
                    store,
                    identity_after_delivery,
                    order,
                    order_return,
                    updated_reverse_order,
                    reverse_delivery,
                  )
                }
              }
            }
          }
        }
      }
  }
}

@internal
pub fn apply_reverse_delivery_shipping_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args: Dict(String, root_field.ResolvedValue),
) -> ReverseDeliveryMutationResult {
  case read_string(args, "reverseDeliveryId") {
    None ->
      ReverseDeliveryMutationResult(None, None, None, None, store, identity, [
        inferred_user_error(
          ["reverseDeliveryId"],
          "Reverse delivery does not exist.",
        ),
      ])
    Some(reverse_delivery_id) ->
      case find_order_reverse_delivery(store, reverse_delivery_id) {
        None ->
          ReverseDeliveryMutationResult(
            None,
            None,
            None,
            None,
            store,
            identity,
            [
              inferred_user_error(
                ["reverseDeliveryId"],
                "Reverse delivery does not exist.",
              ),
            ],
          )
        Some(match) -> {
          let #(order, order_return, reverse_order, reverse_delivery) = match
          let tracking = case read_object(args, "trackingInput") {
            Some(input) -> normalize_reverse_delivery_tracking(Some(input))
            None -> captured_field_or_null(reverse_delivery, "tracking")
          }
          let label = case read_object(args, "labelInput") {
            Some(input) -> normalize_reverse_delivery_label(Some(input))
            None -> captured_field_or_null(reverse_delivery, "label")
          }
          let updated_delivery =
            replace_captured_object_fields(reverse_delivery, [
              #("tracking", tracking),
              #("label", label),
            ])
          let updated_reverse_order =
            replace_captured_object_fields(reverse_order, [
              #(
                "reverseDeliveries",
                CapturedArray(
                  reverse_fulfillment_order_reverse_deliveries(reverse_order)
                  |> list.map(fn(candidate) {
                    case
                      captured_string_field(candidate, "id")
                      == Some(reverse_delivery_id)
                    {
                      True -> updated_delivery
                      False -> candidate
                    }
                  }),
                ),
              ),
            ])
          stage_reverse_delivery_update(
            store,
            identity,
            order,
            order_return,
            updated_reverse_order,
            updated_delivery,
          )
        }
      }
  }
}

@internal
pub fn apply_reverse_fulfillment_order_dispose(
  store: Store,
  identity: SyntheticIdentityRegistry,
  args: Dict(String, root_field.ResolvedValue),
) -> DisposeMutationResult {
  let inputs = read_object_list(args, "dispositionInputs")
  case inputs {
    [] ->
      DisposeMutationResult([], store, identity, [
        inferred_user_error(
          ["dispositionInputs"],
          "Disposition inputs are required.",
        ),
      ])
    _ -> {
      let #(next_store, next_identity, line_items, user_errors) =
        inputs
        |> list.index_fold(#(store, identity, [], []), fn(acc, input, index) {
          let #(current_store, current_identity, disposed_items, errors) = acc
          let line_item_id =
            read_string(input, "reverseFulfillmentOrderLineItemId")
          let quantity = read_int(input, "quantity", 0)
          let match =
            line_item_id
            |> option.then(fn(id) {
              find_order_reverse_fulfillment_order_line_item(current_store, id)
            })
          case line_item_id, match {
            None, _ -> #(
              current_store,
              current_identity,
              disposed_items,
              list.append(errors, [
                inferred_user_error(
                  [
                    "dispositionInputs",
                    int.to_string(index),
                    "reverseFulfillmentOrderLineItemId",
                  ],
                  "Reverse fulfillment order line item does not exist.",
                ),
              ]),
            )
            Some(_), None -> #(
              current_store,
              current_identity,
              disposed_items,
              list.append(errors, [
                inferred_user_error(
                  [
                    "dispositionInputs",
                    int.to_string(index),
                    "reverseFulfillmentOrderLineItemId",
                  ],
                  "Reverse fulfillment order line item does not exist.",
                ),
              ]),
            )
            Some(_), Some(match) -> {
              let #(order, order_return, reverse_order, line_item) = match
              let total_quantity =
                captured_int_field(line_item, "totalQuantity")
                |> option.unwrap(0)
              let disposed_quantity =
                captured_int_field(line_item, "disposedQuantity")
                |> option.unwrap(0)
              let disposable_quantity = total_quantity - disposed_quantity
              case quantity <= 0 || quantity > disposable_quantity {
                True -> #(
                  current_store,
                  current_identity,
                  disposed_items,
                  list.append(errors, [
                    inferred_user_error(
                      ["dispositionInputs", int.to_string(index), "quantity"],
                      "Quantity is not disposable.",
                    ),
                  ]),
                )
                False -> {
                  let updated_line_item =
                    replace_captured_object_fields(line_item, [
                      #(
                        "remainingQuantity",
                        CapturedInt(int.max(
                          0,
                          {
                            captured_int_field(line_item, "remainingQuantity")
                            |> option.unwrap(total_quantity)
                          }
                            - quantity,
                        )),
                      ),
                      #(
                        "disposedQuantity",
                        CapturedInt(disposed_quantity + quantity),
                      ),
                      #(
                        "dispositionType",
                        optional_captured_string(read_string(
                          input,
                          "dispositionType",
                        )),
                      ),
                      #(
                        "dispositionLocationId",
                        optional_captured_string(read_string(
                          input,
                          "locationId",
                        )),
                      ),
                    ])
                  let updated_reverse_order =
                    replace_captured_object_fields(reverse_order, [
                      #(
                        "lineItems",
                        CapturedArray(
                          reverse_fulfillment_order_line_items(reverse_order)
                          |> list.map(fn(candidate) {
                            case
                              captured_string_field(candidate, "id")
                              == captured_string_field(updated_line_item, "id")
                            {
                              True -> updated_line_item
                              False -> candidate
                            }
                          }),
                        ),
                      ),
                    ])
                  let #(staged_store, staged_identity, _) =
                    stage_order_with_return(
                      current_store,
                      current_identity,
                      order,
                      replace_return_reverse_fulfillment_order(
                        order_return,
                        updated_reverse_order,
                      ),
                    )
                  #(
                    staged_store,
                    staged_identity,
                    list.append(disposed_items, [updated_line_item]),
                    errors,
                  )
                }
              }
            }
          }
        })
      case user_errors {
        [] -> DisposeMutationResult(line_items, next_store, next_identity, [])
        _ -> DisposeMutationResult([], store, identity, user_errors)
      }
    }
  }
}
