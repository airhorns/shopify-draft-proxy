//// Products-domain submodule: inventory_core.
//// Combines layered files: inventory_l00, inventory_l01.

import gleam/dict.{type Dict}

import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Definition, type Location, type Selection, OperationDefinition,
}

import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, FloatVal, IntVal, ListVal, NullVal, ObjectVal, StringVal,
}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcString, default_connection_window_options, default_selected_field_options,
  get_field_response_key, get_selected_child_fields, paginate_connection_items,
  project_graphql_value, serialize_connection, src_object,
}

import shopify_draft_proxy/proxy/products/product_types.{
  type InventoryAdjustmentChange, type InventoryAdjustmentChangeInput,
  type InventoryMoveTerminalInput, type InventorySetQuantityInput,
  type NullableFieldUserError, type NumericRead,
  type ProductSetInventoryQuantityInput, type ProductTotalInventorySync,
  type ProductUserError, type QuantityRead, type VariantValidationProblem,
  InventoryAdjustmentChange, InventoryAdjustmentChangeInput,
  InventoryMoveTerminalInput, InventorySetQuantityInput, NullableFieldUserError,
  NumericMissing, NumericNotANumber, NumericNull, NumericValue,
  PreserveProductTotalInventory, ProductUserError, QuantityFloat, QuantityInt,
  QuantityMissing, QuantityNotANumber, QuantityNull,
  RecomputeProductTotalInventory, VariantValidationProblem,
  max_inventory_quantity, max_variant_weight, min_inventory_quantity,
  product_set_inventory_quantities_limit, product_user_error_code_invalid_name,
  product_user_error_code_invalid_quantity_negative,
  product_user_error_code_invalid_quantity_too_high,
  product_user_error_code_invalid_quantity_too_low,
}
import shopify_draft_proxy/proxy/products/shared.{
  find_variable_definition, max_input_size_error, read_bool_argument,
  read_int_field, read_object_field, read_object_list_field, read_string_field,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type InventoryItemRecord, type InventoryLevelRecord,
  type InventoryLocationRecord, type InventoryMeasurementRecord,
  type InventoryQuantityRecord, type InventoryWeightRecord,
  type InventoryWeightValue, type LocationRecord, type ProductVariantRecord,
  InventoryItemRecord, InventoryLevelRecord, InventoryLocationRecord,
  InventoryMeasurementRecord, InventoryQuantityRecord, InventoryWeightFloat,
  InventoryWeightInt, InventoryWeightRecord,
}

// ===== from inventory_l00 =====
@internal
pub fn location_cursor(location: LocationRecord, _index: Int) -> String {
  location.cursor |> option.unwrap(location.id)
}

@internal
pub fn location_source(location: LocationRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Location")),
    #("id", SrcString(location.id)),
    #("name", SrcString(location.name)),
  ])
}

@internal
pub fn inventory_item_variant_cursor(
  variant: ProductVariantRecord,
  _index: Int,
) -> String {
  case variant.inventory_item {
    Some(item) -> item.id
    None -> variant.id
  }
}

@internal
pub fn inventory_quantity_name_definitions() -> List(
  #(String, String, Bool, List(String), List(String)),
) {
  [
    #("available", "Available", True, ["on_hand"], []),
    #("committed", "Committed", True, ["on_hand"], []),
    #("damaged", "Damaged", False, ["on_hand"], []),
    #("incoming", "Incoming", False, [], []),
    #("on_hand", "On hand", True, [], [
      "available",
      "committed",
      "damaged",
      "quality_control",
      "reserved",
      "safety_stock",
    ]),
    #("quality_control", "Quality control", False, ["on_hand"], []),
    #("reserved", "Reserved", True, ["on_hand"], []),
    #("safety_stock", "Safety stock", False, ["on_hand"], []),
  ]
}

@internal
pub fn inventory_quantity_name_source(
  definition: #(String, String, Bool, List(String), List(String)),
) -> SourceValue {
  let #(name, display_name, is_in_use, belongs_to, comprises) = definition
  src_object([
    #("name", SrcString(name)),
    #("displayName", SrcString(display_name)),
    #("isInUse", SrcBool(is_in_use)),
    #("belongsTo", SrcList(list.map(belongs_to, SrcString))),
    #("comprises", SrcList(list.map(comprises, SrcString))),
  ])
}

