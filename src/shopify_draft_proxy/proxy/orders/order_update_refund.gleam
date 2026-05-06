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
import shopify_draft_proxy/proxy/customers/customer_mutations
import shopify_draft_proxy/proxy/customers/customer_types.{UserError}
import shopify_draft_proxy/proxy/customers/inputs as customer_inputs
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
  captured_int_field, captured_json_source, captured_money_amount,
  captured_money_amount_field, captured_money_presentment_amount,
  captured_object_field, captured_string_field, field_arguments,
  find_order_line_item, float_to_fixed_2, format_decimal_amount,
  money_set_string_with_presentment, mutation_user_error, nonzero_float,
  optional_captured_string, order_currency_code, order_money_set_from_input,
  order_money_set_string, order_presentment_currency_code, order_refunds,
  order_total_price, order_transactions, prepend_captured_replacement, read_bool,
  read_int, read_number, read_object, read_object_list, read_string,
  read_string_list, replace_captured_object_fields, replace_if_present,
  selection_children,
}
import shopify_draft_proxy/proxy/orders/draft_order_builders.{
  build_order_update_address, captured_attributes,
}
import shopify_draft_proxy/proxy/orders/hydration.{maybe_hydrate_order_by_id}
import shopify_draft_proxy/proxy/orders/order_create.{
  serialize_order_mutation_error_payload,
}
import shopify_draft_proxy/proxy/orders/order_types.{
  type OrderMutationUserError, type RefundCreateUserError, RefundCreateUserError,
}
import shopify_draft_proxy/proxy/orders/serializers.{
  serialize_order_mutation_payload, serialize_order_node,
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
import shopify_draft_proxy/state/store/types as store_types
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
pub fn handle_order_update_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
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
  let errors = case field {
    Field(arguments: arguments, ..) ->
      case find_argument(arguments, "input") {
        Some(input_argument) ->
          case input_argument.value {
            ObjectValue(fields: fields, ..) ->
              validate_order_update_inline_input(operation_path, fields)
            VariableValue(variable: variable) ->
              validate_order_update_variable_input(
                variable.name.value,
                variables,
              )
            _ -> []
          }
        None -> []
      }
    _ -> []
  }
  case errors {
    [_, ..] -> #(key, json.null(), store, identity, [], errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case dict.get(args, "input") {
        Ok(root_field.ObjectVal(input)) ->
          case read_string(input, "id") {
            Some(id) -> {
              case validate_order_update_business_input(input) {
                [first, ..rest] -> #(
                  key,
                  serialize_order_mutation_payload(
                    field,
                    None,
                    [first, ..rest],
                    fragments,
                  ),
                  store,
                  identity,
                  [],
                  [],
                  [],
                )
                [] -> {
                  // Pattern 2: orderUpdate merges input into the existing order,
                  // so hydrate the upstream/cassette order before staging locally.
                  let hydrated_store =
                    maybe_hydrate_order_by_id(store, id, upstream)
                  case store.get_order_by_id(hydrated_store, id) {
                    None -> #(
                      key,
                      serialize_order_mutation_error_payload(field, [
                        mutation_user_error(["id"], "Order does not exist"),
                      ]),
                      hydrated_store,
                      identity,
                      [],
                      [],
                      [],
                    )
                    Some(order) -> {
                      let #(updated_order, next_identity) =
                        build_updated_order(order, input, identity)
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
                          "orderUpdate",
                          [id],
                          store_types.Staged,
                          "orders",
                          "stage-locally",
                          Some(
                            "Locally staged orderUpdate in shopify-draft-proxy.",
                          ),
                        )
                      #(key, payload, next_store, next_identity, [id], [], [
                        draft,
                      ])
                    }
                  }
                }
              }
            }
            None -> #(key, json.null(), store, identity, [], [], [])
          }
        _ -> #(key, json.null(), store, identity, [], [], [])
      }
    }
  }
}

