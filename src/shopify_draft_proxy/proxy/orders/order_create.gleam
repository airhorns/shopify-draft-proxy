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
  captured_int_field, captured_money_amount, captured_money_presentment_amount,
  captured_money_presentment_value, captured_money_value, captured_number,
  captured_object_field, captured_string_field, captured_tax_lines,
  field_arguments, first_money_set_presentment_currency, max_float, money_set,
  money_set_with_presentment, money_sets_from_field, option_to_result,
  optional_captured_string, order_money_set_from_input,
  order_mutation_user_error, read_bool, read_int, read_number, read_object,
  read_object_list, read_string, read_string_list, selection_children,
  serialize_order_mutation_user_error, tax_line_money_sets,
}
import shopify_draft_proxy/proxy/orders/draft_order_builders.{
  build_draft_order_address, captured_attributes,
}

import shopify_draft_proxy/proxy/orders/order_types.{
  type OrderCreateDiscount, type OrderMutationUserError, OrderCreateDiscount,
  UserErrorField, UserErrorIndex,
}
import shopify_draft_proxy/proxy/orders/serializers.{
  serialize_order_mutation_payload,
}

import shopify_draft_proxy/proxy/user_error_codes

import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type OrderRecord, CapturedArray, CapturedBool,
  CapturedFloat, CapturedInt, CapturedNull, CapturedObject, CapturedString,
  CustomerOrderSummaryRecord, OrderRecord,
}

@internal
pub fn handle_order_create_validation_guardrail(
  document: String,
  operation_path: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, List(Json)) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "orderCreate",
      [
        RequiredArgument(name: "order", expected_type: "OrderCreateOrderInput!"),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), validation_errors)
    [] -> {
      let args = field_arguments(field, variables)
      case dict.get(args, "order") {
        Ok(root_field.ObjectVal(input)) -> {
          let user_errors = validate_order_create_input(input)
          case user_errors {
            [] -> #(key, json.null(), [])
            _ -> #(
              key,
              serialize_order_mutation_error_payload(field, user_errors),
              [],
            )
          }
        }
        _ -> #(key, json.null(), [])
      }
    }
  }
}

@internal
pub fn handle_order_create_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
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
  let #(validation_key, validation_payload, validation_errors) =
    handle_order_create_validation_guardrail(
      document,
      operation_path,
      field,
      variables,
    )
  case validation_errors {
    [_, ..] -> #(
      validation_key,
      validation_payload,
      store,
      identity,
      [],
      validation_errors,
      [],
    )
    [] -> {
      let args = field_arguments(field, variables)
      case dict.get(args, "order") {
        Ok(root_field.ObjectVal(input)) -> {
          let user_errors = validate_order_create_input(input)
          case user_errors {
            [_, ..] -> #(
              key,
              serialize_order_mutation_error_payload(field, user_errors),
              store,
              identity,
              [],
              [],
              [],
            )
            [] -> {
              let #(order, next_identity) =
                build_order_from_create_input(store, identity, input)
              let next_store =
                store.stage_order(store, order)
                |> stage_order_create_customer_summary(order, input)
              let payload =
                serialize_order_mutation_payload(
                  field,
                  Some(order),
                  [],
                  fragments,
                )
              let draft =
                single_root_log_draft(
                  "orderCreate",
                  [order.id],
                  store_types.Staged,
                  "orders",
                  "stage-locally",
                  Some("Locally staged orderCreate in shopify-draft-proxy."),
                )
              #(key, payload, next_store, next_identity, [order.id], [], [draft])
            }
          }
        }
        _ -> #(key, validation_payload, store, identity, [], [], [])
      }
    }
  }
}

@internal
pub fn validate_order_create_input(
  input: Dict(String, root_field.ResolvedValue),
) -> List(OrderMutationUserError) {
  let line_items = read_object_list(input, "lineItems")
  list.flatten([
    validate_order_create_line_item_presence(line_items),
    validate_order_create_processed_at(input),
    validate_order_create_customer_fields(input),
    validate_order_create_tax_line_rates("lineItems", line_items),
    validate_order_create_tax_line_rates(
      "shippingLines",
      read_object_list(input, "shippingLines"),
    ),
  ])
}