@internal
pub fn quantity_source(quantity: InventoryQuantityRecord) -> SourceValue {
  src_object([
    #("name", SrcString(quantity.name)),
    #("quantity", SrcInt(quantity.quantity)),
    #("updatedAt", graphql_helpers.option_string_source(quantity.updated_at)),
  ])
}

@internal
pub fn inventory_weight_value_source(value) -> SourceValue {
  case value {
    InventoryWeightInt(value) -> SrcInt(value)
    InventoryWeightFloat(value) -> SrcFloat(value)
  }
}

@internal
pub fn inventory_level_cursor(
  level: InventoryLevelRecord,
  _index: Int,
) -> String {
  case level.cursor {
    Some(cursor) -> cursor
    None -> level.id
  }
}

@internal
pub fn inventory_missing_change_from_error(
  field: Selection,
  input_type: String,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        input_type
        <> " must include the following argument: changeFromQuantity.",
      ),
    ),
    #(
      "extensions",
      json.object([#("code", json.string("INVALID_FIELD_ARGUMENTS"))]),
    ),
    #("path", json.array([get_field_response_key(field)], json.string)),
  ])
}

@internal
pub fn inventory_item_legacy_id(inventory_item_id: String) -> String {
  let tail = case list.last(string.split(inventory_item_id, "/")) {
    Ok(value) -> value
    Error(_) -> inventory_item_id
  }
  case string.split(tail, "?") {
    [id, ..] -> id
    [] -> tail
  }
}

@internal
pub fn write_inventory_quantity_with_timestamp(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
  updated_at: Option(String),
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True ->
      list.map(quantities, fn(quantity) {
        case quantity.name == name {
          True ->
            InventoryQuantityRecord(
              ..quantity,
              quantity: amount,
              updated_at: updated_at,
            )
          False -> quantity
        }
      })
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(
          name: name,
          quantity: amount,
          updated_at: updated_at,
        ),
      ])
  }
}

@internal
pub fn replace_first_inventory_level(
  levels: List(InventoryLevelRecord),
  next_level: InventoryLevelRecord,
) -> List(InventoryLevelRecord) {
  case levels {
    [] -> [next_level]
    [_first, ..rest] -> [next_level, ..rest]
  }
}

@internal
pub fn locations_payload(
  loc: Option(Location),
  document: String,
) -> Option(Json) {
  option.map(loc, graphql_helpers.locations_json(_, document))
}

@internal
pub fn inventory_adjustment_app_source() -> SourceValue {
  src_object([
    #("__typename", SrcString("App")),
    #("id", SrcNull),
    #("title", SrcString("hermes-conformance-products")),
    #("apiKey", SrcNull),
    #("handle", SrcString("hermes-conformance-products")),
  ])
}

@internal
pub fn default_inventory_item_measurement() -> InventoryMeasurementRecord {
  InventoryMeasurementRecord(
    weight: Some(InventoryWeightRecord(
      unit: "KILOGRAMS",
      value: InventoryWeightInt(0),
    )),
  )
}

@internal
pub fn ensure_product_set_inventory_item(
  identity: SyntheticIdentityRegistry,
  inventory_item: Option(InventoryItemRecord),
) -> #(InventoryItemRecord, SyntheticIdentityRegistry) {
  case inventory_item {
    Some(item) -> #(item, identity)
    None -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        InventoryItemRecord(
          id: id,
          tracked: None,
          requires_shipping: None,
          measurement: None,
          country_code_of_origin: None,
          province_code_of_origin: None,
          harmonized_system_code: None,
          inventory_levels: [],
        ),
        next_identity,
      )
    }
  }
}

@internal
pub fn write_inventory_quantity(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
  updated_at: Option(String),
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True ->
      list.map(quantities, fn(quantity) {
        case quantity.name == name {
          True ->
            InventoryQuantityRecord(
              ..quantity,
              quantity: amount,
              updated_at: updated_at,
            )
          False -> quantity
        }
      })
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(
          name: name,
          quantity: amount,
          updated_at: updated_at,
        ),
      ])
  }
}

@internal
pub fn ensure_inventory_quantity(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True -> quantities
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(name: name, quantity: amount, updated_at: None),
      ])
  }
}

