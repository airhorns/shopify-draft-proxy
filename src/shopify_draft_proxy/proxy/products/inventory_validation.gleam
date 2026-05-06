//// Products-domain submodule: inventory_validation.
//// Combines layered files: inventory_l02, inventory_l03, inventory_l04.

import gleam/dict.{type Dict}

import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import gleam/string
import shopify_draft_proxy/graphql/ast.{type Location, type Selection, Field}

import shopify_draft_proxy/graphql/parser
import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, ListVal, ObjectVal, get_field_arguments,
}
import shopify_draft_proxy/graphql/source as graphql_source

import shopify_draft_proxy/proxy/customers/inputs as customer_inputs
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SerializeConnectionConfig, SrcList,
  SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_field_response_key, get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, serialize_connection,
  src_object,
}

import shopify_draft_proxy/proxy/products/inventory_core.{
  active_inventory_levels, ensure_product_set_inventory_item,
  find_inventory_level, find_variable_definition_location_in_definitions,
  invalid_inventory_adjust_quantity_name_error,
  invalid_inventory_quantity_name_error,
  inventory_deactivate_item_not_found_error,
  inventory_deactivate_location_deleted_error, inventory_item_variants,
  inventory_level_cursor, inventory_level_is_active, inventory_level_item_id,
  inventory_level_source, inventory_missing_change_from_error,
  inventory_properties_source, inventory_quantity_amount,
  inventory_quantity_bounds_errors, inventory_set_quantity_bounds_errors,
  normalize_inventory_item_id, optional_weight_source,
  product_set_available_quantity, product_set_inventory_level_id,
  product_set_inventory_location, quantity_problem, quantity_source,
  read_inventory_move_terminal, read_inventory_quantities_available_total,
  read_inventory_weight_input, read_quantity_field, read_variant_weight_input,
  upsert_product_set_quantity_group, valid_inventory_adjust_quantity_name,
  valid_staged_inventory_quantity_name, valid_weight_unit,
  validate_inventory_move_ledger_document_uri,
  variant_top_level_weight_unit_problems, variant_weight_unit_problems,
  variant_weight_value_problems,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type BulkVariantUserError, type InventoryAdjustmentChange,
  type InventoryAdjustmentChangeInput, type InventoryAdjustmentGroup,
  type InventoryMoveQuantityInput, type InventorySetQuantityInput,
  type NullableFieldUserError, type ProductSetInventoryQuantityInput,
  type ProductUserError, type VariantValidationProblem, BulkVariantUserError,
  InventoryAdjustmentGroup, InventoryMoveQuantityInput, NumericValue,
  ProductSetInventoryQuantityInput, ProductUserError, QuantityFloat, QuantityInt,
  QuantityMissing, QuantityNotANumber, QuantityNull, max_inventory_quantity,
  min_inventory_quantity, product_set_inventory_quantities_limit,
}
import shopify_draft_proxy/proxy/products/products_core.{
  apply_product_set_level_quantities, enumerate_items,
}
import shopify_draft_proxy/proxy/products/shared.{
  bool_string, connection_page_info_source, dedupe_preserving_order,
  input_list_has_object_missing_field, missing_idempotency_key_error,
  nullable_field_user_errors_source, read_arg_string_list, read_bool_field,
  read_include_inactive_argument, read_int_field, read_non_empty_string_field,
  read_numeric_field, read_object_field, read_object_list_field,
  read_string_argument, read_string_field, resource_id_matches,
}
import shopify_draft_proxy/proxy/products/shared_money.{has_idempotency_key}
import shopify_draft_proxy/proxy/products/variants_helpers.{
  product_search_parse_options, product_string_match_options,
}
import shopify_draft_proxy/proxy/products/variants_options_core.{
  variant_title_with_fallback,
}

import shopify_draft_proxy/search_query_parser

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type InventoryItemRecord, type InventoryLevelRecord,
  type InventoryMeasurementRecord, type ProductOperationUserErrorRecord,
  type ProductVariantRecord, type ProductVariantSelectedOptionRecord,
  InventoryItemRecord, InventoryLevelRecord, InventoryMeasurementRecord,
  ProductOperationUserErrorRecord, ProductVariantRecord,
}

// ===== from inventory_l02 =====
@internal
pub fn inventory_item_variant_matches_positive_query_term(
  variant: ProductVariantRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  case variant.inventory_item {
    Some(item) -> {
      let value = search_query_parser.search_query_term_value(term)
      case option.map(term.field, string.lowercase) {
        None ->
          list.any(
            [item.id, option.unwrap(variant.sku, ""), variant.id],
            fn(candidate) {
              search_query_parser.matches_search_query_string(
                Some(candidate),
                value,
                search_query_parser.IncludesMatch,
                product_string_match_options(),
              )
            },
          )
        Some("id") -> resource_id_matches(item.id, None, value)
        Some("sku") ->
          search_query_parser.matches_search_query_string(
            variant.sku,
            value,
            search_query_parser.ExactMatch,
            product_string_match_options(),
          )
        Some("tracked") ->
          bool_string(option.unwrap(item.tracked, False))
          == string.lowercase(value)
        _ -> True
      }
    }
    None -> False
  }
}