@internal
pub fn validate_order_create_line_item_presence(
  line_items: List(Dict(String, root_field.ResolvedValue)),
) -> List(OrderMutationUserError) {
  case line_items {
    [] -> [
      order_mutation_user_error(
        [UserErrorField("order"), UserErrorField("lineItems")],
        "Line items must have at least one line item",
        Some(user_error_codes.invalid),
      ),
    ]
    _ -> []
  }
}

@internal
pub fn validate_order_create_processed_at(
  input: Dict(String, root_field.ResolvedValue),
) -> List(OrderMutationUserError) {
  case read_string(input, "processedAt") {
    Some(value) ->
      case
        iso_timestamp.parse_iso(value),
        iso_timestamp.parse_iso(iso_timestamp.now_iso())
      {
        Ok(processed_at), Ok(now) ->
          case processed_at > now {
            True -> [
              order_mutation_user_error(
                [UserErrorField("order"), UserErrorField("processedAt")],
                "Processed at must not be in the future",
                Some(user_error_codes.processed_at_invalid),
              ),
            ]
            False -> []
          }
        _, _ -> []
      }
    None -> []
  }
}

@internal
pub fn validate_order_create_customer_fields(
  input: Dict(String, root_field.ResolvedValue),
) -> List(OrderMutationUserError) {
  case
    non_empty_string_field(input, "customerId"),
    has_non_null_field(input, "customer")
  {
    True, True -> [
      order_mutation_user_error(
        [UserErrorField("order")],
        "Cannot specify both customerId and customer",
        Some(user_error_codes.redundant_customer_fields),
      ),
    ]
    _, _ -> []
  }
}

@internal
pub fn validate_order_create_tax_line_rates(
  parent_field: String,
  parents: List(Dict(String, root_field.ResolvedValue)),
) -> List(OrderMutationUserError) {
  parents
  |> list.index_map(fn(parent, parent_index) {
    read_object_list(parent, "taxLines")
    |> list.index_map(fn(tax_line, tax_line_index) {
      case has_non_empty_rate(tax_line) {
        True -> []
        False -> [
          order_mutation_user_error(
            [
              UserErrorField("order"),
              UserErrorField(parent_field),
              UserErrorIndex(parent_index),
              UserErrorField("taxLines"),
              UserErrorIndex(tax_line_index),
              UserErrorField("rate"),
            ],
            "Tax line rate must be provided",
            Some(user_error_codes.tax_line_rate_missing),
          ),
        ]
      }
    })
    |> list.flatten
  })
  |> list.flatten
}

@internal
pub fn non_empty_string_field(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case read_string(input, name) {
    Some(value) -> string.trim(value) != ""
    None -> False
  }
}

@internal
pub fn has_non_null_field(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(input, name) {
    Ok(root_field.NullVal) -> False
    Ok(_) -> True
    Error(_) -> False
  }
}

@internal
pub fn has_non_empty_rate(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case dict.get(input, "rate") {
    Ok(root_field.NullVal) -> False
    Ok(root_field.StringVal(value)) -> string.trim(value) != ""
    Ok(_) -> True
    Error(_) -> False
  }
}

