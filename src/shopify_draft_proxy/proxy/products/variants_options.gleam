//// Products-domain submodule: variants_options.
//// Combines layered files: variants_l02, variants_l03, variants_l04, variants_l05.

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
import shopify_draft_proxy/proxy/products/inventory_core.{
  clone_default_inventory_item,
}
import shopify_draft_proxy/proxy/products/inventory_validation.{
  read_variant_inventory_item, read_variant_inventory_quantity,
  variant_quantity_problems, variant_weight_problems,
}
import shopify_draft_proxy/proxy/products/products_core.{
  enumerate_items, format_price_amount, parse_price_amount, product_of_counts,
  renamed_value_name,
}
import shopify_draft_proxy/proxy/products/shared.{
  is_known_missing_shopify_gid, read_bool_field, read_int_field,
  read_non_empty_string_field, read_numeric_field, read_object_field,
  read_object_list_field, read_string_field, user_errors_source,
}
import shopify_draft_proxy/proxy/products/types.{
  type BulkVariantUserError, type ProductUserError,
  type ProductVariantPositionInput, type RenamedOptionValue,
  type VariantValidationProblem, BulkVariantUserError, NumericMissing,
  NumericNotANumber, NumericNull, NumericValue, ProductUserError,
  ProductVariantPositionInput, VariantValidationProblem, max_product_variants,
  max_variant_price, max_variant_text_length, product_option_name_limit,
  product_set_option_limit, product_set_option_value_limit,
} as product_types
import shopify_draft_proxy/proxy/products/variants_helpers.{
  bulk_variant_option_field_name, compare_variant_price, find_product_option,
  has_variant_id, has_variant_option_input, make_created_option_value_records,
  make_default_option_record, make_default_variant_record,
  option_value_id_exists, position_options, product_option_identity_key,
  product_set_variant_signature_title, remaining_option_values,
  variant_selected_options_with_value, variant_title,
}
import shopify_draft_proxy/proxy/products/variants_options_core.{
  duplicate_option_input_name_errors_loop,
  duplicate_option_value_input_errors_loop, find_option_value_update,
  find_variant_for_combination, make_default_variant_for_options,
  make_variant_for_combination, move_variant_to_position,
  option_input_display_name, option_input_value_count, option_name_exists,
  option_name_exists_excluding, option_value_combinations, product_option_source,
  product_set_option_positions, product_set_variant_signature,
  product_uses_only_default_option_state, read_option_value_create_names,
  read_option_value_names, read_variant_sku, sort_and_position_options,
  sync_product_options_with_variants, take_matching_option,
  upsert_option_selection_loop, validate_bulk_variant_required_options,
  validate_bulk_variant_selected_options, variant_for_combination,
  variant_title_with_fallback,
}
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

// ===== from variants_l02 =====
@internal
pub fn product_options_source(
  options: List(ProductOptionRecord),
) -> SourceValue {
  SrcList(list.map(options, product_option_source))
}

@internal
pub fn serialize_product_option_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_product_option_by_id(store, id) {
    Some(option) ->
      project_graphql_value(
        product_option_source(option),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

@internal
pub fn product_set_option_limit_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  let options = read_object_list_field(input, "productOptions")
  let option_count_errors = case
    list.length(options) > product_set_option_limit
  {
    True -> [
      ProductOperationUserErrorRecord(
        field: Some(["input", "productOptions"]),
        message: "Options count is over the allowed limit.",
        code: Some("INVALID_INPUT"),
      ),
    ]
    False -> []
  }
  let value_count_errors =
    options
    |> enumerate_items()
    |> list.filter_map(fn(pair) {
      let #(option_input, index) = pair
      let values = read_object_list_field(option_input, "values")
      case list.length(values) > product_set_option_value_limit {
        True ->
          Ok(ProductOperationUserErrorRecord(
            field: Some([
              "input",
              "productOptions",
              int.to_string(index),
              "values",
            ]),
            message: "Option values count is over the allowed limit.",
            code: Some("INVALID_INPUT"),
          ))
        False -> Error(Nil)
      }
    })
  list.append(option_count_errors, value_count_errors)
}

@internal
pub fn product_set_duplicate_variant_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  let variant_inputs = read_object_list_field(input, "variants")
  case variant_inputs {
    [] -> []
    _ -> {
      let positions = product_set_option_positions(input)
      let signatures =
        list.map(variant_inputs, fn(variant_input) {
          product_set_variant_signature(variant_input, positions)
        })
      list.index_map(signatures, fn(signature, index) { #(index, signature) })
      |> list.filter_map(fn(pair) {
        let #(index, signature) = pair
        let earlier = list.take(signatures, index)
        case list.contains(earlier, signature) {
          False -> Error(Nil)
          True ->
            Ok(ProductOperationUserErrorRecord(
              field: Some(["input", "variants", int.to_string(index)]),
              message: "The variant '"
                <> product_set_variant_signature_title(signature)
                <> "' already exists. Please change at least one option value.",
              code: None,
            ))
        }
      })
    }
  }
}

@internal
pub fn product_variant_delete_payload(
  deleted_product_variant_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let deleted_value = case deleted_product_variant_id {
    Some(id) -> SrcString(id)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductVariantDeletePayload")),
      #("deletedProductVariantId", deleted_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_set_option_value_records(
  identity: SyntheticIdentityRegistry,
  existing_values: Option(List(ProductOptionValueRecord)),
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductOptionValueRecord), SyntheticIdentityRegistry, List(String)) {
  let existing_values = existing_values |> option.unwrap([])
  let #(reversed, final_identity, ids) =
    list.fold(inputs, #([], identity, []), fn(acc, input) {
      let #(records, current_identity, collected_ids) = acc
      let existing = case read_string_field(input, "id") {
        Some(id) ->
          existing_values
          |> list.find(fn(value) { value.id == id })
          |> option.from_result
        None -> None
      }
      let #(value_id, next_identity, ids) = case existing {
        Some(value) -> #(value.id, current_identity, [value.id])
        None -> {
          let #(id, next_identity) =
            synthetic_identity.make_synthetic_gid(
              current_identity,
              "ProductOptionValue",
            )
          #(id, next_identity, [id])
        }
      }
      let value =
        ProductOptionValueRecord(
          id: value_id,
          name: read_non_empty_string_field(input, "name")
            |> option.unwrap(
              option.map(existing, fn(value) { value.name })
              |> option.unwrap("Option value"),
            ),
          has_variants: option.map(existing, fn(value) { value.has_variants })
            |> option.unwrap(False),
        )
      #([value, ..records], next_identity, list.append(collected_ids, ids))
    })
  #(list.reverse(reversed), final_identity, ids)
}

