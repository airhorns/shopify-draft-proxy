//// Discounts payload builders, projection helpers, hydration, and validation helpers.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection}

import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, field_locations_json, get_field_response_key,
  project_graphql_value,
}

import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type DiscountRecord, type ShopifyFunctionAppRecord, type ShopifyFunctionRecord,
  DiscountRecord, ShopifyFunctionAppRecord, ShopifyFunctionRecord,
}

@internal
pub const discount_function_app_id: String = "347082227713"

import shopify_draft_proxy/proxy/discounts/queries.{
  child_fields, discount_record_timestamp,
}

import shopify_draft_proxy/proxy/discounts/types as discount_types

@internal
pub fn payload_json(
  root: String,
  field: Selection,
  fragments: FragmentMap,
  record: Option(DiscountRecord),
  user_errors: List(SourceValue),
) -> Json {
  let owner_field = owner_node_field(root)
  let owner_payload = case record {
    Some(record) -> discount_types.discount_owner_source(record)
    None -> SrcNull
  }
  let discount_payload = case record {
    Some(record) ->
      case discount_types.discount_owner_source(record) {
        SrcObject(fields) ->
          case record.owner_kind {
            "automatic" ->
              dict.get(fields, "automaticDiscount") |> result.unwrap(SrcNull)
            _ -> dict.get(fields, "codeDiscount") |> result.unwrap(SrcNull)
          }
        _ -> SrcNull
      }
    None -> SrcNull
  }
  project_graphql_value(
    SrcObject(
      dict.from_list([
        #(owner_field, owner_payload),
        #("codeDiscountNode", owner_payload),
        #("automaticDiscountNode", owner_payload),
        #("codeAppDiscount", discount_payload),
        #("automaticAppDiscount", discount_payload),
        #("userErrors", SrcList(user_errors)),
      ]),
    ),
    child_fields(field),
    fragments,
  )
}

@internal
pub fn owner_node_field(root: String) -> String {
  case string.starts_with(root, "discountAutomatic") {
    True -> "automaticDiscountNode"
    False -> "codeDiscountNode"
  }
}

@internal
pub fn build_discount_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  id: String,
  owner_kind: String,
  discount_type: String,
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(DiscountRecord),
) -> #(DiscountRecord, SyntheticIdentityRegistry) {
  let title =
    discount_types.read_string(input, "title")
    |> option.or(existing |> option.then(fn(r) { r.title }))
    |> option.unwrap("")
  let code =
    discount_types.read_string(input, "code")
    |> option.or(discount_types.read_string(input, "codePrefix"))
    |> option.or(existing |> option.then(fn(r) { r.code }))
  let owner_field = case owner_kind {
    "automatic" -> "automaticDiscount"
    _ -> "codeDiscount"
  }
  let starts_at =
    input_or_existing_discount_source(input, existing, owner_field, "startsAt")
  let ends_at =
    input_or_existing_discount_source(input, existing, owner_field, "endsAt")
  let status =
    derive_discount_status(starts_at, ends_at, synthetic_now(identity))
  let typename = typename_for(owner_kind, discount_type)
  let #(code_source, next_identity) =
    discount_types.code_connection_for_record(identity, code, existing)
  let #(mutation_timestamp, next_identity) =
    synthetic_identity.make_synthetic_timestamp(next_identity)
  let created_at =
    existing
    |> option.then(fn(record) { discount_record_timestamp(record, "createdAt") })
    |> option.unwrap(mutation_timestamp)
  let discount_classes = discount_classes_for_input(input, discount_type)
  let discount_class = primary_discount_class(discount_classes)
  let allocation_method =
    allocation_method_for_input(input, discount_type, discount_class)
  let discount =
    SrcObject(
      dict.from_list([
        #("__typename", SrcString(typename)),
        #("discountId", SrcString(id)),
        #("title", SrcString(title)),
        #("status", SrcString(status)),
        #("summary", SrcString(summary_for(input, discount_type))),
        #("startsAt", starts_at),
        #("endsAt", ends_at),
        #("createdAt", SrcString(created_at)),
        #("updatedAt", SrcString(mutation_timestamp)),
        #("asyncUsageCount", SrcInt(0)),
        #(
          "discountClasses",
          discount_types.string_list_source(discount_classes),
        ),
        #("discountClass", SrcString(discount_class)),
        #("allocationMethod", SrcString(allocation_method)),
        #(
          "combinesWith",
          discount_types.object_value_or_default(
            input,
            "combinesWith",
            discount_types.combines_default(),
          ),
        ),
        #("codes", code_source),
        #(
          "codesCount",
          discount_types.count_source(case code {
            Some(_) -> 1
            None -> 0
          }),
        ),
        #(
          "context",
          discount_types.context_source(discount_types.read_value(
            input,
            "context",
          )),
        ),
        #(
          "customerGets",
          customer_gets_record_source(input, existing, owner_field),
        ),
        #(
          "customerBuys",
          customer_buys_record_source(input, existing, owner_field),
        ),
        #(
          "minimumRequirement",
          discount_types.minimum_source(discount_types.read_value(
            input,
            "minimumRequirement",
          )),
        ),
        #(
          "destinationSelection",
          discount_types.destination_source(discount_types.read_value(
            input,
            "destination",
          )),
        ),
        #(
          "maximumShippingPrice",
          discount_types.money_source(discount_types.read_value(
            input,
            "maximumShippingPrice",
          )),
        ),
        #(
          "appliesOncePerCustomer",
          discount_types.bool_source(
            discount_types.read_value(input, "appliesOncePerCustomer"),
            False,
          ),
        ),
        #(
          "appliesOnOneTimePurchase",
          discount_types.bool_source(
            discount_types.read_value(input, "appliesOnOneTimePurchase"),
            True,
          ),
        ),
        #(
          "appliesOnSubscription",
          discount_types.bool_source(
            discount_types.read_value(input, "appliesOnSubscription"),
            False,
          ),
        ),
        #(
          "recurringCycleLimit",
          discount_types.resolved_to_source(discount_types.read_value(
            input,
            "recurringCycleLimit",
          )),
        ),
        #(
          "usageLimit",
          discount_types.resolved_to_source(discount_types.read_value(
            input,
            "usageLimit",
          )),
        ),
        #(
          "usesPerOrderLimit",
          discount_types.resolved_to_source(discount_types.read_value(
            input,
            "usesPerOrderLimit",
          )),
        ),
        #(
          "appDiscountType",
          discount_types.app_discount_type_source(store, input),
        ),
      ]),
    )
  #(
    DiscountRecord(
      id: id,
      owner_kind: owner_kind,
      discount_type: discount_type,
      title: Some(title),
      status: status,
      code: code,
      payload: discount_types.source_to_captured(
        SrcObject(
          dict.from_list([
            #("id", SrcString(id)),
            #(owner_field, discount),
          ]),
        ),
      ),
      cursor: None,
    ),
    next_identity,
  )
}

@internal
pub fn customer_gets_record_source(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(DiscountRecord),
  owner_field: String,
) -> SourceValue {
  case discount_types.read_value(input, "customerGets") {
    root_field.ObjectVal(fields) as value ->
      case customer_items_remove_only(fields) {
        True ->
          existing_discount_nested_source(existing, owner_field, [
            "customerGets",
          ])
          |> option.unwrap(discount_types.customer_gets_source(value))
        False -> discount_types.customer_gets_source(value)
      }
    value -> discount_types.customer_gets_source(value)
  }
}

@internal
pub fn customer_buys_record_source(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(DiscountRecord),
  owner_field: String,
) -> SourceValue {
  case discount_types.read_value(input, "customerBuys") {
    root_field.ObjectVal(fields) as value ->
      case customer_items_remove_only(fields) {
        True ->
          existing_discount_nested_source(existing, owner_field, [
            "customerBuys",
          ])
          |> option.unwrap(discount_types.customer_buys_source(value))
        False -> discount_types.customer_buys_source(value)
      }
    value -> discount_types.customer_buys_source(value)
  }
}

@internal
pub fn customer_items_remove_only(
  fields: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case discount_types.read_value(fields, "items") {
    root_field.ObjectVal(items) -> item_refs_remove_only(items)
    _ -> False
  }
}

@internal
pub fn item_refs_remove_only(
  items: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case dict.get(items, "products"), dict.get(items, "collections") {
    Ok(root_field.ObjectVal(products)), Error(_) ->
      has_any_remove_refs(products) && !has_any_product_add_refs(products)
    Error(_), Ok(root_field.ObjectVal(collections)) ->
      has_field_string_refs(collections, "remove")
      && !has_field_string_refs(collections, "add")
    _, _ -> False
  }
}

@internal
pub fn has_any_remove_refs(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  has_field_string_refs(input, "productsToRemove")
  || has_field_string_refs(input, "productVariantsToRemove")
}

@internal
pub fn has_any_product_add_refs(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  has_field_string_refs(input, "productsToAdd")
  || has_field_string_refs(input, "productVariantsToAdd")
}

