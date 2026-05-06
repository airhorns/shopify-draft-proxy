//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

import gleam/dict.{type Dict}
import gleam/float

import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_field_response_key,
}

import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, RequiredArgument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/orders/common.{
  captured_bool_field, captured_money_amount, captured_money_presentment_amount,
  captured_object_field, captured_string_field, field_arguments,
  inferred_user_error, max_float, money_set, money_set_with_presentment,
  mutation_user_error, nonzero_float, optional_captured_string,
  order_currency_code, order_money_set, order_money_set_from_input,
  order_mutation_user_error, order_payment_amount_set,
  order_presentment_currency_code, order_transactions, read_bool, read_number,
  read_object, read_string, read_string_arg, replace_captured_object_fields,
  selection_children, serialize_captured_selection, serialize_job,
  serialize_order_mutation_user_error, serialize_user_error, user_error,
  valid_email_address,
}
import shopify_draft_proxy/proxy/orders/hydration.{maybe_hydrate_order_by_id}
import shopify_draft_proxy/proxy/orders/order_types.{
  type OrderMutationUserError, UserErrorField,
}
import shopify_draft_proxy/proxy/orders/serializers.{
  order_returns, serialize_order_mutation_payload, serialize_order_node,
}

import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/proxy/user_error_codes

import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type OrderRecord, CapturedArray, CapturedBool,
  CapturedNull, CapturedObject, CapturedString, OrderRecord,
}

@internal
pub fn handle_order_lifecycle_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  document: String,
  operation_path: String,
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
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let input_type = case root_name {
    "orderClose" -> "OrderCloseInput!"
    "orderOpen" -> "OrderOpenInput!"
    _ -> "OrderInput!"
  }
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      root_name,
      [RequiredArgument(name: "input", expected_type: input_type)],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let order_id = case dict.get(args, "input") {
        Ok(root_field.ObjectVal(input)) -> read_string(input, "id")
        _ -> None
      }
      case order_id {
        None -> {
          let payload =
            serialize_order_mutation_payload(
              field,
              None,
              [
                mutation_user_error(["id"], "Order does not exist"),
              ],
              fragments,
            )
          #(key, payload, store, identity, [], [], [])
        }
        Some(id) -> {
          // Pattern 2: order lifecycle mutations need the prior order record
          // before local staging can project Shopify-shaped mutation and
          // downstream read payloads.
          let hydrated_store = maybe_hydrate_order_by_id(store, id, upstream)
          case store.get_order_by_id(hydrated_store, id) {
            None -> {
              let payload =
                serialize_order_mutation_payload(
                  field,
                  None,
                  [
                    mutation_user_error(["id"], "Order does not exist"),
                  ],
                  fragments,
                )
              #(key, payload, hydrated_store, identity, [], [], [])
            }
            Some(order) -> {
              let #(updated_order, next_identity, changed) =
                apply_order_lifecycle_update(order, identity, root_name)
              let payload =
                serialize_order_mutation_payload(
                  field,
                  Some(updated_order),
                  [],
                  fragments,
                )
              case changed {
                False -> #(
                  key,
                  payload,
                  hydrated_store,
                  next_identity,
                  [],
                  [],
                  [],
                )
                True -> {
                  let next_store =
                    store.stage_order(hydrated_store, updated_order)
                  let draft =
                    single_root_log_draft(
                      root_name,
                      [id],
                      store_types.Staged,
                      "orders",
                      "stage-locally",
                      Some(
                        "Locally staged "
                        <> root_name
                        <> " in shopify-draft-proxy.",
                      ),
                    )
                  #(key, payload, next_store, next_identity, [id], [], [draft])
                }
              }
            }
          }
        }
      }
    }
  }
}

@internal
pub fn apply_order_lifecycle_update(
  order: OrderRecord,
  identity: SyntheticIdentityRegistry,
  root_name: String,
) -> #(OrderRecord, SyntheticIdentityRegistry, Bool) {
  case root_name {
    "orderClose" ->
      case order_lifecycle_transition_is_noop(order, root_name) {
        True -> #(order, identity, False)
        False -> {
          let #(timestamp, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let replacements = [
            #("closed", CapturedBool(True)),
            #("closedAt", CapturedString(timestamp)),
            #("updatedAt", CapturedString(timestamp)),
          ]
          #(
            OrderRecord(
              ..order,
              data: replace_captured_object_fields(order.data, replacements),
            ),
            next_identity,
            True,
          )
        }
      }
    "orderOpen" ->
      case order_lifecycle_transition_is_noop(order, root_name) {
        True -> #(order, identity, False)
        False -> {
          let #(timestamp, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let replacements = [
            #("closed", CapturedBool(False)),
            #("closedAt", CapturedNull),
            #("updatedAt", CapturedString(timestamp)),
          ]
          #(
            OrderRecord(
              ..order,
              data: replace_captured_object_fields(order.data, replacements),
            ),
            next_identity,
            True,
          )
        }
      }
    _ -> #(order, identity, False)
  }
}

@internal
pub fn order_lifecycle_transition_is_noop(
  order: OrderRecord,
  root_name: String,
) -> Bool {
  case root_name {
    "orderClose" -> order_currently_closed(order)
    "orderOpen" -> !order_currently_closed(order)
    _ -> False
  }
}

@internal
pub fn order_currently_closed(order: OrderRecord) -> Bool {
  case captured_bool_field(order.data, "closed") {
    Some(value) -> value
    None ->
      case captured_object_field(order.data, "closedAt") {
        Some(CapturedString(_)) -> True
        _ -> False
      }
  }
}

