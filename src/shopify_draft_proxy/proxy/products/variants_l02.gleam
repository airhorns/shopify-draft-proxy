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
import shopify_draft_proxy/proxy/products/products_l00.{
  parse_price_amount, product_of_counts,
}
import shopify_draft_proxy/proxy/products/products_l01.{
  enumerate_items, format_price_amount, renamed_value_name,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  is_known_missing_shopify_gid, read_int_field, read_object_list_field,
  read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  read_non_empty_string_field, read_numeric_field, user_errors_source,
}
import shopify_draft_proxy/proxy/products/types.{
  type ProductUserError, type ProductVariantPositionInput,
  type RenamedOptionValue, type VariantValidationProblem, NumericMissing,
  NumericNotANumber, NumericNull, NumericValue, ProductUserError,
  ProductVariantPositionInput, VariantValidationProblem, max_variant_price,
  max_variant_text_length, product_option_name_limit, product_set_option_limit,
  product_set_option_value_limit,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  has_variant_id, make_created_option_value_records, option_value_id_exists,
  position_options, product_option_identity_key,
  product_set_variant_signature_title, remaining_option_values,
  variant_selected_options_with_value, variant_title,
}
import shopify_draft_proxy/proxy/products/variants_l01.{
  duplicate_option_input_name_errors_loop,
  duplicate_option_value_input_errors_loop, find_option_value_update,
  find_variant_for_combination, make_variant_for_combination,
  move_variant_to_position, option_input_value_count,
  option_name_exists_excluding, option_value_combinations, product_option_source,
  product_set_option_positions, product_set_variant_signature,
  product_uses_only_default_option_state, read_option_value_names,
  read_variant_sku, take_matching_option, upsert_option_selection_loop,
  variant_for_combination,
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
