//// Products-domain submodule: variants_options_core.
//// Combines layered files: variants_l01.

import gleam/dict.{type Dict}

import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection}

import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, ListVal, ObjectVal,
}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcInt, SrcList, SrcNull, SrcString,
  project_graphql_value, src_object,
}

import shopify_draft_proxy/proxy/products/inventory_core.{
  default_inventory_item_measurement, duplicate_inventory_item,
  make_inventory_item_for_variant,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type BulkVariantUserError, type ProductUserError, type VariantCombination,
  type VariantValidationProblem, BulkVariantUserError, ProductUserError,
  VariantValidationProblem, product_set_variant_limit,
}
import shopify_draft_proxy/proxy/products/shared.{
  captured_json_source, max_input_size_error, read_int_field, read_object_field,
  read_object_list_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/variants_helpers.{
  duplicate_product_option_values, find_selected_option,
  first_missing_selected_product_option, first_option_value_name,
  has_staged_variants_for_product, insert_variant_at_position,
  option_values_are_default, position_options,
  product_option_duplicate_input_key, product_option_linked_metafield_source,
  product_option_named, product_option_value_source, remove_variant_by_id,
  selected_options_equal, take_matching_option_loop, upsert_option_value,
  variant_title, variants_use_option_value,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type ProductOperationUserErrorRecord,
  type ProductOptionRecord, type ProductRecord, type ProductVariantRecord,
  type ProductVariantSelectedOptionRecord, InventoryItemRecord,
  ProductOperationUserErrorRecord, ProductOptionRecord, ProductOptionValueRecord,
  ProductVariantRecord, ProductVariantSelectedOptionRecord,
}

// ===== from variants_l01 =====
@internal
pub fn product_searchable_variants(
  store: Store,
  product_id: String,
) -> List(ProductVariantRecord) {
  let base_variants = store.get_base_variants_by_product_id(store, product_id)
  let effective_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  case has_staged_variants_for_product(store, product_id), base_variants {
    False, _ -> effective_variants
    True, [] -> effective_variants
    True, _ ->
      case list.length(effective_variants) > list.length(base_variants) {
        True -> base_variants
        False -> []
      }
  }
}

@internal
pub fn product_option_source(option: ProductOptionRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductOption")),
    #("id", SrcString(option.id)),
    #("name", SrcString(option.name)),
    #("position", SrcInt(option.position)),
    #("linkedMetafield", case option.linked_metafield {
      Some(linked_metafield) ->
        product_option_linked_metafield_source(linked_metafield)
      None -> SrcNull
    }),
    #(
      "values",
      SrcList(
        option.option_values
        |> list.filter(fn(value) { value.has_variants })
        |> list.map(fn(value) { SrcString(value.name) }),
      ),
    ),
    #(
      "optionValues",
      SrcList(list.map(option.option_values, product_option_value_source)),
    ),
  ])
}

@internal
pub fn serialize_product_option_value_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_product_option_value_by_id(store, id) {
    Some(option_value) ->
      project_graphql_value(
        product_option_value_source(option_value),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

@internal
pub fn optional_captured_json_source(
  value: Option(CapturedJsonValue),
) -> SourceValue {
  case value {
    Some(value) -> captured_json_source(value)
    None -> SrcNull
  }
}

@internal
pub fn product_set_variant_limit_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  let variants = read_object_list_field(input, "variants")
  case list.length(variants) > product_set_variant_limit {
    True -> [
      ProductOperationUserErrorRecord(
        field: Some(["input", "variants"]),
        message: "Product can have a maximum of 2048 variants.",
        code: Some("EXCEEDED_VARIANT_LIMIT"),
      ),
    ]
    False -> []
  }
}

@internal
pub fn product_set_variant_max_input_size_errors(
  input: Dict(String, ResolvedValue),
) -> List(Json) {
  let variants = read_object_list_field(input, "variants")
  case list.length(variants) > product_set_variant_limit {
    True -> [
      max_input_size_error(list.length(variants), product_set_variant_limit, [
        "productSet",
        "input",
        "variants",
      ]),
    ]
    False -> []
  }
}

@internal
pub fn product_set_requires_variants_for_options_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  case
    read_object_list_field(input, "productOptions"),
    read_object_list_field(input, "variants")
  {
    [_, ..], [] -> [
      ProductOperationUserErrorRecord(
        field: Some(["input", "variants"]),
        message: "Variants input is required when updating product options",
        code: None,
      ),
    ]
    _, _ -> []
  }
}

