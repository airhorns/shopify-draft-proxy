//// Shared internal payments domain types and helpers.

import gleam/bit_array
import gleam/dict.{type Dict}
import gleam/dynamic/decode
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcInt, SrcList, SrcNull,
  SrcString, get_field_response_key, project_graphql_value, src_object,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type Money, type PaymentTermsTemplateRecord, PaymentTermsTemplateRecord,
}

@internal
pub const customization_app_id: String = "347082227713"

@internal
pub const duplication_prefix: String = "shopify-draft-proxy:customer-payment-method-duplication:"

@internal
pub const credit_card_processing_session_id: String = "shopify-draft-proxy:processing"

@internal
pub const payment_terms_creation_unsuccessful_code: String = "PAYMENT_TERMS_CREATION_UNSUCCESSFUL"

@internal
pub const payment_terms_update_unsuccessful_code: String = "PAYMENT_TERMS_UPDATE_UNSUCCESSFUL"

@internal
pub const payment_terms_delete_unsuccessful_code: String = "PAYMENT_TERMS_DELETE_UNSUCCESSFUL"

@internal
pub const multiple_payment_schedules_message: String = "Cannot create payment terms with multiple schedules."

@internal
pub type UserError {
  UserError(field: Option(List(String)), message: String, code: Option(String))
}

@internal
pub type PaymentTermsSchedulePlan {
  PaymentTermsSchedulePlan(
    issued_at: Option(String),
    due_at: Option(String),
    include_schedule: Bool,
  )
}

@internal
pub type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
    root_name: String,
    notes: Option(String),
  )
}

@internal
pub fn has_key(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Bool {
  case dict.get(input, key) {
    Ok(_) -> True
    Error(_) -> False
  }
}

@internal
pub fn read_string_field(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case dict.get(input, key) {
    Ok(root_field.StringVal(value)) ->
      case string.trim(value) {
        "" -> None
        _ -> Some(value)
      }
    _ -> None
  }
}

@internal
pub fn read_bool_field(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Bool) {
  case dict.get(input, key) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn is_shopify_gid(value: Option(String), resource_type: String) -> Bool {
  case value {
    Some(id) -> string.starts_with(id, "gid://shopify/" <> resource_type <> "/")
    None -> False
  }
}

@internal
pub fn gid_tail(id: String) -> String {
  case string.split(id, on: "/") |> list.reverse {
    [tail, ..] -> tail
    [] -> id
  }
}

@internal
pub fn option_string_source(value: Option(String)) -> SourceValue {
  case value {
    Some(value) -> SrcString(value)
    None -> SrcNull
  }
}

@internal
pub fn option_bool_source(value: Option(Bool)) -> SourceValue {
  case value {
    Some(value) -> SrcBool(value)
    None -> SrcNull
  }
}

@internal
pub fn option_int_source(value: Option(Int)) -> SourceValue {
  case value {
    Some(value) -> SrcInt(value)
    None -> SrcNull
  }
}

@internal
pub fn money_source(value: Money) -> SourceValue {
  src_object([
    #("__typename", SrcString("MoneyV2")),
    #("amount", SrcString(value.amount)),
    #("currencyCode", SrcString(value.currency_code)),
  ])
}

@internal
pub fn option_money_source(value: Option(Money)) -> SourceValue {
  case value {
    Some(money) -> money_source(money)
    None -> SrcNull
  }
}

@internal
pub fn payment_terms_templates() -> List(PaymentTermsTemplateRecord) {
  [
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/1",
      "Due on receipt",
      "Due on receipt",
      None,
      "RECEIPT",
      "Due on receipt",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/9",
      "Due on fulfillment",
      "Due on fulfillment",
      None,
      "FULFILLMENT",
      "Due on fulfillment",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/2",
      "Net 7",
      "Within 7 days",
      Some(7),
      "NET",
      "Net 7",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/3",
      "Net 15",
      "Within 15 days",
      Some(15),
      "NET",
      "Net 15",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/4",
      "Net 30",
      "Within 30 days",
      Some(30),
      "NET",
      "Net 30",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/8",
      "Net 45",
      "Within 45 days",
      Some(45),
      "NET",
      "Net 45",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/5",
      "Net 60",
      "Within 60 days",
      Some(60),
      "NET",
      "Net 60",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/6",
      "Net 90",
      "Within 90 days",
      Some(90),
      "NET",
      "Net 90",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/7",
      "Fixed",
      "Fixed date",
      None,
      "FIXED",
      "Fixed",
    ),
  ]
}

@internal
pub fn empty_connection_source() -> SourceValue {
  src_object([
    #("nodes", SrcList([])),
    #("edges", SrcList([])),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", SrcNull),
        #("endCursor", SrcNull),
      ]),
    ),
  ])
}