@internal
pub fn has_field_string_refs(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Bool {
  discount_types.read_string_array(input, field, []) != []
}

@internal
pub fn existing_discount_nested_source(
  existing: Option(DiscountRecord),
  owner_field: String,
  path: List(String),
) -> Option(SourceValue) {
  use record <- option.then(existing)
  case discount_types.captured_to_source(record.payload) {
    SrcObject(node) ->
      case dict.get(node, owner_field) {
        Ok(source) -> nested_source(source, path)
        _ -> None
      }
    _ -> None
  }
}

@internal
pub fn nested_source(
  source: SourceValue,
  path: List(String),
) -> Option(SourceValue) {
  case path {
    [] -> Some(source)
    [key, ..rest] ->
      case source {
        SrcObject(fields) ->
          case dict.get(fields, key) {
            Ok(next) -> nested_source(next, rest)
            _ -> None
          }
        _ -> None
      }
  }
}

@internal
pub fn input_or_existing_discount_source(
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(DiscountRecord),
  owner_field: String,
  name: String,
) -> SourceValue {
  case dict.get(input, name) {
    Ok(value) -> discount_types.resolved_to_source(value)
    Error(_) ->
      existing_discount_source(existing, owner_field, name)
      |> option.unwrap(SrcNull)
  }
}

@internal
pub fn existing_discount_source(
  existing: Option(DiscountRecord),
  owner_field: String,
  name: String,
) -> Option(SourceValue) {
  existing
  |> option.then(fn(record) {
    case discount_types.captured_to_source(record.payload) {
      SrcObject(node) ->
        case dict.get(node, owner_field) {
          Ok(SrcObject(discount)) ->
            dict.get(discount, name) |> option.from_result
          _ -> None
        }
      _ -> None
    }
  })
}

@internal
pub fn synthetic_now(identity: SyntheticIdentityRegistry) -> String {
  iso_timestamp.format_iso(identity.next_synthetic_time)
}

@internal
pub fn derive_discount_status(
  starts_at: SourceValue,
  ends_at: SourceValue,
  now: String,
) -> String {
  case iso_timestamp.parse_iso(now) {
    Ok(now_ms) ->
      derive_discount_status_ms(
        source_timestamp_ms(starts_at),
        source_timestamp_ms(ends_at),
        now_ms,
      )
    Error(_) -> "ACTIVE"
  }
}

@internal
pub fn derive_discount_status_ms(
  starts_at: Option(Int),
  ends_at: Option(Int),
  now_ms: Int,
) -> String {
  case starts_at, ends_at {
    Some(starts_ms), Some(ends_ms)
      if starts_ms > now_ms && ends_ms >= starts_ms
    -> "SCHEDULED"
    Some(starts_ms), None if starts_ms > now_ms -> "SCHEDULED"
    Some(starts_ms), Some(ends_ms)
      if ends_ms <= now_ms && starts_ms <= ends_ms
    -> "EXPIRED"
    None, Some(ends_ms) if ends_ms <= now_ms -> "EXPIRED"
    Some(starts_ms), Some(ends_ms) if starts_ms <= now_ms && ends_ms > now_ms ->
      "ACTIVE"
    Some(starts_ms), None if starts_ms <= now_ms -> "ACTIVE"
    None, Some(ends_ms) if ends_ms > now_ms -> "ACTIVE"
    None, None -> "ACTIVE"
    _, _ -> "ACTIVE"
  }
}

@internal
pub fn source_timestamp_ms(value: SourceValue) -> Option(Int) {
  case value {
    SrcString(timestamp) ->
      iso_timestamp.parse_iso(timestamp) |> option.from_result
    _ -> None
  }
}

@internal
pub fn typename_for(owner_kind: String, discount_type: String) -> String {
  case owner_kind, discount_type {
    "automatic", "basic" -> "DiscountAutomaticBasic"
    "automatic", "bxgy" -> "DiscountAutomaticBxgy"
    "automatic", "free_shipping" -> "DiscountAutomaticFreeShipping"
    "automatic", "app" -> "DiscountAutomaticApp"
    "code", "bxgy" -> "DiscountCodeBxgy"
    "code", "free_shipping" -> "DiscountCodeFreeShipping"
    "code", "app" -> "DiscountCodeApp"
    _, _ -> "DiscountCodeBasic"
  }
}

@internal
pub fn default_discount_classes(discount_type: String) -> List(String) {
  case discount_type {
    "free_shipping" -> ["SHIPPING"]
    "bxgy" -> ["PRODUCT"]
    _ -> ["ORDER"]
  }
}

@internal
pub fn discount_classes_for_input(
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
) -> List(String) {
  case discount_type {
    "free_shipping" -> default_discount_classes(discount_type)
    _ ->
      case discount_types.read_string(input, "discountClass") {
        Some(discount_class) -> [discount_class]
        None ->
          case discount_types.read_string_array(input, "discountClasses", []) {
            [_, ..] as classes -> classes
            [] ->
              case discount_type {
                "basic" -> infer_basic_discount_classes(input)
                _ -> default_discount_classes(discount_type)
              }
          }
      }
  }
}

@internal
pub fn infer_basic_discount_classes(
  input: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  case customer_gets_items_fields(input) {
    Some(items) ->
      case items_targets_entitled_resources(items) {
        True -> ["PRODUCT"]
        False -> ["ORDER"]
      }
    None -> ["ORDER"]
  }
}

@internal
pub fn items_targets_entitled_resources(
  items: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.has_key(items, "products")
  || dict.has_key(items, "productVariants")
  || dict.has_key(items, "collections")
}

@internal
pub fn primary_discount_class(classes: List(String)) -> String {
  case classes {
    [first, ..] -> first
    [] -> "ORDER"
  }
}

@internal
pub fn allocation_method_for_input(
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
  discount_class: String,
) -> String {
  case
    discount_type == "basic"
    && discount_class == "PRODUCT"
    && customer_gets_value_has_percentage(input)
  {
    True -> "EACH"
    False -> "ACROSS"
  }
}

@internal
pub fn customer_gets_value_has_percentage(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case customer_gets_value_fields(input) {
    Some(fields) -> dict.has_key(fields, "percentage")
    None -> False
  }
}

@internal
pub fn discount_class_for_record(record: DiscountRecord) -> String {
  case discount_types.captured_to_source(record.payload) {
    SrcObject(fields) -> {
      let discount = case record.owner_kind {
        "automatic" ->
          dict.get(fields, "automaticDiscount") |> result.unwrap(SrcNull)
        _ -> dict.get(fields, "codeDiscount") |> result.unwrap(SrcNull)
      }
      case discount {
        SrcObject(discount_fields) ->
          case dict.get(discount_fields, "discountClass") {
            Ok(SrcString(class)) -> class
            _ ->
              case dict.get(discount_fields, "discountClasses") {
                Ok(SrcList([SrcString(class), ..])) -> class
                _ ->
                  default_discount_classes(record.discount_type)
                  |> primary_discount_class
              }
          }
        _ ->
          default_discount_classes(record.discount_type)
          |> primary_discount_class
      }
    }
    _ ->
      default_discount_classes(record.discount_type) |> primary_discount_class
  }
}

@internal
pub fn summary_for(
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
) -> String {
  case discount_type {
    "free_shipping" -> "Free shipping"
    "bxgy" -> discount_types.bxgy_summary(input)
    _ ->
      case discount_types.read_string(input, "title") {
        Some(title) -> title
        None -> ""
      }
  }
}

@internal
pub fn validate_discount_input(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
  require_code: Bool,
  is_create: Bool,
  ignored_discount_id: Option(String),
) -> List(SourceValue) {
  let code_errors = case discount_type {
    "app" -> validate_app_discount_code_input(input_name, input, require_code)
    _ -> validate_discount_code_input(input_name, input, require_code)
  }
  let errors =
    list.append(
      code_errors,
      validate_context_customer_selection_conflict(input_name, input),
    )
  let errors = case discount_type {
    "app" ->
      list.append(
        errors,
        validate_app_discount_create_input(input_name, input, is_create),
      )
    _ -> errors
  }
  let errors = case discount_types.read_string(input, "code") {
    Some(code) ->
      case errors {
        [_, ..] -> errors
        [] ->
          case
            discount_types.find_effective_discount_by_code_ignoring(
              store,
              code,
              ignored_discount_id,
            )
          {
            Some(_) ->
              list.append(errors, [
                discount_types.user_error(
                  [input_name, "code"],
                  "Code must be unique. Please try a different code.",
                  "TAKEN",
                ),
              ])
            None -> errors
          }
      }
    None -> errors
  }
  let errors = case discount_type {
    "bxgy" -> list.append(errors, validate_bxgy_input(input_name, input))
    "basic" ->
      list.append(
        errors,
        basic_disallowed_discount_on_quantity_errors(input_name, input),
      )
    _ -> errors
  }
  let errors = case discount_type {
    "bxgy" -> errors
    _ ->
      list.append(
        errors,
        validate_customer_gets_value_bounds(input_name, input),
      )
  }
  let errors =
    list.append(
      errors,
      validate_subscription_fields(store, input_name, input, discount_type),
    )
  let errors =
    list.append(
      errors,
      validate_markets_fields(store, input_name, input, is_create),
    )
  let errors =
    list.append(
      errors,
      validate_cart_line_combination_tag_settings(
        input_name,
        input,
        discount_classes_for_input(input, discount_type),
      ),
    )
  let errors =
    list.append(errors, validate_minimum_requirement(input_name, input))
  let errors = case discount_type {
    "free_shipping" -> {
      case invalid_free_shipping_combines(input) {
        True ->
          list.append(errors, [
            discount_types.user_error(
              [input_name, "combinesWith"],
              "The combinesWith settings are not valid for the discount class.",
              "INVALID_COMBINES_WITH_FOR_DISCOUNT_CLASS",
            ),
          ])
        False -> errors
      }
      |> append_blank_title_error(input_name, input)
    }
    _ -> errors
  }
  let errors = case invalid_date_range(input) {
    True ->
      list.append(errors, [
        discount_types.user_error(
          [input_name, "endsAt"],
          "Ends at needs to be after starts_at",
          "INVALID",
        ),
      ])
    False -> errors
  }
  let errors =
    list.append(
      errors,
      validate_discount_item_refs(store, input_name, input, discount_type),
    )
  case errors {
    [_, ..] -> errors
    [] ->
      list.append(
        errors,
        validate_app_discount_function_input(
          store,
          input_name,
          input,
          discount_type,
        ),
      )
  }
}

@internal
pub fn validate_discount_create_input(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
  require_code: Bool,
  ignored_discount_id: Option(String),
) -> List(SourceValue) {
  validate_discount_input(
    store,
    input_name,
    input,
    discount_type,
    require_code,
    True,
    ignored_discount_id,
  )
}

@internal
pub fn validate_customer_gets_value_bounds(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case customer_gets_value_fields(input) {
    Some(fields) -> {
      let errors = case dict.get(fields, "percentage") {
        Ok(value) -> validate_percentage_value(input_name, value)
        Error(_) -> []
      }
      case dict.get(fields, "discountAmount") {
        Ok(root_field.ObjectVal(amount_fields)) ->
          list.append(
            errors,
            validate_discount_amount_value(
              input_name,
              discount_types.read_value(amount_fields, "amount"),
            ),
          )
        _ -> errors
      }
    }
    None -> []
  }
}

@internal
pub fn validate_percentage_value(
  input_name: String,
  value: root_field.ResolvedValue,
) -> List(SourceValue) {
  case resolved_decimal_float(value) {
    Some(percentage) if percentage <. 0.0 || percentage >. 1.0 -> [
      discount_types.user_error(
        [input_name, "customerGets", "value", "percentage"],
        "Value must be between 0.0 and 1.0",
        "VALUE_OUTSIDE_RANGE",
      ),
    ]
    _ -> []
  }
}

@internal
pub fn validate_discount_amount_value(
  input_name: String,
  value: root_field.ResolvedValue,
) -> List(SourceValue) {
  case resolved_decimal_float(value) {
    Some(amount) if amount <. 0.0 -> [
      discount_types.user_error(
        [input_name, "customerGets", "value", "discountAmount", "amount"],
        "Value must be less than or equal to 0",
        "LESS_THAN_OR_EQUAL_TO",
      ),
    ]
    _ -> []
  }
}

@internal
pub fn resolved_decimal_float(
  value: root_field.ResolvedValue,
) -> Option(Float) {
  case value {
    root_field.FloatVal(value) -> Some(value)
    root_field.IntVal(value) -> Some(int.to_float(value))
    root_field.StringVal(value) ->
      case float.parse(value) {
        Ok(value) -> Some(value)
        Error(_) ->
          int.parse(value) |> result.map(int.to_float) |> option.from_result
      }
    _ -> None
  }
}

@internal
pub fn validate_app_discount_create_input(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  is_create: Bool,
) -> List(SourceValue) {
  let errors = case input_name, title_is_blank(input) {
    "automaticAppDiscount", True -> [
      discount_types.user_error(
        [input_name, "title"],
        "Title can't be blank.",
        "INVALID",
      ),
    ]
    _, _ -> []
  }
  let errors = case is_create, input_value_is_present(input, "startsAt") {
    True, False ->
      list.append(errors, [
        discount_types.user_error(
          [input_name, "startsAt"],
          "Starts at can't be blank.",
          "INVALID",
        ),
      ])
    _, _ -> errors
  }
  let errors = case dict.get(input, "discountClasses") {
    Ok(root_field.ListVal([])) ->
      list.append(errors, [
        discount_types.user_error(
          [input_name, "discountClasses"],
          "Discount classes can't be empty.",
          "INVALID",
        ),
      ])
    _ -> errors
  }
  list.append(
    errors,
    validate_app_discount_customer_selection(input_name, input),
  )
}

@internal
pub fn validate_app_discount_code_input(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  require_code: Bool,
) -> List(SourceValue) {
  case
    input_name == "codeAppDiscount",
    discount_types.read_string(input, "code")
  {
    True, None ->
      case require_code {
        True -> [app_discount_code_blank_error(input_name)]
        False -> []
      }
    True, Some(code) ->
      case string.trim(code) {
        "" -> [app_discount_code_blank_error(input_name)]
        _ ->
          case string.length(code) > 255 {
            True -> [
              discount_types.user_error(
                [input_name, "code"],
                "Code is too long (maximum is 255 characters)",
                "TOO_LONG",
              ),
            ]
            False -> []
          }
      }
    False, _ -> []
  }
}

@internal
pub fn app_discount_code_blank_error(input_name: String) -> SourceValue {
  discount_types.user_error(
    [input_name, "code"],
    "Discount code can't be blank.",
    "INVALID",
  )
}

@internal
pub fn validate_app_discount_customer_selection(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case customer_selection_fields(input) {
    Some(fields) ->
      case
        customer_selection_empty_add(fields, "customers")
        || customer_selection_empty_add(fields, "customerSegments")
      {
        True -> [
          discount_types.user_error(
            [input_name, "customerSelection"],
            "a minimum of one prerequisite segment or prerequisite customer must be provided",
            "INVALID",
          ),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn customer_selection_empty_add(
  fields: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(fields, name) {
    Ok(root_field.ObjectVal(selection)) ->
      case dict.get(selection, "add") {
        Ok(root_field.ListVal([])) -> True
        _ -> False
      }
    _ -> False
  }
}

@internal
pub fn validate_app_discount_function_input(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
) -> List(SourceValue) {
  case discount_type {
    "app" -> {
      let function_id = discount_types.read_string(input, "functionId")
      let function_handle = discount_types.read_string(input, "functionHandle")
      case function_id, function_handle {
        None, None -> [
          app_discount_missing_function_identifier_error(input_name),
        ]
        Some(_), Some(_) -> [
          app_discount_multiple_function_identifiers_error(input_name),
        ]
        Some(value), None ->
          validate_app_discount_function_reference(
            store,
            input_name,
            "functionId",
            value,
          )
        None, Some(value) ->
          validate_app_discount_function_reference(
            store,
            input_name,
            "functionHandle",
            value,
          )
      }
    }
    _ -> []
  }
}

@internal
pub fn validate_app_discount_function_reference(
  store: Store,
  input_name: String,
  field_name: String,
  value: String,
) -> List(SourceValue) {
  case discount_types.find_shopify_function(store, value) {
    None -> [
      app_discount_function_not_found_error(input_name, field_name, value),
    ]
    Some(record) ->
      case app_discount_function_api_supported(record) {
        True -> []
        False -> [
          app_discount_function_does_not_implement_error(input_name, field_name),
        ]
      }
  }
}

@internal
pub fn app_discount_missing_function_identifier_error(
  input_name: String,
) -> SourceValue {
  discount_types.user_error(
    [input_name, "functionHandle"],
    "Function id can't be blank.",
    "MISSING_FUNCTION_IDENTIFIER",
  )
}

@internal
pub fn app_discount_multiple_function_identifiers_error(
  input_name: String,
) -> SourceValue {
  discount_types.user_error(
    [input_name],
    "Only one of functionId or functionHandle is allowed.",
    "MULTIPLE_FUNCTION_IDENTIFIERS",
  )
}

@internal
pub fn app_discount_function_not_found_error(
  input_name: String,
  field_name: String,
  value: String,
) -> SourceValue {
  discount_types.user_error(
    [input_name, field_name],
    "Function "
      <> value
      <> " not found. Ensure that it is released in the current app ("
      <> discount_function_app_id
      <> "), and that the app is installed.",
    "INVALID",
  )
}

@internal
pub fn app_discount_function_does_not_implement_error(
  input_name: String,
  field_name: String,
) -> SourceValue {
  discount_types.user_error_with_code(
    [input_name, field_name],
    "Unexpected Function API. The provided function must implement one of the following extension targets: [product_discounts, order_discounts, shipping_discounts, discount].",
    None,
  )
}

@internal
pub fn app_discount_function_api_supported(
  record: ShopifyFunctionRecord,
) -> Bool {
  case record.api_type {
    None -> True
    Some(api_type) ->
      list.contains(
        [
          "DISCOUNT",
          "PRODUCT_DISCOUNT",
          "PRODUCT_DISCOUNTS",
          "ORDER_DISCOUNT",
          "ORDER_DISCOUNTS",
          "SHIPPING_DISCOUNT",
          "SHIPPING_DISCOUNTS",
          "PURCHASE_PRODUCT_DISCOUNT_RUN",
          "PURCHASE_ORDER_DISCOUNT_RUN",
          "PURCHASE_SHIPPING_DISCOUNT_RUN",
        ],
        normalize_function_api_type(api_type),
      )
  }
}

@internal
pub fn normalize_function_api_type(api_type: String) -> String {
  api_type
  |> string.uppercase
  |> string.replace("-", "_")
  |> string.replace(".", "_")
}

@internal
pub fn validate_context_customer_selection_conflict(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case
    input_value_is_present(input, "context"),
    input_value_is_present(input, "customerSelection")
  {
    True, True -> [
      discount_types.user_error(
        [input_name, "context"],
        "Only one of context or customerSelection can be provided.",
        "INVALID",
      ),
    ]
    _, _ -> []
  }
}

@internal
pub fn input_value_is_present(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(input, name) {
    Ok(root_field.NullVal) | Error(_) -> False
    Ok(_) -> True
  }
}

@internal
pub fn validate_subscription_fields(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
) -> List(SourceValue) {
  case subscription_field_location(discount_type, input_name) {
    Some(location) ->
      validate_subscription_field_values(store, input_name, input, location)
    None -> []
  }
}

@internal
pub type SubscriptionFieldLocation {
  SubscriptionCustomerGetsFields
  SubscriptionTopLevelFields
}

@internal
pub fn subscription_field_location(
  discount_type: String,
  input_name: String,
) -> Option(SubscriptionFieldLocation) {
  case discount_type, input_name {
    "basic", _ -> Some(SubscriptionCustomerGetsFields)
    "free_shipping", "freeShippingAutomaticDiscount" -> None
    "free_shipping", _ -> Some(SubscriptionTopLevelFields)
    _, _ -> None
  }
}

@internal
pub fn maybe_hydrate_discount_subscription_capability(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
  upstream: UpstreamContext,
) -> Store {
  case store.get_effective_shop(store) {
    Some(_) -> store
    None ->
      case subscription_field_location(discount_type, input_name) {
        Some(location) ->
          case has_subscription_validation_fields(input, location) {
            True -> fetch_shop_subscription_capability(store, upstream)
            False -> store
          }
        None -> store
      }
  }
}

@internal
pub fn has_subscription_validation_fields(
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
) -> Bool {
  let #(fields, _) = subscription_field_source("", input, location)
  dict.has_key(fields, "appliesOnSubscription")
  || dict.has_key(fields, "appliesOnOneTimePurchase")
  || dict.has_key(input, "recurringCycleLimit")
}

@internal
pub fn fetch_shop_subscription_capability(
  store: Store,
  upstream: UpstreamContext,
) -> Store {
  let query =
    "query DraftProxyShopSubscriptionCapability {
  shop {
    features {
      sellsSubscriptions
    }
  }
}
"
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "DraftProxyShopSubscriptionCapability",
      query,
      json.object([]),
    )
  {
    Ok(value) ->
      case shop_sells_subscriptions_from_response(value) {
        Some(sells_subscriptions) ->
          store.set_shop_sells_subscriptions(store, sells_subscriptions)
        None -> store
      }
    Error(_) -> store
  }
}

@internal
pub fn shop_sells_subscriptions_from_response(
  value: commit.JsonValue,
) -> Option(Bool) {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "shop") {
        Some(shop) ->
          case json_get(shop, "features") {
            Some(features) ->
              case json_get(features, "sellsSubscriptions") {
                Some(commit.JsonBool(value)) -> Some(value)
                _ -> None
              }
            None -> None
          }
        None -> None
      }
    None -> None
  }
}

@internal
pub fn maybe_hydrate_discount_markets_capability(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  case store.get_effective_shop(store) {
    Some(_) -> store
    None ->
      case discount_markets_fields(input) {
        Some(_) -> fetch_shop_discount_markets_capability(store, upstream)
        None -> store
      }
  }
}

@internal
pub fn fetch_shop_discount_markets_capability(
  store: Store,
  upstream: UpstreamContext,
) -> Store {
  let query =
    "query DraftProxyShopDiscountMarketsCapability {
  shop {
    features {
      sellsSubscriptions
      discountsByMarketEnabled
    }
  }
}
"
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "DraftProxyShopDiscountMarketsCapability",
      query,
      json.object([]),
    )
  {
    Ok(value) ->
      case shop_discount_markets_capability_from_response(value) {
        Some(enabled) -> {
          let store = store.set_shop_discounts_by_market_enabled(store, enabled)
          case shop_sells_subscriptions_from_response(value) {
            Some(sells_subscriptions) ->
              store.set_shop_sells_subscriptions(store, sells_subscriptions)
            None -> store
          }
        }
        None -> store
      }
    Error(_) -> store
  }
}