@internal
pub fn serialize_inventory_properties(
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    inventory_properties_source(),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn serialize_inventory_item_level_field(
  item: InventoryItemRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let include_inactive = read_include_inactive_argument(field, variables)
  case read_string_argument(field, variables, "locationId") {
    Some(location_id) ->
      case find_inventory_level(item.inventory_levels, location_id) {
        Some(level) ->
          case include_inactive || inventory_level_is_active(level) {
            True -> project_inventory_level(level, field, variables, fragments)
            False -> json.null()
          }
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn inventory_levels_connection_source(
  levels: List(InventoryLevelRecord),
) -> SourceValue {
  let edges =
    levels
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(level, index) = pair
      src_object([
        #("cursor", SrcString(inventory_level_cursor(level, index))),
        #("node", inventory_level_source(level)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #("nodes", SrcList(list.map(levels, inventory_level_source))),
    #("pageInfo", connection_page_info_source(levels, inventory_level_cursor)),
  ])
}

fn project_inventory_level(
  level: InventoryLevelRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let source = inventory_level_source(level)
  let entries =
    get_selected_child_fields(field, default_selected_field_options())
    |> list.map(fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "quantities" ->
              project_graphql_value(
                SrcList(list.map(
                  inventory_level_selected_quantities(
                    level,
                    selection,
                    variables,
                  ),
                  quantity_source,
                )),
                get_selected_child_fields(
                  selection,
                  default_selected_field_options(),
                ),
                fragments,
              )
            _ -> project_graphql_field_value(source, selection, fragments)
          }
        _ -> project_graphql_field_value(source, selection, fragments)
      }
      #(key, value)
    })
  json.object(entries)
}

fn inventory_level_selected_quantities(
  level: InventoryLevelRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) {
  let names = inventory_level_quantity_names(field, variables)
  case names {
    [] -> level.quantities
    _ ->
      names
      |> list.filter_map(fn(requested_name) {
        case
          list.find(level.quantities, fn(quantity) {
            quantity.name == requested_name
          })
        {
          Ok(quantity) -> Ok(quantity)
          Error(_) -> Error(Nil)
        }
      })
  }
}

fn inventory_level_quantity_names(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(String) {
  case get_field_arguments(field, variables) {
    Ok(args) -> read_arg_string_list(args, "names")
    Error(_) -> []
  }
}

@internal
pub fn optional_measurement_source(
  measurement: Option(InventoryMeasurementRecord),
) -> SourceValue {
  case measurement {
    Some(value) ->
      src_object([#("weight", optional_weight_source(value.weight))])
    None -> SrcNull
  }
}

@internal
pub fn product_set_inventory_quantities_limit_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  read_object_list_field(input, "variants")
  |> enumerate_items()
  |> list.filter_map(fn(pair) {
    let #(variant_input, index) = pair
    let quantities =
      read_object_list_field(variant_input, "inventoryQuantities")
    case list.length(quantities) > product_set_inventory_quantities_limit {
      True ->
        Ok(ProductOperationUserErrorRecord(
          field: Some([
            "input",
            "variants",
            int.to_string(index),
            "inventoryQuantities",
          ]),
          message: "Inventory quantities count is over the allowed limit.",
          code: Some("INVENTORY_QUANTITIES_LIMIT_EXCEEDED"),
        ))
      False -> Error(Nil)
    }
  })
}

@internal
pub fn find_variable_definition_location(
  document: String,
  variable_name: String,
) -> Option(Location) {
  case parser.parse(graphql_source.new(document)) {
    Ok(parsed) ->
      find_variable_definition_location_in_definitions(
        parsed.definitions,
        variable_name,
      )
    Error(_) -> None
  }
}

@internal
pub fn inventory_deactivate_payload(
  user_errors: List(NullableFieldUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryDeactivatePayload")),
      #("userErrors", nullable_field_user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn read_inventory_move_quantity_inputs(
  input: Dict(String, ResolvedValue),
) -> List(InventoryMoveQuantityInput) {
  case dict.get(input, "changes") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) ->
            Ok(InventoryMoveQuantityInput(
              inventory_item_id: read_string_field(fields, "inventoryItemId"),
              quantity: read_int_field(fields, "quantity"),
              from: read_inventory_move_terminal(fields, "from"),
              to: read_inventory_move_terminal(fields, "to"),
            ))
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn read_product_set_inventory_quantity_inputs(
  input: Dict(String, ResolvedValue),
) -> List(ProductSetInventoryQuantityInput) {
  read_object_list_field(input, "inventoryQuantities")
  |> list.filter_map(fn(fields) {
    case read_int_field(fields, "quantity") {
      Some(quantity) ->
        Ok(ProductSetInventoryQuantityInput(
          location_id: read_non_empty_string_field(fields, "locationId"),
          name: read_non_empty_string_field(fields, "name")
            |> option.unwrap("available"),
          quantity: quantity,
        ))
      None -> Error(Nil)
    }
  })
}

@internal
pub fn group_product_set_quantities_by_location(
  inputs: List(ProductSetInventoryQuantityInput),
) -> List(#(String, List(ProductSetInventoryQuantityInput))) {
  inputs
  |> list.fold([], fn(groups, input) {
    let location_id =
      input.location_id |> option.unwrap("gid://shopify/Location/1")
    upsert_product_set_quantity_group(groups, location_id, input)
  })
}

@internal
pub fn variant_weight_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  let top_level_errors =
    variant_weight_value_problems(read_numeric_field(input, "weight"))
    |> list.append(variant_top_level_weight_unit_problems(input))
  case read_variant_weight_input(input) {
    None -> top_level_errors
    Some(weight) -> {
      top_level_errors
      |> list.append(
        variant_weight_value_problems(read_numeric_field(weight, "value")),
      )
      |> list.append(variant_weight_unit_problems(weight))
    }
  }
}

@internal
pub fn variant_quantity_range_problems(
  quantity: Int,
  suffix: List(String),
) -> List(VariantValidationProblem) {
  case quantity < min_inventory_quantity {
    True -> [
      quantity_problem(
        suffix,
        "Inventory quantity must be greater than or equal to -1000000000",
      ),
    ]
    False ->
      case quantity > max_inventory_quantity {
        True -> [
          quantity_problem(
            suffix,
            "Inventory quantity must be less than or equal to 1000000000",
          ),
        ]
        False -> []
      }
  }
}

@internal
pub fn validate_bulk_create_inventory_quantities(
  store: Store,
  input: Dict(String, ResolvedValue),
  variant_index: Int,
  selected_options: List(ProductVariantSelectedOptionRecord),
) -> List(BulkVariantUserError) {
  let quantities = read_object_list_field(input, "inventoryQuantities")
  let has_invalid_location =
    quantities
    |> list.any(fn(quantity) {
      case read_string_field(quantity, "locationId") {
        Some("gid://shopify/Location/1") -> False
        Some(location_id) ->
          store.get_effective_location_by_id(store, location_id) == None
        None -> False
      }
    })
  case has_invalid_location {
    True -> [
      BulkVariantUserError(
        Some(["variants", int.to_string(variant_index), "inventoryQuantities"]),
        "Quantity for "
          <> variant_title_with_fallback(selected_options, "Default Title")
          <> " couldn't be set because the location was deleted.",
        Some("TRACKED_VARIANT_LOCATION_NOT_FOUND"),
      ),
    ]
    False -> []
  }
}

@internal
pub fn read_variant_inventory_quantity(
  input: Dict(String, ResolvedValue),
  fallback: Option(Int),
) -> Option(Int) {
  case read_int_field(input, "inventoryQuantity") {
    Some(quantity) -> Some(quantity)
    None ->
      read_inventory_quantities_available_total(input) |> option.or(fallback)
  }
}

@internal
pub fn read_inventory_measurement_input(
  input: Dict(String, ResolvedValue),
  fallback: Option(InventoryMeasurementRecord),
) -> Option(InventoryMeasurementRecord) {
  case read_object_field(input, "measurement") {
    Some(measurement) ->
      Some(
        InventoryMeasurementRecord(weight: read_inventory_weight_input(
          measurement,
          option.then(fallback, fn(measurement) { measurement.weight }),
        )),
      )
    None -> fallback
  }
}

@internal
pub fn validate_inventory_adjust_inputs(
  name: String,
  changes: List(InventoryAdjustmentChangeInput),
) -> List(ProductUserError) {
  let name_errors = case valid_inventory_adjust_quantity_name(name) {
    True -> []
    False -> [invalid_inventory_adjust_quantity_name_error(["input", "name"])]
  }
  let ledger_errors = case name {
    "available" -> []
    _ ->
      changes
      |> enumerate_items()
      |> list.filter_map(fn(pair) {
        let #(change, index) = pair
        case change.ledger_document_uri {
          Some(_) -> Error(Nil)
          None ->
            Ok(ProductUserError(
              ["input", "changes", int.to_string(index), "ledgerDocumentUri"],
              "A ledger document URI is required except when adjusting available.",
              None,
            ))
        }
      })
  }
  let quantity_errors =
    changes
    |> enumerate_items()
    |> list.flat_map(fn(pair) {
      let #(change, index) = pair
      case change.delta {
        Some(delta) ->
          inventory_quantity_bounds_errors(delta, [
            "input",
            "changes",
            int.to_string(index),
            "delta",
          ])
        None -> []
      }
    })
  list.append(name_errors, list.append(ledger_errors, quantity_errors))
}

@internal
pub fn validate_inventory_set_quantity(
  quantity: InventorySetQuantityInput,
  index: Int,
) -> List(ProductUserError) {
  let path = ["input", "quantities", int.to_string(index)]
  let required_errors = case
    quantity.inventory_item_id,
    quantity.location_id,
    quantity.quantity
  {
    None, _, _ -> [
      ProductUserError(
        list.append(path, ["inventoryItemId"]),
        "Inventory item id is required",
        None,
      ),
    ]
    _, None, _ -> [
      ProductUserError(
        list.append(path, ["locationId"]),
        "Inventory location id is required",
        None,
      ),
    ]
    _, _, None -> [
      ProductUserError(
        list.append(path, ["quantity"]),
        "Inventory quantity is required",
        None,
      ),
    ]
    _, _, _ -> []
  }
  let quantity_errors = case quantity.quantity {
    Some(quantity) -> inventory_set_quantity_bounds_errors(quantity, path)
    None -> []
  }
  list.append(required_errors, quantity_errors)
}

@internal
pub fn has_duplicate_inventory_item_location_pair(
  quantities: List(InventorySetQuantityInput),
  index: Int,
  inventory_item_id: String,
  location_id: String,
) -> Bool {
  quantities
  |> enumerate_items()
  |> list.any(fn(pair) {
    let #(quantity, other_index) = pair
    other_index != index
    && quantity.inventory_item_id == Some(inventory_item_id)
    && quantity.location_id == Some(location_id)
  })
}

@internal
pub fn validate_inventory_move_input(
  change: InventoryMoveQuantityInput,
  index: Int,
) -> List(ProductUserError) {
  let path = ["input", "changes", int.to_string(index)]
  let name_errors =
    list.filter_map(
      [
        #(change.from.name, list.append(path, ["from", "name"])),
        #(change.to.name, list.append(path, ["to", "name"])),
      ],
      fn(candidate) {
        let #(name, field_path) = candidate
        case name {
          Some(name) ->
            case valid_staged_inventory_quantity_name(name) {
              True -> Error(Nil)
              False -> Ok(invalid_inventory_quantity_name_error(field_path))
            }
          None -> Error(Nil)
        }
      },
    )
  let location_error = case change.from.location_id, change.to.location_id {
    Some(from), Some(to) if from != to -> [
      ProductUserError(
        path,
        "The quantities can't be moved between different locations.",
        None,
      ),
    ]
    _, _ -> []
  }
  let same_name_error = case change.from.name, change.to.name {
    Some(from), Some(to) if from == to -> [
      ProductUserError(
        path,
        "The quantity names for each change can't be the same.",
        None,
      ),
    ]
    _, _ -> []
  }
  let ledger_errors =
    list.append(
      validate_inventory_move_ledger_document_uri(
        change.from.name,
        change.from.ledger_document_uri,
        list.append(path, ["from", "ledgerDocumentUri"]),
      ),
      validate_inventory_move_ledger_document_uri(
        change.to.name,
        change.to.ledger_document_uri,
        list.append(path, ["to", "ledgerDocumentUri"]),
      ),
    )
  list.append(
    name_errors,
    list.append(location_error, list.append(same_name_error, ledger_errors)),
  )
}

@internal
pub fn make_inventory_adjustment_group(
  identity: SyntheticIdentityRegistry,
  reason: String,
  reference_document_uri: Option(String),
  changes: List(InventoryAdjustmentChange),
) -> #(InventoryAdjustmentGroup, SyntheticIdentityRegistry) {
  let #(id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "InventoryAdjustmentGroup")
  let #(created_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  #(
    InventoryAdjustmentGroup(
      id: id,
      created_at: created_at,
      reason: reason,
      reference_document_uri: reference_document_uri,
      changes: changes,
    ),
    next_identity,
  )
}

@internal
pub fn inventory_adjustment_staged_ids(
  group: InventoryAdjustmentGroup,
) -> List(String) {
  [
    group.id,
    ..dedupe_preserving_order(
      list.map(group.changes, fn(change) { change.inventory_item_id }),
    )
  ]
}

@internal
pub fn inventory_deactivate_missing_target_errors(
  store: Store,
  inventory_level_id: Option(String),
) -> List(NullableFieldUserError) {
  case inventory_level_id {
    Some(id) -> {
      case inventory_level_item_id(id) {
        Some(item_id) ->
          case
            store.find_effective_variant_by_inventory_item_id(
              store,
              normalize_inventory_item_id(item_id),
            )
          {
            Some(_) -> [inventory_deactivate_location_deleted_error()]
            None -> [inventory_deactivate_item_not_found_error()]
          }
        None -> [inventory_deactivate_item_not_found_error()]
      }
    }
    None -> [inventory_deactivate_item_not_found_error()]
  }
}

@internal
pub fn filter_inventory_levels_by_include_inactive(
  levels: List(InventoryLevelRecord),
  include_inactive: Bool,
) -> List(InventoryLevelRecord) {
  case include_inactive {
    True -> levels
    False -> active_inventory_levels(levels)
  }
}

@internal
pub fn sum_inventory_level_available(
  levels: List(InventoryLevelRecord),
) -> Option(Int) {
  Some(
    levels
    |> active_inventory_levels
    |> list.fold(0, fn(total, level) {
      total + inventory_quantity_amount(level.quantities, "available")
    }),
  )
}

@internal
pub fn variant_available_quantity(variant: ProductVariantRecord) -> Int {
  case variant.inventory_item {
    Some(item) ->
      case item.inventory_levels {
        [] -> option.unwrap(variant.inventory_quantity, 0)
        levels ->
          levels
          |> active_inventory_levels
          |> list.fold(0, fn(total, level) {
            total + inventory_quantity_amount(level.quantities, "available")
          })
      }
    None -> option.unwrap(variant.inventory_quantity, 0)
  }
}

// ===== from inventory_l03 =====
@internal
pub fn filtered_inventory_item_variants(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductVariantRecord) {
  search_query_parser.apply_search_query(
    inventory_item_variants(store),
    read_string_argument(field, variables, "query"),
    product_search_parse_options(),
    inventory_item_variant_matches_positive_query_term,
  )
}

@internal
pub fn inventory_item_source_with_variant(
  item: InventoryItemRecord,
  variant: SourceValue,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("InventoryItem")),
    #("id", SrcString(item.id)),
    #("tracked", graphql_helpers.option_bool_source(item.tracked)),
    #(
      "requiresShipping",
      graphql_helpers.option_bool_source(item.requires_shipping),
    ),
    #("measurement", optional_measurement_source(item.measurement)),
    #(
      "countryCodeOfOrigin",
      graphql_helpers.option_string_source(item.country_code_of_origin),
    ),
    #(
      "provinceCodeOfOrigin",
      graphql_helpers.option_string_source(item.province_code_of_origin),
    ),
    #(
      "harmonizedSystemCode",
      graphql_helpers.option_string_source(item.harmonized_system_code),
    ),
    #(
      "inventoryLevels",
      inventory_levels_connection_source(active_inventory_levels(
        item.inventory_levels,
      )),
    ),
    #("variant", variant),
  ])
}