@internal
pub fn normalize_payment_customization_metafield_namespace(
  namespace: String,
) -> String {
  case string.starts_with(namespace, "$app:") {
    True ->
      "app--" <> customization_app_id <> "--" <> string.drop_start(namespace, 5)
    False -> namespace
  }
}

@internal
pub fn project_payload(
  field: Selection,
  fragments: FragmentMap,
  entries: List(#(String, SourceValue)),
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(src_object(entries), selections, fragments)
    _ -> json.object([])
  }
}

@internal
pub fn user_errors_source(errors: List(UserError)) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      src_object([
        #("field", case error.field {
          Some(field) -> SrcList(list.map(field, SrcString))
          None -> SrcNull
        }),
        #("message", SrcString(error.message)),
        #("code", case error.code {
          Some(code) -> SrcString(code)
          None -> SrcNull
        }),
      ])
    }),
  )
}

@internal
pub fn mutation_payload_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  payload: Json,
  staged_ids: List(String),
  root_name: String,
  notes: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      get_field_response_key(field),
      payload,
      staged_ids,
      root_name,
      notes,
    ),
    store,
    identity,
  )
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
          #(name, child) if name == key -> Ok(child)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

@internal
pub fn non_null_json(
  value: Option(commit.JsonValue),
) -> Option(commit.JsonValue) {
  case value {
    Some(commit.JsonNull) -> None
    Some(v) -> Some(v)
    None -> None
  }
}

@internal
pub fn json_array_items(
  value: Option(commit.JsonValue),
) -> List(commit.JsonValue) {
  case non_null_json(value) {
    Some(commit.JsonArray(items)) -> items
    _ -> []
  }
}

@internal
pub fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  json_get(value, key) |> option.then(json_scalar_string)
}

@internal
pub fn json_get_bool(value: commit.JsonValue, key: String) -> Option(Bool) {
  case json_get(value, key) {
    Some(commit.JsonBool(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn json_get_int(value: commit.JsonValue, key: String) -> Option(Int) {
  case json_get(value, key) {
    Some(commit.JsonInt(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn json_scalar_string(value: commit.JsonValue) -> Option(String) {
  case value {
    commit.JsonString(s) -> Some(s)
    _ -> None
  }
}

@internal
pub fn json_get_data_string(
  value: commit.JsonValue,
  key: String,
) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonNull) -> Some("__null")
    Some(commit.JsonString(s)) -> Some(s)
    Some(commit.JsonBool(True)) -> Some("true")
    Some(commit.JsonBool(False)) -> Some("false")
    _ -> None
  }
}

@internal
pub fn encode_duplication_data(
  method_id: String,
  target_customer_id: String,
  target_shop_id: String,
) -> String {
  let body =
    json.object([
      #("customerPaymentMethodId", json.string(method_id)),
      #("targetCustomerId", json.string(target_customer_id)),
      #("targetShopId", json.string(target_shop_id)),
    ])
    |> json.to_string
    |> bit_array.from_string
    |> bit_array.base64_url_encode(False)
  duplication_prefix <> body
}

@internal
pub fn decode_duplication_data(
  raw: String,
) -> Result(Dict(String, String), Nil) {
  case string.starts_with(raw, duplication_prefix) {
    False -> Error(Nil)
    True -> {
      let encoded =
        string.drop_start(raw, up_to: string.length(duplication_prefix))
      use bits <- result.try(bit_array.base64_url_decode(encoded))
      use text <- result.try(bit_array.to_string(bits))
      json.parse(text, decode.dict(decode.string, decode.string))
      |> result.replace_error(Nil)
    }
  }
}

@internal
pub fn uri_encode(value: String) -> String {
  value
  |> string.replace(" ", "%20")
  |> string.replace("/", "%2F")
  |> string.replace("?", "%3F")
  |> string.replace("&", "%26")
}