@internal
pub fn serialize_order_mutation_error_payload(
  field: Selection,
  user_errors: List(OrderMutationUserError),
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "order" -> #(key, json.null())
            "userErrors" -> #(
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
pub fn build_order_from_create_input(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(OrderRecord, SyntheticIdentityRegistry) {
  let currency_code = order_create_currency(input)
  let #(order_id, identity_after_order) =
    synthetic_identity.make_synthetic_gid(identity, "Order")
  let #(created_at, identity_after_time) =
    synthetic_identity.make_synthetic_timestamp(identity_after_order)
  let #(line_items, identity_after_lines) =
    build_order_create_line_items(identity_after_time, input, currency_code)
  let #(transactions, next_identity) =
    build_order_create_transactions(identity_after_lines, input, currency_code)
  let shipping_lines = build_order_create_shipping_lines(input, currency_code)
  let subtotal = order_line_items_subtotal(line_items)
  let presentment_subtotal = order_line_items_presentment_subtotal(line_items)
  let shipping_total = order_shipping_lines_total(shipping_lines)
  let presentment_shipping_total =
    order_shipping_lines_presentment_total(shipping_lines)
  let tax_total =
    order_create_tax_total(input, line_items, shipping_lines, currency_code)
  let presentment_tax_total =
    order_create_presentment_tax_total(
      input,
      line_items,
      shipping_lines,
      currency_code,
    )
  let discount =
    order_create_discount(input, currency_code, subtotal, shipping_total)
  let discount_total = captured_money_value(discount.total_discounts_set)
  let presentment_discount_total =
    captured_money_presentment_value(discount.total_discounts_set)
  let total = subtotal +. shipping_total
  let presentment_total = presentment_subtotal +. presentment_shipping_total
  let current_total = max_float(0.0, total +. tax_total -. discount_total)
  let presentment_current_total =
    max_float(
      0.0,
      presentment_total +. presentment_tax_total -. presentment_discount_total,
    )
  let presentment_currency_code =
    order_create_presentment_currency(
      line_items,
      shipping_lines,
      discount.total_discounts_set,
      currency_code,
    )
  let has_paid_transaction = order_transactions_include_paid(transactions)
  let has_authorization = order_transactions_include_authorization(transactions)
  let financial_status = case read_string(input, "financialStatus") {
    Some(value) -> string.uppercase(value)
    None ->
      case has_paid_transaction {
        True -> "PAID"
        False ->
          case has_authorization {
            True -> "AUTHORIZED"
            False -> "PENDING"
          }
      }
  }
  let fulfillment_status =
    read_string(input, "fulfillmentStatus")
    |> option.map(string.uppercase)
    |> option.unwrap("UNFULFILLED")
  let payment_gateways = order_transaction_gateways(transactions)
  let payment_gateway_names = case payment_gateways {
    [] ->
      case has_paid_transaction {
        True -> [CapturedString("manual")]
        False -> []
      }
    _ -> list.map(payment_gateways, CapturedString)
  }
  let current_total_set =
    money_set_with_presentment(
      current_total,
      currency_code,
      presentment_current_total,
      presentment_currency_code,
    )
  let zero_money =
    money_set_with_presentment(
      0.0,
      currency_code,
      0.0,
      presentment_currency_code,
    )
  let total_capturable = case has_authorization {
    True -> current_total
    False -> 0.0
  }
  let total_received = case has_paid_transaction {
    True -> current_total
    False -> 0.0
  }
  let data =
    CapturedObject([
      #("id", CapturedString(order_id)),
      #(
        "name",
        CapturedString(
          "#"
          <> int.to_string(list.length(store.list_effective_orders(store)) + 1),
        ),
      ),
      #("createdAt", CapturedString(created_at)),
      #("updatedAt", CapturedString(created_at)),
      #("email", optional_captured_string(read_string(input, "email"))),
      #("phone", optional_captured_string(read_string(input, "phone"))),
      #("poNumber", optional_captured_string(read_string(input, "poNumber"))),
      #("closed", CapturedBool(False)),
      #("closedAt", CapturedNull),
      #("cancelledAt", CapturedNull),
      #("cancelReason", CapturedNull),
      #(
        "sourceName",
        optional_captured_string(read_string(input, "sourceName")),
      ),
      #("paymentGatewayNames", CapturedArray(payment_gateway_names)),
      #("displayFinancialStatus", CapturedString(financial_status)),
      #("displayFulfillmentStatus", CapturedString(fulfillment_status)),
      #("note", optional_captured_string(read_string(input, "note"))),
      #("tags", CapturedArray(order_create_tags(input))),
      #(
        "customAttributes",
        captured_attributes(read_object_list(input, "customAttributes")),
      ),
      #("metafields", CapturedArray([])),
      #(
        "billingAddress",
        build_draft_order_address(read_object(input, "billingAddress")),
      ),
      #(
        "shippingAddress",
        build_draft_order_address(read_object(input, "shippingAddress")),
      ),
      #(
        "subtotalPriceSet",
        money_set_with_presentment(
          subtotal,
          currency_code,
          presentment_subtotal,
          presentment_currency_code,
        ),
      ),
      #(
        "currentSubtotalPriceSet",
        money_set_with_presentment(
          subtotal,
          currency_code,
          presentment_subtotal,
          presentment_currency_code,
        ),
      ),
      #("currentTotalPriceSet", current_total_set),
      #("currentTotalDiscountsSet", discount.total_discounts_set),
      #(
        "currentTotalTaxSet",
        money_set_with_presentment(
          tax_total,
          currency_code,
          presentment_tax_total,
          presentment_currency_code,
        ),
      ),
      #("totalPriceSet", current_total_set),
      #(
        "totalOutstandingSet",
        money_set_with_presentment(
          case has_paid_transaction {
            True -> 0.0
            False -> current_total
          },
          currency_code,
          case has_paid_transaction {
            True -> 0.0
            False -> presentment_current_total
          },
          presentment_currency_code,
        ),
      ),
      #("totalCapturable", CapturedString(float.to_string(total_capturable))),
      #(
        "totalCapturableSet",
        money_set_with_presentment(
          total_capturable,
          currency_code,
          case has_authorization {
            True -> presentment_current_total
            False -> 0.0
          },
          presentment_currency_code,
        ),
      ),
      #("capturable", CapturedBool(has_authorization)),
      #("totalRefundedSet", zero_money),
      #("totalRefundedShippingSet", zero_money),
      #(
        "totalReceivedSet",
        money_set_with_presentment(
          total_received,
          currency_code,
          case has_paid_transaction {
            True -> presentment_current_total
            False -> 0.0
          },
          presentment_currency_code,
        ),
      ),
      #(
        "netPaymentSet",
        money_set_with_presentment(
          total_received,
          currency_code,
          case has_paid_transaction {
            True -> presentment_current_total
            False -> 0.0
          },
          presentment_currency_code,
        ),
      ),
      #(
        "totalShippingPriceSet",
        money_set_with_presentment(
          shipping_total,
          currency_code,
          presentment_shipping_total,
          presentment_currency_code,
        ),
      ),
      #(
        "totalTaxSet",
        money_set_with_presentment(
          tax_total,
          currency_code,
          presentment_tax_total,
          presentment_currency_code,
        ),
      ),
      #("totalDiscountsSet", discount.total_discounts_set),
      #(
        "discountCodes",
        CapturedArray(list.map(discount.codes, CapturedString)),
      ),
      #("discountApplications", CapturedArray(discount.applications)),
      #(
        "taxLines",
        CapturedArray(build_order_create_tax_lines(input, currency_code)),
      ),
      #("taxesIncluded", CapturedBool(read_bool(input, "taxesIncluded", False))),
      #("customer", build_order_create_customer(store, input)),
      #(
        "shippingLines",
        CapturedObject([#("nodes", CapturedArray(shipping_lines))]),
      ),
      #("lineItems", CapturedObject([#("nodes", CapturedArray(line_items))])),
      #("paymentTerms", CapturedNull),
      #("fulfillments", CapturedArray([])),
      #("fulfillmentOrders", CapturedArray([])),
      #("transactions", CapturedArray(transactions)),
      #("refunds", CapturedArray([])),
      #("returns", CapturedArray([])),
    ])
  #(OrderRecord(id: order_id, cursor: None, data: data), next_identity)
}

