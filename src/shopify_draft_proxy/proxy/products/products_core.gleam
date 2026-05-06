//// Products-domain submodule: products_core.
//// Combines layered files: products_l00, products_l01, products_l02.

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
  type Location, type Selection, Field, NullValue,
}

import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, ListVal, ObjectVal, StringVal,
}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcList, SrcNull, SrcString,
  default_selected_field_options, get_field_response_key,
  get_selected_child_fields, project_graphql_value, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  build_null_argument_error, find_argument,
}

import shopify_draft_proxy/proxy/products/inventory_core.{
  add_inventory_quantity_amount, ensure_inventory_quantity,
  is_on_hand_component_quantity_name, locations_payload,
  product_set_inventory_quantities_max_input_size_errors,
  write_inventory_quantity,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type ProductSetInventoryQuantityInput, type ProductUserError,
  type RenamedOptionValue, ProductUserError,
  product_description_html_limit_bytes, product_set_file_limit,
  product_string_character_limit, product_tag_character_limit, product_tag_limit,
}
import shopify_draft_proxy/proxy/products/publications_core.{
  products_published_to_publication,
}
import shopify_draft_proxy/proxy/products/shared.{
  dedupe_preserving_order, host_from_origin, max_input_size_error,
  read_bool_argument, read_list_field_length, read_non_empty_string_field,
  read_object_list_field, read_string_argument, read_string_field,
  read_string_list_field, resource_id_matches, store_slug_from_admin_origin,
  trimmed_non_empty, user_errors_source,
}
import shopify_draft_proxy/proxy/products/variants_helpers.{
  compare_optional_strings_as_empty, optional_string, optional_string_json,
  product_string_match_options,
}
import shopify_draft_proxy/proxy/products/variants_options_core.{
  product_set_variant_max_input_size_errors,
}

import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type InventoryQuantityRecord, type ProductMetafieldRecord,
  type ProductOperationUserErrorRecord, type ProductRecord,
  type ProductSeoRecord, type SellingPlanGroupRecord, ProductMetafieldRecord,
  ProductOperationUserErrorRecord, ProductSeoRecord,
}

// ===== from products_l00 =====
@internal
pub fn has_effective_product_metafield_owner(
  store: Store,
  owner_id: String,
) -> Bool {
  case store.get_effective_metafields_by_owner_id(store, owner_id) {
    [] -> False
    _ -> True
  }
}

@internal
pub fn product_metafield_to_core(
  record: ProductMetafieldRecord,
) -> metafields.MetafieldRecordCore {
  metafields.MetafieldRecordCore(
    id: record.id,
    namespace: record.namespace,
    key: record.key,
    type_: record.type_,
    value: record.value,
    compare_digest: record.compare_digest,
    json_value: record.json_value,
    created_at: record.created_at,
    updated_at: record.updated_at,
    owner_type: record.owner_type,
  )
}

@internal
pub fn matches_nullable_product_timestamp(
  value: Option(String),
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  case
    search_query_parser.strip_search_query_value_quotes(
      search_query_parser.search_query_term_value(term),
    )
  {
    "*" -> option.is_some(value)
    _ -> search_query_parser.matches_search_query_date(value, term, 0)
  }
}

@internal
pub fn product_searchable_status(
  store: Store,
  product: ProductRecord,
) -> String {
  case dict.get(store.base_state.products, product.id) {
    Ok(base_product) ->
      case base_product.status == product.status {
        True -> product.status
        False -> base_product.status
      }
    Error(_) -> product.status
  }
}

@internal
pub fn product_searchable_tags(
  store: Store,
  product: ProductRecord,
) -> List(String) {
  case dict.get(store.base_state.products, product.id) {
    Ok(base_product) ->
      case base_product.tags == product.tags {
        True -> product.tags
        False -> base_product.tags
      }
    Error(_) -> product.tags
  }
}

@internal
pub fn product_tags(store: Store) -> List(String) {
  store.list_effective_products(store)
  |> list.flat_map(fn(product) { product.tags })
}

@internal
pub fn product_types(store: Store) -> List(String) {
  store.list_effective_products(store)
  |> list.filter_map(fn(product) {
    case product.product_type {
      Some(value) -> Ok(value)
      None -> Error(Nil)
    }
  })
}

