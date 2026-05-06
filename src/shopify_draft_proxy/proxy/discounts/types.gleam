//// Shared internal Discounts scalar/source helpers and record update utilities.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SelectedFieldOptions, SrcBool, SrcFloat,
  SrcInt, SrcList, SrcNull, SrcObject, SrcString, field_locations_json,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  project_graphql_value,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, type RequiredArgument, MutationOutcome, RequiredArgument,
  single_root_log_draft, validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type DiscountBulkOperationRecord, type DiscountRecord,
  type ShopifyFunctionAppRecord, type ShopifyFunctionRecord, CapturedArray,
  CapturedBool, CapturedFloat, CapturedInt, CapturedNull, CapturedObject,
  CapturedString, DiscountBulkOperationRecord, DiscountRecord,
  ShopifyFunctionAppRecord, ShopifyFunctionRecord,
}

@internal
pub fn discount_owner_source(record: DiscountRecord) -> SourceValue {
  captured_to_source(record.payload)
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
pub fn primary_discount_class(classes: List(String)) -> String {
  case classes {
    [first, ..] -> first
    [] -> "ORDER"
  }
}

@internal
pub fn discount_class_for_record(record: DiscountRecord) -> String {
  case captured_to_source(record.payload) {
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
pub fn bxgy_summary(input: Dict(String, root_field.ResolvedValue)) -> String {
  let buys_quantity =
    read_bxgy_quantity(read_value(input, "customerBuys")) |> option.unwrap("1")
  let gets_value = read_value(input, "customerGets")
  let gets_quantity =
    read_discount_on_quantity(gets_value, "quantity")
    |> option.unwrap("1")
  let effect = read_discount_on_quantity_percentage(gets_value)
  let suffix = case effect {
    Some(percentage) ->
      case percentage {
        1.0 -> " free"
        _ -> " at " <> percentage_to_label(percentage) <> " off"
      }
    None -> ""
  }
  "Buy "
  <> buys_quantity
  <> " "
  <> plural_item(buys_quantity)
  <> ", get "
  <> gets_quantity
  <> " "
  <> plural_item(gets_quantity)
  <> suffix
}

@internal
pub fn read_bxgy_quantity(value: root_field.ResolvedValue) -> Option(String) {
  case value {
    root_field.ObjectVal(fields) ->
      case read_value(fields, "value") {
        root_field.ObjectVal(value_fields) ->
          resolved_string(read_value(value_fields, "quantity"))
        _ -> None
      }
    _ -> None
  }
}

@internal
pub fn read_discount_on_quantity(
  value: root_field.ResolvedValue,
  key: String,
) -> Option(String) {
  case discount_on_quantity_fields(value) {
    Some(fields) -> resolved_string(read_value(fields, key))
    None -> None
  }
}

@internal
pub fn read_discount_on_quantity_percentage(
  value: root_field.ResolvedValue,
) -> Option(Float) {
  case discount_on_quantity_fields(value) {
    Some(fields) ->
      case read_value(fields, "effect") {
        root_field.ObjectVal(effect) ->
          case read_value(effect, "percentage") {
            root_field.FloatVal(value) -> Some(value)
            root_field.IntVal(value) -> Some(int.to_float(value))
            _ -> None
          }
        _ -> None
      }
    None -> None
  }
}

@internal
pub fn discount_on_quantity_fields(
  value: root_field.ResolvedValue,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case value {
    root_field.ObjectVal(fields) ->
      case read_value(fields, "value") {
        root_field.ObjectVal(value_fields) ->
          case dict.get(value_fields, "discountOnQuantity") {
            Ok(root_field.ObjectVal(discount_on_quantity)) ->
              Some(discount_on_quantity)
            _ ->
              case dict.get(value_fields, "onQuantity") {
                Ok(root_field.ObjectVal(on_quantity)) -> Some(on_quantity)
                _ -> None
              }
          }
        _ -> None
      }
    _ -> None
  }
}

@internal
pub fn resolved_string(value: root_field.ResolvedValue) -> Option(String) {
  case value {
    root_field.StringVal(value) -> Some(value)
    root_field.IntVal(value) -> Some(int.to_string(value))
    _ -> None
  }
}

@internal
pub fn percentage_to_label(value: Float) -> String {
  int.to_string(float.round(value *. 100.0)) <> "%"
}

@internal
pub fn plural_item(quantity: String) -> String {
  case quantity {
    "1" -> "item"
    _ -> "items"
  }
}

@internal
pub fn user_error(
  field: List(String),
  message: String,
  code: String,
) -> SourceValue {
  user_error_with_code(field, message, Some(code))
}

@internal
pub fn user_error_with_code(
  field: List(String),
  message: String,
  code: Option(String),
) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("field", SrcList(list.map(field, SrcString))),
      #("message", SrcString(message)),
      #("code", code |> option.map(SrcString) |> option.unwrap(SrcNull)),
      #("extraInfo", SrcNull),
    ]),
  )
}