@internal
pub fn build_order_create_customer(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  case read_string(input, "customerId") {
    Some(customer_id) ->
      case store.get_effective_customer_by_id(store, customer_id) {
        Some(customer) ->
          CapturedObject([
            #("id", CapturedString(customer.id)),
            #("email", optional_captured_string(customer.email)),
            #("displayName", optional_captured_string(customer.display_name)),
          ])
        None -> CapturedObject([#("id", CapturedString(customer_id))])
      }
    None -> CapturedNull
  }
}

@internal
pub fn stage_order_create_customer_summary(
  store: Store,
  order: OrderRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> Store {
  case read_string(input, "customerId") {
    Some(customer_id) ->
      store.stage_customer_order_summary(
        store,
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
    None -> store
  }
}

@internal
pub fn order_create_currency(
  input: Dict(String, root_field.ResolvedValue),
) -> String {
  case read_string(input, "currency") {
    Some(currency) -> currency
    None ->
      read_object_list(input, "lineItems")
      |> list.find_map(fn(line_item) {
        case read_object(line_item, "priceSet") {
          Some(price_set) ->
            case read_object(price_set, "shopMoney") {
              Some(shop_money) ->
                read_string(shop_money, "currencyCode") |> option_to_result
              None -> Error(Nil)
            }
          None -> Error(Nil)
        }
      })
      |> result.unwrap("CAD")
  }
}

@internal
pub fn build_order_create_line_items(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  currency_code: String,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    [],
    identity,
  )
  read_object_list(input, "lineItems")
  |> list.fold(initial, fn(acc, line_item) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(current_identity, "LineItem")
    let price_set =
      read_object(line_item, "originalUnitPriceSet")
      |> option.or(read_object(line_item, "priceSet"))
    let variant_id = read_string(line_item, "variantId")
    let current_quantity = case dict.get(line_item, "currentQuantity") {
      Ok(root_field.IntVal(value)) -> [#("currentQuantity", CapturedInt(value))]
      _ -> []
    }
    let item =
      CapturedObject(list.append(
        [
          #("id", CapturedString(id)),
          #("title", optional_captured_string(read_string(line_item, "title"))),
          #("quantity", CapturedInt(read_int(line_item, "quantity", 0))),
          #("sku", optional_captured_string(read_string(line_item, "sku"))),
          #("variantId", optional_captured_string(variant_id)),
          #("variant", case variant_id {
            Some(id) -> CapturedObject([#("id", CapturedString(id))])
            None -> CapturedNull
          }),
          #(
            "variantTitle",
            optional_captured_string(read_string(line_item, "variantTitle")),
          ),
          #(
            "originalUnitPriceSet",
            order_money_set_from_input(price_set, currency_code, 0.0),
          ),
          #(
            "taxLines",
            CapturedArray(build_order_create_tax_lines(line_item, currency_code)),
          ),
        ],
        current_quantity,
      ))
    #(list.append(items, [item]), next_identity)
  })
}