@internal
pub fn shop_discount_markets_capability_from_response(
  value: commit.JsonValue,
) -> Option(Bool) {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "shop") {
        Some(shop) ->
          case json_get(shop, "features") {
            Some(features) ->
              case json_get(features, "discountsByMarketEnabled") {
                Some(commit.JsonBool(value)) -> Some(value)
                _ -> None
              }
            None -> None
          }
        None -> None
      }
    None -> None
  }
}

@internal
pub fn validate_markets_fields(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  is_create: Bool,
) -> List(SourceValue) {
  case discount_markets_fields(input) {
    Some(markets) ->
      case store.shop_discounts_by_market_enabled(store) {
        False -> [
          discount_types.user_error(
            [input_name, "context", "markets"],
            "Discounts by market is not supported for this shop",
            "INVALID",
          ),
        ]
        True ->
          list.append(
            remove_on_create_market_errors(input_name, markets, is_create),
            invalid_market_id_errors(store, input_name, markets),
          )
      }
    None -> []
  }
}

@internal
pub fn discount_markets_fields(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, "context") {
    Ok(root_field.ObjectVal(context)) ->
      case dict.get(context, "markets") {
        Ok(root_field.ObjectVal(markets)) -> Some(markets)
        _ -> None
      }
    _ -> None
  }
}

