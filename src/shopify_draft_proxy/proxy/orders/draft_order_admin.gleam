//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, SrcList, SrcString, get_field_response_key,
  project_graphql_value, src_object,
}

import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, RequiredArgument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/orders/common.{
  captured_json_source, captured_object_field, captured_string_field,
  draft_order_gid_tail, field_arguments, inferred_nullable_user_error,
  inferred_user_error, order_fulfillment_orders, order_fulfillments,
  order_transactions, read_bool, read_object, read_string, read_string_arg,
  read_string_list, replace_captured_object_fields, selection_children,
  serialize_job, serialize_nullable_user_error, serialize_user_error, user_error,
  user_error_source,
}
import shopify_draft_proxy/proxy/orders/draft_order_builders.{
  build_draft_order_from_input,
}
import shopify_draft_proxy/proxy/orders/draft_order_tags.{
  append_too_many_draft_order_tags_error, draft_order_bulk_tag_input,
  draft_order_tag_count_exceeds_limit, draft_order_tags,
  draft_order_tags_max_input_size_error, update_draft_order_tags,
}
import shopify_draft_proxy/proxy/orders/draft_orders.{
  build_updated_draft_order, captured_order_currency, complete_draft_order,
  draft_order_input_tag_count_over_graphql_limit, draft_order_line_items,
  duplicate_draft_order, serialize_draft_order_mutation_payload,
  validate_draft_order_calculate_input, validate_draft_order_input_tags,
}
import shopify_draft_proxy/proxy/orders/hydration.{
  maybe_hydrate_draft_order_by_id, maybe_hydrate_draft_order_customer_from_input,
  maybe_hydrate_draft_order_variant_catalog_from_input,
}
import shopify_draft_proxy/proxy/orders/serializers.{
  order_returns, serialize_draft_order_node,
}

import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/proxy/user_error_codes

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type DraftOrderRecord, type OrderRecord, CapturedArray,
  CapturedString, CustomerOrderSummaryRecord, OrderRecord,
}

@internal
pub fn handle_draft_order_complete(
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
      "draftOrderComplete",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let id = read_string_arg(args, "id")
      case id {
        Some(id) -> {
          // Pattern 2: completing an upstream-created draft needs the prior
          // draft payload before the proxy can stage the completed local copy.
          let hydrated_store =
            maybe_hydrate_draft_order_by_id(store, id, upstream)
          case store.get_draft_order_by_id(hydrated_store, id) {
            Some(draft_order) -> {
              case read_string_arg(args, "paymentGatewayId") {
                Some(payment_gateway_id) -> {
                  case
                    store.payment_gateway_by_id(
                      hydrated_store,
                      payment_gateway_id,
                    )
                  {
                    Some(payment_gateway) ->
                      case payment_gateway.active {
                        True -> {
                          let #(completed_draft_order, next_identity) =
                            complete_draft_order(
                              store,
                              identity,
                              draft_order,
                              read_string_arg(args, "sourceName"),
                              read_bool(args, "paymentPending", False),
                              Some(payment_gateway),
                            )
                          let next_store =
                            stage_completed_draft_order_graph(
                              hydrated_store,
                              completed_draft_order,
                            )
                          let payload =
                            serialize_draft_order_mutation_payload(
                              field,
                              Some(completed_draft_order),
                              [],
                              fragments,
                            )
                          let draft =
                            single_root_log_draft(
                              "draftOrderComplete",
                              [completed_draft_order.id],
                              store_types.Staged,
                              "orders",
                              "stage-locally",
                              Some(
                                "Locally staged draftOrderComplete in shopify-draft-proxy.",
                              ),
                            )
                          #(
                            key,
                            payload,
                            next_store,
                            next_identity,
                            [completed_draft_order.id],
                            [],
                            [draft],
                          )
                        }
                        False -> {
                          let payload =
                            serialize_draft_order_mutation_payload(
                              field,
                              None,
                              [
                                user_error(
                                  ["paymentGatewayId"],
                                  "payment_gateway_disabled",
                                  Some(user_error_codes.invalid),
                                ),
                              ],
                              fragments,
                            )
                          #(key, payload, hydrated_store, identity, [], [], [])
                        }
                      }
                    None -> {
                      let payload =
                        serialize_draft_order_mutation_payload(
                          field,
                          None,
                          [
                            user_error(
                              ["paymentGatewayId"],
                              "payment_gateway_not_found",
                              Some(user_error_codes.invalid),
                            ),
                          ],
                          fragments,
                        )
                      #(key, payload, hydrated_store, identity, [], [], [])
                    }
                  }
                }
                None -> {
                  let #(completed_draft_order, next_identity) =
                    complete_draft_order(
                      store,
                      identity,
                      draft_order,
                      read_string_arg(args, "sourceName"),
                      read_bool(args, "paymentPending", False),
                      None,
                    )
                  let next_store =
                    stage_completed_draft_order_graph(
                      hydrated_store,
                      completed_draft_order,
                    )
                  let payload =
                    serialize_draft_order_mutation_payload(
                      field,
                      Some(completed_draft_order),
                      [],
                      fragments,
                    )
                  let draft =
                    single_root_log_draft(
                      "draftOrderComplete",
                      [completed_draft_order.id],
                      store_types.Staged,
                      "orders",
                      "stage-locally",
                      Some(
                        "Locally staged draftOrderComplete in shopify-draft-proxy.",
                      ),
                    )
                  #(
                    key,
                    payload,
                    next_store,
                    next_identity,
                    [completed_draft_order.id],
                    [],
                    [draft],
                  )
                }
              }
            }
            None -> {
              let payload =
                serialize_draft_order_mutation_payload(
                  field,
                  None,
                  [inferred_user_error(["id"], "Draft order does not exist")],
                  fragments,
                )
              #(key, payload, store, identity, [], [], [])
            }
          }
        }
        None -> #(key, json.null(), store, identity, [], [], [])
      }
    }
  }
}

