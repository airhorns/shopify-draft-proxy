//// Read-only Products foundation for the Gleam port.
////
//// This pass intentionally covers Shopify-like no-data behavior for product
//// and product-adjacent query roots. Stateful product lifecycle, variants,
//// inventory, collections, publications, selling plans, and metafields land in
//// later passes before the TS product runtime can be removed.

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
  default_selected_field_options, get_field_response_key,
  get_selected_child_fields, serialize_empty_connection,
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
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(Json, ProductsError) {
  case get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> Ok(serialize_root_fields(fields, variables))
  }
}

fn serialize_root_fields(
  fields: List(Selection),
  variables: Dict(String, ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = case field {
        Field(name: name, ..) ->
          case name.value {
            "product"
            | "productByIdentifier"
            | "collection"
            | "productVariant"
            | "productVariantByIdentifier"
            | "inventoryItem"
            | "inventoryLevel"
            | "productFeed"
            | "productResourceFeedback"
            | "productOperation" -> json.null()
            "products"
            | "collections"
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
              serialize_exact_zero_count(field)
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

fn serialize_exact_zero_count(field: Selection) -> Json {
  let entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "count" -> #(key, json.int(0))
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

pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

pub fn process(
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(Json, ProductsError) {
  use data <- result.try(handle_products_query(document, variables))
  Ok(wrap_data(data))
}
