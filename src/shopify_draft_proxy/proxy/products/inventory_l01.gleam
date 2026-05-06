//// Internal products-domain implementation split from proxy/products.gleam.

import gleam/bit_array
import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Definition, type Location, type ObjectField, type Selection,
  type VariableDefinition, Argument, Directive, Field, InlineFragment, NullValue,
  ObjectField, ObjectValue, OperationDefinition, SelectionSet, StringValue,
  VariableDefinition, VariableValue,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/parser
import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, type RootFieldError, BoolVal, FloatVal, IntVal, ListVal,
  NullVal, ObjectVal, StringVal, get_field_arguments, get_root_fields,
}
import shopify_draft_proxy/graphql/source as graphql_source
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  paginate_connection_items, project_graphql_field_value, project_graphql_value,
  serialize_connection, serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, RequiredArgument,
  build_null_argument_error, find_argument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/products/inventory_l00.{
  find_inventory_level, inventory_item_variant_cursor, inventory_level_is_active,
  inventory_quantity_amount, inventory_quantity_name_definitions,
  inventory_quantity_name_source, inventory_weight_value_source,
  is_on_hand_component_quantity_name, location_cursor, location_source,
  quantity_source, read_inventory_weight_value_input, valid_weight_unit,
  variant_inventory_levels, write_inventory_quantity_amount,
  write_inventory_quantity_with_timestamp,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  find_variable_definition, max_input_size_error, read_bool_argument,
  read_int_field, read_object_field, read_object_list_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type InventoryAdjustmentChange, type InventoryAdjustmentChangeInput,
  type InventoryMoveTerminalInput, type InventorySetQuantityInput,
  type NullableFieldUserError, type NumericRead,
  type ProductSetInventoryQuantityInput, type ProductTotalInventorySync,
  type ProductUserError, type QuantityRead, type VariantValidationProblem,
  InventoryAdjustmentChange, InventoryAdjustmentChangeInput,
  InventoryMoveTerminalInput, InventorySetQuantityInput, NullableFieldUserError,
  NumericMissing, NumericNotANumber, NumericNull, NumericValue,
  PreserveProductTotalInventory, ProductSetInventoryQuantityInput,
  ProductUserError, QuantityFloat, QuantityInt, QuantityMissing,
  QuantityNotANumber, QuantityNull, RecomputeProductTotalInventory,
  VariantValidationProblem, max_inventory_quantity, max_variant_weight,
  min_inventory_quantity, product_set_inventory_quantities_limit,
  product_user_error_code_invalid_name,
  product_user_error_code_invalid_quantity_negative,
  product_user_error_code_invalid_quantity_too_high,
  product_user_error_code_invalid_quantity_too_low,
} as product_types
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type ChannelRecord, type CollectionImageRecord,
  type CollectionRecord, type CollectionRuleRecord, type CollectionRuleSetRecord,
  type InventoryItemRecord, type InventoryLevelRecord,
  type InventoryLocationRecord, type InventoryMeasurementRecord,
  type InventoryQuantityRecord, type InventoryShipmentLineItemRecord,
  type InventoryShipmentRecord, type InventoryShipmentTrackingRecord,
  type InventoryTransferLineItemRecord,
  type InventoryTransferLocationSnapshotRecord, type InventoryTransferRecord,
  type InventoryWeightRecord, type InventoryWeightValue, type LocationRecord,
  type ProductCategoryRecord, type ProductCollectionRecord,
  type ProductFeedRecord, type ProductMediaRecord, type ProductMetafieldRecord,
  type ProductOperationRecord, type ProductOperationUserErrorRecord,
  type ProductOptionRecord, type ProductOptionValueRecord, type ProductRecord,
  type ProductResourceFeedbackRecord, type ProductSeoRecord,
  type ProductVariantRecord, type ProductVariantSelectedOptionRecord,
  type PublicationRecord, type SellingPlanGroupRecord, type SellingPlanRecord,
  type ShopResourceFeedbackRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString, CollectionRecord,
  CollectionRuleRecord, CollectionRuleSetRecord, InventoryItemRecord,
  InventoryLevelRecord, InventoryLocationRecord, InventoryMeasurementRecord,
  InventoryQuantityRecord, InventoryShipmentLineItemRecord,
  InventoryShipmentRecord, InventoryShipmentTrackingRecord,
  InventoryTransferLineItemRecord, InventoryTransferLocationSnapshotRecord,
  InventoryTransferRecord, InventoryWeightFloat, InventoryWeightInt,
  InventoryWeightRecord, LocationRecord, ProductCollectionRecord,
  ProductFeedRecord, ProductMediaRecord, ProductMetafieldRecord,
  ProductOperationRecord, ProductOperationUserErrorRecord, ProductOptionRecord,
  ProductOptionValueRecord, ProductRecord, ProductResourceFeedbackRecord,
  ProductSeoRecord, ProductVariantRecord, ProductVariantSelectedOptionRecord,
  PublicationRecord, SellingPlanGroupRecord, SellingPlanRecord,
  ShopResourceFeedbackRecord,
}

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