@internal
pub fn build_order_create_shipping_lines(
  input: Dict(String, root_field.ResolvedValue),
  currency_code: String,
) -> List(CapturedJsonValue) {
  read_object_list(input, "shippingLines")
  |> list.map(fn(shipping_line) {
    CapturedObject([
      #("title", optional_captured_string(read_string(shipping_line, "title"))),
      #("code", optional_captured_string(read_string(shipping_line, "code"))),
      #(
        "source",
        optional_captured_string(read_string(shipping_line, "source")),
      ),
      #(
        "originalPriceSet",
        order_money_set_from_input(
          read_object(shipping_line, "priceSet"),
          currency_code,
          0.0,
        ),
      ),
      #(
        "taxLines",
        CapturedArray(build_order_create_tax_lines(shipping_line, currency_code)),
      ),
    ])
  })
}

@internal
pub fn build_order_create_transactions(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  currency_code: String,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let initial: #(List(CapturedJsonValue), SyntheticIdentityRegistry) = #(
    [],
    identity,
  )
  read_object_list(input, "transactions")
  |> list.fold(initial, fn(acc, transaction) {
    let #(items, current_identity) = acc
    let #(id, next_identity) =
      synthetic_identity.make_synthetic_gid(
        current_identity,
        "OrderTransaction",
      )
    let amount_set = read_object(transaction, "amountSet")
    let direct_amount = read_number(transaction, "amount") |> option.unwrap(0.0)
    let parent_id = read_string(transaction, "parentTransactionId")
    let item =
      CapturedObject([
        #("id", CapturedString(id)),
        #("kind", optional_captured_string(read_string(transaction, "kind"))),
        #(
          "status",
          CapturedString(
            read_string(transaction, "status") |> option.unwrap("SUCCESS"),
          ),
        ),
        #(
          "gateway",
          optional_captured_string(read_string(transaction, "gateway")),
        ),
        #(
          "amountSet",
          order_money_set_from_input(amount_set, currency_code, direct_amount),
        ),
        #("parentTransactionId", optional_captured_string(parent_id)),
        #("parentTransaction", CapturedNull),
        #(
          "paymentId",
          optional_captured_string(read_string(transaction, "paymentId")),
        ),
        #(
          "paymentReferenceId",
          optional_captured_string(read_string(
            transaction,
            "paymentReferenceId",
          )),
        ),
        #(
          "processedAt",
          optional_captured_string(read_string(transaction, "processedAt")),
        ),
      ])
    #(list.append(items, [item]), next_identity)
  })
}

@internal
pub fn build_order_create_tax_lines(
  input: Dict(String, root_field.ResolvedValue),
  currency_code: String,
) -> List(CapturedJsonValue) {
  read_object_list(input, "taxLines")
  |> list.map(fn(tax_line) {
    let channel_liable = case dict.get(tax_line, "channelLiable") {
      Ok(root_field.BoolVal(value)) -> CapturedBool(value)
      _ -> CapturedNull
    }
    CapturedObject([
      #("title", optional_captured_string(read_string(tax_line, "title"))),
      #("rate", captured_number(tax_line, "rate")),
      #("channelLiable", channel_liable),
      #(
        "priceSet",
        order_money_set_from_input(
          read_object(tax_line, "priceSet"),
          currency_code,
          0.0,
        ),
      ),
    ])
  })
}