@internal
pub fn user_error_null_field(message: String, code: String) -> SourceValue {
  user_error_null_field_with_code(message, Some(code))
}

@internal
pub fn user_error_null_field_with_code(
  message: String,
  code: Option(String),
) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("field", SrcNull),
      #("message", SrcString(message)),
      #("code", code |> option.map(SrcString) |> option.unwrap(SrcNull)),
      #("extraInfo", SrcNull),
    ]),
  )
}

@internal
pub fn read_string_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) -> read_string(args, name)
    Error(_) -> None
  }
}

@internal
pub fn option_to_list(value: Option(a)) -> List(a) {
  case value {
    Some(value) -> [value]
    None -> []
  }
}

@internal
pub fn read_int_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Int) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(root_field.IntVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

@internal
pub fn read_bool_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Bool) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(root_field.BoolVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

@internal
pub fn read_object_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(root_field.ObjectVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
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
pub fn read_value(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> root_field.ResolvedValue {
  dict.get(input, name) |> result.unwrap(root_field.NullVal)
}

@internal
pub fn read_string_array(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: List(String),
) -> List(String) {
  case dict.get(input, name) {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.StringVal(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> fallback
  }
}

@internal
pub fn read_codes_arg_with_shape(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> #(List(String), Bool) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(root_field.ListVal(items)) -> {
          let has_object_inputs =
            list.any(items, fn(item) {
              case item {
                root_field.ObjectVal(_) -> True
                _ -> False
              }
            })
          #(
            list.filter_map(items, fn(item) {
              case item {
                root_field.StringVal(value) -> Ok(value)
                root_field.ObjectVal(fields) ->
                  read_string(fields, "code") |> option.to_result(Nil)
                _ -> Error(Nil)
              }
            }),
            has_object_inputs,
          )
        }
        _ -> #([], False)
      }
    Error(_) -> #([], False)
  }
}

@internal
pub fn resolved_to_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.NullVal -> SrcNull
    root_field.StringVal(value) -> SrcString(value)
    root_field.BoolVal(value) -> SrcBool(value)
    root_field.IntVal(value) -> SrcInt(value)
    root_field.FloatVal(value) -> SrcFloat(value)
    root_field.ListVal(items) -> SrcList(list.map(items, resolved_to_source))
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.map_values(fields, fn(_, value) { resolved_to_source(value) }),
      )
  }
}

@internal
pub fn object_value_or_default(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: SourceValue,
) -> SourceValue {
  case dict.get(input, name) {
    Ok(value) -> resolved_to_source(value)
    Error(_) -> fallback
  }
}

@internal
pub fn combines_default() -> SourceValue {
  SrcObject(
    dict.from_list([
      #("productDiscounts", SrcBool(False)),
      #("orderDiscounts", SrcBool(False)),
      #("shippingDiscounts", SrcBool(False)),
    ]),
  )
}

@internal
pub fn string_list_source(values: List(String)) -> SourceValue {
  SrcList(list.map(values, SrcString))
}