fn stage_completed_draft_order_graph(
  store_in: Store,
  completed_draft_order: DraftOrderRecord,
) -> Store {
  let draft_store = store.stage_draft_order(store_in, completed_draft_order)
  case captured_object_field(completed_draft_order.data, "order") {
    Some(order_data) ->
      case captured_string_field(order_data, "id") {
        Some(order_id) -> {
          let order = OrderRecord(id: order_id, cursor: None, data: order_data)
          store.stage_order(draft_store, order)
          |> stage_completed_order_customer_summary(order)
        }
        None -> draft_store
      }
    None -> draft_store
  }
}

fn stage_completed_order_customer_summary(
  store_in: Store,
  order: OrderRecord,
) -> Store {
  case completed_order_customer_id(order.data) {
    Some(customer_id) ->
      store.stage_customer_order_summary(
        store_in,
        CustomerOrderSummaryRecord(
          id: order.id,
          customer_id: Some(customer_id),
          cursor: None,
          name: captured_string_field(order.data, "name"),
          email: captured_string_field(order.data, "email"),
          created_at: captured_string_field(order.data, "createdAt"),
          current_total_price: None,
        ),
      )
    None -> store_in
  }
}

fn completed_order_customer_id(
  order_data: CapturedJsonValue,
) -> Option(String) {
  case captured_object_field(order_data, "customer") {
    Some(customer) -> captured_string_field(customer, "id")
    None -> None
  }
}