@internal
pub fn order_create_tags(
  input: Dict(String, root_field.ResolvedValue),
) -> List(CapturedJsonValue) {
  read_string_list(input, "tags")
  |> list.sort(string.compare)
  |> list.map(CapturedString)
}

@internal
pub fn order_line_items_subtotal(line_items: List(CapturedJsonValue)) -> Float {
  line_items
  |> list.fold(0.0, fn(sum, line_item) {
    sum
    +. captured_money_amount(line_item, "originalUnitPriceSet")
    *. int.to_float(
      captured_int_field(line_item, "quantity") |> option.unwrap(0),
    )
  })
}

@internal
pub fn order_line_items_presentment_subtotal(
  line_items: List(CapturedJsonValue),
) -> Float {
  line_items
  |> list.fold(0.0, fn(sum, line_item) {
    sum
    +. captured_money_presentment_amount(line_item, "originalUnitPriceSet")
    *. int.to_float(
      captured_int_field(line_item, "quantity") |> option.unwrap(0),
    )
  })
}

@internal
pub fn order_shipping_lines_total(
  shipping_lines: List(CapturedJsonValue),
) -> Float {
  shipping_lines
  |> list.fold(0.0, fn(sum, shipping_line) {
    sum +. captured_money_amount(shipping_line, "originalPriceSet")
  })
}

@internal
pub fn order_shipping_lines_presentment_total(
  shipping_lines: List(CapturedJsonValue),
) -> Float {
  shipping_lines
  |> list.fold(0.0, fn(sum, shipping_line) {
    sum +. captured_money_presentment_amount(shipping_line, "originalPriceSet")
  })
}

@internal
pub fn order_create_tax_total(
  input: Dict(String, root_field.ResolvedValue),
  line_items: List(CapturedJsonValue),
  shipping_lines: List(CapturedJsonValue),
  currency_code: String,
) -> Float {
  sum_captured_tax_lines(build_order_create_tax_lines(input, currency_code))
  +. list.fold(line_items, 0.0, fn(sum, line_item) {
    sum +. sum_captured_tax_lines(captured_tax_lines(line_item))
  })
  +. list.fold(shipping_lines, 0.0, fn(sum, shipping_line) {
    sum +. sum_captured_tax_lines(captured_tax_lines(shipping_line))
  })
}

@internal
pub fn order_create_presentment_tax_total(
  input: Dict(String, root_field.ResolvedValue),
  line_items: List(CapturedJsonValue),
  shipping_lines: List(CapturedJsonValue),
  currency_code: String,
) -> Float {
  sum_captured_tax_lines_presentment(build_order_create_tax_lines(
    input,
    currency_code,
  ))
  +. list.fold(line_items, 0.0, fn(sum, line_item) {
    sum +. sum_captured_tax_lines_presentment(captured_tax_lines(line_item))
  })
  +. list.fold(shipping_lines, 0.0, fn(sum, shipping_line) {
    sum +. sum_captured_tax_lines_presentment(captured_tax_lines(shipping_line))
  })
}

@internal
pub fn sum_captured_tax_lines(tax_lines: List(CapturedJsonValue)) -> Float {
  tax_lines
  |> list.fold(0.0, fn(sum, tax_line) {
    sum +. captured_money_amount(tax_line, "priceSet")
  })
}

@internal
pub fn sum_captured_tax_lines_presentment(
  tax_lines: List(CapturedJsonValue),
) -> Float {
  tax_lines
  |> list.fold(0.0, fn(sum, tax_line) {
    sum +. captured_money_presentment_amount(tax_line, "priceSet")
  })
}

@internal
pub fn order_create_presentment_currency(
  line_items: List(CapturedJsonValue),
  shipping_lines: List(CapturedJsonValue),
  discount_total_set: CapturedJsonValue,
  fallback: String,
) -> String {
  let line_item_money_sets =
    line_items |> money_sets_from_field("originalUnitPriceSet")
  let shipping_money_sets =
    shipping_lines |> money_sets_from_field("originalPriceSet")
  let tax_money_sets =
    list.append(
      tax_line_money_sets(line_items),
      tax_line_money_sets(shipping_lines),
    )
  first_money_set_presentment_currency(
    list.append(
      line_item_money_sets,
      list.append(
        shipping_money_sets,
        list.append(tax_money_sets, [discount_total_set]),
      ),
    ),
    fallback,
  )
}