@internal
pub fn bool_source(
  value: root_field.ResolvedValue,
  fallback: Bool,
) -> SourceValue {
  case value {
    root_field.BoolVal(value) -> SrcBool(value)
    _ -> SrcBool(fallback)
  }
}

@internal
pub fn count_source(count: Int) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("count", SrcInt(count)),
      #("precision", SrcString("EXACT")),
    ]),
  )
}

@internal
pub fn code_connection_for_record(
  identity: SyntheticIdentityRegistry,
  code: Option(String),
  existing: Option(DiscountRecord),
) -> #(SourceValue, SyntheticIdentityRegistry) {
  case code {
    Some(code) -> {
      let #(redeem_id, next_identity) = case
        existing |> option.then(existing_redeem_code_id)
      {
        Some(id) -> #(id, identity)
        None ->
          synthetic_identity.make_proxy_synthetic_gid(
            identity,
            "DiscountRedeemCode",
          )
      }
      #(codes_connection_with_id(code, redeem_id), next_identity)
    }
    None -> #(empty_codes_connection(), identity)
  }
}

@internal
pub fn existing_redeem_code_id(record: DiscountRecord) -> Option(String) {
  case discount_owner_source(record) {
    SrcObject(fields) -> {
      let discount = case record.owner_kind {
        "automatic" ->
          dict.get(fields, "automaticDiscount") |> result.unwrap(SrcNull)
        _ -> dict.get(fields, "codeDiscount") |> result.unwrap(SrcNull)
      }
      case discount {
        SrcObject(discount_fields) ->
          case dict.get(discount_fields, "codes") {
            Ok(SrcObject(codes)) ->
              case dict.get(codes, "nodes") {
                Ok(SrcList([SrcObject(first), ..])) ->
                  case dict.get(first, "id") {
                    Ok(SrcString(id)) -> Some(id)
                    _ -> None
                  }
                _ -> None
              }
            _ -> None
          }
        _ -> None
      }
    }
    _ -> None
  }
}

@internal
pub fn codes_connection_with_id(code: String, id: String) -> SourceValue {
  SrcObject(
    dict.from_list([
      #(
        "nodes",
        SrcList([
          SrcObject(
            dict.from_list([
              #("id", SrcString(id)),
              #("code", SrcString(code)),
              #("asyncUsageCount", SrcInt(0)),
            ]),
          ),
        ]),
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
  )
}

@internal
pub fn empty_codes_connection() -> SourceValue {
  SrcObject(
    dict.from_list([
      #("nodes", SrcList([])),
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
  )
}

@internal
pub fn context_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "all") {
        Ok(_) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("DiscountBuyerSelectionAll")),
              #("all", SrcString("ALL")),
            ]),
          )
        Error(_) ->
          case dict.get(fields, "customers") {
            Ok(root_field.ObjectVal(customers)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountCustomers")),
                  #(
                    "customers",
                    SrcList(
                      read_string_array(customers, "add", [])
                      |> list.map(customer_context_node),
                    ),
                  ),
                ]),
              )
            _ ->
              case dict.get(fields, "customerSegments") {
                Ok(root_field.ObjectVal(segments)) ->
                  SrcObject(
                    dict.from_list([
                      #("__typename", SrcString("DiscountCustomerSegments")),
                      #(
                        "segments",
                        SrcList(
                          read_string_array(segments, "add", [])
                          |> list.map(customer_segment_context_node),
                        ),
                      ),
                    ]),
                  )
                _ -> resolved_to_source(value)
              }
          }
      }
    _ ->
      SrcObject(
        dict.from_list([
          #("__typename", SrcString("DiscountBuyerSelectionAll")),
          #("all", SrcString("ALL")),
        ]),
      )
  }
}

@internal
pub fn customer_context_node(id: String) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("__typename", SrcString("Customer")),
      #("id", SrcString(id)),
      #("displayName", SrcString(customer_display_name(id))),
    ]),
  )
}

@internal
pub fn customer_display_name(id: String) -> String {
  case id {
    "gid://shopify/Customer/10548596015410" -> "HAR390 Buyer Context"
    _ -> ""
  }
}