@internal
pub fn handle_order_cancel_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderCancel",
      [
        RequiredArgument(name: "orderId", expected_type: "ID!"),
        RequiredArgument(name: "restock", expected_type: "Boolean!"),
        RequiredArgument(name: "reason", expected_type: "OrderCancelReason!"),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case order_cancel_argument_errors(args) {
        [_, ..] as user_errors -> {
          let payload = serialize_order_cancel_payload(field, None, user_errors)
          #(key, payload, store, identity, [], [], [])
        }
        [] -> {
          case read_string_arg(args, "orderId") {
            Some(order_id) -> {
              // Pattern 2: hydrate the target order before staging cancellation
              // locally; Shopify applies the supported mutation asynchronously,
              // but the proxy still owns the downstream read-after-write state.
              let hydrated_store =
                maybe_hydrate_order_by_id(store, order_id, upstream)
              case store.get_order_by_id(hydrated_store, order_id) {
                Some(order) -> {
                  case order_cancel_state_error(order) {
                    Some(error) -> {
                      let payload =
                        serialize_order_cancel_payload(field, None, [error])
                      #(key, payload, hydrated_store, identity, [], [], [])
                    }
                    None -> {
                      let reason =
                        read_string_arg(args, "reason")
                        |> option.unwrap("OTHER")
                      let #(updated_order, next_identity) =
                        apply_order_cancel_update(order, identity, reason)
                      let next_store =
                        store.stage_order(hydrated_store, updated_order)
                      let #(job_id, identity_after_job) =
                        synthetic_identity.make_synthetic_gid(
                          next_identity,
                          "Job",
                        )
                      let payload =
                        serialize_order_cancel_payload(field, Some(job_id), [])
                      let draft =
                        single_root_log_draft(
                          "orderCancel",
                          [order_id],
                          store_types.Staged,
                          "orders",
                          "stage-locally",
                          Some(
                            "Locally staged orderCancel in shopify-draft-proxy.",
                          ),
                        )
                      #(
                        key,
                        payload,
                        next_store,
                        identity_after_job,
                        [order_id],
                        [],
                        [draft],
                      )
                    }
                  }
                }
                None -> {
                  let payload =
                    serialize_order_cancel_payload(field, None, [
                      order_cancel_user_error(
                        ["orderId"],
                        "Order does not exist",
                        user_error_codes.not_found,
                      ),
                    ])
                  #(key, payload, hydrated_store, identity, [], [], [])
                }
              }
            }
            None -> {
              let payload =
                serialize_order_cancel_payload(field, None, [
                  order_cancel_user_error(
                    ["orderId"],
                    "Order does not exist",
                    user_error_codes.not_found,
                  ),
                ])
              #(key, payload, store, identity, [], [], [])
            }
          }
        }
      }
    }
  }
}

@internal
pub fn order_cancel_argument_errors(
  args: Dict(String, root_field.ResolvedValue),
) -> List(OrderMutationUserError) {
  let refund_errors = case
    has_non_null_arg(args, "refund") && has_non_null_arg(args, "refundMethod")
  {
    True -> [
      order_cancel_user_error(
        ["refund"],
        "Refund and refundMethod cannot both be present.",
        user_error_codes.invalid,
      ),
    ]
    False -> []
  }
  let staff_note_errors = case read_string_arg(args, "staffNote") {
    Some(note) ->
      case string.length(note) > 255 {
        True -> [
          order_cancel_user_error(
            ["staffNote"],
            "Staff note is too long (maximum is 255 characters)",
            user_error_codes.invalid,
          ),
        ]
        False -> []
      }
    _ -> []
  }
  list.append(refund_errors, staff_note_errors)
}

@internal
pub fn has_non_null_arg(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(args, name) {
    Ok(root_field.NullVal) | Error(_) -> False
    _ -> True
  }
}

@internal
pub fn order_cancel_state_error(
  order: OrderRecord,
) -> Option(OrderMutationUserError) {
  case order_is_cancelled(order) {
    True ->
      Some(order_cancel_user_error(
        ["orderId"],
        "Order has already been cancelled",
        user_error_codes.invalid,
      ))
    False ->
      case captured_string_field(order.data, "displayFinancialStatus") {
        Some("REFUNDED") ->
          Some(order_cancel_user_error(
            ["orderId"],
            "Cannot cancel a refunded order",
            user_error_codes.invalid,
          ))
        _ ->
          case order_has_open_return(order) {
            True ->
              Some(order_cancel_user_error(
                ["orderId"],
                "Cannot cancel an order with open returns",
                user_error_codes.invalid,
              ))
            False -> None
          }
      }
  }
}

@internal
pub fn order_is_cancelled(order: OrderRecord) -> Bool {
  case captured_object_field(order.data, "cancelledAt") {
    Some(CapturedNull) | None -> False
    _ -> True
  }
}

@internal
pub fn order_has_open_return(order: OrderRecord) -> Bool {
  order_returns(order.data)
  |> list.any(fn(order_return) {
    case captured_string_field(order_return, "status") {
      Some("OPEN") | Some("REQUESTED") -> True
      _ -> False
    }
  })
}

@internal
pub fn order_cancel_user_error(
  field_path: List(String),
  message: String,
  code: String,
) -> OrderMutationUserError {
  order_mutation_user_error(
    list.map(field_path, UserErrorField),
    message,
    Some(code),
  )
}

@internal
pub fn apply_order_cancel_update(
  order: OrderRecord,
  identity: SyntheticIdentityRegistry,
  reason: String,
) -> #(OrderRecord, SyntheticIdentityRegistry) {
  let #(timestamp, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let updated_data =
    order.data
    |> replace_captured_object_fields([
      #("closed", CapturedBool(True)),
      #("closedAt", CapturedString(timestamp)),
      #("cancelledAt", CapturedString(timestamp)),
      #("cancelReason", CapturedString(reason)),
      #("updatedAt", CapturedString(timestamp)),
    ])
  #(OrderRecord(..order, data: updated_data), next_identity)
}

@internal
pub fn serialize_order_cancel_payload(
  field: Selection,
  job_id: Option(String),
  user_errors: List(OrderMutationUserError),
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "job" -> #(key, case job_id {
              Some(id) -> serialize_job(child, id)
              None -> json.null()
            })
            "orderCancelUserErrors" | "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_order_mutation_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn handle_order_invoice_send(
  store: Store,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(String, Json, List(Json)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderInvoiceSend",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors)
    [] -> {
      let args = field_arguments(field, variables)
      case read_string_arg(args, "id") {
        Some(order_id) -> {
          // Pattern 2: invoice send returns the order shape but has no local
          // state change, so hydrate just enough to answer the selected order.
          let hydrated_store =
            maybe_hydrate_order_by_id(store, order_id, upstream)
          case store.get_order_by_id(hydrated_store, order_id) {
            Some(order) -> {
              let user_errors =
                order_invoice_send_email_user_errors(
                  args,
                  hydrated_store,
                  order,
                )
              let payload_order = case user_errors {
                [] -> Some(order)
                _ -> None
              }
              #(
                key,
                serialize_order_mutation_payload(
                  field,
                  payload_order,
                  user_errors,
                  fragments,
                ),
                [],
              )
            }
            None -> #(
              key,
              serialize_order_mutation_payload(
                field,
                None,
                [
                  mutation_user_error(["id"], "Order does not exist"),
                ],
                fragments,
              ),
              [],
            )
          }
        }
        None -> #(
          key,
          serialize_order_mutation_payload(
            field,
            None,
            [
              mutation_user_error(["id"], "Order does not exist"),
            ],
            fragments,
          ),
          [],
        )
      }
    }
  }
}