@internal
pub fn product_set_option_positions(
  input: Dict(String, ResolvedValue),
) -> Dict(String, Int) {
  read_object_list_field(input, "productOptions")
  |> list.index_map(fn(option_input, index) { #(option_input, index) })
  |> list.fold(dict.new(), fn(acc, pair) {
    let #(option_input, index) = pair
    case read_string_field(option_input, "name") {
      None -> acc
      Some(name) -> {
        let position =
          read_int_field(option_input, "position")
          |> option.unwrap(index + 1)
        dict.insert(acc, name, position)
      }
    }
  })
}

@internal
pub fn product_set_variant_signature(
  variant_input: Dict(String, ResolvedValue),
  positions: Dict(String, Int),
) -> List(#(Int, String, String)) {
  read_object_list_field(variant_input, "optionValues")
  |> list.filter_map(fn(option_value) {
    case
      read_string_field(option_value, "optionName"),
      read_string_field(option_value, "name")
    {
      Some(option_name), Some(value) -> {
        let position = dict.get(positions, option_name) |> result.unwrap(9999)
        Ok(#(position, option_name, value))
      }
      _, _ -> Error(Nil)
    }
  })
  |> list.sort(fn(a, b) {
    let #(pos_a, _, _) = a
    let #(pos_b, _, _) = b
    int.compare(pos_a, pos_b)
  })
}

@internal
pub fn bulk_variant_user_errors_source(
  errors: List(BulkVariantUserError),
) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let BulkVariantUserError(field: field, message: message, code: code) =
        error
      let field_value = case field {
        Some(field) -> SrcList(list.map(field, SrcString))
        None -> SrcNull
      }
      src_object([
        #("field", field_value),
        #("message", SrcString(message)),
        #("code", graphql_helpers.option_string_source(code)),
      ])
    }),
  )
}

@internal
pub fn product_set_variant_defaults(
  variant: ProductVariantRecord,
) -> ProductVariantRecord {
  let inventory_item = case variant.inventory_item {
    Some(item) ->
      Some(
        InventoryItemRecord(
          ..item,
          measurement: item.measurement
            |> option.or(Some(default_inventory_item_measurement())),
        ),
      )
    None -> None
  }
  ProductVariantRecord(
    ..variant,
    taxable: variant.taxable |> option.or(Some(True)),
    inventory_policy: variant.inventory_policy |> option.or(Some("DENY")),
    inventory_item: inventory_item,
  )
}

@internal
pub fn duplicate_product_options(
  identity: SyntheticIdentityRegistry,
  duplicate_product_id: String,
  options: List(ProductOptionRecord),
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry, List(String)) {
  let #(reversed, next_identity, ids) =
    list.fold(options, #([], identity, []), fn(acc, option_record) {
      let #(collected, current_identity, collected_ids) = acc
      let #(option_id, identity_after_option) =
        synthetic_identity.make_synthetic_gid(current_identity, "ProductOption")
      let #(values, identity_after_values, value_ids) =
        duplicate_product_option_values(
          identity_after_option,
          option_record.option_values,
        )
      #(
        [
          ProductOptionRecord(
            ..option_record,
            id: option_id,
            product_id: duplicate_product_id,
            option_values: values,
          ),
          ..collected
        ],
        identity_after_values,
        list.append(collected_ids, [option_id, ..value_ids]),
      )
    })
  #(list.reverse(reversed), next_identity, ids)
}

@internal
pub fn duplicate_product_variants(
  identity: SyntheticIdentityRegistry,
  duplicate_product_id: String,
  variants: List(ProductVariantRecord),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry, List(String)) {
  let #(reversed, next_identity, ids) =
    list.fold(variants, #([], identity, []), fn(acc, variant) {
      let #(collected, current_identity, collected_ids) = acc
      let #(variant_id, identity_after_variant) =
        synthetic_identity.make_synthetic_gid(
          current_identity,
          "ProductVariant",
        )
      let #(inventory_item, identity_after_inventory, inventory_ids) =
        duplicate_inventory_item(identity_after_variant, variant.inventory_item)
      #(
        [
          ProductVariantRecord(
            ..variant,
            id: variant_id,
            product_id: duplicate_product_id,
            inventory_item: inventory_item,
          ),
          ..collected
        ],
        identity_after_inventory,
        list.append(collected_ids, [variant_id, ..inventory_ids]),
      )
    })
  #(list.reverse(reversed), next_identity, ids)
}