@internal
pub fn customer_segment_context_node(id: String) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("__typename", SrcString("Segment")),
      #("id", SrcString(id)),
      #("name", SrcString(customer_segment_name(id))),
    ]),
  )
}

@internal
pub fn customer_segment_name(id: String) -> String {
  case id {
    "gid://shopify/Segment/647746715954" ->
      "HAR-390 buyer context 1777346878525"
    _ -> ""
  }
}

@internal
pub fn minimum_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "subtotal") {
        Ok(root_field.ObjectVal(subtotal)) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("DiscountMinimumSubtotal")),
              #(
                "greaterThanOrEqualToSubtotal",
                money_source(read_value(
                  subtotal,
                  "greaterThanOrEqualToSubtotal",
                )),
              ),
            ]),
          )
        _ ->
          case dict.get(fields, "quantity") {
            Ok(root_field.ObjectVal(quantity)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountMinimumQuantity")),
                  #(
                    "greaterThanOrEqualToQuantity",
                    resolved_to_source(read_value(
                      quantity,
                      "greaterThanOrEqualToQuantity",
                    )),
                  ),
                ]),
              )
            _ -> SrcNull
          }
      }
    _ -> SrcNull
  }
}

@internal
pub fn customer_gets_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.from_list([
          #("value", discount_value_source(read_value(fields, "value"))),
          #("items", discount_items_source(read_value(fields, "items"))),
          #(
            "appliesOnOneTimePurchase",
            bool_source(read_value(fields, "appliesOnOneTimePurchase"), True),
          ),
          #(
            "appliesOnSubscription",
            bool_source(read_value(fields, "appliesOnSubscription"), False),
          ),
        ]),
      )
    _ -> SrcNull
  }
}

@internal
pub fn customer_buys_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.from_list([
          #("value", discount_value_source(read_value(fields, "value"))),
          #("items", discount_items_source(read_value(fields, "items"))),
        ]),
      )
    _ -> SrcNull
  }
}

@internal
pub fn discount_value_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "percentage") {
        Ok(percentage) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("DiscountPercentage")),
              #("percentage", resolved_to_source(percentage)),
            ]),
          )
        Error(_) ->
          case dict.get(fields, "discountAmount") {
            Ok(root_field.ObjectVal(amount)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountAmount")),
                  #("amount", money_source(read_value(amount, "amount"))),
                  #(
                    "appliesOnEachItem",
                    bool_source(read_value(amount, "appliesOnEachItem"), False),
                  ),
                ]),
              )
            _ ->
              case dict.get(fields, "quantity") {
                Ok(quantity) ->
                  SrcObject(
                    dict.from_list([
                      #("__typename", SrcString("DiscountQuantity")),
                      #("quantity", resolved_to_source(quantity)),
                    ]),
                  )
                Error(_) ->
                  case discount_on_quantity_value(fields) {
                    Ok(root_field.ObjectVal(on_quantity)) ->
                      SrcObject(
                        dict.from_list([
                          #("__typename", SrcString("DiscountOnQuantity")),
                          #(
                            "quantity",
                            SrcObject(
                              dict.from_list([
                                #(
                                  "quantity",
                                  resolved_to_source(read_value(
                                    on_quantity,
                                    "quantity",
                                  )),
                                ),
                              ]),
                            ),
                          ),
                          #(
                            "effect",
                            discount_value_source(read_value(
                              on_quantity,
                              "effect",
                            )),
                          ),
                        ]),
                      )
                    _ -> resolved_to_source(value)
                  }
              }
          }
      }
    _ -> SrcNull
  }
}

@internal
pub fn discount_on_quantity_value(
  fields: Dict(String, root_field.ResolvedValue),
) -> Result(root_field.ResolvedValue, Nil) {
  case dict.get(fields, "discountOnQuantity") {
    Ok(value) -> Ok(value)
    Error(_) ->
      case dict.get(fields, "onQuantity") {
        Ok(value) -> Ok(value)
        Error(_) -> Error(Nil)
      }
  }
}