@internal
pub fn serialize_inventory_item_levels_field(
  item: InventoryItemRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let levels =
    filter_inventory_levels_by_include_inactive(
      item.inventory_levels,
      read_include_inactive_argument(field, variables),
    )

  let window =
    paginate_connection_items(
      levels,
      field,
      variables,
      inventory_level_cursor,
      default_connection_window_options(),
    )

  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: inventory_level_cursor,
      serialize_node: fn(level, node_field, _index) {
        project_inventory_level(level, node_field, variables, fragments)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

@internal
pub fn inventory_adjust_202604_contract_error(
  enabled: Bool,
  input: Dict(String, ResolvedValue),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(Json) {
  case enabled {
    False -> None
    True ->
      case
        input_list_has_object_missing_field(
          input,
          "changes",
          "changeFromQuantity",
        )
      {
        True ->
          Some(inventory_missing_change_from_error(
            field,
            "InventoryChangeInput",
          ))
        False ->
          case has_idempotency_key(field, variables) {
            True -> None
            False -> Some(missing_idempotency_key_error(field))
          }
      }
  }
}

@internal
pub fn inventory_set_202604_contract_error(
  enabled: Bool,
  input: Dict(String, ResolvedValue),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(Json) {
  case enabled {
    False -> None
    True ->
      case
        input_list_has_object_missing_field(
          input,
          "quantities",
          "changeFromQuantity",
        )
      {
        True ->
          Some(inventory_missing_change_from_error(
            field,
            "InventoryQuantityInput",
          ))
        False ->
          case has_idempotency_key(field, variables) {
            True -> None
            False -> Some(missing_idempotency_key_error(field))
          }
      }
  }
}

@internal
pub fn product_set_inventory_levels(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inventory_item: InventoryItemRecord,
  inputs: List(ProductSetInventoryQuantityInput),
) -> #(List(InventoryLevelRecord), SyntheticIdentityRegistry) {
  inputs
  |> group_product_set_quantities_by_location
  |> list.fold(#([], identity), fn(acc, entry) {
    let #(levels, current_identity) = acc
    let #(location_id, location_inputs) = entry
    let existing =
      find_inventory_level(inventory_item.inventory_levels, location_id)
    let base_quantities = case existing {
      Some(level) -> level.quantities
      None -> []
    }
    let #(quantities, next_identity) =
      apply_product_set_level_quantities(
        current_identity,
        base_quantities,
        location_inputs,
      )
    let level =
      InventoryLevelRecord(
        id: existing
          |> option.map(fn(level) { level.id })
          |> option.unwrap(product_set_inventory_level_id(
            inventory_item.id,
            location_id,
          )),
        location: product_set_inventory_location(store, existing, location_id),
        quantities: quantities,
        is_active: Some(True),
        cursor: option.then(existing, fn(level) { level.cursor }),
      )
    #([level, ..levels], next_identity)
  })
  |> fn(result) {
    let #(levels, final_identity) = result
    #(list.reverse(levels), final_identity)
  }
}

@internal
pub fn inventory_quantity_list_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  read_object_list_field(input, "inventoryQuantities")
  |> list.flat_map(fn(quantity_input) {
    let path = ["inventoryQuantities"]
    case read_quantity_field(quantity_input, "availableQuantity") {
      QuantityInt(quantity) -> variant_quantity_range_problems(quantity, path)
      QuantityFloat(_) -> [
        quantity_problem(path, "Inventory quantity must be an integer"),
      ]
      QuantityNotANumber -> [
        quantity_problem(path, "Inventory quantity must be an integer"),
      ]
      QuantityMissing | QuantityNull ->
        case read_quantity_field(quantity_input, "quantity") {
          QuantityInt(quantity) ->
            variant_quantity_range_problems(quantity, path)
          QuantityFloat(_) -> [
            quantity_problem(path, "Inventory quantity must be an integer"),
          ]
          QuantityNotANumber -> [
            quantity_problem(path, "Inventory quantity must be an integer"),
          ]
          QuantityMissing | QuantityNull -> []
        }
    }
  })
}

@internal
pub fn read_variant_inventory_item(
  identity: SyntheticIdentityRegistry,
  input: Option(Dict(String, ResolvedValue)),
  existing: Option(InventoryItemRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry) {
  case input {
    None -> #(existing, identity)
    Some(input) -> {
      let #(id, next_identity) = case existing {
        Some(item) -> #(item.id, identity)
        None -> synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      }
      let current_levels = case existing {
        Some(item) -> item.inventory_levels
        None -> []
      }
      #(
        Some(InventoryItemRecord(
          id: id,
          tracked: read_bool_field(input, "tracked")
            |> option.or(option.then(existing, fn(item) { item.tracked })),
          requires_shipping: read_bool_field(input, "requiresShipping")
            |> option.or(
              option.then(existing, fn(item) { item.requires_shipping }),
            ),
          measurement: read_inventory_measurement_input(
            input,
            option.then(existing, fn(item) { item.measurement }),
          ),
          country_code_of_origin: read_string_field(
            input,
            "countryCodeOfOrigin",
          )
            |> option.or(
              option.then(existing, fn(item) { item.country_code_of_origin }),
            ),
          province_code_of_origin: read_string_field(
            input,
            "provinceCodeOfOrigin",
          )
            |> option.or(
              option.then(existing, fn(item) { item.province_code_of_origin }),
            ),
          harmonized_system_code: read_string_field(
            input,
            "harmonizedSystemCode",
          )
            |> option.or(
              option.then(existing, fn(item) { item.harmonized_system_code }),
            ),
          inventory_levels: current_levels,
        )),
        next_identity,
      )
    }
  }
}