@internal
pub fn product_set_inventory_location(
  store: Store,
  existing: Option(InventoryLevelRecord),
  location_id: String,
) -> InventoryLocationRecord {
  case store.get_effective_location_by_id(store, location_id) {
    Some(location) ->
      InventoryLocationRecord(id: location.id, name: location.name)
    None ->
      case existing {
        Some(level) -> level.location
        None -> InventoryLocationRecord(id: location_id, name: "")
      }
  }
}

@internal
pub fn product_set_inventory_level_id(
  inventory_item_id: String,
  location_id: String,
) -> String {
  let inventory_tail =
    inventory_item_id |> string.split("/") |> list.last |> result.unwrap("0")
  let location_tail =
    location_id |> string.split("/") |> list.last |> result.unwrap("0")
  "gid://shopify/InventoryLevel/"
  <> inventory_tail
  <> "-"
  <> location_tail
  <> "?inventory_item_id="
  <> inventory_item_id
}

@internal
pub fn duplicate_inventory_item(
  identity: SyntheticIdentityRegistry,
  inventory_item: Option(InventoryItemRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry, List(String)) {
  case inventory_item {
    None -> #(None, identity, [])
    Some(record) -> {
      let #(inventory_item_id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        Some(InventoryItemRecord(..record, id: inventory_item_id)),
        next_identity,
        [inventory_item_id],
      )
    }
  }
}

@internal
pub fn valid_weight_unit(unit: String) -> Bool {
  case unit {
    "KILOGRAMS" | "GRAMS" | "POUNDS" | "OUNCES" -> True
    _ -> False
  }
}

@internal
pub fn read_inventory_weight_value_input(
  input: Dict(String, ResolvedValue),
) -> Option(InventoryWeightValue) {
  case dict.get(input, "value") {
    Ok(IntVal(value)) -> Some(InventoryWeightInt(value))
    Ok(FloatVal(value)) -> Some(InventoryWeightFloat(value))
    _ -> None
  }
}

@internal
pub fn clone_default_inventory_item(
  identity: SyntheticIdentityRegistry,
  item: Option(InventoryItemRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry) {
  case item {
    None -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        Some(
          InventoryItemRecord(
            id: id,
            tracked: Some(False),
            requires_shipping: Some(True),
            measurement: None,
            country_code_of_origin: None,
            province_code_of_origin: None,
            harmonized_system_code: None,
            inventory_levels: [],
          ),
        ),
        next_identity,
      )
    }
    Some(item) -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        Some(InventoryItemRecord(..item, id: id, tracked: Some(True))),
        next_identity,
      )
    }
  }
}

@internal
pub fn inventory_compare_field_name(use_change_from_quantity: Bool) -> String {
  case use_change_from_quantity {
    True -> "changeFromQuantity"
    False -> "compareQuantity"
  }
}

@internal
pub fn inventory_activate_staged_ids(
  resolved: Option(#(ProductVariantRecord, InventoryLevelRecord)),
) -> List(String) {
  case resolved {
    Some(#(variant, level)) ->
      case variant.inventory_item {
        Some(item) -> [level.id, item.id]
        None -> [level.id]
      }
    None -> []
  }
}

@internal
pub fn variant_inventory_levels(
  variant: ProductVariantRecord,
) -> List(InventoryLevelRecord) {
  case variant.inventory_item {
    Some(item) -> item.inventory_levels
    None -> []
  }
}

@internal
pub fn inventory_level_is_active(level: InventoryLevelRecord) -> Bool {
  case level.is_active {
    Some(False) -> False
    _ -> True
  }
}

@internal
pub fn normalize_inventory_item_id(id: String) -> String {
  case string.starts_with(id, "gid://shopify/InventoryItem/") {
    True -> id
    False -> "gid://shopify/InventoryItem/" <> id
  }
}

@internal
pub fn inventory_level_item_id(id: String) -> Option(String) {
  case string.split(id, "?inventory_item_id=") {
    [_, item_id] -> Some(item_id)
    _ -> None
  }
}

@internal
pub fn find_inventory_level(
  levels: List(InventoryLevelRecord),
  location_id: String,
) -> Option(InventoryLevelRecord) {
  levels
  |> list.find(fn(level) { level.location.id == location_id })
  |> option.from_result
}

@internal
pub fn replace_inventory_level(
  levels: List(InventoryLevelRecord),
  location_id: String,
  next_level: InventoryLevelRecord,
) -> List(InventoryLevelRecord) {
  list.map(levels, fn(level) {
    case level.location.id == location_id {
      True -> next_level
      False -> level
    }
  })
}

@internal
pub fn inventory_quantity_amount(
  quantities: List(InventoryQuantityRecord),
  name: String,
) -> Int {
  case list.find(quantities, fn(quantity) { quantity.name == name }) {
    Ok(quantity) -> quantity.quantity
    Error(_) -> 0
  }
}

@internal
pub fn write_inventory_quantity_amount(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True ->
      list.map(quantities, fn(quantity) {
        case quantity.name == name {
          True -> InventoryQuantityRecord(..quantity, quantity: amount)
          False -> quantity
        }
      })
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(name: name, quantity: amount, updated_at: None),
      ])
  }
}

