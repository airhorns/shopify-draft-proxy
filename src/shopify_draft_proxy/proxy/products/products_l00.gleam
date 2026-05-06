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
                isActive
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
          isActive
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
      isActive
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
            isActive
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
