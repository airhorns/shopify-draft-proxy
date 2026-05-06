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
pub fn compare_optional_strings_as_empty(
  left: Option(String),
  right: Option(String),
) -> order.Order {
  string.compare(option.unwrap(left, ""), option.unwrap(right, ""))
}

@internal
pub fn product_search_parse_options() -> search_query_parser.SearchQueryParseOptions {
  search_query_parser.SearchQueryParseOptions(
    ..search_query_parser.default_parse_options(),
    recognize_not_keyword: True,
  )
}

@internal
pub fn product_string_match_options() -> search_query_parser.SearchQueryStringMatchOptions {
  search_query_parser.SearchQueryStringMatchOptions(word_prefix: True)
}

@internal
pub fn has_staged_variants_for_product(
  store: Store,
  product_id: String,
) -> Bool {
  store.staged_state.product_variants
  |> dict.values
  |> list.any(fn(variant) { variant.product_id == product_id })
}

@internal
pub fn product_variant_cursor(
  variant: ProductVariantRecord,
  _index: Int,
) -> String {
  case variant.cursor {
    Some(cursor) -> cursor
    None -> variant.id
  }
}

@internal
pub fn option_to_result(value: Option(a)) -> Result(a, Nil) {
  case value {
    Some(value) -> Ok(value)
    None -> Error(Nil)
  }
}

@internal
pub fn optional_string(value: Option(String)) -> Json {
  case value {
    Some(value) -> json.string(value)
    None -> json.null()
  }
}

@internal
pub fn product_option_value_source(
  option_value: ProductOptionValueRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductOptionValue")),
    #("id", SrcString(option_value.id)),
    #("name", SrcString(option_value.name)),
    #("hasVariants", SrcBool(option_value.has_variants)),
  ])
}

@internal
pub fn selected_option_source(
  selected_option: ProductVariantSelectedOptionRecord,
) -> SourceValue {
  src_object([
    #("name", SrcString(selected_option.name)),
    #("value", SrcString(selected_option.value)),
  ])
}

@internal
pub fn optional_product_category_source(
  category: Option(ProductCategoryRecord),
) -> SourceValue {
  case category {
    Some(category) ->
      src_object([
        #("__typename", SrcString("TaxonomyCategory")),
        #("id", SrcString(category.id)),
        #("fullName", SrcString(category.full_name)),
      ])
    None -> SrcNull
  }
}

@internal
pub fn optional_string_json(value: Option(String)) -> Json {
  case value {
    Some(value) -> json.string(value)
    None -> json.null()
  }
}

@internal
pub fn optional_int_json(value: Option(Int)) -> Json {
  case value {
    Some(value) -> json.int(value)
    None -> json.null()
  }
}

@internal
pub fn product_set_variant_signature_title(
  signature: List(#(Int, String, String)),
) -> String {
  signature
  |> list.map(fn(entry) {
    let #(_, _, value) = entry
    value
  })
  |> string.join(" / ")
}

@internal
pub fn find_variant_by_id(
  variants: List(ProductVariantRecord),
  variant_id: String,
) -> Option(ProductVariantRecord) {
  variants
  |> list.find(fn(variant) { variant.id == variant_id })
  |> option.from_result
}

@internal
pub fn optional_string_list_json(value: Option(Option(List(String)))) -> Json {
  case value {
    Some(Some(items)) -> json.array(items, json.string)
    _ -> json.null()
  }
}

@internal
pub fn optional_string_value_json(value: Option(Option(String))) -> Json {
  case value {
    Some(Some(item)) -> json.string(item)
    _ -> json.null()
  }
}

@internal
pub fn existing_group_options(
  group: Option(SellingPlanGroupRecord),
) -> List(String) {
  case group {
    Some(group) -> group.options
    None -> []
  }
}

@internal
pub fn optional_captured_string(value: Option(String)) -> CapturedJsonValue {
  case value {
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

@internal
pub fn optional_captured_int(value: Option(Int)) -> CapturedJsonValue {
  case value {
    Some(value) -> CapturedInt(value)
    None -> CapturedNull
  }
}

@internal
pub fn duplicate_product_option_values(
  identity: SyntheticIdentityRegistry,
  values: List(ProductOptionValueRecord),
) -> #(List(ProductOptionValueRecord), SyntheticIdentityRegistry, List(String)) {
  let #(reversed, next_identity, ids) =
    list.fold(values, #([], identity, []), fn(acc, value_record) {
      let #(collected, current_identity, collected_ids) = acc
      let #(value_id, next_identity) =
        synthetic_identity.make_synthetic_gid(
          current_identity,
          "ProductOptionValue",
        )
      #(
        [ProductOptionValueRecord(..value_record, id: value_id), ..collected],
        next_identity,
        list.append(collected_ids, [value_id]),
      )
    })
  #(list.reverse(reversed), next_identity, ids)
}