@internal
pub fn is_on_hand_component_quantity_name(name: String) -> Bool {
  case name {
    "available"
    | "committed"
    | "damaged"
    | "quality_control"
    | "reserved"
    | "safety_stock" -> True
    _ -> False
  }
}

@internal
pub fn valid_inventory_set_quantity_name(name: String) -> Bool {
  case name {
    "available" | "on_hand" -> True
    _ -> False
  }
}

@internal
pub fn valid_staged_inventory_quantity_name(name: String) -> Bool {
  case name {
    "available"
    | "committed"
    | "damaged"
    | "incoming"
    | "quality_control"
    | "reserved"
    | "safety_stock" -> True
    _ -> False
  }
}

@internal
pub fn valid_inventory_adjust_quantity_name(name: String) -> Bool {
  case name {
    "available"
    | "on_hand"
    | "committed"
    | "damaged"
    | "incoming"
    | "quality_control"
    | "reserved"
    | "safety_stock" -> True
    _ -> False
  }
}

@internal
pub fn variant_tracks_inventory(variant: ProductVariantRecord) -> Bool {
  case variant.inventory_item {
    Some(item) ->
      case item.tracked {
        Some(True) -> True
        Some(False) -> False
        None -> variant.inventory_quantity != None
      }
    None -> False
  }
}

@internal
pub fn make_inventory_item_for_variant(
  identity: SyntheticIdentityRegistry,
  template: Option(ProductVariantRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry) {
  let template_item =
    option.then(template, fn(variant) { variant.inventory_item })
  case template_item {
    Some(item) -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        Some(InventoryItemRecord(..item, id: id, inventory_levels: [])),
        next_identity,
      )
    }
    None -> #(None, identity)
  }
}

// ===== from inventory_l01 =====
@internal
pub fn serialize_locations_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let locations = store.list_effective_locations(store)
  let window =
    paginate_connection_items(
      locations,
      field,
      variables,
      location_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: location_cursor,
      serialize_node: fn(location, node_field, _index) {
        project_graphql_value(
          location_source(location),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: ConnectionPageInfoOptions(
        include_inline_fragments: False,
        prefix_cursors: False,
        include_cursors: True,
        fallback_start_cursor: None,
        fallback_end_cursor: None,
      ),
    ),
  )
}

@internal
pub fn inventory_item_variants(store: Store) -> List(ProductVariantRecord) {
  store.list_effective_product_variants(store)
  |> list.filter(fn(variant) {
    case variant.inventory_item {
      Some(_) -> True
      None -> False
    }
  })
  |> list.sort(fn(left, right) {
    string.compare(
      inventory_item_variant_cursor(left, 0),
      inventory_item_variant_cursor(right, 0),
    )
  })
}

@internal
pub fn reverse_inventory_item_variants(
  variants: List(ProductVariantRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductVariantRecord) {
  case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(variants)
    _ -> variants
  }
}

@internal
pub fn inventory_properties_source() -> SourceValue {
  src_object([
    #(
      "quantityNames",
      SrcList(list.map(
        inventory_quantity_name_definitions(),
        inventory_quantity_name_source,
      )),
    ),
  ])
}

@internal
pub fn inventory_level_source(level: InventoryLevelRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("InventoryLevel")),
    #("id", SrcString(level.id)),
    #("isActive", graphql_helpers.option_bool_source(level.is_active)),
    #(
      "location",
      src_object([
        #("__typename", SrcString("Location")),
        #("id", SrcString(level.location.id)),
        #("name", SrcString(level.location.name)),
      ]),
    ),
    #("quantities", SrcList(list.map(level.quantities, quantity_source))),
  ])
}

