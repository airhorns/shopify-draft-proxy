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
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_int_field, read_object_list_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  read_non_empty_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type ProductUserError, type RenamedOptionValue, type VariantValidationProblem,
  ProductUserError, VariantValidationProblem, max_product_variants,
  product_set_option_value_limit,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  compare_variant_price, find_product_option, make_created_option_value_records,
}
import shopify_draft_proxy/proxy/products/variants_l01.{
  option_input_display_name, read_option_value_create_names,
}
import shopify_draft_proxy/proxy/products/variants_l02.{
  create_variants_for_all_combinations, create_variants_for_single_new_option,
  duplicate_option_value_input_errors, make_created_option_record,
  option_value_length_problems, option_value_name_length_errors,
  product_set_option_value_records,
  projected_product_options_create_variant_count, read_variant_option_value,
  update_option_values, upsert_option_selection, variant_prices,
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