fn order_invoice_send_email_user_errors(
  args: Dict(String, root_field.ResolvedValue),
  store: Store,
  order: OrderRecord,
) -> List(OrderMutationUserError) {
  case explicit_invoice_email_to(args) {
    Some(email) ->
      case string.trim(email) {
        "" -> [
          order_invoice_send_email_error(
            "No recipient email address was provided",
          ),
        ]
        trimmed ->
          case valid_email_address(trimmed) {
            True -> []
            False -> [order_invoice_send_email_error("To is invalid")]
          }
      }
    None ->
      case resolved_order_invoice_email(store, order) {
        Some(_) -> []
        None -> [
          order_invoice_send_email_error(
            "No recipient email address was provided",
          ),
        ]
      }
  }
}

fn explicit_invoice_email_to(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case dict.get(args, "email") {
    Ok(root_field.ObjectVal(email_input)) ->
      Some(read_string(email_input, "to") |> option.unwrap(""))
    _ -> None
  }
}

fn resolved_order_invoice_email(
  store: Store,
  order: OrderRecord,
) -> Option(String) {
  captured_string_field(order.data, "email")
  |> option.then(non_blank_string)
  |> option.or(order_customer_email(store, order))
}

fn order_customer_email(store: Store, order: OrderRecord) -> Option(String) {
  case captured_object_field(order.data, "customer") {
    Some(customer) ->
      captured_string_field(customer, "email")
      |> option.then(non_blank_string)
      |> option.or(customer_record_email(store, customer))
    None -> None
  }
}

fn customer_record_email(
  store: Store,
  customer: CapturedJsonValue,
) -> Option(String) {
  use customer_id <- option.then(captured_string_field(customer, "id"))
  use record <- option.then(store.get_effective_customer_by_id(
    store,
    customer_id,
  ))
  record.email |> option.then(non_blank_string)
}

fn non_blank_string(value: String) -> Option(String) {
  case string.trim(value) {
    "" -> None
    trimmed -> Some(trimmed)
  }
}

fn order_invoice_send_email_error(message: String) -> OrderMutationUserError {
  order_mutation_user_error(
    [],
    message,
    Some(user_error_codes.order_invoice_send_unsuccessful),
  )
}

@internal
pub fn handle_order_mark_as_paid_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
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
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderMarkAsPaid",
      [RequiredArgument(name: "input", expected_type: "OrderMarkAsPaidInput!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let order_id = case dict.get(args, "input") {
        Ok(root_field.ObjectVal(input)) -> read_string(input, "id")
        _ -> None
      }
      case order_id {
        None -> {
          let payload =
            serialize_order_mutation_payload(
              field,
              None,
              [
                mutation_user_error(["id"], "Order does not exist"),
              ],
              fragments,
            )
          #(key, payload, store, identity, [], [], [])
        }
        Some(id) -> {
          // Pattern 2: hydrate the order totals/transactions before applying
          // the local mark-as-paid projection.
          let hydrated_store = maybe_hydrate_order_by_id(store, id, upstream)
          case store.get_order_by_id(hydrated_store, id) {
            None -> {
              let payload =
                serialize_order_mutation_payload(
                  field,
                  None,
                  [
                    mutation_user_error(["id"], "Order does not exist"),
                  ],
                  fragments,
                )
              #(key, payload, hydrated_store, identity, [], [], [])
            }
            Some(order) -> {
              case order_mark_as_paid_invalid_error(order) {
                Some(error) -> {
                  let payload =
                    serialize_order_mutation_payload(
                      field,
                      Some(order),
                      [error],
                      fragments,
                    )
                  #(key, payload, hydrated_store, identity, [], [], [])
                }
                None -> {
                  let #(updated_order, next_identity) =
                    apply_order_mark_as_paid_update(order, identity)
                  let next_store =
                    store.stage_order(hydrated_store, updated_order)
                  let payload =
                    serialize_order_mutation_payload(
                      field,
                      Some(updated_order),
                      [],
                      fragments,
                    )
                  let draft =
                    single_root_log_draft(
                      "orderMarkAsPaid",
                      [id],
                      store_types.Staged,
                      "orders",
                      "stage-locally",
                      Some(
                        "Locally staged orderMarkAsPaid in shopify-draft-proxy.",
                      ),
                    )
                  #(key, payload, next_store, next_identity, [id], [], [draft])
                }
              }
            }
          }
        }
      }
    }
  }
}

@internal
pub fn order_mark_as_paid_invalid_error(
  order: OrderRecord,
) -> Option(OrderMutationUserError) {
  case order_cancelled(order) {
    True ->
      Some(invalid_id_user_error("Order is cancelled and cannot be marked paid"))
    False ->
      case captured_string_field(order.data, "displayFinancialStatus") {
        Some("PAID") ->
          Some(invalid_id_user_error("Order cannot be marked as paid."))
        Some("REFUNDED") ->
          Some(invalid_id_user_error("Order cannot be marked as paid."))
        Some("PARTIALLY_REFUNDED") ->
          Some(invalid_id_user_error("Order cannot be marked as paid."))
        Some("VOIDED") ->
          Some(invalid_id_user_error("Order cannot be marked as paid."))
        _ -> None
      }
  }
}

@internal
pub fn order_cancelled(order: OrderRecord) -> Bool {
  case captured_object_field(order.data, "cancelledAt") {
    Some(CapturedNull) | None -> False
    _ -> True
  }
}

@internal
pub fn invalid_id_user_error(message: String) -> OrderMutationUserError {
  order_mutation_user_error([UserErrorField("id")], message, Some("INVALID"))
}