@internal
pub fn discount_items_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "all") {
        Ok(_) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("AllDiscountItems")),
              #("allItems", SrcBool(True)),
            ]),
          )
        Error(_) ->
          case dict.get(fields, "products") {
            Ok(root_field.ObjectVal(products)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountProducts")),
                  #(
                    "products",
                    id_connection(
                      read_string_array(products, "productsToAdd", []),
                    ),
                  ),
                  #(
                    "productVariants",
                    id_connection(
                      read_string_array(products, "productVariantsToAdd", []),
                    ),
                  ),
                ]),
              )
            _ ->
              case dict.get(fields, "collections") {
                Ok(root_field.ObjectVal(collections)) ->
                  SrcObject(
                    dict.from_list([
                      #("__typename", SrcString("DiscountCollections")),
                      #(
                        "collections",
                        id_connection(read_string_array(collections, "add", [])),
                      ),
                    ]),
                  )
                _ -> resolved_to_source(value)
              }
          }
      }
    _ -> SrcNull
  }
}

@internal
pub fn id_connection(ids: List(String)) -> SourceValue {
  SrcObject(
    dict.from_list([
      #(
        "nodes",
        SrcList(
          list.map(ids, fn(id) {
            SrcObject(dict.from_list([#("id", SrcString(id))]))
          }),
        ),
      ),
    ]),
  )
}

@internal
pub fn destination_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "all") {
        Ok(_) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("DiscountCountryAll")),
              #("allCountries", SrcBool(True)),
            ]),
          )
        Error(_) ->
          case dict.get(fields, "countries") {
            Ok(root_field.ObjectVal(countries)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountCountries")),
                  #(
                    "countries",
                    string_list_source(
                      read_string_array(countries, "add", [])
                      |> list.sort(string.compare),
                    ),
                  ),
                  #(
                    "includeRestOfWorld",
                    resolved_bool_or_default(
                      countries,
                      "includeRestOfWorld",
                      False,
                    ),
                  ),
                ]),
              )
            _ -> resolved_to_source(value)
          }
      }
    _ -> SrcNull
  }
}

@internal
pub fn resolved_bool_or_default(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: Bool,
) -> SourceValue {
  case dict.get(input, name) {
    Ok(root_field.BoolVal(value)) -> SrcBool(value)
    _ -> SrcBool(fallback)
  }
}

@internal
pub fn money_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.StringVal(amount) ->
      SrcObject(
        dict.from_list([
          #("amount", SrcString(normalize_money(amount))),
          #("currencyCode", SrcString("CAD")),
        ]),
      )
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.from_list([
          #("amount", resolved_to_source(read_value(fields, "amount"))),
          #(
            "currencyCode",
            resolved_to_source(read_value(fields, "currencyCode")),
          ),
        ]),
      )
    _ -> SrcNull
  }
}

@internal
pub fn normalize_money(value: String) -> String {
  case string.split(value, ".") {
    [whole, decimals] -> {
      let trimmed = trim_trailing_zeroes(decimals)
      case trimmed {
        "" -> whole <> ".0"
        _ -> whole <> "." <> trimmed
      }
    }
    _ -> value
  }
}

@internal
pub fn trim_trailing_zeroes(value: String) -> String {
  case string.ends_with(value, "0") {
    True -> trim_trailing_zeroes(string.drop_end(value, 1))
    False -> value
  }
}

@internal
pub fn app_discount_type_source(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> SourceValue {
  let function_reference =
    read_string(input, "functionId")
    |> option.or(read_string(input, "functionHandle"))
  let shopify_function =
    function_reference
    |> option.then(fn(reference) { find_shopify_function(store, reference) })
  SrcObject(
    dict.from_list([
      #(
        "appKey",
        shopify_function
          |> option.then(fn(record) { record.app_key })
          |> option.map(SrcString)
          |> option.unwrap(SrcNull),
      ),
      #(
        "functionId",
        function_reference |> option.map(SrcString) |> option.unwrap(SrcNull),
      ),
      #(
        "title",
        shopify_function
          |> option.then(fn(record) { record.title })
          |> option.or(read_string(input, "title"))
          |> option.map(SrcString)
          |> option.unwrap(SrcNull),
      ),
      #(
        "description",
        shopify_function
          |> option.then(fn(record) { record.description })
          |> option.map(SrcString)
          |> option.unwrap(SrcNull),
      ),
    ]),
  )
}