@internal
pub fn handle_draft_order_delete(
  store: Store,
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(String, Json, Store, List(Json), List(LogDraft)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderDelete",
      [RequiredArgument(name: "input", expected_type: "DraftOrderDeleteInput!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, validation_errors, [])
    [] -> {
      let input = field_arguments(field, variables) |> read_object("input")
      let id = case input {
        Some(input) -> read_string_arg(input, "id")
        None -> None
      }
      case id {
        Some(id) -> {
          // Pattern 2: deleting a captured upstream draft first hydrates the
          // draft so the local delete marker can drive downstream null reads.
          let hydrated_store =
            maybe_hydrate_draft_order_by_id(store, id, upstream)
          case store.get_draft_order_by_id(hydrated_store, id) {
            Some(_) -> {
              let next_store =
                store.delete_staged_draft_order(hydrated_store, id)
              let payload =
                serialize_draft_order_delete_payload(field, Some(id), [])
              let draft =
                single_root_log_draft(
                  "draftOrderDelete",
                  [id],
                  store_types.Staged,
                  "orders",
                  "stage-locally",
                  Some(
                    "Locally staged draftOrderDelete in shopify-draft-proxy.",
                  ),
                )
              #(key, payload, next_store, [], [draft])
            }
            None -> #(
              key,
              serialize_draft_order_delete_payload(field, None, [
                inferred_user_error(["id"], "Draft order does not exist"),
              ]),
              store,
              [],
              [],
            )
          }
        }
        None -> #(
          key,
          serialize_draft_order_delete_payload(field, None, [
            inferred_user_error(["id"], "Draft order does not exist"),
          ]),
          store,
          [],
          [],
        )
      }
    }
  }
}

@internal
pub fn serialize_draft_order_delete_payload(
  field: Selection,
  deleted_id: Option(String),
  user_errors: List(#(List(String), String, Option(String))),
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "deletedId" -> #(key, case deleted_id {
              Some(id) -> json.string(id)
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
    })
  json.object(entries)
}

@internal
pub fn handle_order_delete_mutation(
  store: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, Store, List(String), List(LogDraft)) {
  let key = get_field_response_key(field)
  let args = field_arguments(field, variables)
  let order_id = read_string_arg(args, "orderId")
  case order_id {
    Some(order_id) ->
      case store.get_order_by_id(store, order_id) {
        Some(order) -> {
          case order_is_deletable(order) {
            True -> {
              let next_store = cascade_order_delete(store, order_id)
              let payload =
                serialize_order_delete_payload(field, Some(order_id), [])
              let draft =
                single_root_log_draft(
                  "orderDelete",
                  [order_id],
                  store_types.Staged,
                  "orders",
                  "stage-locally",
                  Some("Locally staged orderDelete in shopify-draft-proxy."),
                )
              #(key, payload, next_store, [order_id], [draft])
            }
            False -> {
              let payload =
                serialize_order_delete_payload(field, None, [
                  user_error(
                    ["orderId"],
                    "order_cannot_be_deleted",
                    Some(user_error_codes.invalid),
                  ),
                ])
              #(key, payload, store, [], [])
            }
          }
        }
        None -> {
          let payload =
            serialize_order_delete_payload(field, None, [
              user_error(
                ["orderId"],
                "Order does not exist",
                Some(user_error_codes.not_found),
              ),
            ])
          #(key, payload, store, [], [])
        }
      }
    None -> {
      let payload =
        serialize_order_delete_payload(field, None, [
          user_error(
            ["orderId"],
            "Order does not exist",
            Some(user_error_codes.not_found),
          ),
        ])
      #(key, payload, store, [], [])
    }
  }
}

fn cascade_order_delete(store: Store, order_id: String) -> Store {
  let without_order = store.delete_staged_order(store, order_id)
  let without_payment_terms = case
    store.get_effective_payment_terms_by_owner_id(without_order, order_id)
  {
    Some(payment_terms) ->
      store.delete_staged_payment_terms(without_order, payment_terms.id)
    None -> without_order
  }
  store.unassociate_abandoned_checkouts_from_order(
    without_payment_terms,
    order_id,
  )
}

fn order_is_deletable(order: OrderRecord) -> Bool {
  !order_has_financial_state(order)
  && !order_has_fulfillment_state(order)
  && !order_has_open_returns(order)
  && !order_has_open_fulfillment_orders(order)
}

fn order_has_financial_state(order: OrderRecord) -> Bool {
  case captured_string_field(order.data, "displayFinancialStatus") {
    Some("PENDING") | None -> {
      !list.is_empty(order_transactions(order))
      || !list.is_empty(captured_list_field(order.data, "refunds"))
    }
    Some(_) -> True
  }
}