@internal
pub fn make_default_option_record(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
) -> #(ProductOptionRecord, SyntheticIdentityRegistry, List(String)) {
  let #(option_id, identity_after_option) =
    synthetic_identity.make_synthetic_gid(identity, "ProductOption")
  let #(value_id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_option,
      "ProductOptionValue",
    )
  #(
    ProductOptionRecord(
      id: option_id,
      product_id: product.id,
      name: "Title",
      position: 1,
      option_values: [
        ProductOptionValueRecord(
          id: value_id,
          name: "Default Title",
          has_variants: True,
        ),
      ],
    ),
    next_identity,
    [option_id, value_id],
  )
}

@internal
pub fn make_default_variant_record(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
) -> #(ProductVariantRecord, SyntheticIdentityRegistry, List(String)) {
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
      title: "Default Title",
      sku: None,
      barcode: None,
      price: Some("0.00"),
      compare_at_price: None,
      taxable: None,
      inventory_policy: None,
      inventory_quantity: Some(0),
      selected_options: [
        ProductVariantSelectedOptionRecord(
          name: "Title",
          value: "Default Title",
        ),
      ],
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
pub fn has_variant_option_input(input: Dict(String, ResolvedValue)) -> Bool {
  dict.has_key(input, "selectedOptions")
  || dict.has_key(input, "optionValues")
  || dict.has_key(input, "options")
}

@internal
pub fn bulk_variant_option_field_name(
  input: Dict(String, ResolvedValue),
) -> String {
  case dict.has_key(input, "optionValues") {
    True -> "optionValues"
    False ->
      case dict.has_key(input, "selectedOptions") {
        True -> "selectedOptions"
        False -> "options"
      }
  }
}

@internal
pub fn product_option_named(
  options: List(ProductOptionRecord),
  name: String,
) -> Option(ProductOptionRecord) {
  options
  |> list.find(fn(option) { option.name == name })
  |> option.from_result
}

@internal
pub fn first_missing_selected_product_option(
  options: List(ProductOptionRecord),
  selected_options: List(ProductVariantSelectedOptionRecord),
) -> Option(ProductOptionRecord) {
  options
  |> list.find(fn(option) {
    !list.any(selected_options, fn(selected) { selected.name == option.name })
  })
  |> option.from_result
}

@internal
pub fn has_variant_id(
  variants: List(ProductVariantRecord),
  id: String,
) -> Bool {
  list.any(variants, fn(variant) { variant.id == id })
}

@internal
pub fn remove_variant_by_id(
  variants: List(ProductVariantRecord),
  variant_id: String,
  reversed_before: List(ProductVariantRecord),
) -> #(Option(ProductVariantRecord), List(ProductVariantRecord)) {
  case variants {
    [] -> #(None, list.reverse(reversed_before))
    [first, ..rest] ->
      case first.id == variant_id {
        True -> #(Some(first), list.append(list.reverse(reversed_before), rest))
        False ->
          remove_variant_by_id(rest, variant_id, [first, ..reversed_before])
      }
  }
}