@internal
pub fn apply_order_mark_as_paid_update(
  order: OrderRecord,
  identity: SyntheticIdentityRegistry,
) -> #(OrderRecord, SyntheticIdentityRegistry) {
  let #(transaction_id, identity_after_transaction) =
    synthetic_identity.make_synthetic_gid(identity, "OrderTransaction")
  let #(timestamp, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_transaction)
  let amount_set = order_payment_amount_set(order)
  let transaction =
    CapturedObject([
      #("id", CapturedString(transaction_id)),
      #("kind", CapturedString("SALE")),
      #("status", CapturedString("SUCCESS")),
      #("gateway", CapturedString("manual")),
      #("amountSet", amount_set),
    ])
  let updated_data =
    order.data
    |> replace_captured_object_fields([
      #("updatedAt", CapturedString(timestamp)),
      #("displayFinancialStatus", CapturedString("PAID")),
      #("paymentGatewayNames", CapturedArray([CapturedString("manual")])),
      #("totalOutstandingSet", order_money_set(order, 0.0)),
      #("totalReceivedSet", amount_set),
      #("netPaymentSet", amount_set),
      #(
        "transactions",
        CapturedArray(list.append(order_transactions(order), [transaction])),
      ),
    ])
  #(OrderRecord(..order, data: updated_data), next_identity)
}

@internal
pub fn handle_order_capture_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let args = field_arguments(field, variables)
  case read_object(args, "input") {
    None -> {
      let payload =
        serialize_order_capture_payload(
          field,
          None,
          None,
          [inferred_user_error(["input"], "Input is required.")],
          fragments,
        )
      #(key, payload, store, identity, [], [
        payment_log_draft("orderCapture", [], store_types.Failed),
      ])
    }
    Some(input) -> {
      let order_id =
        read_string(input, "id") |> option.or(read_string(input, "orderId"))
      let parent_transaction_id =
        read_string(input, "parentTransactionId")
        |> option.or(read_string(input, "transactionId"))
      case order_id {
        None -> {
          let payload =
            serialize_order_capture_payload(
              field,
              None,
              None,
              [inferred_user_error(["input", "id"], "Order does not exist")],
              fragments,
            )
          #(key, payload, store, identity, [], [
            payment_log_draft("orderCapture", [], store_types.Failed),
          ])
        }
        Some(order_id) ->
          case store.get_order_by_id(store, order_id) {
            None -> {
              let payload =
                serialize_order_capture_payload(
                  field,
                  None,
                  None,
                  [inferred_user_error(["input", "id"], "Order does not exist")],
                  fragments,
                )
              #(key, payload, store, identity, [], [
                payment_log_draft("orderCapture", [], store_types.Failed),
              ])
            }
            Some(order) ->
              case parent_transaction_id {
                None -> {
                  let payload =
                    serialize_order_capture_payload(
                      field,
                      None,
                      Some(order),
                      [
                        inferred_user_error(
                          ["input", "parentTransactionId"],
                          "Transaction does not exist",
                        ),
                      ],
                      fragments,
                    )
                  #(key, payload, store, identity, [], [
                    payment_log_draft(
                      "orderCapture",
                      [order.id],
                      store_types.Failed,
                    ),
                  ])
                }
                Some(transaction_id) ->
                  case find_transaction(order, transaction_id) {
                    None -> {
                      let payload =
                        serialize_order_capture_payload(
                          field,
                          None,
                          Some(order),
                          [
                            inferred_user_error(
                              ["input", "parentTransactionId"],
                              "Transaction does not exist",
                            ),
                          ],
                          fragments,
                        )
                      #(key, payload, store, identity, [], [
                        payment_log_draft(
                          "orderCapture",
                          [order.id],
                          store_types.Failed,
                        ),
                      ])
                    }
                    Some(authorization) ->
                      capture_order_payment(
                        key,
                        store,
                        identity,
                        field,
                        fragments,
                        order,
                        authorization,
                        input,
                      )
                  }
              }
          }
      }
    }
  }
}

@internal
pub fn capture_order_payment(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  authorization: CapturedJsonValue,
  input: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let remaining = capturable_amount_for_authorization(order, authorization)
  let amount = payment_input_amount(input, remaining)
  case remaining <=. 0.0 {
    True -> {
      let payload =
        serialize_order_capture_payload(
          field,
          None,
          Some(order),
          [
            inferred_user_error(
              ["input", "parentTransactionId"],
              "Transaction is not capturable",
            ),
          ],
          fragments,
        )
      #(key, payload, store, identity, [], [
        payment_log_draft("orderCapture", [order.id], store_types.Failed),
      ])
    }
    False ->
      case amount <=. 0.0 {
        True -> {
          let payload =
            serialize_order_capture_payload(
              field,
              None,
              Some(order),
              [
                inferred_user_error(
                  ["input", "amount"],
                  "Amount must be greater than zero",
                ),
              ],
              fragments,
            )
          #(key, payload, store, identity, [], [
            payment_log_draft("orderCapture", [order.id], store_types.Failed),
          ])
        }
        False ->
          case amount >. remaining {
            True -> {
              let payload =
                serialize_order_capture_payload(
                  field,
                  None,
                  Some(order),
                  [
                    inferred_user_error(
                      ["input", "amount"],
                      "Amount exceeds capturable amount",
                    ),
                  ],
                  fragments,
                )
              #(key, payload, store, identity, [], [
                payment_log_draft(
                  "orderCapture",
                  [order.id],
                  store_types.Failed,
                ),
              ])
            }
            False -> {
              let #(payment_reference_id, identity_after_reference) =
                synthetic_identity.make_synthetic_gid(
                  identity,
                  "PaymentReference",
                )
              let #(transaction, identity_after_capture) =
                build_payment_transaction(
                  identity_after_reference,
                  "CAPTURE",
                  payment_input_money_set(input, order, amount),
                  captured_string_field(authorization, "gateway"),
                  captured_string_field(authorization, "id"),
                  Some(payment_reference_id),
                )
              let final_capture = read_bool(input, "finalCapture", False)
              let remaining_after_capture = max_float(0.0, remaining -. amount)
              let #(extra_transactions, next_identity) = case
                final_capture && remaining_after_capture >. 0.0
              {
                True -> {
                  let #(void_transaction, identity_after_void) =
                    build_payment_transaction(
                      identity_after_capture,
                      "VOID",
                      order_money_set(order, remaining_after_capture),
                      captured_string_field(authorization, "gateway"),
                      captured_string_field(authorization, "id"),
                      None,
                    )
                  #([void_transaction], identity_after_void)
                }
                False -> #([], identity_after_capture)
              }
              let updated_order =
                order
                |> append_order_transactions([transaction, ..extra_transactions])
                |> apply_payment_derived_fields
              let next_store = store.stage_order(store, updated_order)
              let payload =
                serialize_order_capture_payload(
                  field,
                  Some(transaction),
                  Some(updated_order),
                  [],
                  fragments,
                )
              #(key, payload, next_store, next_identity, [order.id], [
                payment_log_draft(
                  "orderCapture",
                  [order.id],
                  store_types.Staged,
                ),
              ])
            }
          }
      }
  }
}