fn order_has_fulfillment_state(order: OrderRecord) -> Bool {
  case captured_string_field(order.data, "displayFulfillmentStatus") {
    Some("UNFULFILLED") | None -> !list.is_empty(order_fulfillments(order.data))
    Some(_) -> True
  }
}

fn order_has_open_returns(order: OrderRecord) -> Bool {
  order_returns(order.data)
  |> list.any(fn(order_return) {
    case captured_string_field(order_return, "status") {
      Some("CLOSED")
      | Some("CANCELED")
      | Some("CANCELLED")
      | Some("DECLINED") -> False
      _ -> True
    }
  })
}

fn order_has_open_fulfillment_orders(order: OrderRecord) -> Bool {
  order_fulfillment_orders(order.data)
  |> list.any(fn(fulfillment_order) {
    case captured_string_field(fulfillment_order, "status") {
      Some("CLOSED") | Some("CANCELLED") -> False
      _ -> True
    }
  })
}

fn captured_list_field(
  value: CapturedJsonValue,
  name: String,
) -> List(CapturedJsonValue) {
  case captured_object_field(value, name) {
    Some(CapturedArray(values)) -> values
    _ -> []
  }
}

@internal
pub fn serialize_order_delete_payload(
  field: Selection,
  deleted_id: Option(String),
  user_errors: List(#(List(String), String, Option(String))),
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "deletedId" -> #(key, case deleted_id {
              Some(id) -> json.string(id)
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
    })
  json.object(entries)
}

@internal
pub fn handle_draft_order_duplicate(
  store: Store,
  identity: SyntheticIdentityRegistry,
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
  let args = field_arguments(field, variables)
  let id =
    read_string_arg(args, "id")
    |> option.or(read_string_arg(args, "draftOrderId"))
  case id {
    Some(id) -> {
      // Pattern 2: duplication copies Shopify's existing draft payload, then
      // allocates local IDs for the duplicate and stages it locally.
      let hydrated_store = maybe_hydrate_draft_order_by_id(store, id, upstream)
      case store.get_draft_order_by_id(hydrated_store, id) {
        Some(draft_order) -> {
          let #(duplicated_draft_order, next_identity) =
            duplicate_draft_order(hydrated_store, identity, draft_order)
          let next_store =
            store.stage_draft_order(hydrated_store, duplicated_draft_order)
          let payload =
            serialize_draft_order_mutation_payload(
              field,
              Some(duplicated_draft_order),
              [],
              fragments,
            )
          let draft =
            single_root_log_draft(
              "draftOrderDuplicate",
              [duplicated_draft_order.id],
              store_types.Staged,
              "orders",
              "stage-locally",
              Some("Locally staged draftOrderDuplicate in shopify-draft-proxy."),
            )
          #(
            key,
            payload,
            next_store,
            next_identity,
            [duplicated_draft_order.id],
            [draft],
          )
        }
        None ->
          unknown_draft_order_duplicate_result(
            key,
            store,
            identity,
            field,
            fragments,
          )
      }
    }
    None ->
      unknown_draft_order_duplicate_result(
        key,
        store,
        identity,
        field,
        fragments,
      )
  }
}

@internal
pub fn unknown_draft_order_duplicate_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(LogDraft),
) {
  let payload =
    serialize_draft_order_mutation_payload(
      field,
      None,
      [
        inferred_user_error(["id"], "Draft order does not exist"),
      ],
      fragments,
    )
  #(key, payload, store, identity, [], [])
}