@internal
pub fn variant_price_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  case read_numeric_field(input, "price") {
    NumericNull -> [
      VariantValidationProblem(
        "price_blank",
        ["price"],
        ["price"],
        "Price can't be blank",
        Some("INVALID"),
        Some("INVALID"),
      ),
    ]
    NumericValue(value) if value <. 0.0 -> [
      VariantValidationProblem(
        "price_negative",
        ["price"],
        ["price"],
        "Price must be greater than or equal to 0",
        Some("GREATER_THAN_OR_EQUAL_TO"),
        Some("GREATER_THAN_OR_EQUAL_TO"),
      ),
    ]
    NumericValue(value) if value >=. max_variant_price -> [
      VariantValidationProblem(
        "price_too_large",
        ["price"],
        ["price"],
        "Price must be less than 1000000000000000000",
        Some("INVALID_INPUT"),
        Some("INVALID_INPUT"),
      ),
    ]
    NumericNotANumber -> [
      VariantValidationProblem(
        "price_not_a_number",
        ["price"],
        ["price"],
        "Price is not a number",
        Some("NOT_A_NUMBER"),
        Some("NOT_A_NUMBER"),
      ),
    ]
    NumericMissing | NumericValue(_) -> []
  }
}

@internal
pub fn variant_compare_at_price_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  case read_numeric_field(input, "compareAtPrice") {
    NumericValue(value) if value >=. max_variant_price -> [
      VariantValidationProblem(
        "compare_at_price_too_large",
        ["compareAtPrice"],
        ["compareAtPrice"],
        "must be less than 1000000000000000000",
        Some("INVALID_INPUT"),
        Some("INVALID_INPUT"),
      ),
    ]
    NumericNotANumber -> [
      VariantValidationProblem(
        "compare_at_price_not_a_number",
        ["compareAtPrice"],
        ["compareAtPrice"],
        "Compare at price is not a number",
        Some("NOT_A_NUMBER"),
        Some("NOT_A_NUMBER"),
      ),
    ]
    NumericMissing | NumericNull | NumericValue(_) -> []
  }
}

@internal
pub fn variant_text_length_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  let sku_errors = case read_variant_sku(input, None) {
    Some(sku) ->
      case string.length(sku) > max_variant_text_length {
        True -> [
          VariantValidationProblem(
            "sku_too_long",
            ["sku"],
            [],
            "SKU is too long (maximum is 255 characters)",
            Some("INVALID_INPUT"),
            Some("INVALID_INPUT"),
          ),
        ]
        False -> []
      }
    _ -> []
  }
  let barcode_errors = case read_string_field(input, "barcode") {
    Some(barcode) ->
      case string.length(barcode) > max_variant_text_length {
        True -> [
          VariantValidationProblem(
            "barcode_too_long",
            ["barcode"],
            ["barcode"],
            "Barcode is too long (maximum is 255 characters)",
            Some("INVALID_INPUT"),
            Some("INVALID_INPUT"),
          ),
        ]
        False -> []
      }
    _ -> []
  }
  list.append(sku_errors, barcode_errors)
}