fn remove_on_create_market_errors(
  input_name: String,
  markets: Dict(String, root_field.ResolvedValue),
  is_create: Bool,
) -> List(SourceValue) {
  case is_create, input_value_is_present(markets, "remove") {
    True, True -> [
      discount_types.user_error(
        [input_name, "context", "markets", "remove"],
        "Cannot specify market removal on create",
        "INVALID",
      ),
    ]
    _, _ -> []
  }
}

fn invalid_market_id_errors(
  store: Store,
  input_name: String,
  markets: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  list.append(
    market_input_ids(markets, "add"),
    market_input_ids(markets, "remove"),
  )
  |> list.filter_map(fn(id) {
    case market_id_is_valid_for_store(store, id) {
      True -> Error(Nil)
      False ->
        Ok(discount_types.user_error(
          [input_name, "context", "markets"],
          "Market with id: " <> market_error_id_label(id) <> " is invalid",
          "INVALID",
        ))
    }
  })
}

fn market_input_ids(
  markets: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(String) {
  case dict.get(markets, name) {
    Ok(root_field.ListVal(items)) ->
      list.map(items, fn(item) {
        case item {
          root_field.StringVal(value) -> value
          _ -> ""
        }
      })
    Ok(root_field.NullVal) | Error(_) -> []
    Ok(root_field.StringVal(value)) -> [value]
    Ok(_) -> [""]
  }
}

fn market_id_is_valid_for_store(store: Store, id: String) -> Bool {
  case market_gid_has_positive_numeric_tail(id) {
    False -> False
    True ->
      case store.list_effective_markets(store) {
        [] -> True
        _ ->
          case store.get_effective_market_by_id(store, id) {
            Some(_) -> True
            None -> False
          }
      }
  }
}

fn market_gid_has_positive_numeric_tail(id: String) -> Bool {
  case string.starts_with(id, "gid://shopify/Market/") {
    False -> False
    True ->
      case resource_ids.shopify_gid_tail(id) {
        Some(tail) ->
          case int.parse(tail) {
            Ok(value) -> value > 0
            Error(_) -> False
          }
        None -> False
      }
  }
}

fn market_error_id_label(id: String) -> String {
  case string.starts_with(id, "gid://shopify/Market/") {
    True -> resource_ids.shopify_gid_tail(id) |> option.unwrap(id)
    False ->
      case id {
        "" -> "unknown"
        _ -> id
      }
  }
}

@internal
pub fn validate_subscription_field_values(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
) -> List(SourceValue) {
  case store.shop_sells_subscriptions(store) {
    False ->
      subscription_fields_not_permitted_errors(input_name, input, location)
    True -> blank_subscription_field_errors(input_name, input, location)
  }
}

@internal
pub fn subscription_fields_not_permitted_errors(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
) -> List(SourceValue) {
  let errors =
    subscription_field_error(
      input_name,
      input,
      location,
      "appliesOnSubscription",
      subscription_not_permitted_message(location, "appliesOnSubscription"),
    )
  let errors =
    list.append(
      errors,
      subscription_field_error(
        input_name,
        input,
        location,
        "appliesOnOneTimePurchase",
        subscription_not_permitted_message(location, "appliesOnOneTimePurchase"),
      ),
    )
  case dict.has_key(input, "recurringCycleLimit") {
    True ->
      list.append(errors, [
        discount_types.user_error(
          [input_name, "recurringCycleLimit"],
          "Recurring cycle limit is not permitted for this shop.",
          "INVALID",
        ),
      ])
    False -> errors
  }
}

@internal
pub fn subscription_not_permitted_message(
  location: SubscriptionFieldLocation,
  field_name: String,
) -> String {
  case location, field_name {
    SubscriptionCustomerGetsFields, "appliesOnSubscription" ->
      "Customer gets applies on subscription is not permitted for this shop."
    SubscriptionCustomerGetsFields, "appliesOnOneTimePurchase" ->
      "Customer gets applies on one time purchase is not permitted for this shop."
    SubscriptionTopLevelFields, "appliesOnSubscription" ->
      "Applies on subscription is not permitted for this shop."
    SubscriptionTopLevelFields, "appliesOnOneTimePurchase" ->
      "Applies on one time purchase is not permitted for this shop."
    _, _ -> "Subscription field is not permitted for this shop."
  }
}

@internal
pub fn subscription_field_error(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
  field_name: String,
  message: String,
) -> List(SourceValue) {
  let #(fields, path) = subscription_field_source(input_name, input, location)
  case dict.has_key(fields, field_name) {
    True -> [
      discount_types.user_error(
        list.append(path, [field_name]),
        message,
        "INVALID",
      ),
    ]
    False -> []
  }
}

@internal
pub fn blank_subscription_field_errors(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
) -> List(SourceValue) {
  let errors =
    blank_subscription_field_error(
      input_name,
      input,
      location,
      "appliesOnSubscription",
      "applies_on_subscription can't be blank",
    )
  list.append(
    errors,
    blank_subscription_field_error(
      input_name,
      input,
      location,
      "appliesOnOneTimePurchase",
      "applies_on_one_time_purchase can't be blank",
    ),
  )
}

@internal
pub fn blank_subscription_field_error(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
  field_name: String,
  message: String,
) -> List(SourceValue) {
  let #(fields, path) = subscription_field_source(input_name, input, location)
  case dict.get(fields, field_name) {
    Ok(root_field.NullVal) -> [
      discount_types.user_error(
        list.append(path, [field_name]),
        message,
        "INVALID",
      ),
    ]
    _ -> []
  }
}

@internal
pub fn subscription_field_source(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  location: SubscriptionFieldLocation,
) -> #(Dict(String, root_field.ResolvedValue), List(String)) {
  case location {
    SubscriptionCustomerGetsFields ->
      case customer_gets_fields(input) {
        Some(fields) -> #(fields, [input_name, "customerGets"])
        None -> #(dict.new(), [input_name, "customerGets"])
      }
    SubscriptionTopLevelFields -> #(input, [input_name])
  }
}

@internal
pub fn validate_discount_update_input(
  input: Dict(String, root_field.ResolvedValue),
  existing_record: DiscountRecord,
) -> List(SourceValue) {
  case discount_types.read_string(input, "code") {
    Some(_) -> {
      case is_bulk_rule_discount(existing_record) {
        True -> [
          discount_types.user_error(
            ["id"],
            "Cannot update the code of a bulk discount.",
            "INVALID",
          ),
        ]
        False -> []
      }
    }
    None -> []
  }
}

@internal
pub fn is_bulk_rule_discount(record: DiscountRecord) -> Bool {
  list.length(discount_types.existing_code_nodes(record)) > 1
}

@internal
pub fn validate_discount_code_input(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  require_code: Bool,
) -> List(SourceValue) {
  case discount_types.read_string(input, "code") {
    None ->
      case require_code {
        True -> [discount_code_blank_error(input_name)]
        False -> []
      }
    Some(code) ->
      case string.trim(code) {
        "" ->
          case code {
            "" -> [
              discount_types.user_error(
                [input_name, "code"],
                "Code is too short (minimum is 1 character)",
                "TOO_SHORT",
              ),
            ]
            _ -> [discount_code_blank_error(input_name)]
          }
        _ ->
          case string.length(code) > 255 {
            True -> [
              discount_types.user_error(
                [input_name, "code"],
                "Code is too long (maximum is 255 characters)",
                "TOO_LONG",
              ),
            ]
            False ->
              case string.contains(code, "\n") || string.contains(code, "\r") {
                True -> [
                  discount_types.user_error(
                    [input_name, "code"],
                    "Code cannot contain newline characters.",
                    "INVALID",
                  ),
                ]
                False -> []
              }
          }
      }
  }
}

@internal
pub fn discount_code_blank_error(input_name: String) -> SourceValue {
  discount_types.user_error(
    [input_name, "code"],
    "Code can't be blank",
    "BLANK",
  )
}

@internal
pub fn validate_minimum_requirement(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case discount_types.read_value(input, "minimumRequirement") {
    root_field.ObjectVal(fields) -> {
      let has_quantity = has_object_field(fields, "quantity")
      let has_subtotal = has_object_field(fields, "subtotal")
      let errors = case has_quantity && has_subtotal {
        True -> [
          discount_types.user_error(
            [
              input_name,
              "minimumRequirement",
              "subtotal",
              "greaterThanOrEqualToSubtotal",
            ],
            "Minimum subtotal cannot be defined when minimum quantity is.",
            "CONFLICT",
          ),
          discount_types.user_error(
            [
              input_name,
              "minimumRequirement",
              "quantity",
              "greaterThanOrEqualToQuantity",
            ],
            "Minimum quantity cannot be defined when minimum subtotal is.",
            "CONFLICT",
          ),
        ]
        False -> []
      }
      errors
      |> list.append(validate_minimum_quantity_limit(input_name, fields))
      |> list.append(validate_minimum_subtotal_limit(input_name, fields))
    }
    _ -> []
  }
}

@internal
pub fn has_object_field(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(input, name) {
    Ok(root_field.ObjectVal(_)) -> True
    _ -> False
  }
}

@internal
pub fn validate_minimum_quantity_limit(
  input_name: String,
  fields: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case dict.get(fields, "quantity") {
    Ok(root_field.ObjectVal(quantity)) ->
      case read_numeric_string(quantity, "greaterThanOrEqualToQuantity") {
        Some(value) ->
          case decimal_at_least(value, "2147483647") {
            True -> [
              discount_types.user_error(
                [
                  input_name,
                  "minimumRequirement",
                  "quantity",
                  "greaterThanOrEqualToQuantity",
                ],
                "Minimum quantity must be less than 2147483647",
                "LESS_THAN",
              ),
            ]
            False -> []
          }
        None -> []
      }
    _ -> []
  }
}

@internal
pub fn validate_minimum_subtotal_limit(
  input_name: String,
  fields: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case dict.get(fields, "subtotal") {
    Ok(root_field.ObjectVal(subtotal)) ->
      case read_numeric_string(subtotal, "greaterThanOrEqualToSubtotal") {
        Some(value) ->
          case decimal_at_least(value, "1000000000000000000") {
            True -> [
              discount_types.user_error(
                [
                  input_name,
                  "minimumRequirement",
                  "subtotal",
                  "greaterThanOrEqualToSubtotal",
                ],
                "Minimum subtotal must be less than 1000000000000000000",
                "LESS_THAN",
              ),
            ]
            False -> []
          }
        None -> []
      }
    _ -> []
  }
}