@internal
pub fn product_vendors(store: Store) -> List(String) {
  store.list_effective_products(store)
  |> list.filter_map(fn(product) {
    case product.vendor {
      Some(value) -> Ok(value)
      None -> Error(Nil)
    }
  })
}

@internal
pub fn product_cursor(product: ProductRecord, _index: Int) -> String {
  case product.cursor {
    Some(cursor) -> cursor
    None -> product.id
  }
}

@internal
pub fn product_numeric_id_from_gid(id: String) -> Int {
  case list.last(string.split(id, "/")) {
    Ok(tail) ->
      case int.parse(tail) {
        Ok(parsed) -> parsed
        Error(_) -> 0
      }
    Error(_) -> 0
  }
}

@internal
pub fn product_operation_user_error_source(
  error: ProductOperationUserErrorRecord,
) -> SourceValue {
  let field_value = case error.field {
    Some(field) -> SrcList(list.map(field, SrcString))
    None -> SrcNull
  }
  src_object([
    #("field", field_value),
    #("message", SrcString(error.message)),
    #("code", graphql_helpers.option_string_source(error.code)),
  ])
}

@internal
pub fn product_currency_code(store: Store) -> String {
  case store.get_effective_shop(store) {
    Some(shop) -> shop.currency_code
    None -> "USD"
  }
}