@internal
pub fn option_value_length_problems(
  input: Dict(String, ResolvedValue),
  list_field: String,
  value_field: String,
) -> List(VariantValidationProblem) {
  case dict.get(input, list_field) {
    Ok(ListVal(values)) ->
      values
      |> enumerate_items()
      |> list.filter_map(fn(pair) {
        let #(value, index) = pair
        case value {
          ObjectVal(fields) ->
            case read_string_field(fields, value_field) {
              Some(name) ->
                case string.length(name) > max_variant_text_length {
                  True ->
                    Ok(VariantValidationProblem(
                      "option_value_too_long",
                      [list_field, int.to_string(index), value_field],
                      [list_field, int.to_string(index), value_field],
                      "Option value name is too long",
                      Some("INVALID_INPUT"),
                      Some("INVALID_INPUT"),
                    ))
                  False -> Error(Nil)
                }
              _ -> Error(Nil)
            }
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn first_bulk_delete_missing_variant(
  variant_ids: List(String),
  variants: List(ProductVariantRecord),
) -> Option(Int) {
  variant_ids
  |> enumerate_items()
  |> list.find_map(fn(pair) {
    let #(variant_id, index) = pair
    case
      has_variant_id(variants, variant_id)
      && !is_known_missing_shopify_gid(variant_id)
    {
      True -> Error(Nil)
      False ->
        case has_variant_id(variants, variant_id) {
          True -> Error(Nil)
          False -> Ok(index)
        }
    }
  })
  |> option.from_result
}

@internal
pub fn read_product_variant_positions(
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductVariantPositionInput), List(ProductUserError)) {
  case inputs {
    [] -> #([], [
      ProductUserError(["positions"], "At least one position is required", None),
    ])
    _ -> {
      let #(reversed_positions, errors) =
        inputs
        |> enumerate_items()
        |> list.fold(#([], []), fn(acc, pair) {
          let #(positions, errors) = acc
          let #(input, index) = pair
          let path = ["positions", int.to_string(index)]
          let variant_id = graphql_helpers.read_arg_string(input, "id")
          let raw_position = read_int_field(input, "position")
          let id_errors = case variant_id {
            None -> [
              ProductUserError(
                list.append(path, ["id"]),
                "Variant id is required",
                None,
              ),
            ]
            Some(_) -> []
          }
          let position_errors = case raw_position {
            Some(position) if position >= 1 -> []
            _ -> [
              ProductUserError(
                list.append(path, ["position"]),
                "Position is invalid",
                None,
              ),
            ]
          }
          let next_positions = case variant_id, raw_position {
            Some(id), Some(position) if position >= 1 -> [
              ProductVariantPositionInput(id: id, position: position - 1),
              ..positions
            ]
            _, _ -> positions
          }
          #(
            next_positions,
            list.append(errors, list.append(id_errors, position_errors)),
          )
        })
      #(list.reverse(reversed_positions), errors)
    }
  }
}

@internal
pub fn validate_product_variant_positions(
  variants: List(ProductVariantRecord),
  positions: List(ProductVariantPositionInput),
) -> List(ProductUserError) {
  positions
  |> enumerate_items()
  |> list.filter_map(fn(pair) {
    let #(position, index) = pair
    case list.any(variants, fn(variant) { variant.id == position.id }) {
      True -> Error(Nil)
      False ->
        Ok(ProductUserError(
          ["positions", int.to_string(index), "id"],
          "Variant does not exist",
          None,
        ))
    }
  })
}

@internal
pub fn apply_sequential_variant_reorder(
  variants: List(ProductVariantRecord),
  positions: List(ProductVariantPositionInput),
) -> List(ProductVariantRecord) {
  list.fold(positions, variants, fn(current, position) {
    move_variant_to_position(current, position.id, position.position)
  })
}

@internal
pub fn read_variant_option_value(
  fields: Dict(String, ResolvedValue),
) -> Result(ProductVariantSelectedOptionRecord, Nil) {
  case
    read_non_empty_string_field(fields, "optionName"),
    read_non_empty_string_field(fields, "name")
  {
    Some(name), Some(value) ->
      Ok(ProductVariantSelectedOptionRecord(name: name, value: value))
    _, _ -> Error(Nil)
  }
}

@internal
pub fn read_variant_selected_option(
  fields: Dict(String, ResolvedValue),
) -> Result(ProductVariantSelectedOptionRecord, Nil) {
  case
    read_non_empty_string_field(fields, "name"),
    read_non_empty_string_field(fields, "value")
  {
    Some(name), Some(value) ->
      Ok(ProductVariantSelectedOptionRecord(name: name, value: value))
    _, _ -> Error(Nil)
  }
}