@internal
pub fn insert_variant_at_position(
  variants: List(ProductVariantRecord),
  variant: ProductVariantRecord,
  position: Int,
) -> List(ProductVariantRecord) {
  case variants, position <= 0 {
    _, True -> [variant, ..variants]
    [], False -> [variant]
    [first, ..rest], False -> [
      first,
      ..insert_variant_at_position(rest, variant, position - 1)
    ]
  }
}

@internal
pub fn find_variant_update(
  updates: List(Dict(String, ResolvedValue)),
  variant_id: String,
) -> Option(Dict(String, ResolvedValue)) {
  updates
  |> list.find(fn(input) {
    graphql_helpers.read_arg_string(input, "id") == Some(variant_id)
  })
  |> option.from_result
}

@internal
pub fn variant_staged_ids(variant: ProductVariantRecord) -> List(String) {
  case variant.inventory_item {
    Some(item) -> [variant.id, item.id]
    None -> [variant.id]
  }
}

@internal
pub fn compare_variant_price(left: #(Float, String), right: #(Float, String)) {
  case left.0 <. right.0 {
    True -> order.Lt
    False ->
      case left.0 >. right.0 {
        True -> order.Gt
        False -> order.Eq
      }
  }
}

@internal
pub fn has_only_default_variant(variants: List(ProductVariantRecord)) -> Bool {
  case variants {
    [variant] ->
      variant.selected_options
      == [
        ProductVariantSelectedOptionRecord(
          name: "Title",
          value: "Default Title",
        ),
      ]
    _ -> False
  }
}

@internal
pub fn read_option_create_inputs(
  args: Dict(String, ResolvedValue),
) -> List(Dict(String, ResolvedValue)) {
  case dict.get(args, "options") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(input) -> Ok(input)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

@internal
pub fn find_product_option(
  options: List(ProductOptionRecord),
  product_id: String,
  option_id: String,
) -> Option(ProductOptionRecord) {
  options
  |> list.find(fn(option) {
    option.id == option_id && option.product_id == product_id
  })
  |> option.from_result
}

@internal
pub fn option_value_id_exists(
  values: List(ProductOptionValueRecord),
  value_id: String,
) -> Bool {
  list.any(values, fn(value) { value.id == value_id })
}

@internal
pub fn product_option_identity_key(value: String) -> String {
  value
  |> string.trim
  |> string.lowercase
}

@internal
pub fn product_option_duplicate_input_key(value: String) -> String {
  string.trim(value)
}

@internal
pub fn find_selected_option(
  selected_options: List(ProductVariantSelectedOptionRecord),
  name: String,
) -> Result(ProductVariantSelectedOptionRecord, Nil) {
  selected_options
  |> list.find(fn(selected) { selected.name == name })
}

@internal
pub fn take_matching_option_loop(
  options: List(ProductOptionRecord),
  option_id: Option(String),
  option_name: Option(String),
  reversed_before: List(ProductOptionRecord),
) -> #(Option(ProductOptionRecord), List(ProductOptionRecord)) {
  case options {
    [] -> #(None, list.reverse(reversed_before))
    [option, ..rest] -> {
      let matches_id = case option_id {
        Some(id) -> option.id == id
        None -> False
      }
      let matches_name = case option_name {
        Some(name) -> option.name == name
        None -> False
      }
      case matches_id || matches_name {
        True -> #(
          Some(option),
          list.append(list.reverse(reversed_before), rest),
        )
        False ->
          take_matching_option_loop(rest, option_id, option_name, [
            option,
            ..reversed_before
          ])
      }
    }
  }
}

@internal
pub fn restore_default_option_state(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
  variants: List(ProductVariantRecord),
) -> #(
  List(ProductOptionRecord),
  List(ProductVariantRecord),
  SyntheticIdentityRegistry,
  List(String),
) {
  let #(option_id, identity_after_option) =
    synthetic_identity.make_synthetic_gid(identity, "ProductOption")
  let #(value_id, final_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_option,
      "ProductOptionValue",
    )
  let default_option =
    ProductOptionRecord(
      id: option_id,
      product_id: product.id,
      name: "Title",
      position: 1,
      option_values: [
        ProductOptionValueRecord(
          id: value_id,
          name: "Default Title",
          has_variants: True,
        ),
      ],
    )
  let next_variants = case variants {
    [variant, ..] -> [
      ProductVariantRecord(
        ..variant,
        product_id: product.id,
        title: "Default Title",
        selected_options: [
          ProductVariantSelectedOptionRecord(
            name: "Title",
            value: "Default Title",
          ),
        ],
      ),
    ]
    [] -> []
  }
  #([default_option], next_variants, final_identity, [option_id, value_id])
}