@internal
pub fn read_numeric_string(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(input, name) {
    Ok(root_field.StringVal(value)) -> Some(value)
    Ok(root_field.IntVal(value)) -> Some(int.to_string(value))
    Ok(root_field.FloatVal(value)) -> Some(float.to_string(value))
    _ -> None
  }
}

@internal
pub fn decimal_at_least(value: String, limit: String) -> Bool {
  let value = string.trim(value)
  let value = case string.starts_with(value, "+") {
    True -> string.drop_start(value, 1)
    False -> value
  }
  case string.starts_with(value, "-") {
    True -> False
    False ->
      case string.split(value, ".") {
        [whole] -> decimal_parts_at_least(whole, "", limit)
        [whole, decimals] -> decimal_parts_at_least(whole, decimals, limit)
        _ -> False
      }
  }
}

@internal
pub fn decimal_parts_at_least(
  whole: String,
  decimals: String,
  limit: String,
) -> Bool {
  case digits_only(whole) && digits_only(decimals) {
    False -> False
    True -> {
      let whole = trim_leading_zeroes(whole)
      case int.compare(string.length(whole), string.length(limit)) {
        order.Gt -> True
        order.Lt -> False
        order.Eq ->
          case string.compare(whole, limit) {
            order.Lt -> False
            order.Eq | order.Gt -> True
          }
      }
    }
  }
}

@internal
pub fn digits_only(value: String) -> Bool {
  value
  |> string.to_graphemes
  |> list.all(fn(grapheme) {
    case grapheme {
      "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
      _ -> False
    }
  })
}

@internal
pub fn trim_leading_zeroes(value: String) -> String {
  case value {
    "0" <> rest -> trim_leading_zeroes(rest)
    "" -> "0"
    _ -> value
  }
}

/// Pattern 2: ask upstream whether a discount with the proposed code
/// already exists. Returns a `TAKEN` userError when the lookup confirms
/// a hit. Only code-discount creates carry a `code` (automatic
/// discounts never carry one), so automatics short-circuit immediately.
/// In `Snapshot` mode (no `SyncTransport` installed) this is a no-op —
/// the captured-cassette check is the only place a uniqueness signal
/// can come from when no records have been staged yet.
@internal
pub fn fetch_taken_code_error(
  input: Dict(String, root_field.ResolvedValue),
  input_name: String,
  owner_kind: String,
  upstream: UpstreamContext,
) -> Option(SourceValue) {
  case owner_kind {
    "automatic" -> None
    _ -> {
      let code =
        discount_types.read_string(input, "code")
        |> option.or(discount_types.read_string(input, "codePrefix"))
      case code {
        None -> None
        Some(code) -> {
          let query =
            "query DiscountUniquenessCheck($code: String!) {
  codeDiscountNodeByCode(code: $code) { id }
}
"
          let variables = json.object([#("code", json.string(code))])
          case
            upstream_query.fetch_sync(
              upstream.origin,
              upstream.transport,
              upstream.headers,
              "DiscountUniquenessCheck",
              query,
              variables,
            )
          {
            Ok(value) ->
              case existing_discount_id(value) {
                True ->
                  Some(discount_types.user_error(
                    [input_name, "code"],
                    "Code must be unique. Please try a different code.",
                    "TAKEN",
                  ))
                False -> None
              }
            // Snapshot mode (no transport installed) and any other
            // transport-level failure (cassette miss, malformed
            // response, HTTP error) silently fall through to the
            // local-only validation result. Cassette misses surface
            // through the runner directly when a cassette is in play.
            Error(_) -> None
          }
        }
      }
    }
  }
}

/// Pattern 2: ask upstream for the current state of a code-discount
/// (id, basic metadata, codes connection) and seed it into the local
/// `base_state` so that any subsequent staged mutation overlays on top
/// of that real shape. Used by `redeem_code_bulk_add` /
/// `redeem_code_bulk_delete` so the read-after-write `codeDiscountNode`
/// / `codeDiscountNodeByCode` queries find the discount locally and
/// project the right `codesCount`.
///
/// Returns the original `(store, identity)` when:
///  - the discount is already in the local store (nothing to do),
///  - no transport is installed (Snapshot mode / production JS without
///    cassette: cassette miss = silent no-op so the legacy local-only
///    behavior applies),
///  - the upstream response is malformed or contains a null node.
///
/// The hydrated record carries only the fields the read-after targets
/// actually project (id, codeDiscount.codes, codesCount). Other fields
/// are absent — fine because the read targets in this scenario don't
/// project them.
@internal
pub fn maybe_hydrate_discount(
  store: Store,
  identity: SyntheticIdentityRegistry,
  id: String,
  upstream: UpstreamContext,
) -> #(Store, SyntheticIdentityRegistry) {
  case store.get_effective_discount_by_id(store, id) {
    Some(_) -> #(store, identity)
    None -> {
      // The hydrate query asks for both `codeDiscountNode` and
      // `automaticDiscountNode` projections under aliases, so callers
      // that don't know whether the id refers to a code- or
      // automatic-owned discount can use a single query + cassette
      // entry. The handler picks the non-null projection. Status and
      // title are pulled in alongside codes so downstream-read targets
      // that use `discountNodesCount(query: "status:active")` /
      // `status:expired` can compute correct counts after the bulk-job
      // status-mutation effects apply on top of the hydrated base
      // record.
      let query =
        "query DiscountHydrate($id: ID!) {
  codeNode: codeDiscountNode(id: $id) {
    id
    codeDiscount {
      __typename
      ... on DiscountCodeBasic {
        title
        status
        startsAt
        endsAt
        updatedAt
        codes(first: 250) { nodes { id code } }
      }
      ... on DiscountCodeApp {
        title
        status
        startsAt
        endsAt
        updatedAt
      }
      ... on DiscountCodeBxgy {
        title
        status
        startsAt
        endsAt
        updatedAt
      }
      ... on DiscountCodeFreeShipping {
        title
        status
        startsAt
        endsAt
        updatedAt
      }
    }
  }
  automaticNode: automaticDiscountNode(id: $id) {
    id
    automaticDiscount {
      __typename
      ... on DiscountAutomaticBasic {
        title
        status
        startsAt
        endsAt
        updatedAt
      }
      ... on DiscountAutomaticApp {
        title
        status
        startsAt
        endsAt
        updatedAt
      }
      ... on DiscountAutomaticBxgy {
        title
        status
        startsAt
        endsAt
        updatedAt
      }
      ... on DiscountAutomaticFreeShipping {
        title
        status
        startsAt
        endsAt
        updatedAt
      }
    }
  }
}
"
      let variables = json.object([#("id", json.string(id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "DiscountHydrate",
          query,
          variables,
        )
      {
        Ok(value) ->
          case discount_record_from_hydrate(value, id) {
            Some(record) -> #(
              store.upsert_base_discounts(store, [record]),
              identity,
            )
            None -> #(store, identity)
          }
        Error(_) -> #(store, identity)
      }
    }
  }
}

/// Build a minimal `DiscountRecord` from a `DiscountHydrate` upstream
/// response. The record carries the codes connection so the read
/// handlers project `codesCount` and the by-code lookup correctly. The
/// rest of the discount payload is left empty — the read-after-write
/// targets in this scenario only project codes-related fields.
@internal
pub fn discount_record_from_hydrate(
  value: commit.JsonValue,
  id: String,
) -> Option(DiscountRecord) {
  case json_get(value, "data") {
    None -> None
    Some(data) -> {
      // Prefer the non-null projection. The runtime's response will have
      // exactly one of `codeNode` / `automaticNode` non-null for any
      // given id; if both are present (shouldn't happen in practice) we
      // pick code first to match the legacy lookup order.
      //
      // Older cassettes recorded the response under the unaliased
      // `codeDiscountNode` field (before the query learned to ask for
      // both code and automatic projections in one round-trip), so
      // accept that shape too as a fallback.
      let code_node =
        non_null_node(json_get(data, "codeNode"))
        |> option.or(non_null_node(json_get(data, "codeDiscountNode")))
      let automatic_node = non_null_node(json_get(data, "automaticNode"))
      case code_node, automatic_node {
        Some(node), _ -> Some(code_record_from_hydrate_node(node, id))
        None, Some(node) -> Some(automatic_record_from_hydrate_node(node, id))
        None, None -> None
      }
    }
  }
}

@internal
pub fn non_null_node(
  value: Option(commit.JsonValue),
) -> Option(commit.JsonValue) {
  case value {
    Some(commit.JsonNull) -> None
    Some(node) -> Some(node)
    None -> None
  }
}

@internal
pub fn code_record_from_hydrate_node(
  node: commit.JsonValue,
  id: String,
) -> DiscountRecord {
  let discount = json_get(node, "codeDiscount")
  let typename =
    discount
    |> option.then(fn(d) { json_get_string(d, "__typename") })
    |> option.unwrap("DiscountCodeBasic")
  let title = discount |> option.then(fn(d) { json_get_string(d, "title") })
  let status =
    discount
    |> option.then(fn(d) { json_get_string(d, "status") })
    |> option.unwrap("ACTIVE")
  let temporal_fields = discount_temporal_fields(discount)
  let codes = case discount {
    Some(d) ->
      case json_get(d, "codes") {
        Some(codes_obj) ->
          case json_get(codes_obj, "nodes") {
            Some(commit.JsonArray(items)) ->
              list.filter_map(items, json_to_code_pair)
            _ -> []
          }
        None -> []
      }
    None -> []
  }
  let first_code = case codes {
    [#(_, code), ..] -> Some(code)
    [] -> None
  }
  let payload =
    discount_types.source_to_captured(
      SrcObject(
        dict.from_list([
          #("id", SrcString(id)),
          #(
            "codeDiscount",
            SrcObject(
              dict.from_list(list.append(
                [
                  #("__typename", SrcString(typename)),
                  #(
                    "title",
                    title |> option.map(SrcString) |> option.unwrap(SrcNull),
                  ),
                  #("status", SrcString(status)),
                  #(
                    "codes",
                    SrcObject(
                      dict.from_list([
                        #(
                          "nodes",
                          SrcList(
                            list.map(codes, fn(pair) {
                              let #(code_id, code) = pair
                              SrcObject(
                                dict.from_list([
                                  #("id", SrcString(code_id)),
                                  #("code", SrcString(code)),
                                  #("asyncUsageCount", SrcInt(0)),
                                ]),
                              )
                            }),
                          ),
                        ),
                        #("edges", SrcList([])),
                        #(
                          "pageInfo",
                          SrcObject(
                            dict.from_list([
                              #("hasNextPage", SrcBool(False)),
                              #("hasPreviousPage", SrcBool(False)),
                              #("startCursor", SrcNull),
                              #("endCursor", SrcNull),
                            ]),
                          ),
                        ),
                      ]),
                    ),
                  ),
                  #(
                    "codesCount",
                    discount_types.count_source(list.length(codes)),
                  ),
                ],
                temporal_fields,
              )),
            ),
          ),
        ]),
      ),
    )
  DiscountRecord(
    id: id,
    owner_kind: "code",
    discount_type: discount_type_from_typename(typename),
    title: title,
    status: status,
    code: first_code,
    payload: payload,
    cursor: None,
  )
}