@internal
pub fn handle_draft_order_invoice_send(
  store: Store,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(String, Json, List(Json), List(LogDraft)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderInvoiceSend",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let id = read_string_arg(args, "id")
      // Pattern 2: invoiceSend remains a local safety/validation handler, but
      // captured no-recipient branches need the upstream draft status/email.
      let hydrated_store = case id {
        Some(id) -> maybe_hydrate_draft_order_by_id(store, id, upstream)
        None -> store
      }
      let draft_order = case id {
        Some(id) -> store.get_draft_order_by_id(hydrated_store, id)
        None -> None
      }
      let user_errors = invoice_send_user_errors(args, draft_order)
      let payload =
        serialize_draft_order_invoice_send_payload(
          field,
          draft_order,
          user_errors,
          fragments,
        )
      let draft =
        single_root_log_draft(
          "draftOrderInvoiceSend",
          [],
          case user_errors {
            [] -> store_types.Staged
            _ -> store_types.Failed
          },
          "orders",
          "stage-locally",
          Some("Locally handled draftOrderInvoiceSend safety validation."),
        )
      #(key, payload, [], [draft])
    }
  }
}

@internal
pub fn invoice_send_user_errors(
  args: Dict(String, root_field.ResolvedValue),
  draft_order: Option(DraftOrderRecord),
) -> List(#(Option(List(String)), String, Option(String))) {
  case draft_order {
    None -> [inferred_nullable_user_error(None, "Draft order not found")]
    Some(record) -> {
      let recipient_errors = case invoice_send_recipient_present(args, record) {
        True -> []
        False -> [inferred_nullable_user_error(None, "To can't be blank")]
      }
      let status_errors = case captured_string_field(record.data, "status") {
        Some("COMPLETED") -> [
          inferred_nullable_user_error(
            None,
            "Draft order Invoice can't be sent. This draft order is already paid.",
          ),
        ]
        _ -> []
      }
      list.append(recipient_errors, status_errors)
    }
  }
}

@internal
pub fn invoice_send_recipient_present(
  args: Dict(String, root_field.ResolvedValue),
  draft_order: DraftOrderRecord,
) -> Bool {
  case read_object(args, "email") {
    Some(email) ->
      case read_string(email, "to") {
        Some("") -> False
        Some(_) -> True
        None -> False
      }
    None ->
      case captured_string_field(draft_order.data, "email") {
        Some("") -> False
        Some(_) -> True
        None -> False
      }
  }
}

@internal
pub fn serialize_draft_order_invoice_send_payload(
  field: Selection,
  draft_order: Option(DraftOrderRecord),
  user_errors: List(#(Option(List(String)), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "draftOrder" -> #(key, case draft_order {
              Some(record) ->
                serialize_draft_order_node(None, child, record, fragments)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_nullable_user_error(child, error)
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
pub fn handle_draft_order_calculate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(String, Json, List(Json), List(LogDraft)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderCalculate",
      [RequiredArgument(name: "input", expected_type: "DraftOrderInput!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case dict.get(args, "input") {
        Ok(root_field.ObjectVal(input)) -> {
          case draft_order_input_tag_count_over_graphql_limit(input) {
            Some(tag_count) -> #(
              key,
              json.null(),
              [
                draft_order_tags_max_input_size_error(
                  "draftOrderCalculate",
                  tag_count,
                ),
              ],
              [],
            )
            None -> {
              let hydrated_store =
                maybe_hydrate_draft_order_variant_catalog_from_input(
                  store,
                  input,
                  upstream,
                )
                |> maybe_hydrate_draft_order_customer_from_input(
                  input,
                  upstream,
                )
              let user_errors =
                validate_draft_order_calculate_input(hydrated_store, input)
              case user_errors {
                [] -> {
                  let #(draft_order, _) =
                    build_draft_order_from_input(
                      hydrated_store,
                      identity,
                      input,
                    )
                  let calculated =
                    build_calculated_draft_order_from_draft(draft_order)
                  let payload =
                    serialize_draft_order_calculate_payload(
                      field,
                      Some(calculated),
                      [],
                      fragments,
                    )
                  let draft =
                    single_root_log_draft(
                      "draftOrderCalculate",
                      [],
                      store_types.Staged,
                      "orders",
                      "stage-locally",
                      Some(
                        "Locally calculated draftOrderCalculate in shopify-draft-proxy.",
                      ),
                    )
                  #(key, payload, [], [draft])
                }
                _ -> {
                  let payload =
                    serialize_draft_order_calculate_payload(
                      field,
                      None,
                      user_errors,
                      fragments,
                    )
                  let draft =
                    single_root_log_draft(
                      "draftOrderCalculate",
                      [],
                      store_types.Failed,
                      "orders",
                      "stage-locally",
                      Some(
                        "Locally rejected draftOrderCalculate validation branch.",
                      ),
                    )
                  #(key, payload, [], [draft])
                }
              }
            }
          }
        }
        _ -> #(key, json.null(), [], [])
      }
    }
  }
}