@internal
pub fn handle_refund_create_mutation(
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
      "refundCreate",
      [RequiredArgument(name: "input", expected_type: "RefundInput!")],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      case read_object(args, "input") {
        None -> #(
          key,
          serialize_refund_create_payload(
            field,
            None,
            None,
            [
              RefundCreateUserError(Some(["input"]), "Input is required.", None),
            ],
            fragments,
          ),
          store,
          identity,
          [],
          [],
          [refund_create_log_draft([], store_types.Failed)],
        )
        Some(input) -> {
          let order_id = read_string(input, "orderId")
          // Pattern 2: refundCreate needs the existing order to calculate
          // refundable totals and then locally stage downstream refund state.
          let hydrated_store = case order_id {
            Some(id) -> maybe_hydrate_order_by_id(store, id, upstream)
            None -> store
          }
          let order = case order_id {
            Some(id) -> store.get_order_by_id(hydrated_store, id)
            None -> None
          }
          case order {
            None -> #(
              key,
              serialize_refund_create_payload(
                field,
                None,
                None,
                [
                  RefundCreateUserError(
                    Some(["orderId"]),
                    "Order does not exist",
                    Some(user_error_codes.not_found),
                  ),
                ],
                fragments,
              ),
              store,
              identity,
              [],
              [],
              [refund_create_log_draft([], store_types.Failed)],
            )
            Some(order) -> {
              let line_item_errors =
                refund_create_line_item_quantity_errors(input, order)
              case line_item_errors {
                [_, ..] -> #(
                  key,
                  serialize_refund_create_payload(
                    field,
                    None,
                    Some(order),
                    line_item_errors,
                    fragments,
                  ),
                  hydrated_store,
                  identity,
                  [],
                  [],
                  [refund_create_log_draft([order.id], store_types.Failed)],
                )
                [] -> {
                  let refund_amount =
                    refund_create_requested_amount(input, order)
                  let refundable_amount = order_refundable_payment_amount(order)
                  let allow_over_refunding =
                    read_bool(input, "allowOverRefunding", False)
                  case
                    !allow_over_refunding && refund_amount >. refundable_amount
                  {
                    True -> {
                      let message =
                        "Refund amount $"
                        <> float_to_fixed_2(refund_amount)
                        <> " is greater than net payment received $"
                        <> float_to_fixed_2(refundable_amount)
                      #(
                        key,
                        serialize_refund_create_payload(
                          field,
                          None,
                          Some(order),
                          [
                            RefundCreateUserError(
                              Some(over_refund_field_path(input)),
                              message,
                              Some(user_error_codes.invalid),
                            ),
                          ],
                          fragments,
                        ),
                        hydrated_store,
                        identity,
                        [],
                        [],
                        [
                          refund_create_log_draft(
                            [order.id],
                            store_types.Failed,
                          ),
                        ],
                      )
                    }
                    False -> {
                      let #(refund, refund_transaction, next_identity) =
                        build_refund_from_input(order, input, identity)
                      let updated_order =
                        apply_refund_to_order(order, refund, refund_transaction)
                      let next_store =
                        store.stage_order(hydrated_store, updated_order)
                      let payload =
                        serialize_refund_create_payload(
                          field,
                          Some(refund),
                          Some(updated_order),
                          [],
                          fragments,
                        )
                      #(
                        key,
                        payload,
                        next_store,
                        next_identity,
                        [order.id],
                        [],
                        [
                          refund_create_log_draft(
                            [order.id],
                            store_types.Staged,
                          ),
                        ],
                      )
                    }
                  }
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
pub fn refund_create_log_draft(
  staged_resource_ids: List(String),
  status: store.EntryStatus,
) -> LogDraft {
  single_root_log_draft(
    "refundCreate",
    staged_resource_ids,
    status,
    "orders",
    "stage-locally",
    Some("Locally handled refundCreate parity slice."),
  )
}

@internal
pub fn serialize_refund_create_payload(
  field: Selection,
  refund: Option(CapturedJsonValue),
  order: Option(OrderRecord),
  user_errors: List(RefundCreateUserError),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "refund" -> #(key, case refund {
              Some(value) ->
                project_graphql_value(
                  captured_json_source(value),
                  selection_children(child),
                  fragments,
                )
              None -> json.null()
            })
            "order" -> #(key, case order {
              Some(record) ->
                serialize_order_node(None, child, record, fragments, dict.new())
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_refund_create_user_error(child, error)
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
pub fn serialize_refund_create_user_error(
  field: Selection,
  error: RefundCreateUserError,
) -> Json {
  let field_value = case error.field_path {
    Some(path) -> SrcList(list.map(path, SrcString))
    None -> SrcNull
  }
  let code_value = case error.code {
    Some(code) -> SrcString(code)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("field", field_value),
      #("message", SrcString(error.message)),
      #("code", code_value),
    ]),
    selection_children(field),
    dict.new(),
  )
}

