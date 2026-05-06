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
import shopify_draft_proxy/proxy/orders/order_types.{
  type OrderMutationUserError, type UserErrorFieldSegment,
  OrderMutationUserError, UserErrorField, UserErrorIndex,
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
pub fn user_error(
  field_path: List(String),
  message: String,
  code: Option(String),
) -> #(List(String), String, Option(String)) {
  #(field_path, message, code)
}

@internal
pub fn inferred_user_error(
  field_path: List(String),
  message: String,
) -> #(List(String), String, Option(String)) {
  user_error(
    field_path,
    message,
    infer_user_error_code(Some(field_path), message),
  )
}

@internal
pub fn nullable_user_error(
  field_path: Option(List(String)),
  message: String,
  code: Option(String),
) -> #(Option(List(String)), String, Option(String)) {
  #(field_path, message, code)
}

@internal
pub fn inferred_nullable_user_error(
  field_path: Option(List(String)),
  message: String,
) -> #(Option(List(String)), String, Option(String)) {
  nullable_user_error(
    field_path,
    message,
    infer_user_error_code(field_path, message),
  )
}

@internal
pub fn infer_user_error_code(
  field_path: Option(List(String)),
  message: String,
) -> Option(String) {
  case message {
    "Order does not exist"
    | "Order does not exist."
    | "Fulfillment not found."
    | "Fulfillment does not exist."
    | "Draft order does not exist"
    | "Draft order not found"
    | "Return does not exist."
    | "Reverse delivery does not exist."
    | "abandonment_not_found"
    | "Reverse fulfillment order does not exist." ->
      Some(user_error_codes.not_found)
    "Quantity is not removable from return."
    | "Quantity is not available for return." -> Some(user_error_codes.invalid)
    "Quantity cannot refund more items than were purchased" ->
      Some(user_error_codes.invalid)
    _ ->
      case field_path {
        Some(["returnLineItems", _, "quantity"]) ->
          Some(user_error_codes.return_line_item_quantity_invalid)
        _ -> Some(user_error_codes.invalid)
      }
  }
}

@internal
pub fn option_to_result(value: Option(a)) -> Result(a, Nil) {
  case value {
    Some(value) -> Ok(value)
    None -> Error(Nil)
  }
}

@internal
pub fn serialize_nullable_user_error(
  field: Selection,
  error: #(Option(List(String)), String, Option(String)),
) -> Json {
  let #(field_path, message, code) = error
  let source = user_error_source(field_path, message, code)
  project_graphql_value(source, selection_children(field), dict.new())
}

@internal
pub fn draft_order_gid_tail(id: String) -> String {
  case string.split(id, "/") |> list.last {
    Ok(tail) -> tail
    Error(_) -> id
  }
}