@internal
pub fn variant_prices(
  variants: List(ProductVariantRecord),
) -> List(#(Float, String)) {
  variants
  |> list.filter_map(fn(variant) {
    case variant.price {
      Some(price) ->
        case parse_price_amount(price) {
          Ok(value) -> Ok(#(value, format_price_amount(price)))
          Error(_) -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

@internal
pub fn projected_product_options_create_variant_count(
  existing_options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
  inputs: List(Dict(String, ResolvedValue)),
) -> Int {
  case inputs, existing_variants {
    [input], [_, ..] ->
      list.length(existing_variants) * option_input_value_count(input)
    _, _ -> {
      let existing_counts =
        existing_options
        |> list.map(fn(option) { list.length(option.option_values) })
      let input_counts = list.map(inputs, option_input_value_count)
      product_of_counts(list.append(existing_counts, input_counts))
    }
  }
}

@internal
pub fn duplicate_option_input_name_errors(
  inputs: List(Dict(String, ResolvedValue)),
) -> List(ProductUserError) {
  duplicate_option_input_name_errors_loop(inputs, 0, dict.new(), [])
}

@internal
pub fn update_option_name_errors(
  existing_options: List(ProductOptionRecord),
  target_option: ProductOptionRecord,
  option_input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  case read_string_field(option_input, "name") {
    None -> []
    Some(name) -> {
      let trimmed = string.trim(name)
      case
        string.length(trimmed) == 0,
        string.length(name) > product_option_name_limit,
        option_name_exists_excluding(existing_options, name, target_option.id)
      {
        True, _, _ -> [
          ProductUserError(
            ["option", "name"],
            "The name provided is not valid.",
            Some("INVALID_NAME"),
          ),
        ]
        _, True, _ -> [
          ProductUserError(
            ["option", "name"],
            "Option name is too long.",
            Some("OPTION_NAME_TOO_LONG"),
          ),
        ]
        _, _, True -> [
          ProductUserError(
            ["option", "name"],
            "Option already exists.",
            Some("OPTION_ALREADY_EXISTS"),
          ),
        ]
        _, _, False -> []
      }
    }
  }
}

@internal
pub fn option_value_already_exists_errors(
  existing_values: List(ProductOptionValueRecord),
  values_to_add: List(Dict(String, ResolvedValue)),
) -> List(ProductUserError) {
  let existing_keys =
    existing_values
    |> list.map(fn(value) { product_option_identity_key(value.name) })
  values_to_add
  |> enumerate_items()
  |> list.filter_map(fn(pair) {
    let #(input, index) = pair
    case read_string_field(input, "name") {
      Some(name) ->
        case list.contains(existing_keys, product_option_identity_key(name)) {
          True ->
            Ok(ProductUserError(
              ["optionValuesToAdd", int.to_string(index), "name"],
              "Option value already exists.",
              Some("OPTION_VALUE_ALREADY_EXISTS"),
            ))
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
}

@internal
pub fn update_option_result_value_errors(
  target_option: ProductOptionRecord,
  values_to_add: List(Dict(String, ResolvedValue)),
  values_to_update: List(Dict(String, ResolvedValue)),
  value_ids_to_delete: List(String),
) -> List(ProductUserError) {
  let retained_count =
    target_option.option_values
    |> list.filter(fn(value) { !list.contains(value_ids_to_delete, value.id) })
    |> list.length
  let final_count = retained_count + list.length(values_to_add)
  let missing_errors = case final_count == 0 {
    True -> [
      ProductUserError(
        ["optionValuesToDelete"],
        "Each option must have at least one option value specified.",
        Some("OPTION_VALUES_MISSING"),
      ),
    ]
    False -> []
  }
  let limit_errors = case final_count > product_set_option_value_limit {
    True -> [
      ProductUserError(
        ["optionValuesToAdd"],
        "Option values count is over the allowed limit.",
        Some("OPTION_VALUES_OVER_LIMIT"),
      ),
    ]
    False -> []
  }
  let update_id_errors =
    values_to_update
    |> enumerate_items()
    |> list.filter_map(fn(pair) {
      let #(input, index) = pair
      case read_string_field(input, "id") {
        Some(id) ->
          case option_value_id_exists(target_option.option_values, id) {
            True -> Error(Nil)
            False ->
              Ok(ProductUserError(
                ["optionValuesToUpdate", int.to_string(index), "id"],
                "Option value does not exist.",
                Some("OPTION_VALUE_DOES_NOT_EXIST"),
              ))
          }
        None ->
          Ok(ProductUserError(
            ["optionValuesToUpdate", int.to_string(index), "id"],
            "Option value does not exist.",
            Some("OPTION_VALUE_DOES_NOT_EXIST"),
          ))
      }
    })
  list.append(missing_errors, list.append(limit_errors, update_id_errors))
}

@internal
pub fn option_value_name_length_errors(
  values: List(Dict(String, ResolvedValue)),
  path_prefix: List(String),
) -> List(ProductUserError) {
  values
  |> enumerate_items()
  |> list.filter_map(fn(pair) {
    let #(input, index) = pair
    case read_string_field(input, "name") {
      Some(name) ->
        case string.length(name) > product_option_name_limit {
          True ->
            Ok(ProductUserError(
              list.append(path_prefix, [int.to_string(index), "name"]),
              "Option value name is too long.",
              Some("OPTION_VALUE_NAME_TOO_LONG"),
            ))
          False -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
}

@internal
pub fn duplicate_option_value_input_errors(
  values: List(Dict(String, ResolvedValue)),
  path_prefix: List(String),
) -> List(ProductUserError) {
  duplicate_option_value_input_errors_loop(
    values,
    path_prefix,
    0,
    dict.new(),
    [],
  )
}

@internal
pub fn update_option_values(
  values: List(ProductOptionValueRecord),
  updates: List(Dict(String, ResolvedValue)),
) -> #(List(ProductOptionValueRecord), List(RenamedOptionValue)) {
  let #(reversed_values, reversed_renames) =
    list.fold(values, #([], []), fn(acc, value) {
      let #(next_values, renames) = acc
      case find_option_value_update(updates, value.id) {
        Some(name) -> #(
          [ProductOptionValueRecord(..value, name: name), ..next_values],
          [#(value.name, name), ..renames],
        )
        None -> #([value, ..next_values], renames)
      }
    })
  #(list.reverse(reversed_values), list.reverse(reversed_renames))
}

@internal
pub fn remap_variant_selections_for_option_update(
  variants: List(ProductVariantRecord),
  previous_option_name: String,
  next_option_name: String,
  renamed_values: List(RenamedOptionValue),
) -> List(ProductVariantRecord) {
  list.map(variants, fn(variant) {
    let selected_options =
      list.map(variant.selected_options, fn(selected) {
        case selected.name == previous_option_name {
          True ->
            ProductVariantSelectedOptionRecord(
              name: next_option_name,
              value: renamed_value_name(renamed_values, selected.value),
            )
          False -> selected
        }
      })
    ProductVariantRecord(
      ..variant,
      title: variant_title(selected_options),
      selected_options: selected_options,
    )
  })
}

@internal
pub fn reorder_product_options(
  options: List(ProductOptionRecord),
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductOptionRecord), List(ProductUserError)) {
  let #(remaining, reversed_reordered, reversed_errors, _) =
    list.fold(inputs, #(options, [], [], 0), fn(acc, input) {
      let #(current_remaining, reordered, errors, index) = acc
      let #(matched, next_remaining) =
        take_matching_option(current_remaining, input)
      case matched {
        Some(option) -> #(
          next_remaining,
          [option, ..reordered],
          errors,
          index + 1,
        )
        None -> #(
          current_remaining,
          reordered,
          [
            ProductUserError(
              ["options", int.to_string(index)],
              "Option does not exist",
              None,
            ),
            ..errors
          ],
          index + 1,
        )
      }
    })
  let next_options =
    list.append(list.reverse(reversed_reordered), remaining)
    |> position_options(1, [])
  #(next_options, list.reverse(reversed_errors))
}

@internal
pub fn make_created_option_record(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  input: Dict(String, ResolvedValue),
) -> #(ProductOptionRecord, SyntheticIdentityRegistry) {
  let #(id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "ProductOption")
  let #(values, final_identity) =
    make_created_option_value_records(
      identity_after_id,
      read_option_value_names(input),
    )
  #(
    ProductOptionRecord(
      id: id,
      product_id: product_id,
      name: read_string_field(input, "name") |> option.unwrap(""),
      position: read_int_field(input, "position") |> option.unwrap(9999),
      option_values: values,
    ),
    final_identity,
  )
}

@internal
pub fn product_has_standalone_default_variant(
  options: List(ProductOptionRecord),
  variants: List(ProductVariantRecord),
) -> Bool {
  case variants {
    [variant] ->
      product_uses_only_default_option_state(options, variants)
      && variant.title == "Default Title"
    _ -> False
  }
}

@internal
pub fn upsert_option_selection(
  options: List(ProductOptionRecord),
  identity: SyntheticIdentityRegistry,
  product_id: String,
  selected: ProductVariantSelectedOptionRecord,
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry) {
  let #(updated, found, next_identity) =
    upsert_option_selection_loop(options, identity, product_id, selected, [])
  case found {
    True -> #(updated, next_identity)
    False -> {
      let #(option_id, identity_after_option) =
        synthetic_identity.make_synthetic_gid(identity, "ProductOption")
      let #(value_id, identity_after_value) =
        synthetic_identity.make_synthetic_gid(
          identity_after_option,
          "ProductOptionValue",
        )
      #(
        list.append(options, [
          ProductOptionRecord(
            id: option_id,
            product_id: product_id,
            name: selected.name,
            position: list.length(options) + 1,
            option_values: [
              ProductOptionValueRecord(
                id: value_id,
                name: selected.value,
                has_variants: True,
              ),
            ],
          ),
        ]),
        identity_after_value,
      )
    }
  }
}

@internal
pub fn create_variants_for_single_new_option(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  created_option: ProductOptionRecord,
  existing_variants: List(ProductVariantRecord),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry) {
  let remaining_values = remaining_option_values(created_option)
  let #(new_variants, final_identity) =
    list.fold(existing_variants, #([], identity), fn(acc, existing_variant) {
      let #(records, current_identity) = acc
      let #(created, next_identity) =
        list.fold(
          remaining_values,
          #([], current_identity),
          fn(value_acc, value) {
            let #(value_records, value_identity) = value_acc
            let combination =
              variant_selected_options_with_value(
                existing_variant.selected_options,
                created_option.name,
                value.name,
              )
            let #(variant, next_value_identity) =
              make_variant_for_combination(
                value_identity,
                product_id,
                combination,
                Some(existing_variant),
              )
            #(list.append(value_records, [variant]), next_value_identity)
          },
        )
      #(list.append(records, created), next_identity)
    })
  #(list.append(existing_variants, new_variants), final_identity)
}

@internal
pub fn create_variants_for_all_combinations(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry) {
  let combinations = option_value_combinations(options)
  let #(reversed, final_identity) =
    list.fold(combinations, #([], identity), fn(acc, combination) {
      let #(records, current_identity) = acc
      case find_variant_for_combination(existing_variants, combination) {
        Some(variant) -> #(
          [variant_for_combination(variant, combination), ..records],
          current_identity,
        )
        None -> {
          let template = case existing_variants {
            [first, ..] -> Some(first)
            [] -> None
          }
          let #(variant, next_identity) =
            make_variant_for_combination(
              current_identity,
              product_id,
              combination,
              template,
            )
          #([variant, ..records], next_identity)
        }
      }
    })
  #(list.reverse(reversed), final_identity)
}

// ===== from variants_l03 =====
@internal
pub fn product_set_option_records(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry, List(String)) {
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let #(reversed, final_identity, ids) =
    inputs
    |> enumerate_items()
    |> list.fold(#([], identity, []), fn(acc, pair) {
      let #(records, current_identity, collected_ids) = acc
      let #(input, index) = pair
      let existing = case read_string_field(input, "id") {
        Some(id) -> find_product_option(existing_options, product_id, id)
        None -> None
      }
      let #(option_id, identity_after_option, option_ids) = case existing {
        Some(option) -> #(option.id, current_identity, [option.id])
        None -> {
          let #(id, next_identity) =
            synthetic_identity.make_synthetic_gid(
              current_identity,
              "ProductOption",
            )
          #(id, next_identity, [id])
        }
      }
      let #(values, next_identity, value_ids) =
        product_set_option_value_records(
          identity_after_option,
          option.map(existing, fn(option) { option.option_values }),
          read_object_list_field(input, "values"),
        )
      let option_record =
        ProductOptionRecord(
          id: option_id,
          product_id: product_id,
          name: read_non_empty_string_field(input, "name")
            |> option.unwrap(
              option.map(existing, fn(option) { option.name })
              |> option.unwrap(""),
            ),
          position: read_int_field(input, "position")
            |> option.unwrap(index + 1),
          option_values: values,
        )
      #(
        [option_record, ..records],
        next_identity,
        list.append(collected_ids, list.append(option_ids, value_ids)),
      )
    })
  #(list.reverse(reversed), final_identity, ids)
}