@internal
pub fn order_create_discount(
  input: Dict(String, root_field.ResolvedValue),
  currency_code: String,
  subtotal: Float,
  shipping_total: Float,
) -> OrderCreateDiscount {
  case read_object(input, "discountCode") {
    Some(discount_code) ->
      case read_object(discount_code, "itemFixedDiscountCode") {
        Some(fixed) -> {
          let code = read_string(fixed, "code")
          let amount_set =
            order_money_set_from_input(
              read_object(fixed, "amountSet"),
              currency_code,
              0.0,
            )
          OrderCreateDiscount(
            codes: option_to_list_string(code),
            applications: [
              CapturedObject([
                #("code", optional_captured_string(code)),
                #(
                  "value",
                  CapturedObject([
                    #("type", CapturedString("money")),
                    #(
                      "amount",
                      optional_captured_string(captured_string_field(
                        captured_object_field(amount_set, "shopMoney")
                          |> option.unwrap(CapturedNull),
                        "amount",
                      )),
                    ),
                    #("currencyCode", CapturedString(currency_code)),
                  ]),
                ),
              ]),
            ],
            total_discounts_set: amount_set,
          )
        }
        None ->
          case read_object(discount_code, "itemPercentageDiscountCode") {
            Some(percent_discount) -> {
              let code = read_string(percent_discount, "code")
              let percentage =
                read_number(percent_discount, "percentage")
                |> option.unwrap(0.0)
              let amount = subtotal *. percentage /. 100.0
              OrderCreateDiscount(
                codes: option_to_list_string(code),
                applications: [
                  CapturedObject([
                    #("code", optional_captured_string(code)),
                    #(
                      "value",
                      CapturedObject([
                        #("type", CapturedString("percentage")),
                        #("percentage", CapturedFloat(percentage)),
                      ]),
                    ),
                  ]),
                ],
                total_discounts_set: money_set(amount, currency_code),
              )
            }
            None ->
              case read_object(discount_code, "freeShippingDiscountCode") {
                Some(free_shipping) -> {
                  let code = read_string(free_shipping, "code")
                  OrderCreateDiscount(
                    codes: option_to_list_string(code),
                    applications: [
                      CapturedObject([
                        #("code", optional_captured_string(code)),
                        #(
                          "value",
                          CapturedObject([
                            #("type", CapturedString("money")),
                            #(
                              "amount",
                              CapturedString(float.to_string(shipping_total)),
                            ),
                            #("currencyCode", CapturedString(currency_code)),
                          ]),
                        ),
                      ]),
                    ],
                    total_discounts_set: money_set(
                      shipping_total,
                      currency_code,
                    ),
                  )
                }
                None -> empty_order_create_discount(currency_code)
              }
          }
      }
    None -> empty_order_create_discount(currency_code)
  }
}

@internal
pub fn empty_order_create_discount(
  currency_code: String,
) -> OrderCreateDiscount {
  OrderCreateDiscount(
    codes: [],
    applications: [],
    total_discounts_set: money_set(0.0, currency_code),
  )
}

@internal
pub fn option_to_list_string(value: Option(String)) -> List(String) {
  case value {
    Some(value) -> [value]
    None -> []
  }
}

@internal
pub fn order_transactions_include_paid(
  transactions: List(CapturedJsonValue),
) -> Bool {
  transactions
  |> list.any(fn(transaction) {
    captured_string_field(transaction, "status") == Some("SUCCESS")
    && case captured_string_field(transaction, "kind") {
      Some("SALE") | Some("CAPTURE") -> True
      _ -> False
    }
  })
}

@internal
pub fn order_transactions_include_authorization(
  transactions: List(CapturedJsonValue),
) -> Bool {
  transactions
  |> list.any(fn(transaction) {
    captured_string_field(transaction, "status") == Some("SUCCESS")
    && captured_string_field(transaction, "kind") == Some("AUTHORIZATION")
  })
}

@internal
pub fn order_transaction_gateways(
  transactions: List(CapturedJsonValue),
) -> List(String) {
  transactions
  |> list.filter_map(fn(transaction) {
    captured_string_field(transaction, "gateway") |> option_to_result
  })
}
