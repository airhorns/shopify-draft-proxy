//// Bounded shipping/fulfillments port slice.
////
//// Covers the shipping/fulfillment roots ported during HAR-493 while keeping
//// the broader order return/edit domains as captured-state slices.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CalculatedOrderRecord, type CapturedJsonValue,
  type FulfillmentOrderRecord, type ReverseDeliveryRecord,
  type ReverseFulfillmentOrderRecord, type ShippingPackageDimensionsRecord,
  type ShippingPackageRecord, type ShippingPackageWeightRecord,
  type StorePropertyRecord, CalculatedOrderRecord, CapturedArray, CapturedBool,
  CapturedFloat, CapturedInt, CapturedNull, CapturedObject, CapturedString,
  FulfillmentOrderRecord, ReverseFulfillmentOrderRecord,
  ShippingPackageDimensionsRecord, ShippingPackageRecord,
  ShippingPackageWeightRecord, StorePropertyBool, StorePropertyString,
}

@internal
pub fn captured_connection(
  nodes: List(CapturedJsonValue),
) -> CapturedJsonValue {
  CapturedObject([
    #("nodes", CapturedArray(nodes)),
    #(
      "pageInfo",
      CapturedObject([
        #("hasNextPage", CapturedBool(False)),
        #("hasPreviousPage", CapturedBool(False)),
        #("startCursor", CapturedNull),
        #("endCursor", CapturedNull),
      ]),
    ),
  ])
}

@internal
pub fn captured_count(count: Int) -> CapturedJsonValue {
  CapturedObject([
    #("count", CapturedInt(count)),
    #("precision", CapturedString("EXACT")),
  ])
}

@internal
pub fn optional_string_json(value: Option(String)) -> Json {
  case value {
    Some(string) -> json.string(string)
    None -> json.null()
  }
}

@internal
pub fn update_fulfillment_order_fields(
  order: FulfillmentOrderRecord,
  updates: List(#(String, CapturedJsonValue)),
) -> FulfillmentOrderRecord {
  FulfillmentOrderRecord(
    ..order,
    data: captured_upsert_fields(order.data, updates),
  )
}

@internal
pub fn zero_fulfillment_order_line_items(
  data: CapturedJsonValue,
  line_item_fulfillable: Option(Int),
) -> CapturedJsonValue {
  let nodes =
    captured_array_field(data, "lineItems", "nodes")
    |> list.map(fn(node) {
      let line_item = case
        line_item_fulfillable,
        captured_field(node, "lineItem")
      {
        Some(quantity), Some(value) ->
          captured_upsert_fields(value, [
            #("fulfillableQuantity", CapturedInt(quantity)),
          ])
        _, Some(value) -> value
        _, None -> CapturedNull
      }
      captured_upsert_fields(node, [
        #("totalQuantity", CapturedInt(0)),
        #("remainingQuantity", CapturedInt(0)),
        #("lineItem", line_item),
      ])
    })
  captured_connection(nodes)
}

@internal
pub fn apply_package_input(
  draft_store: Store,
  current: ShippingPackageRecord,
  input: Dict(String, root_field.ResolvedValue),
  updated_at: String,
) -> #(ShippingPackageRecord, Store) {
  let requested_default = read_bool(input, "default")
  let updated =
    ShippingPackageRecord(
      ..current,
      name: read_string(input, "name") |> option.or(current.name),
      type_: read_string(input, "type") |> option.or(current.type_),
      default: requested_default |> option.unwrap(current.default),
      weight: read_weight(input, "weight") |> option.or(current.weight),
      dimensions: read_dimensions(input, "dimensions")
        |> option.or(current.dimensions),
      updated_at: updated_at,
    )
  case requested_default {
    Some(True) -> {
      let packages = store.list_effective_shipping_packages(draft_store)
      let cleared_store =
        list.fold(packages, draft_store, fn(current_store, shipping_package) {
          case shipping_package.id == updated.id || !shipping_package.default {
            True -> current_store
            False -> {
              let cleared =
                ShippingPackageRecord(
                  ..shipping_package,
                  default: False,
                  updated_at: updated_at,
                )
              let #(_, next_store) =
                store.stage_update_shipping_package(current_store, cleared)
              next_store
            }
          }
        })
      #(updated, cleared_store)
    }
    _ -> #(updated, draft_store)
  }
}