@internal
pub fn variant_option_value_length_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  list.append(
    option_value_length_problems(input, "optionValues", "name"),
    option_value_length_problems(input, "selectedOptions", "value"),
  )
}

@internal
pub fn read_variant_option_values(
  input: Dict(String, ResolvedValue),
  fallback: List(ProductVariantSelectedOptionRecord),
) -> List(ProductVariantSelectedOptionRecord) {
  case dict.get(input, "optionValues") {
    Ok(ListVal(values)) -> {
      let selected =
        list.filter_map(values, fn(value) {
          case value {
            ObjectVal(fields) -> read_variant_option_value(fields)
            _ -> Error(Nil)
          }
        })
      case selected {
        [] -> fallback
        _ -> selected
      }
    }
    _ -> fallback
  }
}

@internal
pub fn min_variant_price_amount(
  variants: List(ProductVariantRecord),
) -> Option(String) {
  variant_prices(variants)
  |> list.sort(compare_variant_price)
  |> list.first
  |> result.map(fn(price) { price.1 })
  |> option.from_result
}

@internal
pub fn max_variant_price_amount(
  variants: List(ProductVariantRecord),
) -> Option(String) {
  variant_prices(variants)
  |> list.sort(compare_variant_price)
  |> list.last
  |> result.map(fn(price) { price.1 })
  |> option.from_result
}