@internal
pub fn build_calculated_draft_order_from_draft(
  draft_order: DraftOrderRecord,
) -> CapturedJsonValue {
  let line_items = draft_order_line_items(draft_order.data)
  draft_order.data
  |> replace_captured_object_fields([
    #("currencyCode", CapturedString(captured_order_currency(draft_order.data))),
    #("lineItems", CapturedArray(line_items)),
    #("availableShippingRates", CapturedArray([])),
  ])
}

@internal
pub fn serialize_draft_order_calculate_payload(
  field: Selection,
  calculated: Option(CapturedJsonValue),
  user_errors: List(#(Option(List(String)), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "calculatedDraftOrder" -> #(key, case calculated {
              Some(value) ->
                project_graphql_value(
                  captured_json_source(value),
                  selection_children(child),
                  fragments,
                )
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_nullable_user_error(child, error)
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
pub fn handle_draft_order_invoice_preview(
  store: Store,
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json), List(LogDraft)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "draftOrderInvoicePreview",
      [RequiredArgument(name: "id", expected_type: "ID!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let id = read_string_arg(args, "id")
      let draft_order = case id {
        Some(id) -> store.get_draft_order_by_id(store, id)
        None -> None
      }
      let user_errors = case draft_order {
        Some(_) -> []
        None -> [inferred_user_error(["id"], "Draft order does not exist")]
      }
      let payload =
        serialize_draft_order_invoice_preview_payload(field, args, user_errors)
      let draft =
        single_root_log_draft(
          "draftOrderInvoicePreview",
          [],
          case user_errors {
            [] -> store_types.Staged
            _ -> store_types.Failed
          },
          "orders",
          "stage-locally",
          Some(
            "Locally handled draftOrderInvoicePreview in shopify-draft-proxy.",
          ),
        )
      #(key, payload, [], [draft])
    }
  }
}

@internal
pub fn serialize_draft_order_invoice_preview_payload(
  field: Selection,
  args: Dict(String, root_field.ResolvedValue),
  user_errors: List(#(List(String), String, Option(String))),
) -> Json {
  let email = read_object(args, "email") |> option.unwrap(dict.new())
  let subject =
    read_string(email, "subject") |> option.unwrap("Complete your purchase")
  let custom_message = read_string(email, "customMessage") |> option.unwrap("")
  let source =
    src_object([
      #("previewSubject", SrcString(subject)),
      #(
        "previewHtml",
        SrcString(
          "<!DOCTYPE html><html><body><h1>"
          <> subject
          <> "</h1><p>"
          <> custom_message
          <> "</p></body></html>",
        ),
      ),
      #(
        "userErrors",
        SrcList(
          list.map(user_errors, fn(error) {
            let #(field_path, message, code) = error
            user_error_source(Some(field_path), message, code)
          }),
        ),
      ),
    ])
  project_graphql_value(source, selection_children(field), dict.new())
}