@internal
pub fn enumerate_items_loop(
  items: List(a),
  index: Int,
  acc: List(#(a, Int)),
) -> List(#(a, Int)) {
  case items {
    [] -> list.reverse(acc)
    [first, ..rest] ->
      enumerate_items_loop(rest, index + 1, [#(first, index), ..acc])
  }
}

@internal
pub fn product_seo_source(seo: ProductSeoRecord) -> SourceValue {
  src_object([
    #("title", graphql_helpers.option_string_source(seo.title)),
    #("description", graphql_helpers.option_string_source(seo.description)),
  ])
}

@internal
pub const product_hydrate_nodes_query: String = "
query ProductsHydrateNodes($ids: [ID!]!) {
  nodes(ids: $ids) {
    __typename
    id
    ... on Product {
      legacyResourceId
      title
      handle
      status
      combinedListingRole
      vendor
      productType
      tags
      totalInventory
      tracksInventory
      createdAt
      updatedAt
      publishedAt
      descriptionHtml
      onlineStorePreviewUrl
      templateSuffix
      seo { title description }
      options {
        id
        name
        position
        optionValues { id name hasVariants }
      }
      metafields(first: 250) {
        nodes {
          id
          namespace
          key
          type
          value
          compareDigest
          jsonValue
          createdAt
          updatedAt
          ownerType
        }
      }
      media(first: 250) {
        nodes {
          id
          alt
          mediaContentType
          status
          preview { image { url width height } }
          image { id url altText width height }
        }
      }
      collections(first: 250) {
        nodes {
          id
          legacyResourceId
          title
          handle
          updatedAt
          description
          descriptionHtml
          sortOrder
          templateSuffix
          seo { title description }
          productsCount { count }
        }
      }
      variants(first: 250) {
        nodes {
          id
          title
          sku
          barcode
          price
          compareAtPrice
          taxable
          inventoryPolicy
          inventoryQuantity
          selectedOptions { name value }
          metafields(first: 250) {
            nodes {
              id
              namespace
              key
              type
              value
              compareDigest
              jsonValue
              createdAt
              updatedAt
              ownerType
            }
          }
          inventoryItem {
            id
            tracked
            requiresShipping
            measurement { weight { unit value } }
            inventoryLevels(first: 50) {
              nodes {
                id
                location { id name }
                quantities(names: [\"available\", \"on_hand\", \"committed\", \"incoming\", \"reserved\", \"damaged\", \"quality_control\", \"safety_stock\"]) {
                  name
                  quantity
                  updatedAt
                }
              }
            }
          }
          sellingPlanGroups(first: 50) {
            nodes { id name merchantCode }
          }
        }
      }
      sellingPlanGroups(first: 50) {
        nodes { id name merchantCode }
      }
    }
    ... on Collection {
      legacyResourceId
      title
      handle
      updatedAt
      description
      descriptionHtml
      sortOrder
      templateSuffix
      seo { title description }
      ruleSet {
        appliedDisjunctively
        rules { column relation condition }
      }
      metafields(first: 250) {
        nodes {
          id
          namespace
          key
          type
          value
          compareDigest
          jsonValue
          createdAt
          updatedAt
          ownerType
        }
      }
      productsCount { count }
      products(first: 250) {
        edges {
          cursor
          node { id title handle status vendor productType tags totalInventory tracksInventory }
        }
      }
    }
    ... on ProductVariant {
      title
      sku
      barcode
      price
      compareAtPrice
      taxable
      inventoryPolicy
      inventoryQuantity
      selectedOptions { name value }
      product { id title handle status totalInventory tracksInventory }
      product {
        variants(first: 250) {
          nodes {
            id
            title
            sku
            barcode
            price
            compareAtPrice
            taxable
            inventoryPolicy
            inventoryQuantity
            selectedOptions { name value }
            inventoryItem {
              id
              tracked
              requiresShipping
              measurement { weight { unit value } }
            }
          }
        }
      }
      metafields(first: 250) {
        nodes {
          id
          namespace
          key
          type
          value
          compareDigest
          jsonValue
          createdAt
          updatedAt
          ownerType
        }
      }
      inventoryItem {
        id
        tracked
        requiresShipping
        measurement { weight { unit value } }
      }
      sellingPlanGroups(first: 50) {
        nodes { id name merchantCode }
      }
    }
    ... on InventoryItem {
      tracked
      requiresShipping
      measurement { weight { unit value } }
      variant {
        id
        title
        inventoryQuantity
        selectedOptions { name value }
        product {
          id
          title
          handle
          status
          totalInventory
          tracksInventory
        }
      }
      inventoryLevels(first: 50) {
        nodes {
          id
          location { id name }
          quantities(names: [\"available\", \"on_hand\", \"committed\", \"incoming\", \"reserved\", \"damaged\", \"quality_control\", \"safety_stock\"]) {
            name
            quantity
            updatedAt
          }
        }
      }
    }
    ... on InventoryLevel {
      id
      location { id name }
      quantities(names: [\"available\", \"on_hand\", \"committed\", \"incoming\", \"reserved\", \"damaged\", \"quality_control\", \"safety_stock\"]) {
        name
        quantity
        updatedAt
      }
      item {
        id
        tracked
        requiresShipping
        inventoryLevels(first: 50) {
          nodes {
            id
            location { id name }
            quantities(names: [\"available\", \"on_hand\", \"committed\", \"incoming\", \"reserved\", \"damaged\", \"quality_control\", \"safety_stock\"]) {
              name
              quantity
              updatedAt
            }
          }
        }
        variant {
          id
          title
          inventoryQuantity
          selectedOptions { name value }
          product {
            id
            title
            handle
            status
            totalInventory
            tracksInventory
          }
        }
      }
    }
    ... on Location {
      id
      name
      isActive
    }
  }
}
"

@internal
pub fn product_set_identifier_has_reference(
  identifier: Dict(String, ResolvedValue),
) -> Bool {
  dict.has_key(identifier, "id") || dict.has_key(identifier, "handle")
}

@internal
pub fn product_set_product_does_not_exist_error(
  field: List(String),
) -> ProductOperationUserErrorRecord {
  ProductOperationUserErrorRecord(
    field: Some(field),
    message: "Product does not exist",
    code: Some("PRODUCT_DOES_NOT_EXIST"),
  )
}

@internal
pub fn product_set_product_suspended_error() -> ProductOperationUserErrorRecord {
  ProductOperationUserErrorRecord(
    field: Some(["input"]),
    message: "Product is suspended",
    code: Some("INVALID_PRODUCT"),
  )
}

@internal
pub fn make_product_preview_url(product: ProductRecord) -> String {
  "https://shopify-draft-proxy.local/products_preview?product_id="
  <> product.id
  <> "&handle="
  <> product.handle
}

@internal
pub fn json_string_array_literal(values: List(String)) -> String {
  let content =
    values
    |> list.map(fn(value) { "\"" <> value <> "\"" })
    |> string.join(",")
  "[" <> content <> "]"
}

@internal
pub fn pad_start_zero(value: String, width: Int) -> String {
  let length = string.length(value)
  case length >= width {
    True -> value
    False -> string.repeat("0", width - length) <> value
  }
}

@internal
pub fn tags_update_root_name(is_add: Bool) -> String {
  case is_add {
    True -> "tagsAdd"
    False -> "tagsRemove"
  }
}

@internal
pub fn product_change_status_null_product_id_error(
  document: String,
  operation_path: String,
  field: Selection,
) -> Option(Json) {
  let arguments = case field {
    Field(arguments: args, ..) -> args
    _ -> []
  }
  case find_argument(arguments, "productId") {
    Some(argument) ->
      case argument.value {
        NullValue(..) -> {
          let field_loc = case field {
            Field(loc: loc, ..) -> loc
            _ -> None
          }
          Some(build_null_argument_error(
            "productChangeStatus",
            "productId",
            "ID!",
            operation_path,
            field_loc,
            document,
          ))
        }
        _ -> None
      }
    None -> None
  }
}

@internal
pub fn is_valid_product_status(status: Option(String)) -> Bool {
  case status {
    Some("ACTIVE") | Some("ARCHIVED") | Some("DRAFT") -> True
    _ -> False
  }
}

@internal
pub fn existing_group_app_id(
  group: Option(SellingPlanGroupRecord),
) -> Option(String) {
  case group {
    Some(group) -> group.app_id
    None -> None
  }
}

@internal
pub fn existing_group_name(group: Option(SellingPlanGroupRecord)) -> String {
  case group {
    Some(group) -> group.name
    None -> "Selling plan group"
  }
}

@internal
pub fn existing_group_merchant_code(
  group: Option(SellingPlanGroupRecord),
) -> String {
  case group {
    Some(group) -> group.merchant_code
    None -> "selling-plan-group"
  }
}

@internal
pub fn existing_group_description(
  group: Option(SellingPlanGroupRecord),
) -> Option(String) {
  case group {
    Some(group) -> group.description
    None -> None
  }
}

@internal
pub fn existing_group_position(
  group: Option(SellingPlanGroupRecord),
) -> Option(Int) {
  case group {
    Some(group) -> group.position
    None -> None
  }
}

@internal
pub fn is_product_tag_identity_whitespace(grapheme: String) -> Bool {
  case grapheme {
    " " | "\t" | "\n" | "\r" -> True
    _ -> False
  }
}

@internal
pub fn duplicate_product_metafields(
  identity: SyntheticIdentityRegistry,
  duplicate_product_id: String,
  metafields: List(ProductMetafieldRecord),
) -> #(List(ProductMetafieldRecord), SyntheticIdentityRegistry, List(String)) {
  let #(reversed, next_identity, ids) =
    list.fold(metafields, #([], identity, []), fn(acc, metafield) {
      let #(collected, current_identity, collected_ids) = acc
      let #(metafield_id, next_identity) =
        synthetic_identity.make_synthetic_gid(current_identity, "Metafield")
      #(
        [
          ProductMetafieldRecord(
            ..metafield,
            id: metafield_id,
            owner_id: duplicate_product_id,
          ),
          ..collected
        ],
        next_identity,
        list.append(collected_ids, [metafield_id]),
      )
    })
  #(list.reverse(reversed), next_identity, ids)
}

@internal
pub fn finish_handle_parts(parts_state: #(List(String), String)) -> String {
  let #(parts, current) = parts_state
  let parts = case current {
    "" -> parts
    _ -> [current, ..parts]
  }
  parts
  |> list.reverse
  |> string.join("-")
}

@internal
pub fn is_handle_grapheme(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" -> True
    "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" -> True
    "u" | "v" | "w" | "x" | "y" | "z" -> True
    _ -> False
  }
}

@internal
pub fn ensure_unique_handle(
  base_handle: String,
  suffix: Int,
  in_use: fn(String) -> Bool,
) -> String {
  let candidate = case suffix {
    0 -> base_handle
    _ -> base_handle <> "-" <> int.to_string(suffix)
  }
  case in_use(candidate) {
    True -> ensure_unique_handle(base_handle, suffix + 1, in_use)
    False -> candidate
  }
}

@internal
pub fn is_handle_digit(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    _ -> False
  }
}

@internal
pub fn product_handle_in_use(store: Store, handle: String) -> Bool {
  store.list_effective_products(store)
  |> list.any(fn(product) { product.handle == handle })
}

@internal
pub fn product_handle_in_use_by_other(
  store: Store,
  handle: String,
  allowed_product_id: Option(String),
) -> Bool {
  store.list_effective_products(store)
  |> list.any(fn(product) {
    product.handle == handle && Some(product.id) != allowed_product_id
  })
}

@internal
pub fn parse_price_amount(amount: String) -> Result(Float, Nil) {
  case float.parse(amount) {
    Ok(value) -> Ok(value)
    Error(_) ->
      case int.parse(amount) {
        Ok(value) -> Ok(int.to_float(value))
        Error(_) -> Error(Nil)
      }
  }
}

@internal
pub fn two_digit_cents(cents: String) -> String {
  case string.length(cents) {
    0 -> "00"
    1 -> cents <> "0"
    _ -> string.slice(cents, 0, 2)
  }
}

@internal
pub fn product_of_counts(counts: List(Int)) -> Int {
  case counts {
    [] -> 0
    _ -> list.fold(counts, 1, fn(total, count) { total * count })
  }
}

// ===== from products_l01 =====
@internal
pub fn product_by_identifier(
  store: Store,
  identifier: Dict(String, ResolvedValue),
) -> Option(ProductRecord) {
  case read_string_field(identifier, "id") {
    Some(id) -> store.get_effective_product_by_id(store, id)
    None ->
      case read_string_field(identifier, "handle") {
        Some(handle) -> store.get_effective_product_by_handle(store, handle)
        None -> None
      }
  }
}

@internal
pub fn serialize_product_metafield(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let namespace = read_string_argument(field, variables, "namespace")
  let key = read_string_argument(field, variables, "key")
  let found =
    store.get_effective_metafields_by_owner_id(store, owner_id)
    |> list.find(fn(metafield) {
      metafield.namespace == option.unwrap(namespace, "")
      && metafield.key == option.unwrap(key, "")
    })
    |> option.from_result
  case found {
    Some(metafield) ->
      metafields.serialize_metafield_selection(
        product_metafield_to_core(metafield),
        field,
        default_selected_field_options(),
      )
    None -> json.null()
  }
}

@internal
pub fn serialize_product_metafields_connection(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let namespace = read_string_argument(field, variables, "namespace")
  let records =
    store.get_effective_metafields_by_owner_id(store, owner_id)
    |> list.filter(fn(metafield) {
      case namespace {
        Some(ns) -> metafield.namespace == ns
        None -> True
      }
    })
    |> list.map(product_metafield_to_core)
  metafields.serialize_metafields_connection(
    records,
    field,
    variables,
    default_selected_field_options(),
  )
}

@internal
pub fn published_products_count_for_field(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Int {
  case read_string_argument(field, variables, "publicationId") {
    Some(publication_id) ->
      products_published_to_publication(store, publication_id) |> list.length
    None ->
      store.list_effective_products(store)
      |> list.filter(fn(product) {
        product.status == "ACTIVE" && !list.is_empty(product.publication_ids)
      })
      |> list.length
  }
}

@internal
pub fn compare_products_by_sort_key(
  left: ProductRecord,
  right: ProductRecord,
  sort_key: String,
) -> order.Order {
  case sort_key {
    "TITLE" ->
      case string.compare(left.title, right.title) {
        order.Eq -> string.compare(left.id, right.id)
        other -> other
      }
    "VENDOR" ->
      case compare_optional_strings_as_empty(left.vendor, right.vendor) {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    "PRODUCT_TYPE" ->
      case
        compare_optional_strings_as_empty(left.product_type, right.product_type)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    "PUBLISHED_AT" ->
      case
        compare_optional_strings_as_empty(left.published_at, right.published_at)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    "UPDATED_AT" ->
      case
        compare_optional_strings_as_empty(left.updated_at, right.updated_at)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    "ID" -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
    _ -> order.Eq
  }
}

@internal
pub fn product_matches_search_text(
  product: ProductRecord,
  raw_value: String,
) -> Bool {
  let searchable_values = [
    product.title,
    product.handle,
    option.unwrap(product.vendor, ""),
    option.unwrap(product.product_type, ""),
  ]
  list.any(list.append(searchable_values, product.tags), fn(candidate) {
    search_query_parser.matches_search_query_string(
      Some(candidate),
      raw_value,
      search_query_parser.IncludesMatch,
      product_string_match_options(),
    )
  })
}

@internal
pub fn product_numeric_id(product: ProductRecord) -> Int {
  case product.legacy_resource_id {
    Some(value) ->
      case int.parse(value) {
        Ok(parsed) -> parsed
        Error(_) -> product_numeric_id_from_gid(product.id)
      }
    None -> product_numeric_id_from_gid(product.id)
  }
}

@internal
pub fn serialize_product_duplicate_job(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let id = read_string_argument(field, variables, "id")
  let entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "id" -> #(key, optional_string(id))
              "done" -> #(key, json.bool(True))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

@internal
pub fn enumerate_items(items: List(a)) -> List(#(a, Int)) {
  enumerate_items_loop(items, 0, [])
}

@internal
pub fn product_tags_max_input_size_errors(
  root: String,
  input_name: String,
  input: Dict(String, ResolvedValue),
) -> List(Json) {
  let path = case input_name {
    "" -> [root, "tags"]
    _ -> [root, input_name, "tags"]
  }
  case read_list_field_length(input, "tags") {
    Some(length) if length > product_tag_limit -> [
      max_input_size_error(length, product_tag_limit, path),
    ]
    _ -> []
  }
}

@internal
pub fn product_set_file_limit_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  let files = read_object_list_field(input, "files")
  case list.length(files) > product_set_file_limit {
    True -> [
      ProductOperationUserErrorRecord(
        field: Some(["input", "files"]),
        message: "Files count is over the allowed limit.",
        code: Some("INVALID_INPUT"),
      ),
    ]
    False -> []
  }
}

@internal
pub fn product_set_input_product_reference(
  input: Dict(String, ResolvedValue),
) -> Option(#(String, List(String))) {
  case read_string_field(input, "id") {
    Some(id) -> Some(#(id, ["input", "id"]))
    None ->
      case read_string_field(input, "productId") {
        Some(product_id) -> Some(#(product_id, ["input", "productId"]))
        None -> None
      }
  }
}

@internal
pub fn product_set_identifier_reference_field(
  identifier: Dict(String, ResolvedValue),
) -> List(String) {
  case read_string_field(identifier, "id") {
    Some(_) -> ["identifier", "id"]
    None ->
      case read_string_field(identifier, "handle") {
        Some(_) -> ["identifier", "handle"]
        None -> ["identifier"]
      }
  }
}

@internal
pub fn validate_product_set_resolved_product(
  product: ProductRecord,
) -> Result(Option(ProductRecord), ProductOperationUserErrorRecord) {
  case product.status == "SUSPENDED" {
    True -> Error(product_set_product_suspended_error())
    False -> Ok(Some(product))
  }
}

@internal
pub fn mirrored_custom_product_type_length_errors(
  input: Dict(String, ResolvedValue),
  field_prefix: List(String),
) -> List(ProductUserError) {
  case read_string_field(input, "customProductType") {
    Some(_) -> []
    None ->
      case read_string_field(input, "productType") {
        Some(value) ->
          case string.length(value) > product_string_character_limit {
            True -> [
              ProductUserError(
                list.append(field_prefix, ["customProductType"]),
                "Custom product type is too long (maximum is 255 characters)",
                None,
              ),
            ]
            False -> []
          }
        None -> []
      }
  }
}

@internal
pub fn product_string_length_validation_error(
  input: Dict(String, ResolvedValue),
  field_prefix: List(String),
  field_name: String,
  label: String,
) -> List(ProductUserError) {
  case read_string_field(input, field_name) {
    Some(value) ->
      case string.length(value) > product_string_character_limit {
        True -> [
          ProductUserError(
            list.append(field_prefix, [field_name]),
            label <> " is too long (maximum is 255 characters)",
            None,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn product_description_html_validation_errors(
  input: Dict(String, ResolvedValue),
  field_prefix: List(String),
) -> List(ProductUserError) {
  case read_string_field(input, "descriptionHtml") {
    Some(value) ->
      case string.byte_size(value) > product_description_html_limit_bytes {
        True -> [
          ProductUserError(
            list.append(field_prefix, ["bodyHtml"]),
            "Body (HTML) is too big (maximum is 512 KB)",
            None,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn product_tag_values_validation_error(
  tags: List(String),
) -> Option(ProductUserError) {
  case
    list.any(tags, fn(tag) {
      case trimmed_non_empty(tag) {
        Ok(trimmed) -> string.length(trimmed) > product_tag_character_limit
        Error(_) -> False
      }
    })
  {
    True -> Some(ProductUserError(["tags"], "Product tags is invalid", None))
    False -> None
  }
}

@internal
pub fn product_delete_input_object_error(
  message: String,
  loc: Option(Location),
  document: String,
  extensions: List(#(String, Json)),
) -> Json {
  let base = [#("message", json.string(message))]
  let with_locations = case locations_payload(loc, document) {
    Some(locations) -> list.append(base, [#("locations", locations)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array(["mutation", "productDelete", "input", "id"], json.string),
      ),
      #("extensions", json.object(extensions)),
    ]),
  )
}

@internal
pub fn build_product_delete_invalid_variable_error(
  variable_name: String,
  value: Json,
  loc: Option(Location),
  document: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Variable $"
        <> variable_name
        <> " of type ProductDeleteInput! was provided invalid value for id (Expected value to not be null)",
      ),
    ),
  ]
  let with_locations = case locations_payload(loc, document) {
    Some(locations) -> list.append(base, [#("locations", locations)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "extensions",
        json.object([
          #("code", json.string("INVALID_VARIABLE")),
          #("value", value),
          #(
            "problems",
            json.preprocessed_array([
              json.object([
                #("path", json.array(["id"], json.string)),
                #("explanation", json.string("Expected value to not be null")),
              ]),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

@internal
pub fn serialize_product_user_errors_json(
  errors: List(ProductUserError),
  field: Selection,
) -> Json {
  let selections =
    get_selected_child_fields(field, default_selected_field_options())
  json.array(errors, fn(error) {
    let ProductUserError(field: path, message: message, code: code) = error
    json.object(
      list.map(selections, fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "field" -> json.array(path, json.string)
              "message" -> json.string(message)
              "code" -> optional_string_json(code)
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      }),
    )
  })
}

@internal
pub fn read_tag_inputs(
  args: Dict(String, ResolvedValue),
  allow_comma_separated_string: Bool,
) -> List(String) {
  let values = case dict.get(args, "tags") {
    Ok(ListVal(items)) ->
      list.filter_map(items, fn(value) {
        case value {
          StringVal(tag) -> trimmed_non_empty(tag)
          _ -> Error(Nil)
        }
      })
    Ok(StringVal(raw)) ->
      case allow_comma_separated_string {
        True ->
          string.split(raw, on: ",")
          |> list.filter_map(trimmed_non_empty)
        False -> []
      }
    _ -> []
  }
  dedupe_preserving_order(values)
}

@internal
pub fn vendor_from_shop_domain(domain: String) -> Option(String) {
  case string.split(domain, ".") {
    [subdomain, "myshopify", "com"] ->
      trimmed_non_empty(subdomain) |> option.from_result
    _ -> None
  }
}

@internal
pub fn collapse_product_tag_identity_whitespace(value: String) -> String {
  let #(reversed, _) =
    value
    |> string.to_graphemes
    |> list.fold(#([], False), fn(acc, grapheme) {
      let #(items, in_whitespace) = acc
      case is_product_tag_identity_whitespace(grapheme), in_whitespace {
        True, True -> #(items, True)
        True, False -> #([" ", ..items], True)
        False, _ -> #([grapheme, ..items], False)
      }
    })

  reversed
  |> list.reverse
  |> string.join("")
}

@internal
pub fn ensure_product_set_default_quantities(
  quantities: List(InventoryQuantityRecord),
) -> List(InventoryQuantityRecord) {
  quantities
  |> ensure_inventory_quantity("available", 0)
  |> ensure_inventory_quantity("on_hand", 0)
  |> ensure_inventory_quantity("incoming", 0)
}

@internal
pub fn product_set_metafield_records(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductMetafieldRecord), SyntheticIdentityRegistry, List(String)) {
  let existing_metafields =
    store.get_effective_metafields_by_owner_id(store, product_id)
  let #(reversed, final_identity, ids) =
    list.fold(inputs, #([], identity, []), fn(acc, input) {
      let #(records, current_identity, collected_ids) = acc
      let existing = case read_string_field(input, "id") {
        Some(id) ->
          existing_metafields
          |> list.find(fn(metafield) { metafield.id == id })
          |> option.from_result
        None -> None
      }
      let #(metafield_id, next_identity, ids) = case existing {
        Some(metafield) -> #(metafield.id, current_identity, [metafield.id])
        None -> {
          let #(id, next_identity) =
            synthetic_identity.make_synthetic_gid(current_identity, "Metafield")
          #(id, next_identity, [id])
        }
      }
      let type_ =
        read_string_field(input, "type")
        |> option.or(option.then(existing, fn(metafield) { metafield.type_ }))
      let value =
        read_string_field(input, "value")
        |> option.or(option.then(existing, fn(metafield) { metafield.value }))
      let metafield =
        ProductMetafieldRecord(
          id: metafield_id,
          owner_id: product_id,
          namespace: read_string_field(input, "namespace")
            |> option.unwrap(
              option.map(existing, fn(metafield) { metafield.namespace })
              |> option.unwrap(""),
            ),
          key: read_string_field(input, "key")
            |> option.unwrap(
              option.map(existing, fn(metafield) { metafield.key })
              |> option.unwrap(""),
            ),
          type_: type_,
          value: value,
          compare_digest: None,
          json_value: None,
          created_at: option.then(existing, fn(metafield) {
            metafield.created_at
          }),
          updated_at: option.then(existing, fn(metafield) {
            metafield.updated_at
          }),
          owner_type: Some("PRODUCT"),
          market_localizable_content: option.map(existing, fn(metafield) {
            metafield.market_localizable_content
          })
            |> option.unwrap([]),
        )
      #([metafield, ..records], next_identity, list.append(collected_ids, ids))
    })
  #(list.reverse(reversed), final_identity, ids)
}

@internal
pub fn normalize_product_handle(value: String) -> String {
  value
  |> string.trim
  |> string.lowercase
  |> string.to_graphemes
  |> list.fold(#([], ""), fn(acc, grapheme) {
    let #(parts, current) = acc
    case is_handle_grapheme(grapheme) {
      True -> #(parts, current <> grapheme)
      False ->
        case current {
          "" -> #(parts, "")
          _ -> #([current, ..parts], "")
        }
    }
  })
  |> finish_handle_parts
}

@internal
pub fn split_reversed_trailing_digits(
  graphemes: List(String),
  digits: List(String),
) -> #(List(String), List(String)) {
  case graphemes {
    [] -> #(digits, [])
    [first, ..rest] ->
      case is_handle_digit(first) {
        True -> split_reversed_trailing_digits(rest, [first, ..digits])
        False -> #(digits, graphemes)
      }
  }
}

@internal
pub fn on_hand_component_delta(name: String, delta: Int) -> Int {
  case is_on_hand_component_quantity_name(name) {
    True -> delta
    False -> 0
  }
}

@internal
pub fn format_price_amount(amount: String) -> String {
  let trimmed = string.trim(amount)
  case string.split(trimmed, on: ".") {
    [whole] -> whole <> ".00"
    [whole, cents, ..] -> whole <> "." <> two_digit_cents(cents)
    _ -> trimmed
  }
}

@internal
pub fn read_product_status_field(
  input: Dict(String, ResolvedValue),
) -> Option(String) {
  case read_string_field(input, "status") {
    Some("ACTIVE") -> Some("ACTIVE")
    Some("ARCHIVED") -> Some("ARCHIVED")
    Some("DRAFT") -> Some("DRAFT")
    _ -> None
  }
}

@internal
pub fn updated_product_seo(
  current: ProductSeoRecord,
  input: Dict(String, ResolvedValue),
) -> ProductSeoRecord {
  case dict.get(input, "seo") {
    Ok(ObjectVal(seo)) ->
      ProductSeoRecord(
        title: read_string_field(seo, "title") |> option.or(current.title),
        description: read_string_field(seo, "description")
          |> option.or(current.description),
      )
    _ -> current
  }
}

@internal
pub fn renamed_value_name(
  renamed_values: List(RenamedOptionValue),
  current_name: String,
) -> String {
  case renamed_values {
    [] -> current_name
    [#(from, to), ..rest] ->
      case from == current_name {
        True -> to
        False -> renamed_value_name(rest, current_name)
      }
  }
}

// ===== from products_l02 =====
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