@internal
pub fn make_default_variant_for_options(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
  options: List(ProductOptionRecord),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry, List(String)) {
  let selected_options =
    list.map(options, fn(option) {
      ProductVariantSelectedOptionRecord(
        name: option.name,
        value: first_option_value_name(option),
      )
    })
  let #(variant_id, identity_after_variant) =
    synthetic_identity.make_synthetic_gid(identity, "ProductVariant")
  let #(inventory_item_id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_variant,
      "InventoryItem",
    )
  #(
    ProductVariantRecord(
      id: variant_id,
      product_id: product.id,
      title: variant_title(selected_options),
      sku: None,
      barcode: None,
      price: Some("0.00"),
      compare_at_price: None,
      taxable: None,
      inventory_policy: None,
      inventory_quantity: Some(0),
      selected_options: selected_options,
      media_ids: [],
      inventory_item: Some(
        InventoryItemRecord(
          id: inventory_item_id,
          tracked: Some(False),
          requires_shipping: Some(True),
          measurement: None,
          country_code_of_origin: None,
          province_code_of_origin: None,
          harmonized_system_code: None,
          inventory_levels: [],
        ),
      ),
      contextual_pricing: None,
      cursor: None,
    ),
    next_identity,
    [variant_id, inventory_item_id],
  )
}

@internal
pub fn bulk_variant_error_from_problem(
  problem: VariantValidationProblem,
  variant_index: Int,
) -> List(BulkVariantUserError) {
  let VariantValidationProblem(
    kind: kind,
    bulk_suffix: suffix,
    message: message,
    bulk_code: code,
    ..,
  ) = problem
  let field = Some(["variants", int.to_string(variant_index), ..suffix])
  let primary = BulkVariantUserError(field, message, code)
  case kind {
    "sku_too_long" -> [
      primary,
      BulkVariantUserError(
        Some(["variants", int.to_string(variant_index)]),
        "is too long (maximum is 255 characters)",
        None,
      ),
    ]
    _ -> [primary]
  }
}

@internal
pub fn validate_bulk_variant_selected_options(
  selected_options: List(ProductVariantSelectedOptionRecord),
  product_options: List(ProductOptionRecord),
  seen_names: Dict(String, Bool),
  variant_index: Int,
  option_index: Int,
  option_field_name: String,
  mode: String,
) -> List(BulkVariantUserError) {
  case selected_options {
    [] -> []
    [selected, ..rest] ->
      case dict.has_key(seen_names, selected.name) {
        True -> [
          BulkVariantUserError(
            Some(["variants", int.to_string(variant_index), option_field_name]),
            "Duplicated option name '" <> selected.name <> "'",
            Some("INVALID_INPUT"),
          ),
        ]
        False ->
          case
            !list.is_empty(product_options)
            && product_option_named(product_options, selected.name) == None
          {
            True -> [
              BulkVariantUserError(
                Some([
                  "variants",
                  int.to_string(variant_index),
                  option_field_name,
                  int.to_string(option_index),
                ]),
                "Option does not exist",
                Some(case mode {
                  "update" -> "OPTION_DOES_NOT_EXIST"
                  _ -> "INVALID_INPUT"
                }),
              ),
            ]
            False ->
              validate_bulk_variant_selected_options(
                rest,
                product_options,
                dict.insert(seen_names, selected.name, True),
                variant_index,
                option_index + 1,
                option_field_name,
                mode,
              )
          }
      }
  }
}

@internal
pub fn validate_bulk_variant_required_options(
  selected_options: List(ProductVariantSelectedOptionRecord),
  product_options: List(ProductOptionRecord),
  variant_index: Int,
  mode: String,
  has_option_input: Bool,
) -> List(BulkVariantUserError) {
  let should_require = mode == "create" || has_option_input
  case
    should_require,
    list.is_empty(product_options),
    list.is_empty(selected_options)
  {
    True, False, False ->
      case
        first_missing_selected_product_option(product_options, selected_options)
      {
        Some(option) -> [
          BulkVariantUserError(
            Some(["variants", int.to_string(variant_index)]),
            "You need to add option values for " <> option.name,
            Some("NEED_TO_ADD_OPTION_VALUES"),
          ),
        ]
        None -> []
      }
    _, _, _ -> []
  }
}