@internal
pub fn optional_weight_source(
  weight: Option(InventoryWeightRecord),
) -> SourceValue {
  case weight {
    Some(value) ->
      src_object([
        #("unit", SrcString(value.unit)),
        #("value", inventory_weight_value_source(value.value)),
      ])
    None -> SrcNull
  }
}

@internal
pub fn product_set_inventory_quantities_max_input_size_errors(
  input: Dict(String, ResolvedValue),
) -> List(Json) {
  read_object_list_field(input, "variants")
  |> list.filter_map(fn(variant_input) {
    let quantities =
      read_object_list_field(variant_input, "inventoryQuantities")
    case list.length(quantities) > product_set_inventory_quantities_limit {
      True ->
        Ok(
          max_input_size_error(
            list.length(quantities),
            product_set_inventory_quantities_limit,
            ["productSet", "input", "variants", "inventoryQuantities"],
          ),
        )
      False -> Error(Nil)
    }
  })
}

@internal
pub fn write_inventory_quantity_delta(
  level: InventoryLevelRecord,
  identity: SyntheticIdentityRegistry,
  name: String,
  delta: Int,
) -> #(InventoryLevelRecord, SyntheticIdentityRegistry) {
  let current = inventory_quantity_amount(level.quantities, name)
  let next_amount = int.max(0, current + delta)
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let quantities =
    write_inventory_quantity_with_timestamp(
      level.quantities,
      name,
      next_amount,
      Some(updated_at),
    )
  #(InventoryLevelRecord(..level, quantities: quantities), next_identity)
}

@internal
pub fn reactivate_inventory_level(
  level: InventoryLevelRecord,
  identity: SyntheticIdentityRegistry,
) -> #(InventoryLevelRecord, SyntheticIdentityRegistry) {
  let available = inventory_quantity_amount(level.quantities, "available")
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let quantities =
    write_inventory_quantity_with_timestamp(
      level.quantities,
      "available",
      available,
      Some(updated_at),
    )
  #(
    InventoryLevelRecord(..level, quantities: quantities, is_active: Some(True)),
    next_identity,
  )
}

@internal
pub fn find_variable_definition_location_in_definitions(
  definitions: List(Definition),
  variable_name: String,
) -> Option(Location) {
  case definitions {
    [] -> None
    [definition, ..rest] ->
      case definition {
        OperationDefinition(variable_definitions: definitions, ..) ->
          case find_variable_definition(definitions, variable_name) {
            Some(location) -> Some(location)
            None ->
              find_variable_definition_location_in_definitions(
                rest,
                variable_name,
              )
          }
        _ ->
          find_variable_definition_location_in_definitions(rest, variable_name)
      }
  }
}

@internal
pub fn read_inventory_set_quantity_inputs(
  input: Dict(String, ResolvedValue),
) -> List(InventorySetQuantityInput) {
  case dict.get(input, "quantities") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) ->
            Ok(InventorySetQuantityInput(
              inventory_item_id: read_string_field(fields, "inventoryItemId"),
              location_id: read_string_field(fields, "locationId"),
              quantity: read_int_field(fields, "quantity"),
              compare_quantity: read_int_field(fields, "compareQuantity"),
              change_from_quantity: read_int_field(fields, "changeFromQuantity"),
            ))
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn read_inventory_adjustment_change_inputs(
  input: Dict(String, ResolvedValue),
) -> List(InventoryAdjustmentChangeInput) {
  case dict.get(input, "changes") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) ->
            Ok(InventoryAdjustmentChangeInput(
              inventory_item_id: read_string_field(fields, "inventoryItemId"),
              location_id: read_string_field(fields, "locationId"),
              ledger_document_uri: read_string_field(
                fields,
                "ledgerDocumentUri",
              ),
              delta: read_int_field(fields, "delta"),
              change_from_quantity: read_int_field(fields, "changeFromQuantity"),
            ))
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn read_inventory_move_terminal(
  input: Dict(String, ResolvedValue),
  name: String,
) -> InventoryMoveTerminalInput {
  case read_object_field(input, name) {
    Some(fields) ->
      InventoryMoveTerminalInput(
        location_id: read_string_field(fields, "locationId"),
        name: read_string_field(fields, "name"),
        ledger_document_uri: read_string_field(fields, "ledgerDocumentUri"),
      )
    None ->
      InventoryMoveTerminalInput(
        location_id: None,
        name: None,
        ledger_document_uri: None,
      )
  }
}