@internal
pub fn handle_transaction_void_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let args = field_arguments(field, variables)
  let #(transaction_id, _) = transaction_void_reference(args)
  case transaction_id {
    None -> {
      let payload =
        serialize_transaction_void_payload(
          field,
          None,
          [
            transaction_void_user_error(
              "Transaction does not exist",
              "TRANSACTION_NOT_FOUND",
            ),
          ],
          fragments,
        )
      #(key, payload, store, identity, [], [
        payment_log_draft("transactionVoid", [], store_types.Failed),
      ])
    }
    Some(transaction_id) ->
      case find_order_with_transaction(store, transaction_id) {
        None -> {
          let payload =
            serialize_transaction_void_payload(
              field,
              None,
              [
                transaction_void_user_error(
                  "Transaction does not exist",
                  "TRANSACTION_NOT_FOUND",
                ),
              ],
              fragments,
            )
          #(key, payload, store, identity, [], [
            payment_log_draft("transactionVoid", [], store_types.Failed),
          ])
        }
        Some(match) -> {
          let #(order, transaction) = match
          void_order_transaction(
            key,
            store,
            identity,
            field,
            fragments,
            order,
            transaction,
          )
        }
      }
  }
}

@internal
pub fn void_order_transaction(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  authorization: CapturedJsonValue,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let user_errors = case is_successful_authorization(authorization) {
    False -> [
      transaction_void_user_error(
        "Parent transaction must be a successful authorization",
        "AUTH_NOT_SUCCESSFUL",
      ),
    ]
    True ->
      case authorization_is_unvoidable(order, authorization) {
        True -> [
          transaction_void_user_error(
            "Parent transaction require a parent_id referring to a voidable transaction",
            "AUTH_NOT_VOIDABLE",
          ),
        ]
        False -> []
      }
  }
  case user_errors {
    [_, ..] -> {
      let payload =
        serialize_transaction_void_payload(field, None, user_errors, fragments)
      #(key, payload, store, identity, [], [
        payment_log_draft("transactionVoid", [order.id], store_types.Failed),
      ])
    }
    [] -> {
      let #(transaction, next_identity) =
        build_payment_transaction(
          identity,
          "VOID",
          captured_object_field(authorization, "amountSet")
            |> option.unwrap(money_set(0.0, order_currency_code(order))),
          captured_string_field(authorization, "gateway"),
          captured_string_field(authorization, "id"),
          None,
        )
      let updated_order =
        order
        |> append_order_transactions([transaction])
        |> apply_payment_derived_fields
      let next_store = store.stage_order(store, updated_order)
      let payload =
        serialize_transaction_void_payload(
          field,
          Some(transaction),
          [],
          fragments,
        )
      #(key, payload, next_store, next_identity, [order.id], [
        payment_log_draft("transactionVoid", [order.id], store_types.Staged),
      ])
    }
  }
}

@internal
pub fn transaction_void_user_error(
  message: String,
  code: String,
) -> #(List(String), String, Option(String)) {
  user_error(["parentTransactionId"], message, Some(code))
}

@internal
pub fn authorization_is_unvoidable(
  order: OrderRecord,
  authorization: CapturedJsonValue,
) -> Bool {
  let authorization_id =
    captured_string_field(authorization, "id") |> option.unwrap("")
  transaction_has_voiding_child(order, authorization_id)
  || captured_amount_for_authorization(order, authorization_id) >. 0.0
  || authorization_is_expired(authorization)
}

@internal
pub fn authorization_is_expired(authorization: CapturedJsonValue) -> Bool {
  let expires_at =
    captured_string_field(authorization, "authorizationExpiresAt")
    |> option.or(captured_string_field(authorization, "expiresAt"))
  case expires_at {
    None -> False
    Some(expires_at) ->
      case
        iso_timestamp.parse_iso(expires_at),
        iso_timestamp.parse_iso(iso_timestamp.now_iso())
      {
        Ok(expires_ms), Ok(now_ms) -> expires_ms <= now_ms
        _, _ -> False
      }
  }
}

@internal
pub fn handle_order_create_mandate_payment_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let args = field_arguments(field, variables)
  let input = read_object(args, "input") |> option.unwrap(args)
  let order_id =
    read_string(input, "id") |> option.or(read_string(input, "orderId"))
  case order_id {
    None -> {
      let payload =
        serialize_mandate_payment_payload(
          field,
          None,
          None,
          None,
          [inferred_user_error(["id"], "Order does not exist")],
          fragments,
        )
      #(key, payload, store, identity, [], [
        payment_log_draft("orderCreateMandatePayment", [], store_types.Failed),
      ])
    }
    Some(order_id) ->
      case store.get_order_by_id(store, order_id) {
        None -> {
          let payload =
            serialize_mandate_payment_payload(
              field,
              None,
              None,
              None,
              [inferred_user_error(["id"], "Order does not exist")],
              fragments,
            )
          #(key, payload, store, identity, [], [
            payment_log_draft(
              "orderCreateMandatePayment",
              [],
              store_types.Failed,
            ),
          ])
        }
        Some(order) -> {
          let idempotency_key = read_string(input, "idempotencyKey")
          case idempotency_key {
            None -> {
              let payload =
                serialize_mandate_payment_payload(
                  field,
                  None,
                  None,
                  Some(order),
                  [
                    inferred_user_error(
                      ["idempotencyKey"],
                      "Idempotency key is required",
                    ),
                  ],
                  fragments,
                )
              #(key, payload, store, identity, [], [
                payment_log_draft(
                  "orderCreateMandatePayment",
                  [order.id],
                  store_types.Failed,
                ),
              ])
            }
            Some(idempotency_key) ->
              case find_mandate_payment(order, idempotency_key) {
                Some(payment) -> {
                  let payload =
                    serialize_mandate_payment_payload(
                      field,
                      Some(payment),
                      captured_string_field(payment, "paymentReferenceId"),
                      Some(order),
                      [],
                      fragments,
                    )
                  #(key, payload, store, identity, [order.id], [
                    payment_log_draft(
                      "orderCreateMandatePayment",
                      [order.id],
                      store_types.Staged,
                    ),
                  ])
                }
                None ->
                  create_mandate_payment(
                    key,
                    store,
                    identity,
                    field,
                    fragments,
                    order,
                    input,
                    idempotency_key,
                  )
              }
          }
        }
      }
  }
}