@internal
pub fn build_refund_from_input(
  order: OrderRecord,
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(refund_id, identity_after_refund) =
    synthetic_identity.make_synthetic_gid(identity, "Refund")
  let #(created_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity_after_refund)
  let #(refund_line_items, identity_after_lines) =
    build_refund_line_items(order, input, identity_after_time)
  let refund_amount = refund_create_requested_amount(input, order)
  let shipping_amount = refund_shipping_amount(input, order)
  let #(transaction, next_identity) =
    build_refund_transaction(input, order, refund_amount, identity_after_lines)
  let refund =
    CapturedObject([
      #("id", CapturedString(refund_id)),
      #("note", optional_captured_string(read_string(input, "note"))),
      #("createdAt", CapturedString(created_at)),
      #("updatedAt", CapturedString(created_at)),
      #("totalRefundedSet", order_money_set_string(order, refund_amount)),
      #(
        "totalRefundedShippingSet",
        order_money_set_string(order, shipping_amount),
      ),
      #(
        "refundLineItems",
        CapturedObject([#("nodes", CapturedArray(refund_line_items))]),
      ),
      #(
        "transactions",
        CapturedObject([#("nodes", CapturedArray([transaction]))]),
      ),
    ])
  #(refund, transaction, next_identity)
}

@internal
pub fn build_refund_line_items(
  order: OrderRecord,
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  read_object_list(input, "refundLineItems")
  |> list.fold(#([], identity), fn(acc, refund_line_item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(current_identity, "RefundLineItem")
    let quantity = read_int(refund_line_item, "quantity", 0)
    let restock_type =
      read_string(refund_line_item, "restockType")
      |> option.unwrap("NO_RESTOCK")
    let line_item =
      read_string(refund_line_item, "lineItemId")
      |> option.then(fn(line_item_id) {
        find_order_line_item(order, line_item_id)
      })
    let subtotal = case restock_type {
      "NO_RESTOCK" -> 0.0
      _ ->
        line_item
        |> option.map(fn(item) {
          captured_money_amount(item, "originalUnitPriceSet")
          *. int.to_float(quantity)
        })
        |> option.unwrap(0.0)
    }
    let restocked = case restock_type {
      "NO_RESTOCK" -> False
      _ -> True
    }
    let item =
      CapturedObject([
        #("id", CapturedString(id)),
        #("quantity", CapturedInt(quantity)),
        #("restockType", CapturedString(restock_type)),
        #("restocked", CapturedBool(restocked)),
        #("lineItem", refund_line_item_reference(line_item)),
        #("subtotalSet", order_money_set_string(order, subtotal)),
      ])
    #(list.append(items, [item]), next_identity)
  })
}

@internal
pub fn refund_line_item_reference(
  line_item: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  case line_item {
    Some(item) ->
      CapturedObject([
        #("id", optional_captured_string(captured_string_field(item, "id"))),
        #(
          "title",
          optional_captured_string(captured_string_field(item, "title")),
        ),
      ])
    None -> CapturedNull
  }
}