@internal
pub fn serialize_job(field: Selection, id: String) -> Json {
  let source = src_object([#("id", SrcString(id)), #("done", SrcBool(False))])
  project_graphql_value(source, selection_children(field), dict.new())
}

@internal
pub fn order_payment_amount_set(order: OrderRecord) -> CapturedJsonValue {
  let outstanding = case
    captured_object_field(order.data, "totalOutstandingSet")
  {
    Some(value) ->
      case captured_money_value(value) >. 0.0 {
        True -> Some(value)
        False -> None
      }
    None -> None
  }
  let amount_set =
    outstanding
    |> option.or(captured_object_field(order.data, "currentTotalPriceSet"))
    |> option.or(captured_object_field(order.data, "totalPriceSet"))
    |> option.unwrap(order_money_set_string(order, 0.0))
  ensure_order_money_bag_presentment(order, amount_set)
}

@internal
pub fn order_currency_code(order: OrderRecord) -> String {
  captured_object_field(order.data, "currentTotalPriceSet")
  |> option.or(captured_object_field(order.data, "totalOutstandingSet"))
  |> option.or(captured_object_field(order.data, "totalPriceSet"))
  |> option.map(captured_money_set_currency)
  |> option.unwrap("CAD")
}

@internal
pub fn order_presentment_currency_code(order: OrderRecord) -> String {
  captured_string_field(order.data, "presentmentCurrencyCode")
  |> option.unwrap(first_money_set_presentment_currency(
    order_presentment_reference_money_sets(order),
    order_currency_code(order),
  ))
}

@internal
pub fn captured_money_set_currency(value: CapturedJsonValue) -> String {
  case captured_object_field(value, "shopMoney") {
    Some(shop_money) ->
      captured_string_field(shop_money, "currencyCode") |> option.unwrap("CAD")
    None -> "CAD"
  }
}

@internal
pub fn order_money_set(order: OrderRecord, amount: Float) -> CapturedJsonValue {
  money_set_with_presentment(
    amount,
    order_currency_code(order),
    amount *. order_presentment_rate(order),
    order_presentment_currency_code(order),
  )
}

@internal
pub fn order_money_set_string(
  order: OrderRecord,
  amount: Float,
) -> CapturedJsonValue {
  money_set_string_with_presentment(
    format_decimal_amount(amount),
    order_currency_code(order),
    format_decimal_amount(amount *. order_presentment_rate(order)),
    order_presentment_currency_code(order),
  )
}

@internal
pub fn order_presentment_rate(order: OrderRecord) -> Float {
  order_presentment_reference_money_sets(order)
  |> list.find_map(fn(money_set) {
    case captured_object_field(money_set, "presentmentMoney") {
      Some(_) -> {
        let amount = captured_money_value(money_set)
        case amount >. 0.0 {
          True -> Ok(captured_money_presentment_value(money_set) /. amount)
          False -> Error(Nil)
        }
      }
      None -> Error(Nil)
    }
  })
  |> result.unwrap(1.0)
}

@internal
pub fn order_presentment_reference_money_sets(
  order: OrderRecord,
) -> List(CapturedJsonValue) {
  let order_level_sets =
    [
      "currentTotalPriceSet",
      "totalPriceSet",
      "totalOutstandingSet",
      "subtotalPriceSet",
    ]
    |> list.filter_map(fn(field_name) {
      captured_object_field(order.data, field_name) |> option_to_result
    })
  let line_item_sets =
    order_line_items(order.data)
    |> money_sets_from_field("originalUnitPriceSet")
  list.append(order_level_sets, line_item_sets)
}

@internal
pub fn money_set_string(
  amount: String,
  currency_code: String,
) -> CapturedJsonValue {
  money_set_string_with_presentment(
    amount,
    currency_code,
    amount,
    currency_code,
  )
}

@internal
pub fn money_set_string_with_presentment(
  shop_amount: String,
  shop_currency_code: String,
  presentment_amount: String,
  presentment_currency_code: String,
) -> CapturedJsonValue {
  CapturedObject([
    #(
      "shopMoney",
      CapturedObject([
        #("amount", CapturedString(shop_amount)),
        #("currencyCode", CapturedString(shop_currency_code)),
      ]),
    ),
    #(
      "presentmentMoney",
      CapturedObject([
        #("amount", CapturedString(presentment_amount)),
        #("currencyCode", CapturedString(presentment_currency_code)),
      ]),
    ),
  ])
}

@internal
pub fn ensure_order_money_bag_presentment(
  order: OrderRecord,
  value: CapturedJsonValue,
) -> CapturedJsonValue {
  case captured_object_field(value, "presentmentMoney") {
    Some(CapturedObject(_)) -> value
    _ ->
      case captured_object_field(value, "shopMoney") {
        Some(shop_money) -> {
          let amount =
            captured_string_field(shop_money, "amount")
            |> option.unwrap(float.to_string(captured_money_value(value)))
          let currency_code =
            captured_string_field(shop_money, "currencyCode")
            |> option.unwrap(captured_money_set_currency(value))
          money_set_string_with_presentment(
            amount,
            currency_code,
            format_decimal_amount(
              captured_money_value(value) *. order_presentment_rate(order),
            ),
            order_presentment_currency_code(order),
          )
        }
        None -> value
      }
  }
}