@internal
pub fn find_shopify_function(
  store: Store,
  reference: String,
) -> Option(ShopifyFunctionRecord) {
  store.get_effective_shopify_function_by_id(store, reference)
  |> option.lazy_or(fn() {
    store.list_effective_shopify_functions(store)
    |> list.find(fn(record) {
      record.handle == Some(reference) || record.id == reference
    })
    |> option.from_result
  })
}

@internal
pub fn update_payload_status(
  payload: CapturedJsonValue,
  status: String,
  transition_timestamp: Option(String),
) -> CapturedJsonValue {
  case captured_to_source(payload) {
    SrcObject(fields) -> {
      let updated =
        ["codeDiscount", "automaticDiscount"]
        |> list.fold(fields, fn(acc, key) {
          case dict.get(acc, key) {
            Ok(SrcObject(discount)) -> {
              let discount = dict.insert(discount, "status", SrcString(status))
              let discount = case status, transition_timestamp {
                "ACTIVE", Some(timestamp) ->
                  activate_discount_dates(discount, timestamp)
                "ACTIVE", None -> discount
                "EXPIRED", Some(timestamp) ->
                  expire_discount_dates(discount, timestamp)
                _, _ -> discount
              }
              dict.insert(acc, key, SrcObject(discount))
            }
            _ -> acc
          }
        })
      source_to_captured(SrcObject(updated))
    }
    _ -> payload
  }
}

@internal
pub fn bump_discount_updated_at(
  record: DiscountRecord,
  timestamp: String,
) -> DiscountRecord {
  DiscountRecord(
    ..record,
    payload: update_payload_updated_at(record.payload, timestamp),
  )
}

@internal
pub fn update_payload_updated_at(
  payload: CapturedJsonValue,
  timestamp: String,
) -> CapturedJsonValue {
  case captured_to_source(payload) {
    SrcObject(fields) -> {
      let updated =
        ["codeDiscount", "automaticDiscount"]
        |> list.fold(fields, fn(acc, key) {
          case dict.get(acc, key) {
            Ok(SrcObject(discount)) ->
              dict.insert(
                acc,
                key,
                SrcObject(dict.insert(
                  discount,
                  "updatedAt",
                  SrcString(timestamp),
                )),
              )
            _ -> acc
          }
        })
      source_to_captured(SrcObject(updated))
    }
    _ -> payload
  }
}

@internal
pub fn activate_discount_dates(
  discount: Dict(String, SourceValue),
  timestamp: String,
) -> Dict(String, SourceValue) {
  let discount = case should_clear_ends_at(discount, timestamp) {
    True -> dict.insert(discount, "endsAt", SrcNull)
    False -> discount
  }
  case should_bump_starts_at(discount, timestamp) {
    True -> dict.insert(discount, "startsAt", SrcString(timestamp))
    False -> discount
  }
}

@internal
pub fn expire_discount_dates(
  discount: Dict(String, SourceValue),
  timestamp: String,
) -> Dict(String, SourceValue) {
  let discount = dict.insert(discount, "endsAt", SrcString(timestamp))
  case should_bump_starts_at(discount, timestamp) {
    True -> dict.insert(discount, "startsAt", SrcString(timestamp))
    False -> discount
  }
}

@internal
pub fn should_clear_ends_at(
  discount: Dict(String, SourceValue),
  timestamp: String,
) -> Bool {
  case dict.get(discount, "endsAt") {
    Ok(SrcString(value)) -> iso_timestamp_before(value, timestamp)
    Ok(SrcNull) | Error(_) -> True
    _ -> False
  }
}