@internal
pub fn build_refund_transaction(
  input: Dict(String, root_field.ResolvedValue),
  order: OrderRecord,
  fallback_amount: Float,
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let transaction_input = case read_object_list(input, "transactions") {
    [first, ..] -> first
    [] -> dict.new()
  }
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "OrderTransaction")
  let amount =
    refund_transaction_amount(transaction_input)
    |> nonzero_float(fallback_amount)
  let amount_set = case read_object(transaction_input, "amountSet") {
    Some(input_amount_set) ->
      order_money_set_from_input(
        Some(input_amount_set),
        order_currency_code(order),
        amount,
      )
    None -> order_money_set_string(order, amount)
  }
  let transaction =
    CapturedObject([
      #("id", CapturedString(id)),
      #(
        "kind",
        CapturedString(
          read_string(transaction_input, "kind") |> option.unwrap("REFUND"),
        ),
      ),
      #(
        "status",
        CapturedString(
          read_string(transaction_input, "status") |> option.unwrap("SUCCESS"),
        ),
      ),
      #(
        "gateway",
        CapturedString(
          read_string(transaction_input, "gateway") |> option.unwrap("manual"),
        ),
      ),
      #("amountSet", amount_set),
    ])
  #(transaction, next_identity)
}

@internal
pub fn apply_refund_to_order(
  order: OrderRecord,
  refund: CapturedJsonValue,
  refund_transaction: CapturedJsonValue,
) -> OrderRecord {
  let currency_code = order_currency_code(order)
  let presentment_currency_code = order_presentment_currency_code(order)
  let total_refunded =
    sum_order_refunded_amount(order)
    +. captured_money_amount(refund, "totalRefundedSet")
  let presentment_total_refunded =
    sum_order_refunded_presentment_amount(order)
    +. captured_money_presentment_amount(refund, "totalRefundedSet")
  let shipping_refunded =
    sum_order_refunded_shipping_amount(order)
    +. captured_money_amount(refund, "totalRefundedShippingSet")
  let presentment_shipping_refunded =
    sum_order_refunded_shipping_presentment_amount(order)
    +. captured_money_presentment_amount(refund, "totalRefundedShippingSet")
  let total = order_total_price(order)
  let display_status = case total_refunded >=. total && total >. 0.0 {
    True -> "REFUNDED"
    False -> "PARTIALLY_REFUNDED"
  }
  let updated_data =
    order.data
    |> replace_captured_object_fields([
      #("displayFinancialStatus", CapturedString(display_status)),
      #(
        "totalRefundedSet",
        money_set_string_with_presentment(
          format_decimal_amount(total_refunded),
          currency_code,
          format_decimal_amount(presentment_total_refunded),
          presentment_currency_code,
        ),
      ),
      #(
        "totalRefundedShippingSet",
        money_set_string_with_presentment(
          format_decimal_amount(shipping_refunded),
          currency_code,
          format_decimal_amount(presentment_shipping_refunded),
          presentment_currency_code,
        ),
      ),
      #(
        "refunds",
        CapturedArray(list.append(order_refunds(order.data), [refund])),
      ),
      #(
        "transactions",
        CapturedArray(
          list.append(order_transactions(order), [refund_transaction]),
        ),
      ),
    ])
  OrderRecord(..order, data: updated_data)
}

@internal
pub fn refund_create_requested_amount(
  input: Dict(String, root_field.ResolvedValue),
  order: OrderRecord,
) -> Float {
  let transaction_total =
    read_object_list(input, "transactions")
    |> list.fold(0.0, fn(sum, transaction) {
      sum +. refund_transaction_amount(transaction)
    })
  case transaction_total >. 0.0 {
    True -> transaction_total
    False ->
      refund_line_item_subtotal(input, order)
      +. refund_shipping_amount(input, order)
  }
}

@internal
pub fn refund_transaction_amount(
  transaction: Dict(String, root_field.ResolvedValue),
) -> Float {
  read_number(transaction, "amount")
  |> option.or(
    read_object(transaction, "amountSet")
    |> option.then(fn(amount_set) { read_object(amount_set, "shopMoney") })
    |> option.then(fn(shop_money) { read_number(shop_money, "amount") }),
  )
  |> option.unwrap(0.0)
}

@internal
pub fn refund_line_item_subtotal(
  input: Dict(String, root_field.ResolvedValue),
  order: OrderRecord,
) -> Float {
  read_object_list(input, "refundLineItems")
  |> list.fold(0.0, fn(sum, refund_line_item) {
    let quantity = read_int(refund_line_item, "quantity", 0)
    let line_item_id = read_string(refund_line_item, "lineItemId")
    sum
    +. case line_item_id {
      Some(id) ->
        find_order_line_item(order, id)
        |> option.map(fn(line_item) {
          captured_money_amount(line_item, "originalUnitPriceSet")
          *. int.to_float(quantity)
        })
        |> option.unwrap(0.0)
      None -> 0.0
    }
  })
}