@internal
pub fn automatic_record_from_hydrate_node(
  node: commit.JsonValue,
  id: String,
) -> DiscountRecord {
  let discount = json_get(node, "automaticDiscount")
  let typename =
    discount
    |> option.then(fn(d) { json_get_string(d, "__typename") })
    |> option.unwrap("DiscountAutomaticBasic")
  let title = discount |> option.then(fn(d) { json_get_string(d, "title") })
  let status =
    discount
    |> option.then(fn(d) { json_get_string(d, "status") })
    |> option.unwrap("ACTIVE")
  let temporal_fields = discount_temporal_fields(discount)
  let payload =
    discount_types.source_to_captured(
      SrcObject(
        dict.from_list([
          #("id", SrcString(id)),
          #(
            "automaticDiscount",
            SrcObject(
              dict.from_list(list.append(
                [
                  #("__typename", SrcString(typename)),
                  #(
                    "title",
                    title |> option.map(SrcString) |> option.unwrap(SrcNull),
                  ),
                  #("status", SrcString(status)),
                ],
                temporal_fields,
              )),
            ),
          ),
        ]),
      ),
    )
  DiscountRecord(
    id: id,
    owner_kind: "automatic",
    discount_type: discount_type_from_typename(typename),
    title: title,
    status: status,
    code: None,
    payload: payload,
    cursor: None,
  )
}

fn discount_temporal_fields(
  discount: Option(commit.JsonValue),
) -> List(#(String, SourceValue)) {
  case discount {
    None -> []
    Some(discount) ->
      list.append(
        optional_source_field(discount, "startsAt"),
        list.append(
          optional_source_field(discount, "endsAt"),
          optional_source_field(discount, "updatedAt"),
        ),
      )
  }
}