@internal
pub fn validate_inventory_item_update_input(
  input: Dict(String, ResolvedValue),
  existing: InventoryItemRecord,
) -> List(ProductUserError) {
  []
  |> list.append(validate_inventory_item_update_cost(input))
  |> list.append(validate_inventory_item_update_weight(input))
  |> list.append(validate_inventory_item_update_origin(input, existing))
  |> list.append(validate_inventory_item_update_hs_code(input))
  |> list.append(validate_inventory_item_update_country_hs_codes(input))
}

fn validate_inventory_item_update_cost(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  case read_numeric_field(input, "cost") {
    NumericValue(value) if value <. 0.0 -> [
      ProductUserError(
        ["input", "cost"],
        "Cost must be greater than or equal to 0",
        Some("INVALID"),
      ),
    ]
    _ -> []
  }
}

fn validate_inventory_item_update_weight(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  case read_object_field(input, "measurement") {
    Some(measurement) ->
      case read_object_field(measurement, "weight") {
        Some(weight) -> {
          let value_errors = case read_numeric_field(weight, "value") {
            NumericValue(value) if value <. 0.0 -> [
              ProductUserError(
                ["input", "measurement", "weight"],
                "Measurement weight value "
                  <> format_inventory_item_weight_value(value)
                  <> " "
                  <> inventory_item_weight_unit_label(weight)
                  <> " must be >= 0 "
                  <> inventory_item_weight_unit_label(weight),
                Some("GREATER_THAN_OR_EQUAL_TO"),
              ),
            ]
            _ -> []
          }
          let unit_errors = case read_string_field(weight, "unit") {
            Some(unit) ->
              case valid_weight_unit(unit) {
                True -> []
                False -> [
                  ProductUserError(
                    ["input", "measurement", "weight", "unit"],
                    "Weight unit is not included in the list",
                    Some("INVALID_INPUT"),
                  ),
                ]
              }
            None -> []
          }
          list.append(value_errors, unit_errors)
        }
        None -> []
      }
    None -> []
  }
}