@internal
pub fn create_option_value_errors(
  input: Dict(String, ResolvedValue),
  index: Int,
  existing_variants: List(ProductVariantRecord),
  replacing_default: Bool,
) -> List(ProductUserError) {
  let values = read_object_list_field(input, "values")
  let requires_existing_variant_values =
    !replacing_default && !list.is_empty(existing_variants)
  let value_presence_errors = case
    dict.has_key(input, "values"),
    values,
    requires_existing_variant_values
  {
    False, _, True -> [
      ProductUserError(
        ["options", int.to_string(index), "values"],
        "New option must have at least one value for existing variants.",
        Some("NEW_OPTION_WITHOUT_VALUE_FOR_EXISTING_VARIANTS"),
      ),
    ]
    _, [], _ -> [
      ProductUserError(
        ["options", int.to_string(index)],
        "Option '"
          <> option_input_display_name(input)
          <> "' must specify at least one option value.",
        Some("OPTION_VALUES_MISSING"),
      ),
    ]
    _, _, _ -> []
  }
  let value_limit_errors = case
    list.length(values) > product_set_option_value_limit
  {
    True -> [
      ProductUserError(
        ["options", int.to_string(index), "values"],
        "Option values count is over the allowed limit.",
        Some("OPTION_VALUES_OVER_LIMIT"),
      ),
    ]
    False -> []
  }
  list.append(
    value_presence_errors,
    list.append(
      value_limit_errors,
      list.append(
        option_value_name_length_errors(values, [
          "options",
          int.to_string(index),
          "values",
        ]),
        duplicate_option_value_input_errors(values, [
          "options",
          int.to_string(index),
          "values",
        ]),
      ),
    ),
  )
}

@internal
pub fn create_variant_strategy_errors(
  existing_options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
  inputs: List(Dict(String, ResolvedValue)),
  should_create_option_variants: Bool,
) -> List(ProductUserError) {
  case should_create_option_variants {
    False -> []
    True -> {
      let projected_count =
        projected_product_options_create_variant_count(
          existing_options,
          existing_variants,
          inputs,
        )
      case projected_count > max_product_variants {
        True -> [
          ProductUserError(
            ["options"],
            "The number of created variants would exceed the "
              <> int.to_string(max_product_variants)
              <> " variants per product limit",
            Some("TOO_MANY_VARIANTS_CREATED"),
          ),
        ]
        False -> []
      }
    }
  }
}

@internal
pub fn update_option_value_input_errors(
  values: List(Dict(String, ResolvedValue)),
  argument_name: String,
) -> List(ProductUserError) {
  list.append(
    option_value_name_length_errors(values, [argument_name]),
    duplicate_option_value_input_errors(values, [argument_name]),
  )
}

@internal
pub fn update_product_option_record(
  identity: SyntheticIdentityRegistry,
  option: ProductOptionRecord,
  input: Dict(String, ResolvedValue),
  values_to_add: List(Dict(String, ResolvedValue)),
  values_to_update: List(Dict(String, ResolvedValue)),
  value_ids_to_delete: List(String),
) -> #(
  ProductOptionRecord,
  List(RenamedOptionValue),
  SyntheticIdentityRegistry,
  List(String),
) {
  let next_name =
    read_string_field(input, "name")
    |> option.unwrap(option.name)
  let #(updated_values, renamed_values) =
    option.option_values
    |> list.filter(fn(value) { !list.contains(value_ids_to_delete, value.id) })
    |> update_option_values(values_to_update)
  let #(created_values, final_identity) =
    make_created_option_value_records(
      identity,
      read_option_value_create_names(values_to_add),
    )
  let next_option =
    ProductOptionRecord(
      ..option,
      name: next_name,
      option_values: list.append(updated_values, created_values),
    )
  #(
    next_option,
    renamed_values,
    final_identity,
    list.map(created_values, fn(value) { value.id }),
  )
}