@internal
pub fn product_set_available_quantity(
  inputs: List(ProductSetInventoryQuantityInput),
) -> Option(Int) {
  let quantities =
    inputs
    |> list.filter_map(fn(input) {
      case input.name == "available" {
        True -> Ok(input.quantity)
        False -> Error(Nil)
      }
    })
  case quantities {
    [] -> None
    _ ->
      Some(list.fold(quantities, 0, fn(total, quantity) { total + quantity }))
  }
}

@internal
pub fn upsert_product_set_quantity_group(
  groups: List(#(String, List(ProductSetInventoryQuantityInput))),
  location_id: String,
  input: ProductSetInventoryQuantityInput,
) -> List(#(String, List(ProductSetInventoryQuantityInput))) {
  case groups {
    [] -> [#(location_id, [input])]
    [first, ..rest] -> {
      let #(current_id, values) = first
      case current_id == location_id {
        True -> [#(current_id, list.append(values, [input])), ..rest]
        False -> [
          first,
          ..upsert_product_set_quantity_group(rest, location_id, input)
        ]
      }
    }
  }
}

@internal
pub fn variant_weight_value_problems(
  read: NumericRead,
) -> List(VariantValidationProblem) {
  case read {
    NumericValue(value) if value <. 0.0 -> [
      VariantValidationProblem(
        "weight_negative",
        [],
        [],
        "Weight must be greater than or equal to 0",
        Some("GREATER_THAN_OR_EQUAL_TO"),
        Some("GREATER_THAN_OR_EQUAL_TO"),
      ),
    ]
    NumericValue(value) if value >=. max_variant_weight -> [
      VariantValidationProblem(
        "weight_too_large",
        [],
        [],
        "Weight must be less than 2000000000",
        Some("INVALID_INPUT"),
        Some("INVALID_INPUT"),
      ),
    ]
    NumericNotANumber -> [
      VariantValidationProblem(
        "weight_not_a_number",
        [],
        [],
        "Weight is not a number",
        Some("NOT_A_NUMBER"),
        Some("NOT_A_NUMBER"),
      ),
    ]
    NumericMissing | NumericNull | NumericValue(_) -> []
  }
}

@internal
pub fn variant_weight_unit_problems(
  weight: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  case read_string_field(weight, "unit") {
    Some(unit) ->
      case valid_weight_unit(unit) {
        True -> []
        False -> [
          VariantValidationProblem(
            "weight_unit_invalid",
            [],
            [],
            "Weight unit is not included in the list",
            Some("INVALID_INPUT"),
            Some("INVALID_INPUT"),
          ),
        ]
      }
    None -> []
  }
}

@internal
pub fn variant_top_level_weight_unit_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  case read_string_field(input, "weightUnit") {
    Some(unit) ->
      case valid_weight_unit(unit) {
        True -> []
        False -> [
          VariantValidationProblem(
            "weight_unit_invalid",
            [],
            [],
            "Weight unit is not included in the list",
            Some("INVALID_INPUT"),
            Some("INVALID_INPUT"),
          ),
        ]
      }
    None -> []
  }
}

@internal
pub fn quantity_problem(
  suffix: List(String),
  message: String,
) -> VariantValidationProblem {
  VariantValidationProblem(
    "inventory_quantity",
    suffix,
    case suffix {
      ["inventoryQuantity"] -> suffix
      _ -> ["inventoryQuantities"]
    },
    message,
    Some("INVALID_INPUT"),
    Some("INVALID_INPUT"),
  )
}

@internal
pub fn read_quantity_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> QuantityRead {
  case dict.get(input, name) {
    Error(_) -> QuantityMissing
    Ok(NullVal) -> QuantityNull
    Ok(IntVal(value)) -> QuantityInt(value)
    Ok(FloatVal(value)) -> QuantityFloat(value)
    Ok(StringVal(_)) -> QuantityNotANumber
    _ -> QuantityNotANumber
  }
}

@internal
pub fn read_variant_weight_input(
  input: Dict(String, ResolvedValue),
) -> Option(Dict(String, ResolvedValue)) {
  use inventory_item <- option.then(read_object_field(input, "inventoryItem"))
  use measurement <- option.then(read_object_field(
    inventory_item,
    "measurement",
  ))
  read_object_field(measurement, "weight")
}

@internal
pub fn read_inventory_quantities_available_total(
  input: Dict(String, ResolvedValue),
) -> Option(Int) {
  case dict.get(input, "inventoryQuantities") {
    Ok(ListVal(values)) -> {
      let quantities =
        list.filter_map(values, fn(value) {
          case value {
            ObjectVal(fields) ->
              case read_int_field(fields, "availableQuantity") {
                Some(quantity) -> Ok(quantity)
                None -> Error(Nil)
              }
            _ -> Error(Nil)
          }
        })
      case quantities {
        [] -> None
        _ ->
          Some(
            list.fold(quantities, 0, fn(total, quantity) { total + quantity }),
          )
      }
    }
    _ -> None
  }
}

@internal
pub fn read_inventory_weight_input(
  input: Dict(String, ResolvedValue),
  fallback: Option(InventoryWeightRecord),
) -> Option(InventoryWeightRecord) {
  case read_object_field(input, "weight") {
    Some(weight) ->
      case
        read_string_field(weight, "unit"),
        read_inventory_weight_value_input(weight)
      {
        Some(unit), Some(value) ->
          Some(InventoryWeightRecord(unit: unit, value: value))
        _, _ -> fallback
      }
    None -> fallback
  }
}

@internal
pub fn inventory_set_quantity_changes(
  inventory_item_id: String,
  location_id: String,
  name: String,
  delta: Int,
  change: InventoryAdjustmentChange,
) -> #(List(InventoryAdjustmentChange), List(InventoryAdjustmentChange)) {
  case name {
    "on_hand" -> #(
      [
        InventoryAdjustmentChange(
          inventory_item_id: inventory_item_id,
          location_id: location_id,
          name: "available",
          delta: delta,
          quantity_after_change: None,
          ledger_document_uri: None,
        ),
      ],
      [change],
    )
    _ -> {
      let mirrored = case is_on_hand_component_quantity_name(name) {
        True -> [
          InventoryAdjustmentChange(
            inventory_item_id: inventory_item_id,
            location_id: location_id,
            name: "on_hand",
            delta: delta,
            quantity_after_change: None,
            ledger_document_uri: None,
          ),
        ]
        False -> []
      }
      #([change], mirrored)
    }
  }
}