@internal
pub fn move_variant_to_position(
  variants: List(ProductVariantRecord),
  variant_id: String,
  position: Int,
) -> List(ProductVariantRecord) {
  let #(variant, remaining) = remove_variant_by_id(variants, variant_id, [])
  case variant {
    Some(record) -> insert_variant_at_position(remaining, record, position)
    None -> variants
  }
}

@internal
pub fn read_variant_sku(
  input: Dict(String, ResolvedValue),
  fallback: Option(String),
) -> Option(String) {
  case read_string_field(input, "sku") {
    Some(sku) -> Some(sku)
    None ->
      case read_object_field(input, "inventoryItem") {
        Some(item) -> read_string_field(item, "sku") |> option.or(fallback)
        None -> fallback
      }
  }
}

@internal
pub fn variant_title_with_fallback(
  selected_options: List(ProductVariantSelectedOptionRecord),
  fallback: String,
) -> String {
  case selected_options {
    [] -> fallback
    _ -> variant_title(selected_options)
  }
}

@internal
pub fn option_input_value_count(input: Dict(String, ResolvedValue)) -> Int {
  read_object_list_field(input, "values")
  |> list.length
}

@internal
pub fn option_input_display_name(input: Dict(String, ResolvedValue)) -> String {
  read_string_field(input, "name")
  |> option.unwrap("")
}

@internal
pub fn duplicate_option_input_name_errors_loop(
  inputs: List(Dict(String, ResolvedValue)),
  index: Int,
  seen: Dict(String, Bool),
  reversed_errors: List(ProductUserError),
) -> List(ProductUserError) {
  case inputs {
    [] -> list.reverse(reversed_errors)
    [input, ..rest] -> {
      let name_key =
        read_string_field(input, "name")
        |> option.unwrap("")
        |> product_option_duplicate_input_key
      case name_key == "" {
        True ->
          duplicate_option_input_name_errors_loop(
            rest,
            index + 1,
            seen,
            reversed_errors,
          )
        False ->
          case dict.has_key(seen, name_key) {
            True ->
              duplicate_option_input_name_errors_loop(rest, index + 1, seen, [
                ProductUserError(
                  ["options", int.to_string(index)],
                  "Duplicated option name.",
                  Some("DUPLICATED_OPTION_NAME"),
                ),
                ..reversed_errors
              ])
            False ->
              duplicate_option_input_name_errors_loop(
                rest,
                index + 1,
                dict.insert(seen, name_key, True),
                reversed_errors,
              )
          }
      }
    }
  }
}

@internal
pub fn duplicate_option_value_input_errors_loop(
  values: List(Dict(String, ResolvedValue)),
  path_prefix: List(String),
  index: Int,
  seen: Dict(String, Bool),
  reversed_errors: List(ProductUserError),
) -> List(ProductUserError) {
  case values {
    [] -> list.reverse(reversed_errors)
    [input, ..rest] -> {
      let name_key =
        read_string_field(input, "name")
        |> option.unwrap("")
        |> product_option_duplicate_input_key
      case name_key == "" {
        True ->
          duplicate_option_value_input_errors_loop(
            rest,
            path_prefix,
            index + 1,
            seen,
            reversed_errors,
          )
        False ->
          case dict.has_key(seen, name_key) {
            True ->
              duplicate_option_value_input_errors_loop(
                rest,
                path_prefix,
                index + 1,
                seen,
                [
                  ProductUserError(
                    list.append(path_prefix, [int.to_string(index), "name"]),
                    "Duplicated option value.",
                    Some("DUPLICATED_OPTION_VALUE"),
                  ),
                  ..reversed_errors
                ],
              )
            False ->
              duplicate_option_value_input_errors_loop(
                rest,
                path_prefix,
                index + 1,
                dict.insert(seen, name_key, True),
                reversed_errors,
              )
          }
      }
    }
  }
}

@internal
pub fn option_name_exists(
  options: List(ProductOptionRecord),
  name: String,
) -> Bool {
  let name_key = product_option_duplicate_input_key(name)
  list.any(options, fn(option) {
    product_option_duplicate_input_key(option.name) == name_key
  })
}

@internal
pub fn option_name_exists_excluding(
  options: List(ProductOptionRecord),
  name: String,
  excluded_option_id: String,
) -> Bool {
  let name_key = product_option_duplicate_input_key(name)
  list.any(options, fn(option) {
    option.id != excluded_option_id
    && product_option_duplicate_input_key(option.name) == name_key
  })
}

