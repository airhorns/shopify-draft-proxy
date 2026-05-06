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
  is_on_hand_component_quantity_name, write_inventory_quantity,
}
import shopify_draft_proxy/proxy/products/inventory_l01.{
  add_inventory_quantity_amount,
  product_set_inventory_quantities_max_input_size_errors,
}
import shopify_draft_proxy/proxy/products/products_l00.{
  product_set_product_does_not_exist_error,
}
import shopify_draft_proxy/proxy/products/products_l01.{
  collapse_product_tag_identity_whitespace, compare_products_by_sort_key,
  ensure_product_set_default_quantities, enumerate_items,
  mirrored_custom_product_type_length_errors, normalize_product_handle,
  on_hand_component_delta, product_delete_input_object_error, product_numeric_id,
  product_set_input_product_reference, product_string_length_validation_error,
  product_tag_values_validation_error, product_tags_max_input_size_errors,
  serialize_product_metafield, serialize_product_metafields_connection,
  split_reversed_trailing_digits, validate_product_set_resolved_product,
  vendor_from_shop_domain,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  host_from_origin, read_bool_argument, read_string_argument,
  read_string_list_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  read_non_empty_string_field, resource_id_matches, store_slug_from_admin_origin,
  user_errors_source,
}
import shopify_draft_proxy/proxy/products/types.{
  type ProductSetInventoryQuantityInput, type ProductUserError,
  ProductSetInventoryQuantityInput, ProductUserError,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l01.{
  product_set_variant_max_input_size_errors,
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
pub fn serialize_product_metafield_owner_selection(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let selections =
    get_selected_child_fields(field, default_selected_field_options())
  let entries =
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "__typename" -> #(key, json.string("Product"))
            "id" -> #(key, json.string(owner_id))
            "metafield" -> #(
              key,
              serialize_product_metafield(store, owner_id, selection, variables),
            )
            "metafields" -> #(
              key,
              serialize_product_metafields_connection(
                store,
                owner_id,
                selection,
                variables,
              ),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn sort_products(
  products: List(ProductRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductRecord) {
  case read_string_argument(field, variables, "sortKey") {
    None -> products
    Some(sort_key) -> {
      let sorted =
        list.sort(products, fn(left, right) {
          compare_products_by_sort_key(left, right, sort_key)
        })
      case read_bool_argument(field, variables, "reverse") {
        Some(True) -> list.reverse(sorted)
        _ -> sorted
      }
    }
  }
}

@internal
pub fn product_id_matches(product: ProductRecord, raw_value: String) -> Bool {
  resource_id_matches(product.id, product.legacy_resource_id, raw_value)
}

@internal
pub fn product_sort_cursor_payload(
  product: ProductRecord,
  encoded_value: String,
) -> String {
  let payload =
    "{\"last_id\":"
    <> int.to_string(product_numeric_id(product))
    <> ",\"last_value\":"
    <> encoded_value
    <> "}"
  payload
  |> bit_array.from_string
  |> bit_array.base64_encode(True)
}

@internal
pub fn product_set_max_input_size_errors(
  input: Dict(String, ResolvedValue),
) -> List(Json) {
  list.append(
    product_set_variant_max_input_size_errors(input),
    product_set_inventory_quantities_max_input_size_errors(input),
  )
  |> list.append(product_tags_max_input_size_errors(
    "productSet",
    "input",
    input,
  ))
}

@internal
pub fn resolve_product_set_input_product(
  store: Store,
  input: Dict(String, ResolvedValue),
) -> Result(Option(ProductRecord), ProductOperationUserErrorRecord) {
  case product_set_input_product_reference(input) {
    Some(#(id, field)) ->
      case store.get_effective_product_by_id(store, id) {
        Some(product) -> validate_product_set_resolved_product(product)
        None -> Error(product_set_product_does_not_exist_error(field))
      }
    None -> Ok(None)
  }
}

@internal
pub fn product_string_length_validation_errors(
  input: Dict(String, ResolvedValue),
  field_prefix: List(String),
) -> List(ProductUserError) {
  list.append(
    product_string_length_validation_error(
      input,
      field_prefix,
      "title",
      "Title",
    ),
    list.append(
      product_string_length_validation_error(
        input,
        field_prefix,
        "handle",
        "Handle",
      ),
      list.append(
        product_string_length_validation_error(
          input,
          field_prefix,
          "vendor",
          "Vendor",
        ),
        list.append(
          product_string_length_validation_error(
            input,
            field_prefix,
            "productType",
            "Product type",
          ),
          list.append(
            product_string_length_validation_error(
              input,
              field_prefix,
              "customProductType",
              "Custom product type",
            ),
            mirrored_custom_product_type_length_errors(input, field_prefix),
          ),
        ),
      ),
    ),
  )
}

@internal
pub fn product_tags_validation_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  case read_string_list_field(input, "tags") {
    Some(tags) ->
      case product_tag_values_validation_error(tags) {
        Some(error) -> [error]
        None -> []
      }
    None -> []
  }
}

@internal
pub fn enumerate_strings(values: List(String)) -> List(#(String, Int)) {
  values
  |> enumerate_items()
}

@internal
pub fn build_product_delete_missing_input_id_error(
  loc: Option(Location),
  document: String,
) -> Json {
  product_delete_input_object_error(
    "Argument 'id' on InputObject 'ProductDeleteInput' is required. Expected type ID!",
    loc,
    document,
    [
      #("code", json.string("missingRequiredInputObjectAttribute")),
      #("argumentName", json.string("id")),
      #("argumentType", json.string("ID!")),
      #("inputObjectType", json.string("ProductDeleteInput")),
    ],
  )
}

@internal
pub fn build_product_delete_null_input_id_error(
  loc: Option(Location),
  document: String,
) -> Json {
  product_delete_input_object_error(
    "Argument 'id' on InputObject 'ProductDeleteInput' has an invalid value (null). Expected type 'ID!'.",
    loc,
    document,
    [
      #("code", json.string("argumentLiteralsIncompatible")),
      #("typeName", json.string("InputObject")),
      #("argumentName", json.string("id")),
    ],
  )
}

@internal
pub fn product_delete_payload(
  deleted_product_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let deleted_value = case deleted_product_id {
    Some(id) -> SrcString(id)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductDeletePayload")),
      #("deletedProductId", deleted_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn vendor_from_shopify_admin_origin(origin: String) -> Option(String) {
  let host = host_from_origin(origin)
  case vendor_from_shop_domain(host) {
    Some(vendor) -> Some(vendor)
    None -> store_slug_from_admin_origin(origin)
  }
}

@internal
pub fn product_tag_identity_key(tag: String) -> String {
  tag
  |> string.trim
  |> collapse_product_tag_identity_whitespace
  |> string.lowercase
}

@internal
pub fn apply_product_set_level_quantities(
  identity: SyntheticIdentityRegistry,
  quantities: List(InventoryQuantityRecord),
  inputs: List(ProductSetInventoryQuantityInput),
) -> #(List(InventoryQuantityRecord), SyntheticIdentityRegistry) {
  let #(next_quantities, next_identity) =
    list.fold(inputs, #(quantities, identity), fn(acc, input) {
      let #(current_quantities, current_identity) = acc
      let #(updated_at, identity_after_timestamp) =
        synthetic_identity.make_synthetic_timestamp(current_identity)
      let with_named =
        write_inventory_quantity(
          current_quantities,
          input.name,
          input.quantity,
          Some(updated_at),
        )
      let with_on_hand = case input.name == "available" {
        True ->
          write_inventory_quantity(with_named, "on_hand", input.quantity, None)
        False -> with_named
      }
      #(with_on_hand, identity_after_timestamp)
    })
  #(ensure_product_set_default_quantities(next_quantities), next_identity)
}

@internal
pub fn read_explicit_product_handle(
  input: Dict(String, ResolvedValue),
) -> Option(String) {
  case read_non_empty_string_field(input, "handle") {
    Some(handle) -> {
      let normalized = normalize_product_handle(handle)
      case normalized {
        "" -> Some("product")
        _ -> Some(normalized)
      }
    }
    None -> None
  }
}

@internal
pub fn read_collision_checked_explicit_product_handle(
  input: Dict(String, ResolvedValue),
) -> Option(String) {
  case read_non_empty_string_field(input, "handle") {
    Some(handle) -> {
      let normalized = normalize_product_handle(handle)
      case normalized {
        "" -> None
        _ -> Some(normalized)
      }
    }
    None -> None
  }
}

@internal
pub fn product_handle_should_dedupe(
  input: Dict(String, ResolvedValue),
) -> Bool {
  case read_non_empty_string_field(input, "handle") {
    Some(handle) -> normalize_product_handle(handle) == ""
    None -> True
  }
}

@internal
pub fn slugify_product_handle(title: String) -> String {
  let normalized = normalize_product_handle(title)
  case normalized {
    "" -> "untitled-product"
    _ -> normalized
  }
}

@internal
pub fn dedup_base_and_next_suffix(handle: String) -> #(String, Int) {
  let #(digits, rest) =
    handle
    |> string.to_graphemes
    |> list.reverse
    |> split_reversed_trailing_digits([])
  case digits {
    [] -> #(handle, 1)
    _ ->
      case rest {
        ["-", ..base_reversed] ->
          case base_reversed {
            [] -> #(handle, 1)
            _ -> {
              let base_handle = base_reversed |> list.reverse |> string.join("")
              let suffix =
                digits
                |> string.join("")
                |> int.parse
                |> result.unwrap(0)
              #(base_handle, suffix + 1)
            }
          }
        _ -> #(handle, 1)
      }
  }
}

@internal
pub fn maybe_add_on_hand_component_delta(
  quantities: List(InventoryQuantityRecord),
  name: String,
  delta: Int,
) -> List(InventoryQuantityRecord) {
  case is_on_hand_component_quantity_name(name) {
    True -> add_inventory_quantity_amount(quantities, "on_hand", delta)
    False -> quantities
  }
}

@internal
pub fn maybe_add_available_for_on_hand_delta(
  quantities: List(InventoryQuantityRecord),
  name: String,
  delta: Int,
) -> List(InventoryQuantityRecord) {
  case name {
    "on_hand" -> add_inventory_quantity_amount(quantities, "available", delta)
    _ -> quantities
  }
}

@internal
pub fn add_on_hand_move_delta(
  quantities: List(InventoryQuantityRecord),
  from_name: String,
  to_name: String,
  quantity: Int,
) -> List(InventoryQuantityRecord) {
  let delta =
    on_hand_component_delta(from_name, 0 - quantity)
    + on_hand_component_delta(to_name, quantity)
  case delta == 0 {
    True -> quantities
    False -> add_inventory_quantity_amount(quantities, "on_hand", delta)
  }
}