fn optional_source_field(
  object: commit.JsonValue,
  key: String,
) -> List(#(String, SourceValue)) {
  case json_get(object, key) {
    Some(commit.JsonString(value)) -> [#(key, SrcString(value))]
    Some(commit.JsonNull) -> [#(key, SrcNull)]
    _ -> []
  }
}

@internal
pub fn discount_type_from_typename(typename: String) -> String {
  case typename {
    "DiscountCodeBxgy" | "DiscountAutomaticBxgy" -> "bxgy"
    "DiscountCodeFreeShipping" | "DiscountAutomaticFreeShipping" ->
      "free_shipping"
    "DiscountCodeApp" | "DiscountAutomaticApp" -> "app"
    _ -> "basic"
  }
}

@internal
pub fn json_to_code_pair(
  value: commit.JsonValue,
) -> Result(#(String, String), Nil) {
  case json_get(value, "id"), json_get(value, "code") {
    Some(commit.JsonString(id)), Some(commit.JsonString(code)) ->
      Ok(#(id, code))
    _, _ -> Error(Nil)
  }
}

/// Pattern 2: hydrate a `ShopifyFunctionRecord` from upstream when the
/// caller supplies exactly one app-discount `functionHandle`/`functionId`
/// and the local store does not already know about that function. Used at
/// app-discount-create time so validation can distinguish an unknown
/// function from a known non-discount Function and so `appDiscountType.appKey`
/// / `title` / `description` project the real function metadata instead of
/// falling back to the discount input title.
///
/// Cassette miss / Snapshot mode / malformed response is silently
/// tolerated — the existing local-only behavior takes over (input title
/// fallback, null app key/description). Returns the original `store`
/// when the function is already known or the upstream call failed.
@internal
pub fn maybe_hydrate_shopify_function(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  let function_id = discount_types.read_string(input, "functionId")
  let function_handle = discount_types.read_string(input, "functionHandle")
  case function_id, function_handle {
    None, None -> store
    Some(_), Some(_) -> store
    Some(reference), None | None, Some(reference) ->
      case discount_types.find_shopify_function(store, reference) {
        Some(_) -> store
        None -> {
          let query =
            "query ShopifyFunctionByHandle($handle: String!) {
  shopifyFunctions(first: 1, handle: $handle) {
    nodes {
      id
      title
      handle
      apiType
      description
      appKey
      app {
        id
        title
        handle
        apiKey
      }
    }
  }
}
"
          let variables = json.object([#("handle", json.string(reference))])
          case
            upstream_query.fetch_sync(
              upstream.origin,
              upstream.transport,
              upstream.headers,
              "ShopifyFunctionByHandle",
              query,
              variables,
            )
          {
            Ok(value) ->
              case shopify_function_record_from_response(value) {
                Some(record) -> {
                  let #(_, next_store) =
                    store.upsert_staged_shopify_function(store, record)
                  next_store
                }
                None -> store
              }
            Error(_) -> store
          }
        }
      }
  }
}

/// Pull the first `shopifyFunctions.nodes[0]` entry off a
/// `ShopifyFunctionByHandle` upstream response and lift it into a
/// `ShopifyFunctionRecord`. Returns `None` for any shape divergence so
/// the caller falls back to the local-only behavior.
@internal
pub fn shopify_function_record_from_response(
  value: commit.JsonValue,
) -> Option(ShopifyFunctionRecord) {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "shopifyFunctions") {
        Some(connection) ->
          case json_get(connection, "nodes") {
            Some(commit.JsonArray([first, ..])) ->
              shopify_function_record_from_node(first)
            _ -> None
          }
        None -> None
      }
    None -> None
  }
}

@internal
pub fn shopify_function_record_from_node(
  value: commit.JsonValue,
) -> Option(ShopifyFunctionRecord) {
  case json_get(value, "id") {
    Some(commit.JsonString(id)) ->
      Some(ShopifyFunctionRecord(
        id: id,
        title: json_get_string(value, "title"),
        handle: json_get_string(value, "handle"),
        api_type: json_get_string(value, "apiType"),
        description: json_get_string(value, "description"),
        app_key: json_get_string(value, "appKey"),
        app: shopify_function_app_record_from_node(json_get(value, "app")),
      ))
    _ -> None
  }
}

@internal
pub fn shopify_function_app_record_from_node(
  value: Option(commit.JsonValue),
) -> Option(ShopifyFunctionAppRecord) {
  case value {
    Some(node) ->
      Some(ShopifyFunctionAppRecord(
        typename: json_get_string(node, "__typename"),
        id: json_get_string(node, "id"),
        title: json_get_string(node, "title"),
        handle: json_get_string(node, "handle"),
        api_key: json_get_string(node, "apiKey"),
      ))
    None -> None
  }
}

@internal
pub fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

/// Read `data.codeDiscountNodeByCode.id` from the upstream response
/// AST. Treats anything other than a non-null string id as "no such
/// discount." Walks `commit.JsonValue` so we don't have to round-trip
/// through serialized JSON.
@internal
pub fn existing_discount_id(value: commit.JsonValue) -> Bool {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "codeDiscountNodeByCode") {
        Some(node) ->
          case json_get(node, "id") {
            Some(commit.JsonString(_)) -> True
            _ -> False
          }
        None -> False
      }
    None -> False
  }
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
          #(k, v) if k == key -> Ok(v)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

@internal
pub fn append_blank_title_error(
  errors: List(SourceValue),
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case input_name == "freeShippingCodeDiscount" && title_is_blank(input) {
    True ->
      list.append(errors, [
        discount_types.user_error(
          [input_name, "title"],
          "Title can't be blank",
          "BLANK",
        ),
      ])
    False -> errors
  }
}

@internal
pub fn title_is_blank(input: Dict(String, root_field.ResolvedValue)) -> Bool {
  case discount_types.read_string(input, "title") {
    Some(title) -> string.trim(title) == ""
    None -> False
  }
}

@internal
pub fn validate_bxgy_input(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  let errors = case
    nested_has_all(discount_types.read_value(input, "customerGets"), "items")
  {
    True -> [
      discount_types.user_error(
        [input_name, "customerGets"],
        "Items in 'customer get' cannot be set to all",
        "INVALID",
      ),
    ]
    False -> []
  }
  let errors =
    list.append(errors, bxgy_disallowed_value_errors(input_name, input))
  let errors =
    list.append(
      errors,
      bxgy_missing_discount_on_quantity_errors(input_name, input),
    )
  let errors =
    list.append(errors, bxgy_disallowed_subscription_errors(input_name, input))
  let errors = case title_is_blank(input) {
    True ->
      list.append(errors, [
        discount_types.user_error(
          [input_name, "title"],
          "Title can't be blank",
          "BLANK",
        ),
      ])
    False -> errors
  }
  case
    nested_has_all(discount_types.read_value(input, "customerBuys"), "items")
  {
    True ->
      list.append(errors, [
        discount_types.user_error(
          [input_name, "customerBuys", "items"],
          "Items in 'customer buys' must be defined",
          "BLANK",
        ),
      ])
    False -> errors
  }
}

@internal
pub fn bxgy_disallowed_value_errors(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case customer_gets_value_fields(input) {
    Some(fields) -> {
      let errors = case dict.has_key(fields, "percentage") {
        True -> [
          discount_types.user_error(
            [input_name, "customerGets", "value", "percentage"],
            "Only discountOnQuantity permitted with bxgy discounts.",
            "INVALID",
          ),
        ]
        False -> []
      }
      case dict.has_key(fields, "discountAmount") {
        True ->
          list.append(errors, [
            discount_types.user_error(
              [input_name, "customerGets", "value", "discountAmount"],
              "Only discountOnQuantity permitted with bxgy discounts.",
              "INVALID",
            ),
          ])
        False -> errors
      }
    }
    None -> []
  }
}

@internal
pub fn basic_disallowed_discount_on_quantity_errors(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case customer_gets_value_fields(input) {
    Some(fields) ->
      case dict.has_key(fields, "discountOnQuantity") {
        True -> [
          discount_types.user_error(
            [input_name, "customerGets", "value", "discountOnQuantity"],
            "discountOnQuantity field is only permitted with bxgy discounts.",
            "INVALID",
          ),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn bxgy_missing_discount_on_quantity_errors(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case input_name, customer_gets_value_fields(input) {
    "bxgyCodeDiscount", Some(fields) ->
      case dict.get(fields, "discountOnQuantity") {
        Ok(root_field.ObjectVal(on_quantity)) ->
          case discount_types.read_string(on_quantity, "quantity") {
            Some(quantity) ->
              case string.trim(quantity) {
                "" -> [
                  bxgy_discount_on_quantity_quantity_blank_error(input_name),
                ]
                _ -> []
              }
            None -> [bxgy_discount_on_quantity_quantity_blank_error(input_name)]
          }
        Ok(_) -> [bxgy_discount_on_quantity_quantity_blank_error(input_name)]
        Error(_) -> [bxgy_discount_on_quantity_quantity_blank_error(input_name)]
      }
    _, _ -> []
  }
}

@internal
pub fn bxgy_discount_on_quantity_quantity_blank_error(
  input_name: String,
) -> SourceValue {
  discount_types.user_error(
    [input_name, "customerGets", "value", "discountOnQuantity", "quantity"],
    "Quantity cannot be blank.",
    "BLANK",
  )
}

@internal
pub fn bxgy_disallowed_subscription_errors(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case customer_gets_fields(input) {
    Some(fields) -> {
      let message = case input_name {
        "automaticBxgyDiscount" ->
          "This field is not supported by automatic bxgy discounts."
        _ -> "This field is not supported by bxgy discounts."
      }
      let errors = case dict.has_key(fields, "appliesOnSubscription") {
        True -> [
          discount_types.user_error(
            [input_name, "customerGets", "appliesOnSubscription"],
            message,
            "INVALID",
          ),
        ]
        False -> []
      }
      case dict.has_key(fields, "appliesOnOneTimePurchase") {
        True ->
          list.append(errors, [
            discount_types.user_error(
              [input_name, "customerGets", "appliesOnOneTimePurchase"],
              message,
              "INVALID",
            ),
          ])
        False -> errors
      }
    }
    None -> []
  }
}

@internal
pub fn customer_gets_value_fields(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case customer_gets_fields(input) {
    Some(fields) ->
      case dict.get(fields, "value") {
        Ok(root_field.ObjectVal(value_fields)) -> Some(value_fields)
        _ -> None
      }
    None -> None
  }
}

@internal
pub fn customer_gets_fields(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case discount_types.read_value(input, "customerGets") {
    root_field.ObjectVal(fields) -> Some(fields)
    _ -> None
  }
}

@internal
pub fn customer_gets_items_fields(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case customer_gets_fields(input) {
    Some(gets) ->
      case discount_types.read_value(gets, "items") {
        root_field.ObjectVal(items) -> Some(items)
        _ -> None
      }
    None -> None
  }
}

@internal
pub fn nested_has_all(value: root_field.ResolvedValue, child: String) -> Bool {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, child) {
        Ok(root_field.ObjectVal(child_fields)) ->
          dict.has_key(child_fields, "all")
        _ -> False
      }
    _ -> False
  }
}

@internal
pub fn validate_discount_top_level_errors(
  input: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
) -> List(Json) {
  let errors =
    validate_customer_gets_value_type_top_level_errors(input, field, document)
  let errors =
    list.append(
      errors,
      validate_customer_selection_top_level_errors(input, field, document),
    )
  list.append(
    errors,
    validate_cart_line_combination_tag_top_level_errors(input, field, document),
  )
}

@internal
pub fn validate_customer_gets_value_type_top_level_errors(
  input: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
) -> List(Json) {
  case customer_gets_value_fields(input) {
    Some(fields) ->
      case customer_gets_value_type_count(fields) > 1 {
        True -> [
          json.object([
            #(
              "message",
              json.string(
                "A discount can only have one of percentage, discountOnQuantity or discountAmount.",
              ),
            ),
            #("locations", field_locations_json(field, document)),
            #(
              "extensions",
              json.object([#("code", json.string("BAD_REQUEST"))]),
            ),
            #("path", json.array([get_field_response_key(field)], json.string)),
          ]),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn customer_gets_value_type_count(
  fields: Dict(String, root_field.ResolvedValue),
) -> Int {
  let count = case dict.has_key(fields, "percentage") {
    True -> 1
    False -> 0
  }
  let count = case dict.has_key(fields, "discountAmount") {
    True -> count + 1
    False -> count
  }
  case dict.has_key(fields, "discountOnQuantity") {
    True -> count + 1
    False -> count
  }
}

@internal
pub fn validate_customer_selection_top_level_errors(
  input: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
) -> List(Json) {
  case customer_selection_fields(input) {
    Some(fields) ->
      case customer_selection_all_is_true(fields) {
        True ->
          case
            customer_selection_has_customers(fields)
            || customer_selection_has_customer_saved_searches(fields)
          {
            True -> [
              discount_bad_request_error(
                field,
                document,
                "A discount cannot have customerSelection set to all, when customers or customerSavedSearches is specified.",
              ),
            ]
            False ->
              case customer_selection_has_customer_segments(fields) {
                True -> [
                  discount_bad_request_error(
                    field,
                    document,
                    "A discount cannot have customerSelection set to all, when customerSegments is specified.",
                  ),
                ]
                False -> []
              }
          }
        False -> []
      }
    None -> []
  }
}

@internal
pub fn customer_selection_fields(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case discount_types.read_value(input, "customerSelection") {
    root_field.ObjectVal(fields) -> Some(fields)
    _ -> None
  }
}

@internal
pub fn customer_selection_all_is_true(
  fields: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case dict.get(fields, "all") {
    Ok(root_field.BoolVal(True)) -> True
    _ -> False
  }
}

@internal
pub fn customer_selection_has_customers(
  fields: Dict(String, root_field.ResolvedValue),
) -> Bool {
  input_value_is_present(fields, "customers")
}

@internal
pub fn customer_selection_has_customer_saved_searches(
  fields: Dict(String, root_field.ResolvedValue),
) -> Bool {
  input_value_is_present(fields, "customerSavedSearches")
}

@internal
pub fn customer_selection_has_customer_segments(
  fields: Dict(String, root_field.ResolvedValue),
) -> Bool {
  input_value_is_present(fields, "customerSegments")
}

@internal
pub fn discount_bad_request_error(
  field: Selection,
  document: String,
  message: String,
) -> Json {
  json.object([
    #("message", json.string(message)),
    #("locations", field_locations_json(field, document)),
    #("extensions", json.object([#("code", json.string("BAD_REQUEST"))])),
    #("path", json.array([get_field_response_key(field)], json.string)),
  ])
}

@internal
pub fn validate_cart_line_combination_tag_top_level_errors(
  input: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
) -> List(Json) {
  case product_discounts_with_tags_settings(input) {
    Some(settings) ->
      case tag_add_remove_overlap(settings) {
        True -> [
          json.object([
            #(
              "message",
              json.string(
                "The same tag is present in both `add` and `remove` fields of `productDiscountsWithTagsOnSameCartLine`.",
              ),
            ),
            #("locations", field_locations_json(field, document)),
            #(
              "extensions",
              json.object([#("code", json.string("BAD_REQUEST"))]),
            ),
            #("path", json.array([get_field_response_key(field)], json.string)),
          ]),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn validate_cart_line_combination_tag_settings(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_classes: List(String),
) -> List(SourceValue) {
  case product_discounts_with_tags_settings(input) {
    Some(_) -> {
      let path = [
        input_name,
        "combinesWith",
        "productDiscountsWithTagsOnSameCartLine",
      ]
      let errors = [
        discount_types.user_error(
          path,
          "The shop's plan does not allow setting `productDiscountsWithTagsOnSameCartLine`.",
          "PRODUCT_DISCOUNTS_WITH_TAGS_ON_SAME_CART_LINE_NOT_ENTITLED",
        ),
      ]
      case list.contains(discount_classes, "PRODUCT") {
        True -> errors
        False ->
          list.append(errors, [
            discount_types.user_error(
              path,
              "Combines with product discounts with tags on same cart line is only valid for discounts with the PRODUCT discount class",
              "INVALID_PRODUCT_DISCOUNTS_WITH_TAGS_ON_SAME_CART_LINE_FOR_DISCOUNT_CLASS",
            ),
          ])
      }
    }
    None -> []
  }
}

@internal
pub fn product_discounts_with_tags_settings(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case discount_types.read_value(input, "combinesWith") {
    root_field.ObjectVal(combines) ->
      case
        discount_types.read_value(
          combines,
          "productDiscountsWithTagsOnSameCartLine",
        )
      {
        root_field.ObjectVal(settings) -> Some(settings)
        _ -> None
      }
    _ -> None
  }
}

@internal
pub fn tag_add_remove_overlap(
  settings: Dict(String, root_field.ResolvedValue),
) -> Bool {
  let add_tags = discount_types.read_string_array(settings, "add", [])
  let remove_tags = discount_types.read_string_array(settings, "remove", [])
  list.any(remove_tags, fn(tag) { list.contains(add_tags, tag) })
}

@internal
pub fn invalid_free_shipping_combines(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case discount_types.read_value(input, "combinesWith") {
    root_field.ObjectVal(fields) -> bool_value(fields, "shippingDiscounts")
    _ -> False
  }
}

@internal
pub fn bool_value(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(input, name) {
    Ok(root_field.BoolVal(value)) -> value
    _ -> False
  }
}

@internal
pub fn invalid_date_range(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case
    discount_types.read_string(input, "startsAt"),
    discount_types.read_string(input, "endsAt")
  {
    Some(starts_at), Some(ends_at) ->
      case
        iso_timestamp.parse_iso(starts_at),
        iso_timestamp.parse_iso(ends_at)
      {
        Ok(starts_at_ms), Ok(ends_at_ms) -> ends_at_ms <= starts_at_ms
        _, _ -> False
      }
    _, _ -> False
  }
}

@internal
pub fn validate_discount_item_refs(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
) -> List(SourceValue) {
  let gets_errors =
    validate_discount_item_ref_root(store, input_name, input, "customerGets")
  let buys_errors = case discount_type {
    "bxgy" ->
      validate_discount_item_ref_root(store, input_name, input, "customerBuys")
    _ -> []
  }
  list.append(gets_errors, buys_errors)
}

@internal
pub fn validate_discount_item_ref_root(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  item_root: String,
) -> List(SourceValue) {
  case discount_types.read_value(input, item_root) {
    root_field.ObjectVal(fields) ->
      case discount_types.read_value(fields, "items") {
        root_field.ObjectVal(items) ->
          validate_discount_items_refs(store, input_name, item_root, items)
        _ -> []
      }
    _ -> []
  }
}

@internal
pub fn validate_discount_items_refs(
  store: Store,
  input_name: String,
  item_root: String,
  items: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  let has_products = dict.has_key(items, "products")
  let has_collections = dict.has_key(items, "collections")
  let errors = case has_products && has_collections {
    True -> [
      discount_types.user_error(
        [input_name, item_root, "items", "collections", "add"],
        "Cannot entitle collections in combination with product variants or products",
        "CONFLICT",
      ),
    ]
    False -> []
  }
  let errors = case dict.get(items, "products") {
    Ok(root_field.ObjectVal(products)) ->
      errors
      |> list.append(invalid_id_errors(
        store,
        products,
        "productsToAdd",
        "Product",
        [input_name, item_root, "items", "products", "productsToAdd"],
        "product",
      ))
      |> list.append(invalid_id_errors(
        store,
        products,
        "productVariantsToAdd",
        "Product variant",
        [input_name, item_root, "items", "products", "productVariantsToAdd"],
        "variant",
      ))
    _ -> errors
  }
  case has_products && has_collections {
    True -> errors
    False ->
      errors
      |> list.append(case dict.get(items, "collections") {
        Ok(root_field.ObjectVal(collections)) ->
          invalid_id_errors(
            store,
            collections,
            "add",
            "Collection",
            [input_name, item_root, "items", "collections", "add"],
            "collection",
          )
        _ -> []
      })
  }
}

@internal
pub fn invalid_id_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  field: String,
  label: String,
  path: List(String),
  resource_type: String,
) -> List(SourceValue) {
  discount_types.read_string_array(input, field, [])
  |> list.filter_map(fn(id) {
    case invalid_discount_item_ref(store, resource_type, id) {
      Some(tail) ->
        Ok(discount_types.user_error(
          path,
          label <> " with id: " <> tail <> " is invalid",
          "INVALID",
        ))
      None -> Error(Nil)
    }
  })
}

@internal
pub fn invalid_discount_item_ref(
  store: Store,
  resource_type: String,
  id: String,
) -> Option(String) {
  let tail = resource_ids.shopify_gid_tail(id) |> option.unwrap(id)
  case tail == "0" {
    True -> Some(tail)
    False ->
      case reference_exists_or_store_is_cold(store, resource_type, id) {
        True -> None
        False -> Some(tail)
      }
  }
}

@internal
pub fn reference_exists_or_store_is_cold(
  store: Store,
  resource_type: String,
  id: String,
) -> Bool {
  case resource_type {
    "product" -> {
      let known_count = list.length(store.list_effective_products(store))
      known_count == 0 || store.get_effective_product_by_id(store, id) != None
    }
    "variant" -> {
      let known_count =
        list.length(store.list_effective_product_variants(store))
      known_count == 0 || store.get_effective_variant_by_id(store, id) != None
    }
    "collection" -> {
      let known_count = list.length(store.list_effective_collections(store))
      known_count == 0
      || store.get_effective_collection_by_id(store, id) != None
    }
    _ -> True
  }
}

@internal
pub fn validate_bulk_selector(
  store: Store,
  root: String,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  let count =
    selector_present(args, "ids")
    + selector_present(args, "search")
    + selector_present(args, "savedSearchId")
    + selector_present(args, "saved_search_id")
  case count {
    0 -> [
      discount_types.user_error_null_field(
        bulk_missing_selector_message(root),
        "MISSING_ARGUMENT",
      ),
    ]
    n if n > 1 -> [
      discount_types.user_error_null_field(
        bulk_too_many_selector_message(root),
        "TOO_MANY_ARGUMENTS",
      ),
    ]
    _ ->
      list.append(
        validate_bulk_search_selector(root, args),
        validate_bulk_saved_search_selector(store, root, args),
      )
  }
}

@internal
pub fn validate_redeem_code_bulk_delete_selector_shape(
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  let count =
    redeem_code_ids_selector_present(args)
    + selector_present(args, "search")
    + selector_present(args, "savedSearchId")
    + selector_present(args, "saved_search_id")
  case count {
    0 -> [
      discount_types.user_error_null_field(
        "Missing expected argument key: 'ids', 'search' or 'saved_search_id'.",
        "MISSING_ARGUMENT",
      ),
    ]
    n if n > 1 -> [
      discount_types.user_error_null_field(
        "Only one of 'ids', 'search' or 'saved_search_id' is allowed.",
        "TOO_MANY_ARGUMENTS",
      ),
    ]
    _ -> []
  }
}

@internal
pub fn validate_redeem_code_bulk_delete_after_hydrate(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case discount_types.read_string(args, "discountId") {
    Some(id) ->
      case store.get_effective_discount_by_id(store, id) {
        None -> [
          discount_types.user_error(
            ["discountId"],
            "Code discount does not exist.",
            "INVALID",
          ),
        ]
        Some(_) ->
          case redeem_code_ids_selector_is_empty(args) {
            True -> [
              discount_types.user_error_null_field_with_code(
                "Something went wrong, please try again.",
                None,
              ),
            ]
            False ->
              list.append(
                validate_redeem_code_bulk_delete_search_selector(args),
                validate_redeem_code_bulk_delete_saved_search_selector(
                  store,
                  args,
                ),
              )
          }
      }
    None -> [
      discount_types.user_error(
        ["discountId"],
        "Code discount does not exist.",
        "INVALID",
      ),
    ]
  }
}

@internal
pub fn validate_redeem_code_bulk_delete_search_selector(
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case discount_types.read_string(args, "search") {
    Some(search) ->
      case string.trim(search) {
        "" -> [
          discount_types.user_error(
            ["search"],
            "'Search' can't be blank.",
            "BLANK",
          ),
        ]
        _ -> []
      }
    _ -> []
  }
}

@internal
pub fn validate_redeem_code_bulk_delete_saved_search_selector(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case read_bulk_saved_search_id(args) {
    Some(id) ->
      case store.get_effective_saved_search_by_id(store, id) {
        Some(_) -> []
        None -> [
          discount_types.user_error(
            ["savedSearchId"],
            "Invalid 'saved_search_id'.",
            "INVALID",
          ),
        ]
      }
    None -> []
  }
}

@internal
pub fn redeem_code_bulk_delete_target_ids(
  store: Store,
  record: DiscountRecord,
  args: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  case dict.has_key(args, "ids") {
    True -> discount_types.read_string_array(args, "ids", [])
    False ->
      case discount_types.read_string(args, "search") {
        Some(query) -> redeem_code_ids_matching_query(record, query)
        None ->
          case read_bulk_saved_search_id(args) {
            Some(id) ->
              case store.get_effective_saved_search_by_id(store, id) {
                Some(saved_search) ->
                  redeem_code_ids_matching_query(record, saved_search.query)
                None -> []
              }
            None -> []
          }
      }
  }
}

@internal
pub fn redeem_code_ids_matching_query(
  record: DiscountRecord,
  query: String,
) -> List(String) {
  discount_types.existing_code_nodes(record)
  |> search_query_parser.apply_search_query(
    Some(query),
    search_query_parser.default_parse_options(),
    redeem_code_matches_positive_search_term,
  )
  |> list.map(fn(pair) { pair.0 })
}

@internal
pub fn redeem_code_matches_positive_search_term(
  pair: #(String, String),
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  let #(_id, code) = pair
  case term.field {
    Some("code") ->
      search_query_parser.matches_search_query_string(
        Some(code),
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        search_query_parser.default_string_match_options(),
      )
    _ -> search_query_parser.matches_search_query_text(Some(code), term)
  }
}

@internal
pub fn bulk_missing_selector_message(root: String) -> String {
  case root {
    "discountAutomaticBulkDelete" ->
      "One of IDs, search argument or saved search ID is required."
    _ -> "Missing expected argument key: 'ids', 'search' or 'saved_search_id'."
  }
}

@internal
pub fn bulk_too_many_selector_message(root: String) -> String {
  case root {
    "discountAutomaticBulkDelete" ->
      "Only one of IDs, search argument or saved search ID is allowed."
    _ -> "Only one of 'ids', 'search' or 'saved_search_id' is allowed."
  }
}

@internal
pub fn validate_bulk_search_selector(
  root: String,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case discount_types.read_string(args, "search") {
    Some(search) -> {
      case string.trim(search) {
        "" ->
          case root {
            "discountAutomaticBulkDelete" -> []
            _ -> [
              discount_types.user_error(
                ["search"],
                "'Search' can't be blank.",
                "BLANK",
              ),
            ]
          }
        _ -> []
      }
    }
    _ -> []
  }
}

@internal
pub fn validate_bulk_saved_search_selector(
  store: Store,
  root: String,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case read_bulk_saved_search_id(args) {
    Some(id) ->
      case store.get_effective_saved_search_by_id(store, id) {
        Some(record) if record.resource_type == "PRICE_RULE" -> []
        _ -> [
          discount_types.user_error(
            ["savedSearchId"],
            bulk_invalid_saved_search_message(root),
            "INVALID",
          ),
        ]
      }
    None -> []
  }
}

@internal
pub fn bulk_invalid_saved_search_message(root: String) -> String {
  case root {
    "discountAutomaticBulkDelete" -> "Invalid savedSearchId."
    _ -> "Invalid 'saved_search_id'."
  }
}

@internal
pub fn read_bulk_saved_search_id(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  discount_types.read_string(args, "savedSearchId")
  |> option.or(discount_types.read_string(args, "saved_search_id"))
}

@internal
pub fn selector_present(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Int {
  case dict.get(args, name) {
    Ok(root_field.NullVal) | Error(_) -> 0
    Ok(root_field.ListVal([])) -> 0
    _ -> 1
  }
}

@internal
pub fn redeem_code_ids_selector_present(
  args: Dict(String, root_field.ResolvedValue),
) -> Int {
  case dict.has_key(args, "ids") {
    True -> 1
    False -> 0
  }
}

@internal
pub fn redeem_code_ids_selector_is_empty(
  args: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case dict.get(args, "ids") {
    Ok(root_field.NullVal) | Ok(root_field.ListVal([])) -> True
    _ -> False
  }
}

@internal
pub fn apply_bulk_effects(
  store: Store,
  root: String,
  args: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(Store, SyntheticIdentityRegistry) {
  let ids = discount_types.read_string_array(args, "ids", [])
  list.fold(ids, #(store, identity), fn(acc, id) {
    let #(current, current_identity) = acc
    case root {
      "discountCodeBulkDelete" | "discountAutomaticBulkDelete" ->
        case store.get_effective_discount_by_id(current, id) {
          Some(_) -> {
            let #(_, next_identity) =
              synthetic_identity.make_synthetic_timestamp(current_identity)
            #(store.delete_staged_discount(current, id), next_identity)
          }
          None -> #(store.delete_staged_discount(current, id), current_identity)
        }
      "discountCodeBulkActivate" ->
        set_record_status(current, current_identity, id, "ACTIVE")
      "discountCodeBulkDeactivate" ->
        set_record_status(current, current_identity, id, "EXPIRED")
      _ -> #(current, current_identity)
    }
  })
}

@internal
pub fn set_record_status(
  store: Store,
  identity: SyntheticIdentityRegistry,
  id: String,
  status: String,
) -> #(Store, SyntheticIdentityRegistry) {
  case store.get_effective_discount_by_id(store, id) {
    Some(record) -> {
      let #(updated_at, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let #(record, next_store) =
        store.stage_discount(
          store,
          DiscountRecord(
            ..record,
            status: status,
            payload: discount_types.update_payload_status(
                record.payload,
                status,
                None,
              )
              |> discount_types.update_payload_updated_at(updated_at),
          ),
        )
      let _ = record
      #(next_store, next_identity)
    }
    None -> #(store, identity)
  }
}