@internal
pub fn should_bump_starts_at(
  discount: Dict(String, SourceValue),
  timestamp: String,
) -> Bool {
  case dict.get(discount, "startsAt") {
    Ok(SrcString(value)) -> iso_timestamp_after(value, timestamp)
    Ok(SrcNull) | Error(_) -> True
    _ -> False
  }
}

@internal
pub fn iso_timestamp_before(value: String, timestamp: String) -> Bool {
  case iso_timestamp.parse_iso(value), iso_timestamp.parse_iso(timestamp) {
    Ok(value_ms), Ok(timestamp_ms) -> value_ms < timestamp_ms
    _, _ -> False
  }
}

@internal
pub fn iso_timestamp_after(value: String, timestamp: String) -> Bool {
  case iso_timestamp.parse_iso(value), iso_timestamp.parse_iso(timestamp) {
    Ok(value_ms), Ok(timestamp_ms) -> value_ms > timestamp_ms
    _, _ -> False
  }
}

@internal
pub fn append_codes(
  store: Store,
  record: DiscountRecord,
  codes: List(String),
  identity: SyntheticIdentityRegistry,
) -> #(DiscountRecord, SyntheticIdentityRegistry, List(#(String, String))) {
  case codes {
    [] -> #(record, identity, [])
    [first, ..] -> {
      let existing_nodes = existing_code_nodes(record)
      let existing_codes = list.map(existing_nodes, fn(pair) { pair.1 })
      let #(new_nodes, next_identity) =
        codes
        |> list.filter(fn(code) { !list.contains(existing_codes, code) })
        |> list.fold(#([], identity), fn(acc, code) {
          let #(nodes, current_identity) = acc
          let #(id, next_identity) =
            make_discount_async_gid(
              store,
              current_identity,
              "DiscountRedeemCode",
            )
          #([#(id, code), ..nodes], next_identity)
        })
      let nodes = list.append(existing_nodes, list.reverse(new_nodes))
      #(
        DiscountRecord(
          ..record,
          code: Some(first),
          payload: update_payload_codes(record.payload, nodes),
        ),
        next_identity,
        list.reverse(new_nodes),
      )
    }
  }
}

@internal
pub fn remove_codes_by_ids(
  record: DiscountRecord,
  ids: List(String),
  updated_at: String,
) -> DiscountRecord {
  let remaining_codes =
    existing_code_nodes(record)
    |> list.filter(fn(pair) {
      let #(id, _code) = pair
      !list.contains(ids, id)
    })
  let remaining_code_values = list.map(remaining_codes, fn(pair) { pair.1 })
  DiscountRecord(
    ..record,
    code: case remaining_code_values {
      [first, ..] -> Some(first)
      [] -> None
    },
    payload: update_payload_codes(record.payload, remaining_codes)
      |> update_payload_updated_at(updated_at),
  )
}

@internal
pub fn existing_code_nodes(record: DiscountRecord) -> List(#(String, String)) {
  case discount_owner_source(record) {
    SrcObject(fields) ->
      case dict.get(fields, "codeDiscount") {
        Ok(SrcObject(discount)) ->
          case dict.get(discount, "codes") {
            Ok(SrcObject(codes)) ->
              case dict.get(codes, "nodes") {
                Ok(SrcList(nodes)) -> list.filter_map(nodes, read_code_node)
                _ -> []
              }
            _ -> []
          }
        _ -> []
      }
    _ -> []
  }
}