@internal
pub fn inventory_set_quantity_bounds_errors(
  quantity: Int,
  path: List(String),
) -> List(ProductUserError) {
  case quantity {
    quantity if quantity > max_inventory_quantity -> [
      ProductUserError(
        list.append(path, ["quantity"]),
        "The quantity can't be higher than 1,000,000,000.",
        Some(product_user_error_code_invalid_quantity_too_high),
      ),
    ]
    quantity if quantity < min_inventory_quantity -> [
      ProductUserError(
        list.append(path, ["quantity"]),
        "The quantity can't be lower than -1,000,000,000.",
        Some(product_user_error_code_invalid_quantity_too_low),
      ),
    ]
    quantity if quantity < 0 -> [
      ProductUserError(
        list.append(path, ["quantity"]),
        "The quantity can't be negative.",
        Some(product_user_error_code_invalid_quantity_negative),
      ),
    ]
    _ -> []
  }
}

@internal
pub fn inventory_quantity_bounds_errors(
  quantity: Int,
  path: List(String),
) -> List(ProductUserError) {
  case quantity {
    quantity if quantity > max_inventory_quantity -> [
      ProductUserError(
        path,
        "The quantity can't be higher than 1,000,000,000.",
        Some(product_user_error_code_invalid_quantity_too_high),
      ),
    ]
    quantity if quantity < min_inventory_quantity -> [
      ProductUserError(
        path,
        "The quantity can't be lower than -1,000,000,000.",
        Some(product_user_error_code_invalid_quantity_too_low),
      ),
    ]
    _ -> []
  }
}