fn validate_inventory_item_update_origin(
  input: Dict(String, ResolvedValue),
  existing: InventoryItemRecord,
) -> List(ProductUserError) {
  let country_code =
    read_string_field(input, "countryCodeOfOrigin")
    |> option.or(existing.country_code_of_origin)
  let province_code = read_string_field(input, "provinceCodeOfOrigin")
  let country_errors = case read_string_field(input, "countryCodeOfOrigin") {
    Some(code) ->
      case customer_inputs.country_catalog_by_code(code) {
        Some(_) -> []
        None -> [
          ProductUserError(
            ["input", "countryCodeOfOrigin"],
            "Country code of origin is invalid",
            Some("INVALID"),
          ),
        ]
      }
    None -> []
  }
  let province_errors = case country_errors, country_code, province_code {
    [], Some(code), Some(province) ->
      case customer_inputs.country_catalog_by_code(code) {
        Some(#(_, _, zones)) ->
          case customer_inputs.zone_name_by_code(zones, province) {
            Some(_) -> []
            None -> [
              ProductUserError(
                ["input", "provinceCodeOfOrigin"],
                "Province is invalid",
                Some("INVALID"),
              ),
            ]
          }
        None -> []
      }
    _, _, _ -> []
  }
  list.append(country_errors, province_errors)
}

fn validate_inventory_item_update_hs_code(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  case read_string_field(input, "harmonizedSystemCode") {
    Some(code) ->
      case valid_harmonized_system_code(code) {
        True -> []
        False -> [
          ProductUserError(
            ["input", "harmonizedSystemCode"],
            "Harmonized system code must be a number between six and thirteen digits",
            Some("INVALID"),
          ),
        ]
      }
    None -> []
  }
}

fn validate_inventory_item_update_country_hs_codes(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  let rows = read_object_list_field(input, "countryHarmonizedSystemCodes")
  rows
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(row, index) = pair
    let path = ["input", "countryHarmonizedSystemCodes", int.to_string(index)]
    let country_errors = case read_string_field(row, "countryCode") {
      Some(code) ->
        case customer_inputs.country_catalog_by_code(code) {
          Some(_) -> duplicate_country_hs_code_errors(rows, index, code, path)
          None -> [
            ProductUserError(
              list.append(path, ["countryCode"]),
              "Country is invalid",
              Some("INVALID"),
            ),
          ]
        }
      None -> []
    }
    let hs_errors = case read_string_field(row, "harmonizedSystemCode") {
      Some(code) ->
        case valid_harmonized_system_code(code) {
          True -> []
          False -> [
            ProductUserError(
              list.append(path, ["harmonizedSystemCode"]),
              "Harmonized system code must be a number between six and thirteen digits",
              Some("INVALID"),
            ),
          ]
        }
      None -> []
    }
    list.append(country_errors, hs_errors)
  })
}