@internal
pub fn refund_create_line_item_quantity_errors(
  input: Dict(String, root_field.ResolvedValue),
  order: OrderRecord,
) -> List(RefundCreateUserError) {
  let #(errors, _) =
    read_object_list(input, "refundLineItems")
    |> list.fold(#([], 0), fn(acc, refund_line_item) {
      let #(errors, index) = acc
      let line_errors =
        refund_create_line_item_quantity_error(order, refund_line_item, index)
      #(list.append(errors, line_errors), index + 1)
    })
  errors
}

@internal
pub fn refund_create_line_item_quantity_error(
  order: OrderRecord,
  refund_line_item: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(RefundCreateUserError) {
  let requested_quantity = read_int(refund_line_item, "quantity", 0)
  let line_item =
    read_string(refund_line_item, "lineItemId")
    |> option.then(fn(line_item_id) {
      find_order_line_item(order, line_item_id)
    })
  case line_item {
    Some(line_item) -> {
      let refundable_quantity =
        order_line_item_refundable_quantity(order, line_item)
      case requested_quantity > refundable_quantity {
        True -> [
          RefundCreateUserError(
            Some(["refundLineItems", int.to_string(index), "quantity"]),
            "Quantity cannot refund more items than were purchased",
            Some(user_error_codes.invalid),
          ),
        ]
        False -> []
      }
    }
    None -> []
  }
}

@internal
pub fn order_line_item_refundable_quantity(
  order: OrderRecord,
  line_item: CapturedJsonValue,
) -> Int {
  let current_quantity =
    captured_int_field(line_item, "currentQuantity")
    |> option.or(captured_int_field(line_item, "quantity"))
    |> option.unwrap(0)
  let line_item_id = captured_string_field(line_item, "id") |> option.unwrap("")
  let refunded_quantity = sum_refunded_line_item_quantity(order, line_item_id)
  let remaining = current_quantity - refunded_quantity
  case remaining < 0 {
    True -> 0
    False -> remaining
  }
}

@internal
pub fn sum_refunded_line_item_quantity(
  order: OrderRecord,
  line_item_id: String,
) -> Int {
  order_refunds(order.data)
  |> list.fold(0, fn(sum, refund) {
    sum
    + {
      refund_line_items(refund)
      |> list.fold(0, fn(line_sum, refund_line_item) {
        case refund_line_item_order_line_item_id(refund_line_item) {
          Some(id) if id == line_item_id ->
            line_sum
            + {
              captured_int_field(refund_line_item, "quantity")
              |> option.unwrap(0)
            }
          _ -> line_sum
        }
      })
    }
  })
}

@internal
pub fn refund_line_items(refund: CapturedJsonValue) -> List(CapturedJsonValue) {
  case captured_object_field(refund, "refundLineItems") {
    Some(CapturedObject(fields)) ->
      dict.from_list(fields)
      |> dict.get("nodes")
      |> result.unwrap(CapturedArray([]))
      |> captured_refund_line_items
    Some(CapturedArray(items)) -> items
    _ -> []
  }
}

@internal
pub fn captured_refund_line_items(
  value: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case value {
    CapturedArray(items) -> items
    _ -> []
  }
}

@internal
pub fn refund_line_item_order_line_item_id(
  refund_line_item: CapturedJsonValue,
) -> Option(String) {
  case captured_string_field(refund_line_item, "lineItemId") {
    Some(id) -> Some(id)
    None ->
      case captured_object_field(refund_line_item, "lineItem") {
        Some(line_item) -> captured_string_field(line_item, "id")
        None -> None
      }
  }
}

@internal
pub fn refund_shipping_amount(
  input: Dict(String, root_field.ResolvedValue),
  order: OrderRecord,
) -> Float {
  case read_object(input, "shipping") {
    Some(shipping) ->
      case read_bool(shipping, "fullRefund", False) {
        True -> order_shipping_total(order)
        False -> read_number(shipping, "amount") |> option.unwrap(0.0)
      }
    None -> 0.0
  }
}

@internal
pub fn order_shipping_total(order: OrderRecord) -> Float {
  case captured_object_field(order.data, "shippingLines") {
    Some(CapturedObject(fields)) ->
      dict.from_list(fields)
      |> dict.get("nodes")
      |> result.unwrap(CapturedArray([]))
      |> captured_shipping_lines_total
    Some(CapturedArray(items)) -> sum_shipping_lines(items)
    _ -> 0.0
  }
}

@internal
pub fn captured_shipping_lines_total(value: CapturedJsonValue) -> Float {
  case value {
    CapturedArray(items) -> sum_shipping_lines(items)
    _ -> 0.0
  }
}

@internal
pub fn sum_shipping_lines(items: List(CapturedJsonValue)) -> Float {
  items
  |> list.fold(0.0, fn(sum, line) {
    sum +. captured_money_amount(line, "originalPriceSet")
  })
}

@internal
pub fn sum_order_refunded_amount(order: OrderRecord) -> Float {
  order_refunds(order.data)
  |> list.fold(0.0, fn(sum, refund) {
    sum +. captured_money_amount(refund, "totalRefundedSet")
  })
}

@internal
pub fn sum_order_refunded_presentment_amount(order: OrderRecord) -> Float {
  order_refunds(order.data)
  |> list.fold(0.0, fn(sum, refund) {
    sum +. captured_money_presentment_amount(refund, "totalRefundedSet")
  })
}

@internal
pub fn order_refundable_payment_amount(order: OrderRecord) -> Float {
  let refunded =
    captured_money_amount_field(order.data, "totalRefundedSet")
    |> option.unwrap(sum_order_refunded_amount(order))
  case captured_money_amount_field(order.data, "totalReceivedSet") {
    Some(received) -> received -. refunded
    None -> order_total_price(order) -. refunded
  }
}

@internal
pub fn over_refund_field_path(
  input: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  case read_object_list(input, "transactions") {
    [_, ..] -> ["transactions"]
    [] ->
      case read_object_list(input, "refundLineItems") {
        [_, ..] -> ["refundLineItems"]
        [] -> ["transactions"]
      }
  }
}

@internal
pub fn sum_order_refunded_shipping_amount(order: OrderRecord) -> Float {
  order_refunds(order.data)
  |> list.fold(0.0, fn(sum, refund) {
    sum +. captured_money_amount(refund, "totalRefundedShippingSet")
  })
}

@internal
pub fn sum_order_refunded_shipping_presentment_amount(
  order: OrderRecord,
) -> Float {
  order_refunds(order.data)
  |> list.fold(0.0, fn(sum, refund) {
    sum +. captured_money_presentment_amount(refund, "totalRefundedShippingSet")
  })
}

@internal
pub fn build_updated_order(
  order: OrderRecord,
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(OrderRecord, SyntheticIdentityRegistry) {
  let #(updated_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(metafield_replacements, next_identity) = case
    dict.has_key(input, "metafields")
  {
    True -> {
      let #(metafields, identity_after_metafields) =
        build_order_metafields(
          order,
          read_object_list(input, "metafields"),
          identity_after_time,
        )
      #(
        [
          #("metafield", first_order_metafield(metafields)),
          #("metafields", order_metafields_connection(metafields)),
        ],
        identity_after_metafields,
      )
    }
    False -> #([], identity_after_time)
  }
  let replacements =
    []
    |> prepend_captured_replacement("updatedAt", CapturedString(updated_at))
    |> replace_if_present(
      input,
      "email",
      optional_captured_string(read_string(input, "email")),
    )
    |> replace_if_present(
      input,
      "phone",
      optional_captured_string(read_string(input, "phone")),
    )
    |> replace_if_present(
      input,
      "poNumber",
      optional_captured_string(read_string(input, "poNumber")),
    )
    |> replace_if_present(
      input,
      "note",
      optional_captured_string(read_string(input, "note")),
    )
    |> replace_if_present(
      input,
      "tags",
      CapturedArray(
        read_string_list(input, "tags")
        |> list.sort(by: string.compare)
        |> list.map(CapturedString),
      ),
    )
    |> replace_if_present(
      input,
      "customAttributes",
      captured_attributes(read_object_list(input, "customAttributes")),
    )
    |> replace_if_present(
      input,
      "shippingAddress",
      build_order_update_address(read_object(input, "shippingAddress")),
    )
  let updated_data =
    order.data
    |> replace_captured_object_fields(list.append(
      replacements,
      metafield_replacements,
    ))
  #(OrderRecord(..order, data: updated_data), next_identity)
}

@internal
pub fn build_order_metafields(
  order: OrderRecord,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let existing = order_metafield_nodes(order.data)
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    existing,
    identity,
  )
  inputs
  |> list.fold(initial, fn(acc, input) {
    let #(metafields, current_identity) = acc
    let namespace = read_string(input, "namespace") |> option.unwrap("")
    let key = read_string(input, "key") |> option.unwrap("")
    let existing_metafield =
      find_order_metafield(metafields, namespace, key)
      |> option.or(find_order_metafield(existing, namespace, key))
    let #(id, next_identity) = case
      read_string(input, "id")
      |> option.or(
        option.then(existing_metafield, fn(metafield) {
          captured_string_field(metafield, "id")
        }),
      )
    {
      Some(id) -> #(id, current_identity)
      None ->
        synthetic_identity.make_synthetic_gid(current_identity, "Metafield")
    }
    let metafield =
      CapturedObject([
        #("id", CapturedString(id)),
        #("namespace", CapturedString(namespace)),
        #("key", CapturedString(key)),
        #(
          "type",
          optional_captured_string(
            read_string(input, "type")
            |> option.or(
              option.then(existing_metafield, fn(metafield) {
                captured_string_field(metafield, "type")
              }),
            ),
          ),
        ),
        #(
          "value",
          optional_captured_string(
            read_string(input, "value")
            |> option.or(
              option.then(existing_metafield, fn(metafield) {
                captured_string_field(metafield, "value")
              }),
            ),
          ),
        ),
      ])
    #(upsert_order_metafield(metafields, metafield), next_identity)
  })
}