@internal
pub fn make_created_option_value_records(
  identity: SyntheticIdentityRegistry,
  names: List(String),
) -> #(List(ProductOptionValueRecord), SyntheticIdentityRegistry) {
  let #(reversed, final_identity) =
    list.fold(names, #([], identity), fn(acc, name) {
      let #(records, current_identity) = acc
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(
          current_identity,
          "ProductOptionValue",
        )
      #(
        [
          ProductOptionValueRecord(id: id, name: name, has_variants: False),
          ..records
        ],
        next_identity,
      )
    })
  #(list.reverse(reversed), final_identity)
}

@internal
pub fn option_values_are_default(
  values: List(ProductOptionValueRecord),
) -> Bool {
  case values {
    [value] -> value.name == "Default Title"
    _ -> False
  }
}

@internal
pub fn position_options(
  options: List(ProductOptionRecord),
  position: Int,
  acc: List(ProductOptionRecord),
) -> List(ProductOptionRecord) {
  case options {
    [] -> list.reverse(acc)
    [option, ..rest] ->
      position_options(rest, position + 1, [
        ProductOptionRecord(..option, position: position),
        ..acc
      ])
  }
}

@internal
pub fn first_option_value_name(option: ProductOptionRecord) -> String {
  case option.option_values {
    [value, ..] -> value.name
    [] -> "Default Title"
  }
}

@internal
pub fn variant_title(
  selected_options: List(ProductVariantSelectedOptionRecord),
) -> String {
  selected_options
  |> list.map(fn(selected) { selected.value })
  |> string.join(" / ")
}

@internal
pub fn upsert_option_value(
  option_values: List(ProductOptionValueRecord),
  identity: SyntheticIdentityRegistry,
  value_name: String,
) -> #(List(ProductOptionValueRecord), SyntheticIdentityRegistry) {
  case
    list.any(option_values, fn(option_value) { option_value.name == value_name })
  {
    True -> #(option_values, identity)
    False -> {
      let #(value_id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "ProductOptionValue")
      #(
        list.append(option_values, [
          ProductOptionValueRecord(
            id: value_id,
            name: value_name,
            has_variants: True,
          ),
        ]),
        next_identity,
      )
    }
  }
}

@internal
pub fn variants_use_option_value(
  variants: List(ProductVariantRecord),
  option_name: String,
  value_name: String,
) -> Bool {
  list.any(variants, fn(variant) {
    list.any(variant.selected_options, fn(selected) {
      selected.name == option_name && selected.value == value_name
    })
  })
}

@internal
pub fn remaining_option_values(
  option: ProductOptionRecord,
) -> List(ProductOptionValueRecord) {
  case option.option_values {
    [] -> []
    [_, ..rest] -> rest
  }
}

@internal
pub fn variant_selected_options_with_value(
  selected_options: List(ProductVariantSelectedOptionRecord),
  option_name: String,
  value_name: String,
) -> List(ProductVariantSelectedOptionRecord) {
  list.map(selected_options, fn(selected) {
    case selected.name == option_name {
      True -> ProductVariantSelectedOptionRecord(..selected, value: value_name)
      False -> selected
    }
  })
}

@internal
pub fn selected_options_equal(
  left: List(ProductVariantSelectedOptionRecord),
  right: List(ProductVariantSelectedOptionRecord),
) -> Bool {
  list.length(left) == list.length(right)
  && list.all(left, fn(selected) {
    list.any(right, fn(other) {
      selected.name == other.name && selected.value == other.value
    })
  })
}