@internal
pub fn read_code_node(node: SourceValue) -> Result(#(String, String), Nil) {
  case node {
    SrcObject(fields) ->
      case dict.get(fields, "id"), dict.get(fields, "code") {
        Ok(SrcString(id)), Ok(SrcString(code)) -> Ok(#(id, code))
        _, _ -> Error(Nil)
      }
    _ -> Error(Nil)
  }
}

@internal
pub fn update_payload_codes(
  payload: CapturedJsonValue,
  codes: List(#(String, String)),
) -> CapturedJsonValue {
  case captured_to_source(payload) {
    SrcObject(fields) -> {
      let updated =
        ["codeDiscount"]
        |> list.fold(fields, fn(acc, key) {
          case dict.get(acc, key) {
            Ok(SrcObject(discount)) -> {
              let nodes =
                codes
                |> list.map(fn(pair) {
                  let #(id, code) = pair
                  SrcObject(
                    dict.from_list([
                      #("id", SrcString(id)),
                      #("code", SrcString(code)),
                      #("asyncUsageCount", SrcInt(0)),
                    ]),
                  )
                })
              let discount =
                discount
                |> dict.insert(
                  "codes",
                  SrcObject(
                    dict.from_list([
                      #("nodes", SrcList(nodes)),
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
                )
                |> dict.insert("codesCount", count_source(list.length(codes)))
              dict.insert(acc, key, SrcObject(discount))
            }
            _ -> acc
          }
        })
      source_to_captured(SrcObject(updated))
    }
    _ -> payload
  }
}

@internal
pub fn find_effective_discount_by_code(
  store: Store,
  code: String,
) -> Option(DiscountRecord) {
  find_effective_discount_by_code_ignoring(store, code, None)
}

@internal
pub fn find_effective_discount_by_code_ignoring(
  store: Store,
  code: String,
  ignored_discount_id: Option(String),
) -> Option(DiscountRecord) {
  let wanted = string.lowercase(code)
  case
    list.find(store.list_effective_discounts(store), fn(record) {
      let ignored = case ignored_discount_id {
        Some(id) -> record.id == id
        None -> False
      }
      !ignored && discount_record_has_code(record, wanted)
    })
  {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}

@internal
pub fn discount_record_has_code(
  record: DiscountRecord,
  wanted: String,
) -> Bool {
  case record.code {
    Some(record_code) -> string.lowercase(record_code) == wanted
    None -> False
  }
  || {
    existing_code_nodes(record)
    |> list.any(fn(pair) { string.lowercase(pair.1) == wanted })
  }
}

@internal
pub fn make_discount_async_gid(
  store: Store,
  identity: SyntheticIdentityRegistry,
  resource_type: String,
) -> #(String, SyntheticIdentityRegistry) {
  // The TS parity harness handles Discounts before route-level mutation-log
  // recording. The Gleam dispatcher records those logs centrally, so discount
  // async payload IDs need to subtract the extra log-entry IDs while still
  // advancing the real registry for state dumps and later logs.
  let log_adjustment = int.max(0, list.length(store.get_log(store)) - 2)
  let visible_id = identity.next_synthetic_id - log_adjustment
  let gid =
    "gid://shopify/"
    <> resource_type
    <> "/"
    <> int.to_string(visible_id)
    <> "?shopify-draft-proxy=synthetic"
  let #(_, next_identity) =
    synthetic_identity.make_proxy_synthetic_gid(identity, resource_type)
  #(gid, next_identity)
}

@internal
pub fn captured_to_source(value: CapturedJsonValue) -> SourceValue {
  case value {
    CapturedNull -> SrcNull
    CapturedBool(value) -> SrcBool(value)
    CapturedInt(value) -> SrcInt(value)
    CapturedFloat(value) -> SrcFloat(value)
    CapturedString(value) -> SrcString(value)
    CapturedArray(items) -> SrcList(list.map(items, captured_to_source))
    CapturedObject(fields) ->
      SrcObject(
        fields
        |> list.map(fn(pair) {
          let #(key, item) = pair
          #(key, captured_to_source(item))
        })
        |> dict.from_list,
      )
  }
}

@internal
pub fn source_to_captured(value: SourceValue) -> CapturedJsonValue {
  case value {
    SrcNull -> CapturedNull
    SrcBool(value) -> CapturedBool(value)
    SrcInt(value) -> CapturedInt(value)
    SrcFloat(value) -> CapturedFloat(value)
    SrcString(value) -> CapturedString(value)
    SrcList(items) -> CapturedArray(list.map(items, source_to_captured))
    SrcObject(fields) ->
      CapturedObject(
        dict.to_list(fields)
        |> list.map(fn(pair) {
          let #(key, item) = pair
          #(key, source_to_captured(item))
        }),
      )
  }
}