@internal
pub fn resolved_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) -> args
    Error(_) -> dict.new()
  }
}

@internal
pub fn read_string(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case dict.get(fields, key) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_trimmed_string(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case read_string(fields, key) {
    Some(value) -> Some(string.trim(value))
    None -> None
  }
}

@internal
pub fn read_carrier_service_callback_url(
  fields: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case read_string(fields, "callbackUrl") {
    Some(value) ->
      case string.trim(value) {
        "" -> None
        _ -> Some(value)
      }
    _ -> None
  }
}

@internal
pub fn read_fulfillment_service_callback_url(
  fields: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case read_string(fields, "callbackUrl") {
    Some(value) ->
      case string.trim(value) {
        "" -> None
        _ -> Some(value)
      }
    _ -> None
  }
}

@internal
pub fn read_bool(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Bool) {
  case dict.get(fields, key) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn bool_string(value: Bool) -> String {
  case value {
    True -> "true"
    False -> "false"
  }
}

@internal
pub fn store_property_bool_field(
  record: StorePropertyRecord,
  key: String,
) -> Option(Bool) {
  case dict.get(record.data, key) {
    Ok(StorePropertyBool(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn store_property_string_field(
  record: StorePropertyRecord,
  key: String,
) -> Option(String) {
  case dict.get(record.data, key) {
    Ok(StorePropertyString(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_float(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Float) {
  case dict.get(fields, key) {
    Ok(root_field.FloatVal(value)) -> Some(value)
    Ok(root_field.IntVal(value)) -> Some(int.to_float(value))
    _ -> None
  }
}

@internal
pub fn read_int(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Int) {
  case dict.get(fields, key) {
    Ok(root_field.IntVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_number(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(CapturedJsonValue) {
  case dict.get(fields, key) {
    Ok(root_field.IntVal(value)) -> Some(CapturedInt(value))
    Ok(root_field.FloatVal(value)) -> Some(CapturedFloat(value))
    _ -> None
  }
}

@internal
pub fn read_object(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(fields, key) {
    Ok(root_field.ObjectVal(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn read_string_array(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(String) {
  case dict.get(fields, key) {
    Ok(root_field.ListVal(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          root_field.StringVal(string) -> Ok(string)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn read_object_array(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(fields, key) {
    Ok(root_field.ListVal(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          root_field.ObjectVal(object) -> Ok(object)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn selected_selections(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

@internal
pub fn unique_strings(values: List(String)) -> List(String) {
  list.fold(values, [], fn(seen, value) {
    case list.contains(seen, value) {
      True -> seen
      False -> list.append(seen, [value])
    }
  })
}

@internal
pub fn count_delivery_profile_locations_to_add(
  input: Dict(String, root_field.ResolvedValue),
) -> Int {
  input
  |> read_object_array("locationGroupsToUpdate")
  |> list.flat_map(fn(group) { read_string_array(group, "locationsToAdd") })
  |> unique_strings
  |> list.length
}

@internal
pub fn delivery_profile_active_method_delta(
  input: Dict(String, root_field.ResolvedValue),
) -> Int {
  let zone_updates =
    input
    |> read_object_array("locationGroupsToUpdate")
    |> list.flat_map(fn(group) { read_object_array(group, "zonesToUpdate") })
  let created_active =
    zone_updates
    |> list.flat_map(fn(zone) {
      read_object_array(zone, "methodDefinitionsToCreate")
    })
    |> list.filter(fn(method) {
      read_bool(method, "active") |> option.unwrap(True)
    })
    |> list.length
  let deactivated =
    zone_updates
    |> list.flat_map(fn(zone) {
      read_object_array(zone, "methodDefinitionsToUpdate")
    })
    |> list.filter(fn(method) { read_bool(method, "active") == Some(False) })
    |> list.length
  created_active - deactivated
}

@internal
pub fn normalize_money_amount(value: String) -> String {
  case string.ends_with(value, ".00") {
    True -> string.drop_end(value, 3)
    False ->
      case string.ends_with(value, "0") && string.contains(value, ".") {
        True -> normalize_money_amount(string.drop_end(value, 1))
        False -> value
      }
  }
}

@internal
pub fn captured_upsert_fields(
  value: CapturedJsonValue,
  updates: List(#(String, CapturedJsonValue)),
) -> CapturedJsonValue {
  let update_keys = list.map(updates, fn(pair) { pair.0 })
  let existing = case value {
    CapturedObject(fields) ->
      list.filter(fields, fn(pair) { !list.contains(update_keys, pair.0) })
    _ -> []
  }
  CapturedObject(list.append(existing, updates))
}

@internal
pub fn captured_string_field(
  value: CapturedJsonValue,
  key: String,
) -> Option(String) {
  case captured_field(value, key) {
    Some(CapturedString(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn captured_bool_field(
  value: CapturedJsonValue,
  key: String,
) -> Option(Bool) {
  case captured_field(value, key) {
    Some(CapturedBool(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn captured_int_field(
  value: CapturedJsonValue,
  key: String,
  nested_key: String,
) -> Option(Int) {
  let source = case nested_key {
    "" -> captured_field(value, key)
    _ ->
      case captured_field(value, key) {
        Some(nested) -> captured_field(nested, nested_key)
        None -> None
      }
  }
  case source {
    Some(CapturedInt(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn captured_array_field(
  value: CapturedJsonValue,
  key: String,
  nested_key: String,
) -> List(CapturedJsonValue) {
  let source = case nested_key {
    "" -> captured_field(value, key)
    _ ->
      case captured_field(value, key) {
        Some(nested) -> captured_field(nested, nested_key)
        None -> None
      }
  }
  case source {
    Some(CapturedArray(values)) -> values
    _ -> []
  }
}

@internal
pub fn captured_field(
  value: CapturedJsonValue,
  key: String,
) -> Option(CapturedJsonValue) {
  case value {
    CapturedObject(fields) ->
      fields
      |> list.find(fn(pair) { pair.0 == key })
      |> option.from_result
      |> option.map(fn(pair) { pair.1 })
    _ -> None
  }
}

@internal
pub fn option_to_captured_string(value: Option(String)) -> CapturedJsonValue {
  case value {
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

@internal
pub fn make_reverse_delivery_line_items(
  reverse_fulfillment_order: ReverseFulfillmentOrderRecord,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  identity: SyntheticIdentityRegistry,
) -> #(List(CapturedJsonValue), SyntheticIdentityRegistry) {
  let available =
    captured_array_field(reverse_fulfillment_order.data, "lineItems", "nodes")
  let requested = case inputs {
    [] ->
      list.map(available, fn(line_item) {
        #(
          line_item,
          captured_int_field(line_item, "remainingQuantity", "")
            |> option.unwrap(
              captured_int_field(line_item, "totalQuantity", "")
              |> option.unwrap(1),
            ),
        )
      })
    _ ->
      inputs
      |> list.filter_map(fn(input) {
        use line_item_id <- result.try(
          read_string(input, "reverseFulfillmentOrderLineItemId")
          |> option.to_result(Nil),
        )
        use line_item <- result.try(find_captured_line_item(
          available,
          line_item_id,
        ))
        Ok(#(line_item, read_int(input, "quantity") |> option.unwrap(1)))
      })
  }
  list.fold(requested, #([], identity), fn(acc, pair) {
    let #(items, current_identity) = acc
    let #(line_item, quantity) = pair
    let #(delivery_line_item, next_identity) =
      make_reverse_delivery_line_item(line_item, quantity, current_identity)
    #(list.append(items, [delivery_line_item]), next_identity)
  })
}

@internal
pub fn make_reverse_delivery_line_item(
  reverse_fulfillment_order_line_item: CapturedJsonValue,
  quantity: Int,
  identity: SyntheticIdentityRegistry,
) -> #(CapturedJsonValue, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "ReverseDeliveryLineItem")
  #(
    CapturedObject([
      #("id", CapturedString(id)),
      #("quantity", CapturedInt(quantity)),
      #("reverseFulfillmentOrderLineItem", reverse_fulfillment_order_line_item),
    ]),
    next_identity,
  )
}

@internal
pub fn reverse_delivery_value(
  id: String,
  args: Dict(String, root_field.ResolvedValue),
  line_items: List(CapturedJsonValue),
) -> CapturedJsonValue {
  CapturedObject([
    #("id", CapturedString(id)),
    #("reverseDeliveryLineItems", captured_connection(line_items)),
    #("deliverable", reverse_delivery_deliverable(args)),
  ])
}

@internal
pub fn append_reverse_delivery(
  reverse_fulfillment_order: ReverseFulfillmentOrderRecord,
  reverse_delivery: ReverseDeliveryRecord,
) -> ReverseFulfillmentOrderRecord {
  let existing =
    captured_array_field(
      reverse_fulfillment_order.data,
      "reverseDeliveries",
      "nodes",
    )
  ReverseFulfillmentOrderRecord(
    ..reverse_fulfillment_order,
    data: captured_upsert_fields(reverse_fulfillment_order.data, [
      #(
        "reverseDeliveries",
        captured_connection(list.append([reverse_delivery.data], existing)),
      ),
    ]),
  )
}

@internal
pub fn update_reverse_delivery_shipping(
  data: CapturedJsonValue,
  args: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let existing_deliverable =
    captured_field(data, "deliverable") |> option.unwrap(CapturedObject([]))
  let tracking = case reverse_delivery_tracking(args) {
    CapturedNull -> captured_field(existing_deliverable, "tracking")
    other -> Some(other)
  }
  let label = case reverse_delivery_label(args) {
    CapturedNull -> captured_field(existing_deliverable, "label")
    other -> Some(other)
  }
  captured_upsert_fields(data, [
    #(
      "deliverable",
      CapturedObject([
        #("__typename", CapturedString("ReverseDeliveryShippingDeliverable")),
        #("tracking", tracking |> option.unwrap(CapturedNull)),
        #("label", label |> option.unwrap(CapturedNull)),
      ]),
    ),
  ])
}

@internal
pub fn reverse_delivery_deliverable(
  args: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  CapturedObject([
    #("__typename", CapturedString("ReverseDeliveryShippingDeliverable")),
    #("tracking", reverse_delivery_tracking(args)),
    #("label", reverse_delivery_label(args)),
  ])
}

@internal
pub fn reverse_delivery_tracking(
  args: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  case read_object(args, "trackingInput") {
    Some(input) ->
      CapturedObject([
        #("number", option_to_captured_string(read_string(input, "number"))),
        #("url", option_to_captured_string(read_string(input, "url"))),
        #("company", option_to_captured_string(read_string(input, "company"))),
      ])
    None -> CapturedNull
  }
}

@internal
pub fn reverse_delivery_label(
  args: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  case read_object(args, "labelInput") {
    Some(input) ->
      CapturedObject([
        #(
          "publicFileUrl",
          option_to_captured_string(read_string(input, "publicFileUrl")),
        ),
      ])
    None -> CapturedNull
  }
}

@internal
pub fn find_reverse_fulfillment_order_line_item(
  draft_store: Store,
  line_item_id: String,
) -> Option(#(ReverseFulfillmentOrderRecord, CapturedJsonValue)) {
  store.list_effective_reverse_fulfillment_orders(draft_store)
  |> list.find_map(fn(reverse_fulfillment_order) {
    case
      find_captured_line_item(
        captured_array_field(
          reverse_fulfillment_order.data,
          "lineItems",
          "nodes",
        ),
        line_item_id,
      )
    {
      Ok(line_item) -> Ok(#(reverse_fulfillment_order, line_item))
      Error(_) -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn find_captured_line_item(
  line_items: List(CapturedJsonValue),
  id: String,
) -> Result(CapturedJsonValue, Nil) {
  list.find(line_items, fn(line_item) { reverse_line_item_id(line_item) == id })
}

@internal
pub fn dispose_reverse_fulfillment_order_line_item(
  line_item: CapturedJsonValue,
  quantity: Int,
  disposition_type: String,
) -> CapturedJsonValue {
  let remaining =
    captured_int_field(line_item, "remainingQuantity", "") |> option.unwrap(0)
  let next_remaining = case remaining - quantity < 0 {
    True -> 0
    False -> remaining - quantity
  }
  captured_upsert_fields(line_item, [
    #("remainingQuantity", CapturedInt(next_remaining)),
    #("dispositionType", CapturedString(disposition_type)),
  ])
}

@internal
pub fn update_reverse_fulfillment_order_line_item(
  reverse_fulfillment_order: ReverseFulfillmentOrderRecord,
  updated_line_item: CapturedJsonValue,
) -> ReverseFulfillmentOrderRecord {
  let line_items =
    captured_array_field(reverse_fulfillment_order.data, "lineItems", "nodes")
    |> list.map(fn(line_item) {
      case
        reverse_line_item_id(line_item)
        == reverse_line_item_id(updated_line_item)
      {
        True -> updated_line_item
        False -> line_item
      }
    })
  ReverseFulfillmentOrderRecord(
    ..reverse_fulfillment_order,
    data: captured_upsert_fields(reverse_fulfillment_order.data, [
      #("lineItems", captured_connection(line_items)),
    ]),
  )
}

@internal
pub fn make_calculated_shipping_line(
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> Option(#(CapturedJsonValue, SyntheticIdentityRegistry)) {
  case read_string(input, "title"), read_money_input(input, "price") {
    Some(title), Some(#(amount, currency_code)) -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(
          identity,
          "CalculatedShippingLine",
        )
      Some(#(
        CapturedObject([
          #("id", CapturedString(id)),
          #("title", CapturedString(title)),
          #("code", CapturedString(title)),
          #("stagedStatus", CapturedString("ADDED")),
          #("price", money_set(amount, currency_code)),
          #("originalPriceSet", money_set(amount, currency_code)),
        ]),
        next_identity,
      ))
    }
    _, _ -> None
  }
}

@internal
pub fn update_calculated_shipping_line(
  line: CapturedJsonValue,
  input: Dict(String, root_field.ResolvedValue),
) -> CapturedJsonValue {
  let title = read_string(input, "title")
  let price = read_money_input(input, "price")
  let updates =
    []
    |> append_optional_captured("title", option.map(title, CapturedString))
    |> append_optional_captured("code", option.map(title, CapturedString))
    |> append_optional_captured(
      "price",
      option.map(price, fn(price) { money_set(price.0, price.1) }),
    )
    |> append_optional_captured(
      "originalPriceSet",
      option.map(price, fn(price) { money_set(price.0, price.1) }),
    )
    |> list.append([
      #(
        "stagedStatus",
        CapturedString(case captured_string_field(line, "stagedStatus") {
          Some("ADDED") -> "ADDED"
          _ -> "UPDATED"
        }),
      ),
    ])
  captured_upsert_fields(line, updates)
}

@internal
pub fn update_calculated_order_shipping_lines(
  calculated_order: CalculatedOrderRecord,
  shipping_lines: List(CapturedJsonValue),
) -> CalculatedOrderRecord {
  let data_with_lines =
    captured_upsert_fields(calculated_order.data, [
      #("shippingLines", CapturedArray(shipping_lines)),
    ])
  let data_with_total =
    update_calculated_order_total(data_with_lines, shipping_lines)
  CalculatedOrderRecord(..calculated_order, data: data_with_total)
}

@internal
pub fn calculated_order_shipping_lines(
  calculated_order: CalculatedOrderRecord,
) -> List(CapturedJsonValue) {
  captured_array_field(calculated_order.data, "shippingLines", "")
}

@internal
pub fn update_calculated_order_total(
  data: CapturedJsonValue,
  shipping_lines: List(CapturedJsonValue),
) -> CapturedJsonValue {
  let base_amount =
    captured_money_amount(data, "subtotalPriceSet")
    |> option.unwrap(
      captured_money_amount(data, "subtotalLineItemsPriceSet")
      |> option.unwrap(0.0),
    )
  let shipping_amount =
    list.fold(shipping_lines, 0.0, fn(sum, line) {
      sum +. { captured_money_amount(line, "price") |> option.unwrap(0.0) }
    })
  let currency_code =
    captured_money_currency(data, "totalPriceSet")
    |> option.unwrap(
      first_shipping_line_currency(shipping_lines) |> option.unwrap("USD"),
    )
  captured_upsert_fields(data, [
    #(
      "totalPriceSet",
      money_set(
        float.to_string(base_amount +. { shipping_amount }),
        currency_code,
      ),
    ),
  ])
}

@internal
pub fn read_money_input(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(#(String, String)) {
  case read_object(fields, key) {
    Some(input) ->
      case read_string(input, "amount") {
        Some(amount) -> {
          let currency_code =
            read_string(input, "currencyCode") |> option.unwrap("USD")
          Some(#(format_money_amount(amount), currency_code))
        }
        None -> None
      }
    None -> None
  }
}

@internal
pub fn money_set(amount: String, currency_code: String) -> CapturedJsonValue {
  let money =
    CapturedObject([
      #("amount", CapturedString(format_money_amount(amount))),
      #("currencyCode", CapturedString(currency_code)),
    ])
  CapturedObject([
    #("shopMoney", money),
    #("presentmentMoney", money),
  ])
}

@internal
pub fn captured_money_amount(
  value: CapturedJsonValue,
  key: String,
) -> Option(Float) {
  case captured_field(value, key) {
    Some(money_set) ->
      case captured_field(money_set, "shopMoney") {
        Some(shop_money) ->
          case captured_string_field(shop_money, "amount") {
            Some(amount) ->
              case float.parse(amount) {
                Ok(value) -> Some(value)
                Error(_) -> None
              }
            None -> None
          }
        None -> None
      }
    None -> None
  }
}

@internal
pub fn captured_money_currency(
  value: CapturedJsonValue,
  key: String,
) -> Option(String) {
  case captured_field(value, key) {
    Some(money_set) ->
      case captured_field(money_set, "shopMoney") {
        Some(shop_money) -> captured_string_field(shop_money, "currencyCode")
        None -> None
      }
    None -> None
  }
}

@internal
pub fn first_shipping_line_currency(
  shipping_lines: List(CapturedJsonValue),
) -> Option(String) {
  shipping_lines
  |> list.find_map(fn(line) {
    captured_money_currency(line, "price") |> option.to_result(Nil)
  })
  |> option.from_result
}

@internal
pub fn format_money_amount(amount: String) -> String {
  case float.parse(amount) {
    Ok(value) -> float.to_string(value)
    Error(_) -> amount
  }
}

@internal
pub fn append_optional_captured(
  fields: List(#(String, CapturedJsonValue)),
  key: String,
  value: Option(CapturedJsonValue),
) -> List(#(String, CapturedJsonValue)) {
  case value {
    Some(value) -> list.append(fields, [#(key, value)])
    None -> fields
  }
}

@internal
pub fn reverse_line_item_id(value: CapturedJsonValue) -> String {
  captured_string_field(value, "id") |> option.unwrap("")
}

@internal
pub fn read_weight(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(ShippingPackageWeightRecord) {
  case read_object(fields, key) {
    Some(weight) ->
      Some(ShippingPackageWeightRecord(
        value: read_float(weight, "value"),
        unit: read_string(weight, "unit"),
      ))
    None -> None
  }
}

@internal
pub fn read_dimensions(
  fields: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(ShippingPackageDimensionsRecord) {
  case read_object(fields, key) {
    Some(dimensions) ->
      Some(ShippingPackageDimensionsRecord(
        length: read_float(dimensions, "length"),
        width: read_float(dimensions, "width"),
        height: read_float(dimensions, "height"),
        unit: read_string(dimensions, "unit"),
      ))
    None -> None
  }
}