@internal
pub fn find_option_value_update(
  updates: List(Dict(String, ResolvedValue)),
  value_id: String,
) -> Option(String) {
  updates
  |> list.find_map(fn(update) {
    case read_string_field(update, "id"), read_string_field(update, "name") {
      Some(id), Some(name) if id == value_id -> Ok(name)
      _, _ -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn read_option_value_create_names(
  inputs: List(Dict(String, ResolvedValue)),
) -> List(String) {
  inputs
  |> list.filter_map(fn(input) {
    case read_string_field(input, "name") {
      Some(name) -> Ok(name)
      None -> Error(Nil)
    }
  })
}

@internal
pub fn insert_option_at_position(
  options: List(ProductOptionRecord),
  option: ProductOptionRecord,
  position: Option(Int),
) -> List(ProductOptionRecord) {
  let insertion_index = case position {
    Some(position) if position > 0 ->
      int.min(position, list.length(options) + 1) - 1
    _ -> list.length(options)
  }
  let before = list.take(options, insertion_index)
  let after = list.drop(options, insertion_index)
  list.append(before, [option, ..after])
  |> position_options(1, [])
}

@internal
pub fn reorder_variant_selections_for_options(
  variants: List(ProductVariantRecord),
  options: List(ProductOptionRecord),
) -> List(ProductVariantRecord) {
  list.map(variants, fn(variant) {
    let selected_options =
      options
      |> list.filter_map(fn(option) {
        find_selected_option(variant.selected_options, option.name)
      })
    ProductVariantRecord(
      ..variant,
      title: variant_title(selected_options),
      selected_options: selected_options,
    )
  })
}

@internal
pub fn unknown_option_errors(
  option_ids: List(String),
  existing_ids: List(String),
) -> List(ProductUserError) {
  option_ids
  |> list.index_map(fn(option_id, index) {
    case list.contains(existing_ids, option_id) {
      True -> None
      False ->
        Some(ProductUserError(
          ["options", int.to_string(index)],
          "Option does not exist",
          Some("OPTION_DOES_NOT_EXIST"),
        ))
    }
  })
  |> list.filter_map(fn(error) {
    case error {
      Some(error) -> Ok(error)
      None -> Error(Nil)
    }
  })
}

@internal
pub fn take_matching_option(
  options: List(ProductOptionRecord),
  input: Dict(String, ResolvedValue),
) -> #(Option(ProductOptionRecord), List(ProductOptionRecord)) {
  let option_id = read_string_field(input, "id")
  let option_name = read_string_field(input, "name")
  take_matching_option_loop(options, option_id, option_name, [])
}

@internal
pub fn read_option_value_names(
  input: Dict(String, ResolvedValue),
) -> List(String) {
  case dict.get(input, "values") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) ->
            case read_string_field(fields, "name") {
              Some(name) -> Ok(name)
              None -> Error(Nil)
            }
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn read_option_value_linked_metafield_values(
  input: Dict(String, ResolvedValue),
) -> List(String) {
  case dict.get(input, "values") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) ->
            case read_string_field(fields, "linkedMetafieldValue") {
              Some(value) -> Ok(value)
              None -> Error(Nil)
            }
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn product_uses_only_default_option_state(
  options: List(ProductOptionRecord),
  variants: List(ProductVariantRecord),
) -> Bool {
  case options, variants {
    [option], [variant] ->
      option.name == "Title"
      && option_values_are_default(option.option_values)
      && variant.selected_options
      == [
        ProductVariantSelectedOptionRecord(
          name: "Title",
          value: "Default Title",
        ),
      ]
    _, _ -> False
  }
}

@internal
pub fn sort_and_position_options(
  options: List(ProductOptionRecord),
) -> List(ProductOptionRecord) {
  options
  |> list.sort(fn(left, right) {
    case int.compare(left.position, right.position) {
      order.Eq -> string.compare(left.id, right.id)
      other -> other
    }
  })
  |> position_options(1, [])
}

@internal
pub fn remap_variant_to_first_option_values(
  variant: ProductVariantRecord,
  options: List(ProductOptionRecord),
) -> ProductVariantRecord {
  let selected_options =
    list.map(options, fn(option) {
      ProductVariantSelectedOptionRecord(
        name: option.name,
        value: first_option_value_name(option),
      )
    })
  ProductVariantRecord(
    ..variant,
    title: variant_title(selected_options),
    selected_options: selected_options,
  )
}

@internal
pub fn map_variants_to_first_new_option_values(
  variants: List(ProductVariantRecord),
  new_options: List(ProductOptionRecord),
) -> List(ProductVariantRecord) {
  list.map(variants, fn(variant) {
    let additions =
      list.map(new_options, fn(option) {
        ProductVariantSelectedOptionRecord(
          name: option.name,
          value: first_option_value_name(option),
        )
      })
    let selected_options = list.append(variant.selected_options, additions)
    ProductVariantRecord(
      ..variant,
      title: variant_title(selected_options),
      selected_options: selected_options,
    )
  })
}

@internal
pub fn sync_product_options_with_variants(
  options: List(ProductOptionRecord),
  variants: List(ProductVariantRecord),
) -> List(ProductOptionRecord) {
  list.map(options, fn(option) {
    ProductOptionRecord(
      ..option,
      option_values: list.map(option.option_values, fn(value) {
        ProductOptionValueRecord(
          ..value,
          has_variants: variants_use_option_value(
            variants,
            option.name,
            value.name,
          ),
        )
      }),
    )
  })
}

@internal
pub fn upsert_option_selection_loop(
  options: List(ProductOptionRecord),
  identity: SyntheticIdentityRegistry,
  product_id: String,
  selected: ProductVariantSelectedOptionRecord,
  reversed_before: List(ProductOptionRecord),
) -> #(List(ProductOptionRecord), Bool, SyntheticIdentityRegistry) {
  case options {
    [] -> #(list.reverse(reversed_before), False, identity)
    [option, ..rest] ->
      case option.name == selected.name {
        True -> {
          let #(option_values, next_identity) =
            upsert_option_value(option.option_values, identity, selected.value)
          #(
            list.append(list.reverse(reversed_before), [
              ProductOptionRecord(
                ..option,
                product_id: product_id,
                option_values: option_values,
              ),
              ..rest
            ]),
            True,
            next_identity,
          )
        }
        False ->
          upsert_option_selection_loop(rest, identity, product_id, selected, [
            option,
            ..reversed_before
          ])
      }
  }
}

@internal
pub fn option_value_combinations(
  options: List(ProductOptionRecord),
) -> List(VariantCombination) {
  case options {
    [] -> [[]]
    [option, ..rest] -> {
      let tail_combinations = option_value_combinations(rest)
      list.flat_map(tail_combinations, fn(tail) {
        option.option_values
        |> list.map(fn(value) {
          [
            ProductVariantSelectedOptionRecord(
              name: option.name,
              value: value.name,
            ),
            ..tail
          ]
        })
      })
    }
  }
}

@internal
pub fn find_variant_for_combination(
  variants: List(ProductVariantRecord),
  combination: VariantCombination,
) -> Option(ProductVariantRecord) {
  variants
  |> list.find(fn(variant) {
    selected_options_equal(variant.selected_options, combination)
  })
  |> option.from_result
}

@internal
pub fn variant_for_combination(
  variant: ProductVariantRecord,
  combination: VariantCombination,
) -> ProductVariantRecord {
  ProductVariantRecord(
    ..variant,
    title: variant_title(combination),
    selected_options: combination,
  )
}

@internal
pub fn make_variant_for_combination(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  combination: VariantCombination,
  template: Option(ProductVariantRecord),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let #(variant_id, identity_after_variant) =
    synthetic_identity.make_synthetic_gid(identity, "ProductVariant")
  let #(inventory_item, final_identity) =
    make_inventory_item_for_variant(identity_after_variant, template)
  #(
    ProductVariantRecord(
      id: variant_id,
      product_id: product_id,
      title: variant_title(combination),
      sku: option.then(template, fn(variant) { variant.sku }),
      barcode: option.then(template, fn(variant) { variant.barcode }),
      price: option.then(template, fn(variant) { variant.price }),
      compare_at_price: option.then(template, fn(variant) {
        variant.compare_at_price
      }),
      taxable: option.then(template, fn(variant) { variant.taxable }),
      inventory_policy: option.then(template, fn(variant) {
        variant.inventory_policy
      }),
      inventory_quantity: option.then(template, fn(variant) {
        variant.inventory_quantity
      }),
      selected_options: combination,
      media_ids: [],
      inventory_item: inventory_item,
      contextual_pricing: option.then(template, fn(variant) {
        variant.contextual_pricing
      }),
      cursor: None,
    ),
    final_identity,
  )
}