@internal
pub fn quantity_compare_quantity(
  quantity: InventorySetQuantityInput,
  use_change_from_quantity: Bool,
) -> Option(Int) {
  case use_change_from_quantity {
    True -> quantity.change_from_quantity
    False -> quantity.compare_quantity
  }
}

@internal
pub fn validate_inventory_move_ledger_document_uri(
  quantity_name: Option(String),
  ledger_document_uri: Option(String),
  path: List(String),
) -> List(ProductUserError) {
  case quantity_name, ledger_document_uri {
    Some("available"), Some(_) -> [
      ProductUserError(
        path,
        "A ledger document URI is not allowed when adjusting available.",
        None,
      ),
    ]
    Some(name), None if name != "available" -> [
      ProductUserError(
        path,
        "A ledger document URI is required except when adjusting available.",
        None,
      ),
    ]
    _, _ -> []
  }
}

@internal
pub fn inventory_adjust_product_total_inventory_sync(
  require_change_from_quantity: Bool,
) -> ProductTotalInventorySync {
  case require_change_from_quantity {
    True -> RecomputeProductTotalInventory
    False -> PreserveProductTotalInventory
  }
}

@internal
pub fn inventory_deactivate_only_state_error(
  level: InventoryLevelRecord,
) -> NullableFieldUserError {
  NullableFieldUserError(
    None,
    "The product couldn't be unstocked from "
      <> level.location.name
      <> " because products need to be stocked at a minimum of 1 location.",
  )
}

@internal
pub fn inventory_deactivate_item_not_found_error() -> NullableFieldUserError {
  NullableFieldUserError(
    None,
    "The product couldn't be unstocked because the product was deleted.",
  )
}

@internal
pub fn inventory_deactivate_location_deleted_error() -> NullableFieldUserError {
  NullableFieldUserError(
    None,
    "The product couldn't be unstocked because the location was deleted.",
  )
}

@internal
pub fn active_inventory_levels(
  levels: List(InventoryLevelRecord),
) -> List(InventoryLevelRecord) {
  list.filter(levels, inventory_level_is_active)
}

@internal
pub fn find_inventory_level_target(
  store: Store,
  inventory_level_id: String,
) -> Option(#(ProductVariantRecord, InventoryLevelRecord)) {
  store.list_effective_product_variants(store)
  |> list.filter_map(fn(variant) {
    case
      list.find(variant_inventory_levels(variant), fn(level) {
        level.id == inventory_level_id
      })
    {
      Ok(level) -> Ok(#(variant, level))
      Error(_) -> Error(Nil)
    }
  })
  |> list.first
  |> option.from_result
}

@internal
pub fn add_inventory_quantity_amount(
  quantities: List(InventoryQuantityRecord),
  name: String,
  delta: Int,
) -> List(InventoryQuantityRecord) {
  write_inventory_quantity_amount(
    quantities,
    name,
    inventory_quantity_amount(quantities, name) + delta,
  )
}

@internal
pub fn invalid_inventory_set_quantity_name_error() -> ProductUserError {
  ProductUserError(
    ["input", "name"],
    "The quantity name must be either 'available' or 'on_hand'.",
    Some(product_user_error_code_invalid_name),
  )
}

@internal
pub fn invalid_inventory_adjust_quantity_name_error(
  field: List(String),
) -> ProductUserError {
  ProductUserError(
    field,
    "The specified quantity name is invalid. Valid values are: available, on_hand, committed, damaged, incoming, quality_control, reserved, safety_stock.",
    Some("INVALID_QUANTITY_NAME"),
  )
}

@internal
pub fn invalid_inventory_quantity_name_error(
  field: List(String),
) -> ProductUserError {
  ProductUserError(
    field,
    "The specified quantity name is invalid. Valid values are: available, damaged, incoming, quality_control, reserved, safety_stock.",
    None,
  )
}

@internal
pub fn inventory_change_location(
  store: Store,
  change: InventoryAdjustmentChange,
) -> InventoryLocationRecord {
  case
    store.find_effective_variant_by_inventory_item_id(
      store,
      change.inventory_item_id,
    )
  {
    Some(variant) ->
      case
        find_inventory_level(
          variant_inventory_levels(variant),
          change.location_id,
        )
      {
        Some(level) -> level.location
        None -> InventoryLocationRecord(id: change.location_id, name: "")
      }
    None -> InventoryLocationRecord(id: change.location_id, name: "")
  }
}
