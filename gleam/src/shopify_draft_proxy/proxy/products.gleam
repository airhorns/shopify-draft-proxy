//// Read-only Products foundation for the Gleam port.
////
//// The module currently covers Shopify-like no-data behavior for
//// product-adjacent query roots plus the first seeded `product(id:)` detail
//// read. Stateful product lifecycle, variants, inventory, collections,
//// publications, selling plans, and metafields land in later passes before the
//// TS product runtime can be removed.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, type RootFieldError, StringVal, get_field_arguments,
  get_root_fields,
}
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcInt, SrcList, SrcNull, SrcString,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  paginate_connection_items, project_graphql_value, serialize_connection,
  serialize_empty_connection, src_object,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type ProductCategoryRecord, type ProductRecord, type ProductSeoRecord,
}

pub type ProductsError {
  ParseFailed(RootFieldError)
}

pub fn is_products_query_root(name: String) -> Bool {
  case name {
    "product"
    | "productByIdentifier"
    | "products"
    | "productsCount"
    | "collection"
    | "collections"
    | "productVariant"
    | "productVariantByIdentifier"
    | "productVariants"
    | "productVariantsCount"
    | "inventoryItem"
    | "inventoryItems"
    | "inventoryLevel"
    | "productFeed"
    | "productFeeds"
    | "productTags"
    | "productTypes"
    | "productVendors"
    | "productSavedSearches"
    | "productResourceFeedback"
    | "productOperation"
    | "productDuplicateJob" -> True
    _ -> False
  }
}

pub fn handle_products_query(
  store: Store,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(Json, ProductsError) {
  case get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) ->
      Ok(serialize_root_fields(
        store,
        fields,
        variables,
        get_document_fragments(document),
      ))
  }
}

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = case field {
        Field(name: name, ..) ->
          case name.value {
            "product" ->
              serialize_product_root(store, field, variables, fragments)
            "productByIdentifier"
            | "collection"
            | "productVariant"
            | "productVariantByIdentifier"
            | "inventoryItem"
            | "inventoryLevel"
            | "productFeed"
            | "productResourceFeedback"
            | "productOperation" -> json.null()
            "products" ->
              serialize_products_connection(store, field, variables, fragments)
            "collections"
            | "productVariants"
            | "inventoryItems"
            | "productFeeds"
            | "productTags"
            | "productTypes"
            | "productVendors"
            | "productSavedSearches" ->
              serialize_empty_connection(
                field,
                default_selected_field_options(),
              )
            "productsCount" | "productVariantsCount" ->
              serialize_exact_count(field, case name.value {
                "productsCount" -> store.get_effective_product_count(store)
                _ -> 0
              })
            "productDuplicateJob" ->
              serialize_product_duplicate_job(field, variables)
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    })
  json.object(entries)
}

fn serialize_product_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_product_by_id(store, id) {
        Some(product) ->
          project_graphql_value(
            product_source(product),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_products_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let products = store.list_effective_products(store)
  case products {
    [] -> serialize_empty_connection(field, default_selected_field_options())
    _ -> {
      let window =
        paginate_connection_items(
          products,
          field,
          variables,
          product_cursor,
          default_connection_window_options(),
        )
      let count = store.get_effective_product_count(store)
      let has_next_page =
        window.has_next_page || count > list.length(window.items)
      serialize_connection(
        field,
        SerializeConnectionConfig(
          items: window.items,
          has_next_page: has_next_page,
          has_previous_page: window.has_previous_page,
          get_cursor_value: product_cursor,
          serialize_node: fn(product, node_field, _index) {
            project_graphql_value(
              product_source(product),
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
  }
}

fn product_cursor(product: ProductRecord, _index: Int) -> String {
  case product.cursor {
    Some(cursor) -> cursor
    None -> product.id
  }
}

fn serialize_exact_count(field: Selection, count: Int) -> Json {
  let entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "count" -> #(key, json.int(count))
              "precision" -> #(key, json.string("EXACT"))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

fn serialize_product_duplicate_job(
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

fn read_string_argument(
  field: Selection,
  variables: Dict(String, ResolvedValue),
  name: String,
) -> Option(String) {
  case get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(StringVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

fn optional_string(value: Option(String)) -> Json {
  case value {
    Some(value) -> json.string(value)
    None -> json.null()
  }
}

pub fn product_source(product: ProductRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Product")),
    #("id", SrcString(product.id)),
    #("legacyResourceId", optional_string_source(product.legacy_resource_id)),
    #("title", SrcString(product.title)),
    #("handle", SrcString(product.handle)),
    #("status", SrcString(product.status)),
    #("vendor", optional_string_source(product.vendor)),
    #("productType", optional_string_source(product.product_type)),
    #("tags", SrcList(list.map(product.tags, SrcString))),
    #("totalInventory", optional_int_source(product.total_inventory)),
    #("tracksInventory", optional_bool_source(product.tracks_inventory)),
    #("createdAt", optional_string_source(product.created_at)),
    #("updatedAt", optional_string_source(product.updated_at)),
    #("descriptionHtml", SrcString(product.description_html)),
    #(
      "onlineStorePreviewUrl",
      optional_string_source(product.online_store_preview_url),
    ),
    #("templateSuffix", optional_string_source(product.template_suffix)),
    #("seo", product_seo_source(product.seo)),
    #("category", optional_product_category_source(product.category)),
    #("collections", empty_connection_source()),
    #("media", empty_connection_source()),
  ])
}

fn product_seo_source(seo: ProductSeoRecord) -> SourceValue {
  src_object([
    #("title", optional_string_source(seo.title)),
    #("description", optional_string_source(seo.description)),
  ])
}

fn optional_product_category_source(
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

fn empty_connection_source() -> SourceValue {
  src_object([
    #("edges", SrcList([])),
    #("nodes", SrcList([])),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", SrcNull),
        #("endCursor", SrcNull),
      ]),
    ),
  ])
}

fn optional_string_source(value: Option(String)) -> SourceValue {
  case value {
    Some(value) -> SrcString(value)
    None -> SrcNull
  }
}

fn optional_int_source(value: Option(Int)) -> SourceValue {
  case value {
    Some(value) -> SrcInt(value)
    None -> SrcNull
  }
}

fn optional_bool_source(value: Option(Bool)) -> SourceValue {
  case value {
    Some(value) -> SrcBool(value)
    None -> SrcNull
  }
}

pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(Json, ProductsError) {
  use data <- result.try(handle_products_query(store, document, variables))
  Ok(wrap_data(data))
}