@internal
pub fn create_mandate_payment(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  order: OrderRecord,
  input: Dict(String, root_field.ResolvedValue),
  idempotency_key: String,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let amount =
    payment_input_amount(
      input,
      captured_money_amount(order.data, "totalOutstandingSet")
        |> nonzero_float(captured_money_amount(
          order.data,
          "currentTotalPriceSet",
        )),
    )
  case amount <=. 0.0 {
    True -> {
      let payload =
        serialize_mandate_payment_payload(
          field,
          None,
          None,
          Some(order),
          [inferred_user_error(["amount"], "Amount must be greater than zero")],
          fragments,
        )
      #(key, payload, store, identity, [], [
        payment_log_draft(
          "orderCreateMandatePayment",
          [order.id],
          store_types.Failed,
        ),
      ])
    }
    False -> {
      let payment_reference_id = order.id <> "/" <> idempotency_key
      let transaction_kind = case read_bool(input, "autoCapture", True) {
        True -> "SALE"
        False -> "AUTHORIZATION"
      }
      let #(transaction, identity_after_transaction) =
        build_payment_transaction(
          identity,
          transaction_kind,
          payment_input_money_set(input, order, amount),
          Some("mandate"),
          None,
          Some(payment_reference_id),
        )
      let #(job_id, identity_after_job) =
        synthetic_identity.make_synthetic_gid(identity_after_transaction, "Job")
      let mandate_payment =
        CapturedObject([
          #("idempotencyKey", CapturedString(idempotency_key)),
          #("jobId", CapturedString(job_id)),
          #("paymentReferenceId", CapturedString(payment_reference_id)),
          #(
            "transactionId",
            optional_captured_string(captured_string_field(transaction, "id")),
          ),
        ])
      let updated_order =
        order
        |> append_order_transactions([transaction])
        |> append_mandate_payment(mandate_payment)
        |> append_payment_gateway("mandate")
        |> apply_payment_derived_fields
      let next_store = store.stage_order(store, updated_order)
      let payload =
        serialize_mandate_payment_payload(
          field,
          Some(mandate_payment),
          Some(payment_reference_id),
          Some(updated_order),
          [],
          fragments,
        )
      #(key, payload, next_store, identity_after_job, [order.id], [
        payment_log_draft(
          "orderCreateMandatePayment",
          [order.id],
          store_types.Staged,
        ),
      ])
    }
  }
}

@internal
pub fn build_payment_transaction(
  identity: SyntheticIdentityRegistry,
  kind: String,
  amount_set: CapturedJsonValue,
  gateway: Option(String),
  parent_transaction_id: Option(String),
  payment_reference_id: Option(String),
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(transaction_id, identity_after_transaction) =
    synthetic_identity.make_synthetic_gid(identity, "OrderTransaction")
  let #(payment_id, identity_after_payment) =
    synthetic_identity.make_synthetic_gid(identity_after_transaction, "Payment")
  let #(processed_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_payment)
  let parent_transaction = case parent_transaction_id {
    Some(id) ->
      CapturedObject([
        #("id", CapturedString(id)),
        #("kind", CapturedString("AUTHORIZATION")),
        #("status", CapturedString("SUCCESS")),
      ])
    None -> CapturedNull
  }
  #(
    CapturedObject([
      #("id", CapturedString(transaction_id)),
      #("kind", CapturedString(kind)),
      #("status", CapturedString("SUCCESS")),
      #("gateway", optional_captured_string(gateway)),
      #("amountSet", amount_set),
      #("parentTransactionId", optional_captured_string(parent_transaction_id)),
      #("parentTransaction", parent_transaction),
      #("paymentId", CapturedString(payment_id)),
      #("paymentReferenceId", optional_captured_string(payment_reference_id)),
      #("processedAt", CapturedString(processed_at)),
    ]),
    next_identity,
  )
}

@internal
pub fn append_order_transactions(
  order: OrderRecord,
  transactions: List(CapturedJsonValue),
) -> OrderRecord {
  let updated =
    order.data
    |> replace_captured_object_fields([
      #(
        "transactions",
        CapturedArray(list.append(order_transactions(order), transactions)),
      ),
    ])
  OrderRecord(..order, data: updated)
}

@internal
pub fn append_mandate_payment(
  order: OrderRecord,
  payment: CapturedJsonValue,
) -> OrderRecord {
  let existing = mandate_payments(order)
  let updated =
    order.data
    |> replace_captured_object_fields([
      #("mandatePayments", CapturedArray(list.append(existing, [payment]))),
    ])
  OrderRecord(..order, data: updated)
}

@internal
pub fn append_payment_gateway(
  order: OrderRecord,
  gateway: String,
) -> OrderRecord {
  let existing = payment_gateway_names(order)
  let gateways = case list.contains(existing, gateway) {
    True -> existing
    False -> list.append(existing, [gateway])
  }
  let updated =
    order.data
    |> replace_captured_object_fields([
      #(
        "paymentGatewayNames",
        CapturedArray(list.map(gateways, CapturedString)),
      ),
    ])
  OrderRecord(..order, data: updated)
}