@internal
pub fn make_created_option_records(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry) {
  let #(reversed, final_identity) =
    list.fold(inputs, #([], identity), fn(acc, input) {
      let #(records, current_identity) = acc
      let #(option, next_identity) =
        make_created_option_record(current_identity, product_id, input)
      #([option, ..records], next_identity)
    })
  #(list.reverse(reversed), final_identity)
}

@internal
pub fn upsert_variant_selections_into_options(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  options: List(ProductOptionRecord),
  variants: List(ProductVariantRecord),
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry) {
  variants
  |> list.flat_map(fn(variant) { variant.selected_options })
  |> list.fold(#(options, identity), fn(acc, selected) {
    let #(current_options, current_identity) = acc
    upsert_option_selection(
      current_options,
      current_identity,
      product_id,
      selected,
    )
  })
}

@internal
pub fn create_variants_for_option_value_combinations(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  options: List(ProductOptionRecord),
  created_options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry) {
  case created_options, existing_variants {
    [created_option], [_, ..] ->
      create_variants_for_single_new_option(
        identity,
        product_id,
        created_option,
        existing_variants,
      )
    _, _ ->
      create_variants_for_all_combinations(
        identity,
        product_id,
        options,
        existing_variants,
      )
  }
}

// ===== from variants_l04 =====
@internal
pub fn make_product_create_option_graph(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
  option_inputs: List(Dict(String, ResolvedValue)),
) -> #(
  List(ProductOptionRecord),
  ProductVariantRecord,
  SyntheticIdentityRegistry,
  List(String),
) {
  case option_inputs {
    [] -> {
      let #(default_option, identity_after_option, option_ids) =
        make_default_option_record(identity, product)
      let #(default_variant, final_identity, variant_ids) =
        make_default_variant_record(identity_after_option, product)
      #(
        [default_option],
        default_variant,
        final_identity,
        list.append(option_ids, variant_ids),
      )
    }
    _ -> {
      let #(options, identity_after_options) =
        make_created_option_records(identity, product.id, option_inputs)
      let positioned_options = sort_and_position_options(options)
      let #(default_variant, final_identity, variant_ids) =
        make_default_variant_for_options(
          identity_after_options,
          product,
          positioned_options,
        )
      let synced_options =
        sync_product_options_with_variants(positioned_options, [default_variant])
      let option_ids =
        list.append(
          list.map(synced_options, fn(option) { option.id }),
          list.flat_map(synced_options, fn(option) {
            list.map(option.option_values, fn(value) { value.id })
          }),
        )
      #(
        synced_options,
        default_variant,
        final_identity,
        list.append(option_ids, variant_ids),
      )
    }
  }
}

@internal
pub fn read_variant_selected_options(
  input: Dict(String, ResolvedValue),
  fallback: List(ProductVariantSelectedOptionRecord),
) -> List(ProductVariantSelectedOptionRecord) {
  case dict.get(input, "selectedOptions") {
    Ok(ListVal(values)) -> {
      let selected =
        list.filter_map(values, fn(value) {
          case value {
            ObjectVal(fields) -> read_variant_selected_option(fields)
            _ -> Error(Nil)
          }
        })
      case selected {
        [] -> fallback
        _ -> selected
      }
    }
    _ -> read_variant_option_values(input, fallback)
  }
}

@internal
pub fn create_single_option_input_errors(
  input: Dict(String, ResolvedValue),
  index: Int,
  existing_options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
  replacing_default: Bool,
) -> List(ProductUserError) {
  let name_errors = case read_string_field(input, "name") {
    None -> [
      ProductUserError(
        ["options", int.to_string(index), "name"],
        "Each option must have a name specified.",
        Some("OPTION_NAME_MISSING"),
      ),
    ]
    Some(name) -> {
      let trimmed = string.trim(name)
      case
        string.length(trimmed) == 0,
        string.length(name) > product_option_name_limit,
        option_name_exists(existing_options, name)
      {
        True, _, _ -> [
          ProductUserError(
            ["options", int.to_string(index), "name"],
            "Each option must have a name specified.",
            Some("OPTION_NAME_MISSING"),
          ),
        ]
        _, True, _ -> [
          ProductUserError(
            ["options", int.to_string(index)],
            "Option name is too long.",
            Some("OPTION_NAME_TOO_LONG"),
          ),
        ]
        _, _, True -> [
          ProductUserError(
            ["options", int.to_string(index)],
            "Option '" <> name <> "' already exists.",
            Some("OPTION_ALREADY_EXISTS"),
          ),
        ]
        _, _, False -> []
      }
    }
  }
  let value_errors =
    create_option_value_errors(
      input,
      index,
      existing_variants,
      replacing_default,
    )
  list.append(name_errors, value_errors)
}

@internal
pub fn validate_product_option_update_inputs(
  existing_options: List(ProductOptionRecord),
  target_option: ProductOptionRecord,
  option_input: Dict(String, ResolvedValue),
  values_to_add: List(Dict(String, ResolvedValue)),
  values_to_update: List(Dict(String, ResolvedValue)),
  value_ids_to_delete: List(String),
) -> List(ProductUserError) {
  list.append(
    update_option_name_errors(existing_options, target_option, option_input),
    list.append(
      update_option_value_input_errors(values_to_add, "optionValuesToAdd"),
      list.append(
        option_value_already_exists_errors(
          target_option.option_values,
          values_to_add,
        ),
        list.append(
          update_option_value_input_errors(
            values_to_update,
            "optionValuesToUpdate",
          ),
          update_option_result_value_errors(
            target_option,
            values_to_add,
            values_to_update,
            value_ids_to_delete,
          ),
        ),
      ),
    ),
  )
}