fn duplicate_country_hs_code_errors(
  rows: List(Dict(String, ResolvedValue)),
  index: Int,
  country_code: String,
  path: List(String),
) -> List(ProductUserError) {
  let normalized = string.uppercase(country_code)
  let seen_before =
    rows
    |> list.take(index)
    |> list.any(fn(row) {
      read_string_field(row, "countryCode")
      |> option.map(string.uppercase)
      == Some(normalized)
    })
  case seen_before {
    True -> [
      ProductUserError(
        list.append(path, ["countryCode"]),
        "Country code has already been taken",
        Some("TAKEN"),
      ),
    ]
    False -> []
  }
}

fn valid_harmonized_system_code(code: String) -> Bool {
  let normalized = string.trim(code)
  let length = string.length(normalized)
  length >= 6
  && length <= 13
  && list.all(string.to_graphemes(normalized), is_digit)
}

fn inventory_item_weight_unit_label(
  weight: Dict(String, ResolvedValue),
) -> String {
  case read_string_field(weight, "unit") {
    Some("GRAMS") -> "g"
    Some("POUNDS") -> "lb"
    Some("OUNCES") -> "oz"
    _ -> "kg"
  }
}

fn format_inventory_item_weight_value(value: Float) -> String {
  let truncated = float.truncate(value)
  case value == int.to_float(truncated) {
    True -> int.to_string(truncated)
    False -> float.to_string(value)
  }
}