@internal
pub fn apply_payment_derived_fields(order: OrderRecord) -> OrderRecord {
  let currency_code = order_currency_code(order)
  let presentment_currency_code = order_presentment_currency_code(order)
  let received = total_received_amount(order)
  let presentment_received = total_received_presentment_amount(order)
  let total =
    captured_money_amount(order.data, "currentTotalPriceSet")
    |> nonzero_float(captured_money_amount(order.data, "totalPriceSet"))
  let presentment_total =
    captured_money_presentment_amount(order.data, "currentTotalPriceSet")
    |> nonzero_float(captured_money_presentment_amount(
      order.data,
      "totalPriceSet",
    ))
  let outstanding = max_float(0.0, total -. received)
  let presentment_outstanding =
    max_float(0.0, presentment_total -. presentment_received)
  let capturable = total_capturable_amount(order)
  let presentment_capturable = total_capturable_presentment_amount(order)
  let has_voided_authorization =
    order_transactions(order)
    |> list.any(fn(transaction) {
      is_successful_authorization(transaction)
      && transaction_has_voiding_child(
        order,
        captured_string_field(transaction, "id") |> option.unwrap(""),
      )
    })
  let display_status = case received >=. total && total >. 0.0 {
    True -> "PAID"
    False ->
      case received >. 0.0 {
        True -> "PARTIALLY_PAID"
        False ->
          case capturable >. 0.0 {
            True -> "AUTHORIZED"
            False ->
              case has_voided_authorization {
                True -> "VOIDED"
                False ->
                  captured_string_field(order.data, "displayFinancialStatus")
                  |> option.unwrap("PENDING")
              }
          }
      }
  }
  let updated =
    order.data
    |> replace_captured_object_fields([
      #("displayFinancialStatus", CapturedString(display_status)),
      #("capturable", CapturedBool(capturable >. 0.0)),
      #("totalCapturable", CapturedString(float.to_string(capturable))),
      #(
        "totalCapturableSet",
        money_set_with_presentment(
          capturable,
          currency_code,
          presentment_capturable,
          presentment_currency_code,
        ),
      ),
      #(
        "totalOutstandingSet",
        money_set_with_presentment(
          outstanding,
          currency_code,
          presentment_outstanding,
          presentment_currency_code,
        ),
      ),
      #(
        "totalReceivedSet",
        money_set_with_presentment(
          received,
          currency_code,
          presentment_received,
          presentment_currency_code,
        ),
      ),
      #(
        "netPaymentSet",
        money_set_with_presentment(
          received,
          currency_code,
          presentment_received,
          presentment_currency_code,
        ),
      ),
    ])
  OrderRecord(..order, data: updated)
}