@internal
pub fn order_metafield_nodes(
  order_data: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(order_data, "metafields") {
    Some(CapturedObject(fields)) ->
      case list.find(fields, fn(pair) { pair.0 == "nodes" }) {
        Ok(#(_, CapturedArray(nodes))) -> nodes
        _ -> []
      }
    _ -> []
  }
}

@internal
pub fn find_order_metafield(
  metafields: List(CapturedJsonValue),
  namespace: String,
  key: String,
) -> Option(CapturedJsonValue) {
  metafields
  |> list.find(fn(metafield) {
    captured_string_field(metafield, "namespace") == Some(namespace)
    && captured_string_field(metafield, "key") == Some(key)
  })
  |> option.from_result
}

@internal
pub fn upsert_order_metafield(
  metafields: List(CapturedJsonValue),
  metafield: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  let namespace = captured_string_field(metafield, "namespace")
  let key = captured_string_field(metafield, "key")
  case metafields {
    [] -> [metafield]
    [first, ..rest] ->
      case
        captured_string_field(first, "namespace") == namespace
        && captured_string_field(first, "key") == key
      {
        True -> [metafield, ..rest]
        False -> [first, ..upsert_order_metafield(rest, metafield)]
      }
  }
}

@internal
pub fn first_order_metafield(
  metafields: List(CapturedJsonValue),
) -> CapturedJsonValue {
  case metafields {
    [first, ..] -> first
    [] -> CapturedNull
  }
}

@internal
pub fn order_metafields_connection(
  metafields: List(CapturedJsonValue),
) -> CapturedJsonValue {
  CapturedObject([#("nodes", CapturedArray(metafields))])
}

@internal
pub fn validate_order_update_business_input(
  input: Dict(String, root_field.ResolvedValue),
) -> List(OrderMutationUserError) {
  let empty_errors = case has_order_update_mutable_input(input) {
    True -> []
    False -> [mutation_user_error(["base"], "no_fields_to_update")]
  }
  empty_errors
  |> list.append(validate_order_update_phone(input))
  |> list.append(validate_order_update_shipping_address(input))
}

@internal
pub fn has_order_update_mutable_input(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.keys(input)
  |> list.any(fn(field_name) {
    list.contains(order_update_mutable_fields(), field_name)
  })
}

@internal
pub fn order_update_mutable_fields() -> List(String) {
  [
    "note",
    "phone",
    "email",
    "poNumber",
    "tags",
    "metafields",
    "customAttributes",
    "shippingAddress",
  ]
}

@internal
pub fn validate_order_update_phone(
  input: Dict(String, root_field.ResolvedValue),
) -> List(OrderMutationUserError) {
  case read_string(input, "phone") {
    Some(phone) ->
      case customer_mutations.valid_phone(phone) {
        True -> []
        False -> [mutation_user_error(["input", "phone"], "Phone is invalid")]
      }
    None -> []
  }
}

@internal
pub fn validate_order_update_shipping_address(
  input: Dict(String, root_field.ResolvedValue),
) -> List(OrderMutationUserError) {
  case read_object(input, "shippingAddress") {
    Some(address) ->
      customer_inputs.validate_address_input(address, None, [
        "input",
        "shippingAddress",
      ])
      |> list.map(fn(error) {
        let UserError(field: field_path, message: message, ..) = error
        mutation_user_error(field_path, message)
      })
    None -> []
  }
}

@internal
pub fn validate_order_update_inline_input(
  operation_path: String,
  fields: List(ObjectField),
) -> List(Json) {
  case find_object_field(fields, "id") {
    None -> [build_order_update_missing_inline_id_error(operation_path)]
    Some(ObjectField(value: NullValue(..), ..)) -> [
      build_order_update_null_inline_id_error(operation_path),
    ]
    _ -> []
  }
}

@internal
pub fn validate_order_update_variable_input(
  variable_name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(Json) {
  case dict.get(variables, variable_name) {
    Ok(root_field.ObjectVal(input)) ->
      case dict.get(input, "id") {
        Ok(root_field.NullVal) | Error(_) -> [
          build_order_update_missing_variable_id_error(
            variable_name,
            root_field.ObjectVal(input),
          ),
        ]
        _ -> []
      }
    _ -> []
  }
}

@internal
pub fn find_object_field(
  fields: List(ObjectField),
  name: String,
) -> Option(ObjectField) {
  case fields {
    [] -> None
    [first, ..rest] -> {
      let ObjectField(name: field_name, ..) = first
      case field_name.value == name {
        True -> Some(first)
        False -> find_object_field(rest, name)
      }
    }
  }
}

@internal
pub fn build_order_update_missing_inline_id_error(
  operation_path: String,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Argument 'id' on InputObject 'OrderInput' is required. Expected type ID!",
      ),
    ),
    #(
      "path",
      json.array([operation_path, "orderUpdate", "input", "id"], json.string),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("missingRequiredInputObjectAttribute")),
        #("argumentName", json.string("id")),
        #("argumentType", json.string("ID!")),
        #("inputObjectType", json.string("OrderInput")),
      ]),
    ),
  ])
}

@internal
pub fn build_order_update_null_inline_id_error(operation_path: String) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Argument 'id' on InputObject 'OrderInput' has an invalid value (null). Expected type 'ID!'.",
      ),
    ),
    #(
      "path",
      json.array([operation_path, "orderUpdate", "input", "id"], json.string),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("argumentLiteralsIncompatible")),
        #("typeName", json.string("InputObject")),
        #("argumentName", json.string("id")),
      ]),
    ),
  ])
}

@internal
pub fn build_order_update_missing_variable_id_error(
  variable_name: String,
  value: root_field.ResolvedValue,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Variable $"
        <> variable_name
        <> " of type OrderInput! was provided invalid value for id (Expected value to not be null)",
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("INVALID_VARIABLE")),
        #("value", source_to_json(resolved_value_to_source(value))),
        #(
          "problems",
          json.array(
            [
              json.object([
                #("path", json.array(["id"], json.string)),
                #("explanation", json.string("Expected value to not be null")),
              ]),
            ],
            fn(problem) { problem },
          ),
        ),
      ]),
    ),
  ])
}