@internal
pub fn order_transactions(order: OrderRecord) -> List(CapturedJsonValue) {
  case captured_object_field(order.data, "transactions") {
    Some(CapturedArray(items)) -> items
    _ -> []
  }
}

@internal
pub fn serialize_captured_selection(
  field: Selection,
  value: Option(CapturedJsonValue),
  fragments: FragmentMap,
) -> Json {
  case value {
    Some(value) ->
      project_graphql_value(
        captured_json_source(value),
        selection_children(field),
        fragments,
      )
    None -> json.null()
  }
}

@internal
pub fn connection_nodes(value: CapturedJsonValue) -> List(CapturedJsonValue) {
  case value {
    CapturedObject(fields) ->
      dict.from_list(fields)
      |> dict.get("nodes")
      |> result.unwrap(CapturedArray([]))
      |> captured_array_values
    _ -> []
  }
}

@internal
pub fn captured_array_values(
  value: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case value {
    CapturedArray(values) -> values
    _ -> []
  }
}

@internal
pub fn find_order_with_fulfillment_order(
  store: Store,
  fulfillment_order_id: String,
) -> Option(#(OrderRecord, CapturedJsonValue)) {
  store.list_effective_orders(store)
  |> list.find_map(fn(order) {
    case
      order_fulfillment_orders(order.data)
      |> list.find(fn(fulfillment_order) {
        captured_string_field(fulfillment_order, "id")
        == Some(fulfillment_order_id)
      })
      |> option.from_result
    {
      Some(fulfillment_order) -> Ok(#(order, fulfillment_order))
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn order_fulfillment_orders(
  order_data: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(order_data, "fulfillmentOrders") {
    Some(CapturedArray(fulfillment_orders)) -> fulfillment_orders
    Some(value) -> connection_nodes(value)
    _ -> []
  }
}

@internal
pub const max_fulfillment_holds_per_api_client = 10

@internal
pub const fulfillment_hold_handle_max_length = 64

@internal
pub fn fulfillment_order_supported_actions(
  fulfillment_order: CapturedJsonValue,
) -> List(String) {
  case captured_string_list_field(fulfillment_order, "supportedActions") {
    [] ->
      computed_fulfillment_order_supported_actions(
        captured_string_field(fulfillment_order, "status"),
        fulfillment_order_line_items(fulfillment_order),
      )
    actions -> actions
  }
}

@internal
pub fn computed_fulfillment_order_supported_actions(
  status: Option(String),
  line_items: List(CapturedJsonValue),
) -> List(String) {
  case status {
    Some("ON_HOLD") -> ["RELEASE_HOLD", "HOLD", "MOVE"]
    Some("IN_PROGRESS") -> [
      "CREATE_FULFILLMENT",
      "REPORT_PROGRESS",
      "HOLD",
      "MARK_AS_OPEN",
    ]
    Some("CLOSED") -> []
    _ ->
      case fulfillment_order_supports_split(line_items) {
        True -> [
          "CREATE_FULFILLMENT",
          "REPORT_PROGRESS",
          "MOVE",
          "HOLD",
          "SPLIT",
        ]
        False -> ["CREATE_FULFILLMENT", "REPORT_PROGRESS", "MOVE", "HOLD"]
      }
  }
}

@internal
pub fn fulfillment_order_supported_actions_with_merge(
  status: Option(String),
  line_items: List(CapturedJsonValue),
  include_merge: Bool,
) -> List(String) {
  let actions = computed_fulfillment_order_supported_actions(status, line_items)
  case include_merge && !list.contains(actions, "MERGE") {
    True -> list.append(actions, ["MERGE"])
    False -> actions
  }
}

@internal
pub fn captured_supported_actions(actions: List(String)) -> CapturedJsonValue {
  CapturedArray(list.map(actions, CapturedString))
}

@internal
pub fn captured_string_list_field(
  value: CapturedJsonValue,
  name: String,
) -> List(String) {
  case captured_object_field(value, name) {
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
pub fn fulfillment_order_supports_split(
  line_items: List(CapturedJsonValue),
) -> Bool {
  line_items
  |> list.any(fn(line_item) {
    let total =
      captured_int_field(line_item, "totalQuantity") |> option.unwrap(0)
    let remaining =
      captured_int_field(line_item, "remainingQuantity") |> option.unwrap(0)
    int.max(total, remaining) > 1
  })
}

@internal
pub fn fulfillment_source_line_item_id(line_item: CapturedJsonValue) -> String {
  captured_string_field(line_item, "lineItemId")
  |> option.or({
    case captured_object_field(line_item, "lineItem") {
      Some(line_item) -> captured_string_field(line_item, "id")
      None -> None
    }
  })
  |> option.unwrap("")
}

@internal
pub fn fulfillment_source_line_item_title(
  line_item: CapturedJsonValue,
) -> String {
  captured_string_field(line_item, "title")
  |> option.or({
    case captured_object_field(line_item, "lineItem") {
      Some(line_item) -> captured_string_field(line_item, "title")
      None -> None
    }
  })
  |> option.unwrap("")
}

@internal
pub fn fulfillment_order_line_items(
  fulfillment_order: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(fulfillment_order, "lineItems") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

@internal
pub fn order_fulfillment_holds(
  fulfillment_order: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(fulfillment_order, "fulfillmentHolds") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

@internal
pub fn fulfillment_order_merchant_requests(
  fulfillment_order: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(fulfillment_order, "merchantRequests") {
    Some(CapturedArray(values)) -> values
    Some(value) -> connection_nodes(value)
    None -> []
  }
}

@internal
pub fn has_pending_cancellation_request(
  fulfillment_order: CapturedJsonValue,
) -> Bool {
  captured_string_field(fulfillment_order, "requestStatus") == Some("ACCEPTED")
  && {
    fulfillment_order_merchant_requests(fulfillment_order)
    |> list.any(fn(request) {
      captured_string_field(request, "kind") == Some("CANCELLATION_REQUEST")
    })
  }
}

@internal
pub fn option_is_in(value: Option(String), values: List(String)) -> Bool {
  case value {
    Some(value) -> list.contains(values, value)
    None -> False
  }
}

@internal
pub fn nonzero_float(value: Float, fallback: Float) -> Float {
  case value >. 0.0 {
    True -> value
    False -> fallback
  }
}

@internal
pub fn order_line_items(
  order_data: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(order_data, "lineItems") {
    Some(line_items) ->
      case captured_object_field(line_items, "nodes") {
        Some(CapturedArray(items)) -> items
        _ -> []
      }
    None -> []
  }
}

@internal
pub fn find_order_line_item(
  order: OrderRecord,
  line_item_id: String,
) -> Option(CapturedJsonValue) {
  order_line_items(order.data)
  |> list.find(fn(line_item) {
    captured_string_field(line_item, "id") == Some(line_item_id)
  })
  |> option.from_result
}

@internal
pub fn captured_money_amount_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(Float) {
  captured_object_field(value, name)
  |> option.map(captured_money_value)
}

@internal
pub fn order_refunds(order_data: CapturedJsonValue) -> List(CapturedJsonValue) {
  case captured_object_field(order_data, "refunds") {
    Some(CapturedArray(refunds)) -> refunds
    _ -> []
  }
}

@internal
pub fn order_total_price(order: OrderRecord) -> Float {
  captured_object_field(order.data, "totalPriceSet")
  |> option.or(captured_object_field(order.data, "currentTotalPriceSet"))
  |> option.map(captured_money_value)
  |> option.unwrap(0.0)
}

@internal
pub fn float_to_fixed_2(value: Float) -> String {
  let negative = value <. 0.0
  let abs_value = case negative {
    True -> 0.0 -. value
    False -> value
  }
  let cents = float.round(abs_value *. 100.0)
  let dollars = cents / 100
  let remainder = cents - dollars * 100
  let cents_str = case remainder < 10 {
    True -> "0" <> int.to_string(remainder)
    False -> int.to_string(remainder)
  }
  let sign = case negative {
    True -> "-"
    False -> ""
  }
  sign <> int.to_string(dollars) <> "." <> cents_str
}

@internal
pub fn format_decimal_amount(value: Float) -> String {
  let fixed = float_to_fixed_2(value)
  case string.ends_with(fixed, "00") {
    True -> string.drop_end(fixed, 3) <> ".0"
    False ->
      case string.ends_with(fixed, "0") {
        True -> string.drop_end(fixed, 1)
        False -> fixed
      }
  }
}

@internal
pub fn mutation_user_error(
  field_path: List(String),
  message: String,
) -> OrderMutationUserError {
  order_mutation_user_error(
    list.map(field_path, UserErrorField),
    message,
    infer_user_error_code(Some(field_path), message),
  )
}

@internal
pub fn order_mutation_user_error(
  field_path: List(UserErrorFieldSegment),
  message: String,
  code: Option(String),
) -> OrderMutationUserError {
  OrderMutationUserError(field_path: field_path, message: message, code: code)
}

@internal
pub fn serialize_order_mutation_user_error(
  field: Selection,
  error: OrderMutationUserError,
) -> Json {
  let base_fields = [
    #("field", SrcList(list.map(error.field_path, user_error_path_source))),
    #("message", SrcString(error.message)),
  ]
  let fields = case error.code {
    Some(code) -> list.append(base_fields, [#("code", SrcString(code))])
    None -> list.append(base_fields, [#("code", SrcNull)])
  }
  project_graphql_value(
    src_object(fields),
    selection_children(field),
    dict.new(),
  )
}

@internal
pub fn user_error_path_source(segment: UserErrorFieldSegment) -> SourceValue {
  case segment {
    UserErrorField(value) -> SrcString(value)
    UserErrorIndex(value) -> SrcInt(value)
  }
}

@internal
pub fn order_money_set_from_input(
  input: Option(Dict(String, root_field.ResolvedValue)),
  currency_code: String,
  fallback_amount: Float,
) -> CapturedJsonValue {
  let fields = input |> option.unwrap(dict.new())
  let shop_money = read_object(fields, "shopMoney")
  let amount =
    case shop_money {
      Some(money) -> read_number(money, "amount")
      None -> read_number(fields, "amount")
    }
    |> option.unwrap(fallback_amount)
  let shop_currency =
    case shop_money {
      Some(money) -> read_string(money, "currencyCode")
      None -> read_string(fields, "currencyCode")
    }
    |> option.unwrap(currency_code)
  let presentment_money = read_object(fields, "presentmentMoney")
  let presentment_amount =
    case presentment_money {
      Some(money) -> read_number(money, "amount")
      None -> None
    }
    |> option.unwrap(amount)
  let presentment_currency =
    case presentment_money {
      Some(money) -> read_string(money, "currencyCode")
      None -> None
    }
    |> option.unwrap(shop_currency)
  money_set_with_presentment(
    amount,
    shop_currency,
    presentment_amount,
    presentment_currency,
  )
}

@internal
pub fn captured_tax_lines(value: CapturedJsonValue) -> List(CapturedJsonValue) {
  case captured_object_field(value, "taxLines") {
    Some(CapturedArray(tax_lines)) -> tax_lines
    _ -> []
  }
}

@internal
pub fn find_order_with_fulfillment(
  store: Store,
  fulfillment_id: String,
) -> Option(#(OrderRecord, CapturedJsonValue)) {
  store.list_effective_orders(store)
  |> list.find_map(fn(order) {
    case find_fulfillment(order_fulfillments(order.data), fulfillment_id) {
      Some(fulfillment) -> Ok(#(order, fulfillment))
      None -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn find_fulfillment(
  fulfillments: List(CapturedJsonValue),
  fulfillment_id: String,
) -> Option(CapturedJsonValue) {
  fulfillments
  |> list.find(fn(fulfillment) {
    captured_string_field(fulfillment, "id") == Some(fulfillment_id)
  })
  |> option.from_result
}

@internal
pub fn order_fulfillments(
  order_data: CapturedJsonValue,
) -> List(CapturedJsonValue) {
  case captured_object_field(order_data, "fulfillments") {
    Some(CapturedArray(fulfillments)) -> fulfillments
    _ -> []
  }
}

@internal
pub fn captured_field_or_null(
  value: CapturedJsonValue,
  name: String,
) -> CapturedJsonValue {
  captured_object_field(value, name) |> option.unwrap(CapturedNull)
}

@internal
pub fn replace_if_present(
  replacements: List(#(String, CapturedJsonValue)),
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  value: CapturedJsonValue,
) -> List(#(String, CapturedJsonValue)) {
  case dict.has_key(input, name) {
    True -> prepend_captured_replacement(replacements, name, value)
    False -> replacements
  }
}

@internal
pub fn prepend_captured_replacement(
  replacements: List(#(String, CapturedJsonValue)),
  name: String,
  value: CapturedJsonValue,
) -> List(#(String, CapturedJsonValue)) {
  [#(name, value), ..replacements]
}

@internal
pub fn replace_captured_object_fields(
  value: CapturedJsonValue,
  replacements: List(#(String, CapturedJsonValue)),
) -> CapturedJsonValue {
  case value {
    CapturedObject(fields) ->
      CapturedObject(upsert_captured_fields(fields, replacements))
    _ -> CapturedObject(replacements)
  }
}

@internal
pub fn upsert_captured_fields(
  fields: List(#(String, CapturedJsonValue)),
  replacements: List(#(String, CapturedJsonValue)),
) -> List(#(String, CapturedJsonValue)) {
  let replaced =
    list.map(fields, fn(pair) {
      let #(key, existing) = pair
      case find_captured_replacement(replacements, key) {
        Some(value) -> #(key, value)
        None -> #(key, existing)
      }
    })
  let appended =
    replacements
    |> list.filter(fn(pair) {
      let #(key, _) = pair
      !list.any(fields, fn(existing_pair) {
        let #(existing_key, _) = existing_pair
        existing_key == key
      })
    })
  list.append(replaced, appended)
}

@internal
pub fn find_captured_replacement(
  replacements: List(#(String, CapturedJsonValue)),
  name: String,
) -> Option(CapturedJsonValue) {
  replacements
  |> list.find_map(fn(pair) {
    let #(key, value) = pair
    case key == name {
      True -> Ok(value)
      False -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn money_set(amount: Float, currency_code: String) -> CapturedJsonValue {
  money_set_with_presentment(amount, currency_code, amount, currency_code)
}

@internal
pub fn money_set_with_presentment(
  shop_amount: Float,
  shop_currency_code: String,
  presentment_amount: Float,
  presentment_currency_code: String,
) -> CapturedJsonValue {
  money_set_string_with_presentment(
    float.to_string(shop_amount),
    shop_currency_code,
    float.to_string(presentment_amount),
    presentment_currency_code,
  )
}

@internal
pub fn captured_money_presentment_amount(
  value: CapturedJsonValue,
  name: String,
) -> Float {
  case captured_object_field(value, name) {
    Some(money) -> captured_money_presentment_value(money)
    None -> 0.0
  }
}

@internal
pub fn captured_money_presentment_value(value: CapturedJsonValue) -> Float {
  case captured_object_field(value, "presentmentMoney") {
    Some(presentment_money) ->
      case captured_object_field(presentment_money, "amount") {
        Some(CapturedString(amount)) -> parse_amount(amount)
        _ -> captured_money_value(value)
      }
    None -> captured_money_value(value)
  }
}

@internal
pub fn first_money_set_presentment_currency(
  money_sets: List(CapturedJsonValue),
  fallback: String,
) -> String {
  money_sets
  |> list.find_map(fn(money_set) {
    case captured_object_field(money_set, "presentmentMoney") {
      Some(presentment_money) ->
        captured_string_field(presentment_money, "currencyCode")
        |> option_to_result
      None -> Error(Nil)
    }
  })
  |> result.unwrap(fallback)
}

@internal
pub fn money_sets_from_field(
  values: List(CapturedJsonValue),
  field_name: String,
) -> List(CapturedJsonValue) {
  values
  |> list.filter_map(fn(value) {
    captured_object_field(value, field_name) |> option_to_result
  })
}

@internal
pub fn tax_line_money_sets(
  values: List(CapturedJsonValue),
) -> List(CapturedJsonValue) {
  values
  |> list.flat_map(captured_tax_lines)
  |> money_sets_from_field("priceSet")
}

@internal
pub fn captured_money_amount(value: CapturedJsonValue, name: String) -> Float {
  case captured_object_field(value, name) {
    Some(money) -> captured_money_value(money)
    None -> 0.0
  }
}

@internal
pub fn captured_money_value(value: CapturedJsonValue) -> Float {
  case captured_object_field(value, "shopMoney") {
    Some(shop_money) ->
      case captured_object_field(shop_money, "amount") {
        Some(CapturedString(amount)) -> parse_amount(amount)
        _ -> 0.0
      }
    None -> 0.0
  }
}

@internal
pub fn read_object(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, name) {
    Ok(root_field.ObjectVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_object_list(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, name) {
    Ok(root_field.ListVal(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          root_field.ObjectVal(fields) -> Ok(fields)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn read_string(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(input, name) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_string_list(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(String) {
  case dict.get(input, name) {
    Ok(root_field.ListVal(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          root_field.StringVal(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn read_int(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: Int,
) -> Int {
  case dict.get(input, name) {
    Ok(root_field.IntVal(value)) -> value
    _ -> fallback
  }
}

@internal
pub fn read_optional_int(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Int) {
  case dict.get(input, name) {
    Ok(root_field.IntVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_bool(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: Bool,
) -> Bool {
  case dict.get(input, name) {
    Ok(root_field.BoolVal(value)) -> value
    _ -> fallback
  }
}

@internal
pub fn read_number(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Float) {
  case dict.get(input, name) {
    Ok(root_field.IntVal(value)) -> Some(int.to_float(value))
    Ok(root_field.FloatVal(value)) -> Some(value)
    Ok(root_field.StringVal(value)) -> Some(parse_amount(value))
    _ -> None
  }
}

@internal
pub fn captured_number(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> CapturedJsonValue {
  case dict.get(input, name) {
    Ok(root_field.IntVal(value)) -> CapturedInt(value)
    Ok(root_field.FloatVal(value)) -> CapturedFloat(value)
    Ok(root_field.StringVal(value)) -> CapturedString(value)
    _ -> CapturedNull
  }
}

@internal
pub fn parse_amount(value: String) -> Float {
  float.parse(value) |> result.unwrap(0.0)
}

@internal
pub fn json_get(
  value: commit.JsonValue,
  key: String,
) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(field_key, field_value) if field_key == key -> Ok(field_value)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

@internal
pub fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn json_get_bool(value: commit.JsonValue, key: String) -> Option(Bool) {
  case json_get(value, key) {
    Some(commit.JsonBool(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn non_null_json(value: commit.JsonValue) -> Option(commit.JsonValue) {
  case value {
    commit.JsonNull -> None
    _ -> Some(value)
  }
}

@internal
pub fn captured_json_from_commit(value: commit.JsonValue) -> CapturedJsonValue {
  case value {
    commit.JsonNull -> CapturedNull
    commit.JsonBool(value) -> CapturedBool(value)
    commit.JsonInt(value) -> CapturedInt(value)
    commit.JsonFloat(value) -> CapturedFloat(value)
    commit.JsonString(value) -> CapturedString(value)
    commit.JsonArray(items) ->
      CapturedArray(list.map(items, captured_json_from_commit))
    commit.JsonObject(fields) ->
      CapturedObject(
        list.map(fields, fn(pair) {
          #(pair.0, captured_json_from_commit(pair.1))
        }),
      )
  }
}

@internal
pub fn optional_captured_string(value: Option(String)) -> CapturedJsonValue {
  case value {
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

@internal
pub fn optional_captured_number(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> CapturedJsonValue {
  case dict.get(input, name) {
    Ok(root_field.IntVal(value)) -> CapturedInt(value)
    Ok(root_field.FloatVal(value)) -> CapturedFloat(value)
    _ -> CapturedNull
  }
}

@internal
pub fn max_float(left: Float, right: Float) -> Float {
  case left >. right {
    True -> left
    False -> right
  }
}

@internal
pub fn captured_object_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(CapturedJsonValue) {
  case value {
    CapturedObject(fields) ->
      fields
      |> list.find_map(fn(pair) {
        let #(key, item) = pair
        case key == name {
          True -> Ok(item)
          False -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

@internal
pub fn captured_string_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(String) {
  case captured_object_field(value, name) {
    Some(CapturedString(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn captured_bool_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(Bool) {
  case captured_object_field(value, name) {
    Some(CapturedBool(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn captured_number_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(Float) {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> Some(int.to_float(value))
    Some(CapturedFloat(value)) -> Some(value)
    Some(CapturedString(value)) -> Some(parse_amount(value))
    _ -> None
  }
}

@internal
pub fn captured_int_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(Int) {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn serialize_user_error(
  field: Selection,
  error: #(List(String), String, Option(String)),
) -> Json {
  let #(field_path, message, code) = error
  let source = user_error_source(Some(field_path), message, code)
  project_graphql_value(source, selection_children(field), dict.new())
}

@internal
pub fn user_error_source(
  field_path: Option(List(String)),
  message: String,
  code: Option(String),
) -> SourceValue {
  src_object([
    #("field", user_error_field_source(field_path)),
    #("message", SrcString(message)),
    #("code", case code {
      Some(code) -> SrcString(code)
      None -> SrcNull
    }),
  ])
}

@internal
pub fn user_error_field_source(
  field_path: Option(List(String)),
) -> SourceValue {
  case field_path {
    Some(path) -> SrcList(list.map(path, SrcString))
    None -> SrcNull
  }
}

@internal
pub fn captured_json_source(value: CapturedJsonValue) -> SourceValue {
  case value {
    CapturedNull -> SrcNull
    CapturedBool(value) -> SrcBool(value)
    CapturedInt(value) -> SrcInt(value)
    CapturedFloat(value) -> SrcFloat(value)
    CapturedString(value) -> SrcString(value)
    CapturedArray(items) -> SrcList(list.map(items, captured_json_source))
    CapturedObject(fields) ->
      SrcObject(
        fields
        |> list.fold(dict.new(), fn(acc, pair) {
          let #(key, item) = pair
          dict.insert(acc, key, captured_json_source(item))
        }),
      )
  }
}

@internal
pub fn selection_children(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

@internal
pub fn field_arguments(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  root_field.get_field_arguments(field, variables)
  |> result.unwrap(dict.new())
}

@internal
pub fn read_string_argument(
  field: Selection,
  name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  field_arguments(field, variables) |> read_string_arg(name)
}

@internal
pub fn read_string_arg(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_int_argument(
  field: Selection,
  name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(Int) {
  case dict.get(field_arguments(field, variables), name) {
    Ok(root_field.IntVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_bool_argument(
  field: Selection,
  name: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(Bool) {
  case dict.get(field_arguments(field, variables), name) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn get_operation_path_label(document: String) -> String {
  case parse_operation.parse_operation(document) {
    Ok(parsed) -> {
      let kind = case parsed.type_ {
        parse_operation.QueryOperation -> "query"
        parse_operation.MutationOperation -> "mutation"
      }
      case parsed.name {
        Some(name) -> kind <> " " <> name
        None -> kind
      }
    }
    Error(_) -> "mutation"
  }
}

@internal
pub fn min_int(left: Int, right: Int) -> Int {
  case left < right {
    True -> left
    False -> right
  }
}