@internal
pub fn find_order_with_transaction(
  store: Store,
  transaction_id: String,
) -> Option(#(OrderRecord, CapturedJsonValue)) {
  store.list_effective_orders(store)
  |> list.find_map(fn(order) {
    case find_transaction(order, transaction_id) {
      Some(transaction) -> Ok(#(order, transaction))
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn find_transaction(
  order: OrderRecord,
  transaction_id: String,
) -> Option(CapturedJsonValue) {
  order_transactions(order)
  |> list.find(fn(transaction) {
    captured_string_field(transaction, "id") == Some(transaction_id)
  })
  |> option.from_result
}

@internal
pub fn is_successful_authorization(transaction: CapturedJsonValue) -> Bool {
  captured_string_field(transaction, "kind") == Some("AUTHORIZATION")
  && captured_string_field(transaction, "status") == Some("SUCCESS")
}

@internal
pub fn is_successful_payment_capture(transaction: CapturedJsonValue) -> Bool {
  captured_string_field(transaction, "status") == Some("SUCCESS")
  && case captured_string_field(transaction, "kind") {
    Some("SALE") | Some("CAPTURE") | Some("MANDATE_PAYMENT") -> True
    _ -> False
  }
}

@internal
pub fn transaction_has_voiding_child(
  order: OrderRecord,
  parent_transaction_id: String,
) -> Bool {
  order_transactions(order)
  |> list.any(fn(transaction) {
    captured_string_field(transaction, "kind") == Some("VOID")
    && captured_string_field(transaction, "status") == Some("SUCCESS")
    && captured_string_field(transaction, "parentTransactionId")
    == Some(parent_transaction_id)
  })
}

@internal
pub fn captured_amount_for_authorization(
  order: OrderRecord,
  parent_transaction_id: String,
) -> Float {
  order_transactions(order)
  |> list.filter(fn(transaction) {
    captured_string_field(transaction, "kind") == Some("CAPTURE")
    && captured_string_field(transaction, "status") == Some("SUCCESS")
    && captured_string_field(transaction, "parentTransactionId")
    == Some(parent_transaction_id)
  })
  |> list.fold(0.0, fn(sum, transaction) {
    sum +. captured_money_amount(transaction, "amountSet")
  })
}

@internal
pub fn captured_presentment_amount_for_authorization(
  order: OrderRecord,
  parent_transaction_id: String,
) -> Float {
  order_transactions(order)
  |> list.filter(fn(transaction) {
    captured_string_field(transaction, "kind") == Some("CAPTURE")
    && captured_string_field(transaction, "status") == Some("SUCCESS")
    && captured_string_field(transaction, "parentTransactionId")
    == Some(parent_transaction_id)
  })
  |> list.fold(0.0, fn(sum, transaction) {
    sum +. captured_money_presentment_amount(transaction, "amountSet")
  })
}

@internal
pub fn capturable_amount_for_authorization(
  order: OrderRecord,
  authorization: CapturedJsonValue,
) -> Float {
  let authorization_id =
    captured_string_field(authorization, "id") |> option.unwrap("")
  case
    !is_successful_authorization(authorization)
    || transaction_has_voiding_child(order, authorization_id)
  {
    True -> 0.0
    False ->
      max_float(
        0.0,
        captured_money_amount(authorization, "amountSet")
          -. captured_amount_for_authorization(order, authorization_id),
      )
  }
}

@internal
pub fn capturable_presentment_amount_for_authorization(
  order: OrderRecord,
  authorization: CapturedJsonValue,
) -> Float {
  let authorization_id =
    captured_string_field(authorization, "id") |> option.unwrap("")
  case
    !is_successful_authorization(authorization)
    || transaction_has_voiding_child(order, authorization_id)
  {
    True -> 0.0
    False -> {
      let captured_presentment =
        captured_presentment_amount_for_authorization(order, authorization_id)
      max_float(
        0.0,
        captured_money_presentment_amount(authorization, "amountSet")
          -. captured_presentment,
      )
    }
  }
}

@internal
pub fn total_capturable_amount(order: OrderRecord) -> Float {
  order_transactions(order)
  |> list.filter(is_successful_authorization)
  |> list.fold(0.0, fn(sum, transaction) {
    sum +. capturable_amount_for_authorization(order, transaction)
  })
}

@internal
pub fn total_capturable_presentment_amount(order: OrderRecord) -> Float {
  order_transactions(order)
  |> list.filter(is_successful_authorization)
  |> list.fold(0.0, fn(sum, transaction) {
    sum +. capturable_presentment_amount_for_authorization(order, transaction)
  })
}

@internal
pub fn total_received_amount(order: OrderRecord) -> Float {
  order_transactions(order)
  |> list.filter(is_successful_payment_capture)
  |> list.fold(0.0, fn(sum, transaction) {
    sum +. captured_money_amount(transaction, "amountSet")
  })
}

@internal
pub fn total_received_presentment_amount(order: OrderRecord) -> Float {
  order_transactions(order)
  |> list.filter(is_successful_payment_capture)
  |> list.fold(0.0, fn(sum, transaction) {
    sum +. captured_money_presentment_amount(transaction, "amountSet")
  })
}

@internal
pub fn payment_gateway_names(order: OrderRecord) -> List(String) {
  case captured_object_field(order.data, "paymentGatewayNames") {
    Some(CapturedArray(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          CapturedString(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn mandate_payments(order: OrderRecord) -> List(CapturedJsonValue) {
  case captured_object_field(order.data, "mandatePayments") {
    Some(CapturedArray(values)) -> values
    _ -> []
  }
}

@internal
pub fn find_mandate_payment(
  order: OrderRecord,
  idempotency_key: String,
) -> Option(CapturedJsonValue) {
  mandate_payments(order)
  |> list.find(fn(payment) {
    captured_string_field(payment, "idempotencyKey") == Some(idempotency_key)
  })
  |> option.from_result
}

@internal
pub fn payment_input_amount(
  input: Dict(String, root_field.ResolvedValue),
  fallback: Float,
) -> Float {
  case dict.get(input, "amount") {
    Ok(root_field.ObjectVal(amount)) ->
      read_number(amount, "amount") |> option.unwrap(fallback)
    _ ->
      read_number(input, "amount")
      |> option.or(case read_object(input, "amountSet") {
        Some(amount_set) ->
          case read_object(amount_set, "shopMoney") {
            Some(shop_money) -> read_number(shop_money, "amount")
            None -> None
          }
        None -> None
      })
      |> option.unwrap(fallback)
  }
}

@internal
pub fn payment_input_money_set(
  input: Dict(String, root_field.ResolvedValue),
  order: OrderRecord,
  fallback_amount: Float,
) -> CapturedJsonValue {
  case read_object(input, "amountSet") {
    Some(amount_set) ->
      order_money_set_from_input(
        Some(amount_set),
        payment_input_currency(input, order_currency_code(order)),
        fallback_amount,
      )
    None -> order_money_set(order, payment_input_amount(input, fallback_amount))
  }
}

@internal
pub fn payment_input_currency(
  input: Dict(String, root_field.ResolvedValue),
  fallback: String,
) -> String {
  read_string(input, "currency")
  |> option.or(case read_object(input, "amount") {
    Some(amount) -> read_string(amount, "currencyCode")
    None -> None
  })
  |> option.or(case read_object(input, "amountSet") {
    Some(amount_set) ->
      case read_object(amount_set, "shopMoney") {
        Some(shop_money) -> read_string(shop_money, "currencyCode")
        None -> None
      }
    None -> None
  })
  |> option.unwrap(fallback)
}

@internal
pub fn transaction_void_reference(
  args: Dict(String, root_field.ResolvedValue),
) -> #(Option(String), String) {
  case read_string(args, "parentTransactionId") {
    Some(id) -> #(Some(id), "parentTransactionId")
    None ->
      case read_string(args, "id") {
        Some(id) -> #(Some(id), "id")
        None ->
          case read_object(args, "input") {
            Some(input) ->
              case read_string(input, "parentTransactionId") {
                Some(id) -> #(Some(id), "parentTransactionId")
                None -> #(read_string(input, "id"), "id")
              }
            None -> #(None, "parentTransactionId")
          }
      }
  }
}

@internal
pub fn serialize_order_capture_payload(
  field: Selection,
  transaction: Option(CapturedJsonValue),
  order: Option(OrderRecord),
  user_errors: List(#(List(String), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  json.object(
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "transaction" -> #(
              key,
              serialize_captured_selection(child, transaction, fragments),
            )
            "order" -> #(key, case order {
              Some(order) ->
                serialize_order_node(None, child, order, fragments, dict.new())
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

@internal
pub fn serialize_transaction_void_payload(
  field: Selection,
  transaction: Option(CapturedJsonValue),
  user_errors: List(#(List(String), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  json.object(
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "transaction" -> #(
              key,
              serialize_captured_selection(child, transaction, fragments),
            )
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

@internal
pub fn serialize_mandate_payment_payload(
  field: Selection,
  payment: Option(CapturedJsonValue),
  payment_reference_id: Option(String),
  order: Option(OrderRecord),
  user_errors: List(#(List(String), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  json.object(
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "job" -> #(key, case payment {
              Some(payment) ->
                serialize_job_selection(
                  child,
                  captured_string_field(payment, "jobId"),
                )
              None -> json.null()
            })
            "paymentReferenceId" -> #(
              key,
              graphql_helpers.option_string_json(payment_reference_id),
            )
            "order" -> #(key, case order {
              Some(order) ->
                serialize_order_node(None, child, order, fragments, dict.new())
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

@internal
pub fn serialize_job_selection(
  field: Selection,
  job_id: Option(String),
) -> Json {
  case job_id {
    None -> json.null()
    Some(id) ->
      json.object(
        list.map(selection_children(field), fn(child) {
          let key = get_field_response_key(child)
          case child {
            Field(name: name, ..) ->
              case name.value {
                "id" -> #(key, json.string(id))
                "done" -> #(key, json.bool(True))
                _ -> #(key, json.null())
              }
            _ -> #(key, json.null())
          }
        }),
      )
  }
}

@internal
pub fn payment_log_draft(
  root_name: String,
  staged_ids: List(String),
  status: store.EntryStatus,
) -> LogDraft {
  single_root_log_draft(
    root_name,
    staged_ids,
    status,
    "payments",
    "stage-locally",
    Some("Locally staged " <> root_name <> " in shopify-draft-proxy."),
  )
}