@internal
pub fn make_options_from_variant_selections(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  variants: List(ProductVariantRecord),
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry) {
  let #(options, next_identity) =
    upsert_variant_selections_into_options(identity, product_id, [], variants)
  #(sync_product_options_with_variants(options, variants), next_identity)
}

// ===== from variants_l05 =====
@internal
pub fn make_created_variant_record(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  input: Dict(String, ResolvedValue),
  defaults: Option(ProductVariantRecord),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let selected_options = read_variant_selected_options(input, [])
  let #(variant_id, identity_after_variant) =
    synthetic_identity.make_synthetic_gid(identity, "ProductVariant")
  let #(inventory_item, final_identity) = case
    read_object_field(input, "inventoryItem")
  {
    Some(inventory_item_input) ->
      read_variant_inventory_item(
        identity_after_variant,
        Some(inventory_item_input),
        None,
      )
    None ->
      clone_default_inventory_item(
        identity_after_variant,
        option.then(defaults, fn(variant) { variant.inventory_item }),
      )
  }
  #(
    ProductVariantRecord(
      id: variant_id,
      product_id: product_id,
      title: read_non_empty_string_field(input, "title")
        |> option.unwrap(variant_title_with_fallback(
          selected_options,
          "Default Title",
        )),
      sku: read_variant_sku(input, None),
      barcode: read_string_field(input, "barcode"),
      price: read_string_field(input, "price")
        |> option.or(option.then(defaults, fn(variant) { variant.price })),
      compare_at_price: read_string_field(input, "compareAtPrice")
        |> option.or(
          option.then(defaults, fn(variant) { variant.compare_at_price }),
        ),
      taxable: read_bool_field(input, "taxable")
        |> option.or(option.then(defaults, fn(variant) { variant.taxable })),
      inventory_policy: read_string_field(input, "inventoryPolicy")
        |> option.or(
          option.then(defaults, fn(variant) { variant.inventory_policy }),
        ),
      inventory_quantity: read_variant_inventory_quantity(input, Some(0)),
      selected_options: selected_options,
      media_ids: [],
      inventory_item: inventory_item,
      contextual_pricing: None,
      cursor: None,
    ),
    final_identity,
  )
}

@internal
pub fn variant_validation_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  []
  |> list.append(variant_price_problems(input))
  |> list.append(variant_compare_at_price_problems(input))
  |> list.append(variant_weight_problems(input))
  |> list.append(variant_quantity_problems(input))
  |> list.append(variant_text_length_problems(input))
  |> list.append(variant_option_value_length_problems(input))
}

@internal
pub fn validate_bulk_variant_option_input(
  store: Store,
  product_id: String,
  input: Dict(String, ResolvedValue),
  variant_index: Int,
  mode: String,
) -> #(List(ProductVariantSelectedOptionRecord), List(BulkVariantUserError)) {
  let selected_options = read_variant_selected_options(input, [])
  let product_options =
    store.get_effective_options_by_product_id(store, product_id)
  let option_field_name = bulk_variant_option_field_name(input)
  let user_errors =
    validate_bulk_variant_selected_options(
      selected_options,
      product_options,
      dict.new(),
      variant_index,
      0,
      option_field_name,
      mode,
    )
  let user_errors = case user_errors {
    [] ->
      validate_bulk_variant_required_options(
        selected_options,
        product_options,
        variant_index,
        mode,
        has_variant_option_input(input),
      )
    _ -> user_errors
  }
  #(selected_options, user_errors)
}

@internal
pub fn update_variant_record(
  identity: SyntheticIdentityRegistry,
  existing: ProductVariantRecord,
  input: Dict(String, ResolvedValue),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let selected_options =
    read_variant_selected_options(input, existing.selected_options)
  let #(inventory_item, next_identity) =
    read_variant_inventory_item(
      identity,
      read_object_field(input, "inventoryItem"),
      existing.inventory_item,
    )
  #(
    ProductVariantRecord(
      ..existing,
      title: read_non_empty_string_field(input, "title")
        |> option.unwrap(variant_title_with_fallback(
          selected_options,
          existing.title,
        )),
      sku: read_variant_sku(input, existing.sku),
      barcode: read_string_field(input, "barcode")
        |> option.or(existing.barcode),
      price: read_string_field(input, "price") |> option.or(existing.price),
      compare_at_price: read_string_field(input, "compareAtPrice")
        |> option.or(existing.compare_at_price),
      taxable: read_bool_field(input, "taxable") |> option.or(existing.taxable),
      inventory_policy: read_string_field(input, "inventoryPolicy")
        |> option.or(existing.inventory_policy),
      inventory_quantity: read_variant_inventory_quantity(
        input,
        existing.inventory_quantity,
      ),
      selected_options: selected_options,
      inventory_item: inventory_item,
    ),
    next_identity,
  )
}

@internal
pub fn create_option_input_errors(
  existing_options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
  inputs: List(Dict(String, ResolvedValue)),
  replacing_default: Bool,
) -> List(ProductUserError) {
  let field_errors =
    inputs
    |> enumerate_items()
    |> list.flat_map(fn(pair) {
      let #(input, index) = pair
      create_single_option_input_errors(
        input,
        index,
        existing_options,
        existing_variants,
        replacing_default,
      )
    })
  list.append(field_errors, duplicate_option_input_name_errors(inputs))
}