@internal
pub fn handle_draft_order_bulk_helper(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
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
  let tags = read_string_list(args, "tags")
  let #(normalized_tags, tag_user_errors, request_blocked) =
    draft_order_bulk_tag_input(root_name, tags)
  let #(targets, id_user_errors) = case request_blocked {
    True -> #([], [])
    False -> select_draft_order_bulk_targets(store, args)
  }
  let #(next_store, changed_ids, target_user_errors) = case request_blocked {
    True -> #(store, [], [])
    False ->
      apply_draft_order_bulk_helper(store, root_name, targets, normalized_tags)
  }
  let user_errors =
    list.append(
      tag_user_errors,
      list.append(id_user_errors, target_user_errors),
    )
  let should_create_job =
    list.is_empty(user_errors) || !list.is_empty(changed_ids)
  let #(job_id, next_identity) = case should_create_job {
    True -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "Job")
      #(Some(id), next_identity)
    }
    False -> #(None, identity)
  }
  let payload = serialize_draft_order_bulk_payload(field, job_id, user_errors)
  let draft =
    single_root_log_draft(
      root_name,
      changed_ids,
      case list.is_empty(changed_ids), list.is_empty(user_errors) {
        False, _ -> store_types.Staged
        True, True -> store_types.Staged
        True, False -> store_types.Failed
      },
      "orders",
      "stage-locally",
      Some("Locally staged " <> root_name <> " in shopify-draft-proxy."),
    )
  #(key, payload, next_store, next_identity, changed_ids, [draft])
}

@internal
pub fn apply_draft_order_bulk_helper(
  store: Store,
  root_name: String,
  targets: List(DraftOrderRecord),
  tags: List(String),
) -> #(Store, List(String), List(#(List(String), String, Option(String)))) {
  targets
  |> list.fold(#(store, [], []), fn(acc, draft_order) {
    let #(current_store, ids, user_errors) = acc
    case root_name {
      "draftOrderBulkDelete" -> #(
        store.delete_staged_draft_order(current_store, draft_order.id),
        [draft_order.id, ..ids],
        user_errors,
      )
      "draftOrderBulkAddTags" -> {
        let updated = update_draft_order_tags(draft_order, tags, "add")
        case
          draft_order_tag_count_exceeds_limit(draft_order_tags(updated.data))
        {
          True -> #(
            current_store,
            ids,
            append_too_many_draft_order_tags_error(user_errors),
          )
          False -> #(
            store.stage_draft_order(current_store, updated),
            [draft_order.id, ..ids],
            user_errors,
          )
        }
      }
      "draftOrderBulkRemoveTags" -> {
        let updated = update_draft_order_tags(draft_order, tags, "remove")
        #(
          store.stage_draft_order(current_store, updated),
          [draft_order.id, ..ids],
          user_errors,
        )
      }
      _ -> #(current_store, ids, user_errors)
    }
  })
}

@internal
pub fn select_draft_order_bulk_targets(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> #(List(DraftOrderRecord), List(#(List(String), String, Option(String)))) {
  let ids = read_string_list(args, "ids")
  case ids {
    [_, ..] ->
      ids
      |> list.fold(#([], [], 0), fn(acc, id) {
        let #(targets, user_errors, index) = acc
        case store.get_draft_order_by_id(store, id) {
          Some(record) -> #([record, ..targets], user_errors, index + 1)
          None -> #(
            targets,
            [
              user_error(
                ["input", "ids", int.to_string(index)],
                "Draft order does not exist",
                Some(user_error_codes.not_found),
              ),
              ..user_errors
            ],
            index + 1,
          )
        }
      })
      |> fn(result) {
        let #(targets, user_errors, _) = result
        #(list.reverse(targets), list.reverse(user_errors))
      }
    [] ->
      case read_string(args, "search") {
        Some(search) -> #(
          store.list_effective_draft_orders(store)
            |> list.filter(fn(record) {
              draft_order_matches_bulk_search(record, search)
            }),
          [],
        )
        None ->
          case read_string(args, "savedSearchId") {
            Some(_) -> #(
              store.list_effective_draft_orders(store)
                |> list.filter(fn(record) {
                  captured_string_field(record.data, "status") == Some("OPEN")
                }),
              [],
            )
            None -> #([], [])
          }
      }
  }
}