fn is_digit(grapheme: String) -> Bool {
  string.contains("0123456789", grapheme)
}

@internal
pub fn duplicate_inventory_set_quantity_errors(
  quantities: List(InventorySetQuantityInput),
) -> List(ProductUserError) {
  quantities
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(quantity, index) = pair
    case quantity.inventory_item_id, quantity.location_id {
      Some(inventory_item_id), Some(location_id) -> {
        case
          has_duplicate_inventory_item_location_pair(
            quantities,
            index,
            inventory_item_id,
            location_id,
          )
        {
          True -> [
            ProductUserError(
              ["input", "quantities", int.to_string(index), "locationId"],
              "The combination of inventoryItemId and locationId must be unique.",
              Some("NO_DUPLICATE_INVENTORY_ITEM_ID_GROUP_ID_PAIR"),
            ),
          ]
          False -> []
        }
      }
      _, _ -> []
    }
  })
}

@internal
pub fn validate_inventory_move_inputs(
  changes: List(InventoryMoveQuantityInput),
) -> List(ProductUserError) {
  changes
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(change, index) = pair
    validate_inventory_move_input(change, index)
  })
}

@internal
pub fn variant_with_inventory_levels(
  variant: ProductVariantRecord,
  next_levels: List(InventoryLevelRecord),
) -> ProductVariantRecord {
  ProductVariantRecord(
    ..variant,
    inventory_quantity: sum_inventory_level_available(next_levels),
    inventory_item: option.map(variant.inventory_item, fn(item) {
      InventoryItemRecord(..item, inventory_levels: next_levels)
    }),
  )
}

// ===== from inventory_l04 =====
@internal
pub fn inventory_item_source_without_variant(
  item: InventoryItemRecord,
) -> SourceValue {
  inventory_item_source_with_variant(item, SrcNull)
}

@internal
pub fn apply_product_set_inventory_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  variant: ProductVariantRecord,
  input: Dict(String, ResolvedValue),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let quantity_inputs = read_product_set_inventory_quantity_inputs(input)
  case quantity_inputs {
    [] -> #(variant, identity)
    _ -> {
      let #(inventory_item, identity_after_item) =
        ensure_product_set_inventory_item(identity, variant.inventory_item)
      let #(levels, next_identity) =
        product_set_inventory_levels(
          store,
          identity_after_item,
          inventory_item,
          quantity_inputs,
        )
      let available = product_set_available_quantity(quantity_inputs)
      #(
        ProductVariantRecord(
          ..variant,
          inventory_quantity: available |> option.or(variant.inventory_quantity),
          inventory_item: Some(
            InventoryItemRecord(..inventory_item, inventory_levels: levels),
          ),
        ),
        next_identity,
      )
    }
  }
}

@internal
pub fn variant_quantity_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  let direct_errors = case read_quantity_field(input, "inventoryQuantity") {
    QuantityInt(quantity) ->
      variant_quantity_range_problems(quantity, ["inventoryQuantity"])
    QuantityFloat(_) -> [
      quantity_problem(
        ["inventoryQuantity"],
        "Inventory quantity must be an integer",
      ),
    ]
    QuantityNotANumber -> [
      quantity_problem(
        ["inventoryQuantity"],
        "Inventory quantity must be an integer",
      ),
    ]
    QuantityMissing | QuantityNull -> []
  }
  list.append(direct_errors, inventory_quantity_list_problems(input))
}

@internal
pub fn validate_inventory_set_quantity_inputs(
  quantities: List(InventorySetQuantityInput),
) -> List(ProductUserError) {
  let input_errors =
    quantities
    |> enumerate_items()
    |> list.flat_map(fn(pair) {
      let #(quantity, index) = pair
      validate_inventory_set_quantity(quantity, index)
    })
  list.append(input_errors, duplicate_inventory_set_quantity_errors(quantities))
}

@internal
pub fn stage_variant_inventory_levels(
  store: Store,
  variant: ProductVariantRecord,
  next_levels: List(InventoryLevelRecord),
) -> Store {
  let next_variant = variant_with_inventory_levels(variant, next_levels)
  let next_variants =
    store.get_effective_variants_by_product_id(store, variant.product_id)
    |> list.map(fn(candidate) {
      case candidate.id == variant.id {
        True -> next_variant
        False -> candidate
      }
    })
  store.replace_staged_variants_for_product(
    store,
    variant.product_id,
    next_variants,
  )
}