@internal
pub fn draft_order_matches_bulk_search(
  draft_order: DraftOrderRecord,
  search: String,
) -> Bool {
  let query = string.trim(search)
  case string.split_once(query, ":") {
    Ok(#("tag", tag)) ->
      list.contains(draft_order_tags(draft_order.data), string.trim(tag))
    Ok(#("id", id)) -> {
      let expected = string.trim(id)
      draft_order.id == expected
      || draft_order_gid_tail(draft_order.id) == expected
    }
    _ -> False
  }
}

@internal
pub fn unique_strings(values: List(String)) -> List(String) {
  values
  |> list.fold([], fn(acc, value) {
    case list.contains(acc, value) {
      True -> acc
      False -> [value, ..acc]
    }
  })
}

@internal
pub fn serialize_draft_order_bulk_payload(
  field: Selection,
  job_id: Option(String),
  user_errors: List(#(List(String), String, Option(String))),
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
    })
  json.object(entries)
}

@internal
pub fn handle_draft_order_update(
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
      "draftOrderUpdate",
      [
        RequiredArgument(name: "id", expected_type: "ID!"),
        RequiredArgument(name: "input", expected_type: "DraftOrderInput!"),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let id = read_string_arg(args, "id")
      let input = read_object(args, "input")
      case id, input {
        Some(id), Some(input) -> {
          case draft_order_input_tag_count_over_graphql_limit(input) {
            Some(tag_count) -> #(
              key,
              json.null(),
              store,
              identity,
              [],
              [
                draft_order_tags_max_input_size_error(
                  "draftOrderUpdate",
                  tag_count,
                ),
              ],
              [],
            )
            None -> {
              // Pattern 2: updates merge user input into Shopify's existing draft
              // order payload before staging the changed draft locally.
              let hydrated_store =
                maybe_hydrate_draft_order_by_id(store, id, upstream)
              case store.get_draft_order_by_id(hydrated_store, id) {
                Some(draft_order) -> {
                  let user_errors =
                    validate_draft_order_input_tags(input, "draftOrderUpdate")
                  case user_errors {
                    [] -> {
                      let #(updated_draft_order, next_identity) =
                        build_updated_draft_order(
                          hydrated_store,
                          identity,
                          draft_order,
                          input,
                        )
                      let next_store =
                        store.stage_draft_order(
                          hydrated_store,
                          updated_draft_order,
                        )
                      let payload =
                        serialize_draft_order_mutation_payload(
                          field,
                          Some(updated_draft_order),
                          [],
                          fragments,
                        )
                      let draft =
                        single_root_log_draft(
                          "draftOrderUpdate",
                          [id],
                          store_types.Staged,
                          "orders",
                          "stage-locally",
                          Some(
                            "Locally staged draftOrderUpdate in shopify-draft-proxy.",
                          ),
                        )
                      #(key, payload, next_store, next_identity, [id], [], [
                        draft,
                      ])
                    }
                    _ -> {
                      let payload =
                        serialize_draft_order_nullable_update_payload(
                          field,
                          None,
                          user_errors,
                          fragments,
                        )
                      let draft =
                        single_root_log_draft(
                          "draftOrderUpdate",
                          [],
                          store_types.Failed,
                          "orders",
                          "stage-locally",
                          Some(
                            "Locally rejected draftOrderUpdate validation branch.",
                          ),
                        )
                      #(key, payload, hydrated_store, identity, [], [], [draft])
                    }
                  }
                }
                None ->
                  unknown_draft_order_update_result(
                    key,
                    store,
                    identity,
                    field,
                    fragments,
                  )
              }
            }
          }
        }
        _, _ ->
          unknown_draft_order_update_result(
            key,
            store,
            identity,
            field,
            fragments,
          )
      }
    }
  }
}

@internal
pub fn serialize_draft_order_nullable_update_payload(
  field: Selection,
  draft_order: Option(DraftOrderRecord),
  user_errors: List(#(Option(List(String)), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "draftOrder" -> #(key, case draft_order {
              Some(record) ->
                serialize_draft_order_node(None, child, record, fragments)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_nullable_user_error(child, error)
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
pub fn unknown_draft_order_update_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let payload =
    serialize_draft_order_mutation_payload(
      field,
      None,
      [
        inferred_user_error(["id"], "Draft order does not exist"),
      ],
      fragments,
    )
  #(key, payload, store, identity, [], [], [])
}
