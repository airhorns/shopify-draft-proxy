//// Read-only Products foundation for the Gleam port.
////
//// The module currently covers Shopify-like no-data behavior for
//// product-adjacent query roots plus the first seeded `product(id:)` detail
//// read. Stateful product lifecycle, variants, inventory, collections,
//// publications, selling plans, and metafields land in later passes before the
//// TS product runtime can be removed.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, type RootFieldError, BoolVal, IntVal, ListVal, NullVal,
  ObjectVal, StringVal, get_field_arguments, get_root_fields,
}
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  paginate_connection_items, project_graphql_value, serialize_connection,
  serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, single_root_log_draft,
}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type InventoryItemRecord, type InventoryLevelRecord,
  type InventoryMeasurementRecord, type InventoryQuantityRecord,
  type InventoryWeightRecord, type ProductCategoryRecord,
  type ProductOptionRecord, type ProductOptionValueRecord, type ProductRecord,
  type ProductSeoRecord, type ProductVariantRecord,
  type ProductVariantSelectedOptionRecord, InventoryItemRecord,
  InventoryWeightFloat, InventoryWeightInt, ProductOptionRecord,
  ProductOptionValueRecord, ProductVariantRecord,
  ProductVariantSelectedOptionRecord,
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
    | "inventoryProperties"
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

pub fn is_products_mutation_root(name: String) -> Bool {
  case name {
    "productOptionsCreate" -> True
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
            "productByIdentifier" ->
              serialize_product_by_identifier_root(
                store,
                field,
                variables,
                fragments,
              )
            "collection"
            | "productFeed"
            | "productResourceFeedback"
            | "productOperation" -> json.null()
            "inventoryItem" ->
              serialize_inventory_item_root(store, field, variables, fragments)
            "inventoryLevel" ->
              serialize_inventory_level_root(store, field, variables, fragments)
            "productVariant" ->
              serialize_product_variant_root(store, field, variables, fragments)
            "productVariantByIdentifier" ->
              serialize_product_variant_by_identifier_root(
                store,
                field,
                variables,
                fragments,
              )
            "products" ->
              serialize_products_connection(store, field, variables, fragments)
            "productVariants" ->
              serialize_product_variants_connection(
                store,
                field,
                variables,
                fragments,
              )
            "inventoryItems" ->
              serialize_inventory_items_connection(
                store,
                field,
                variables,
                fragments,
              )
            "inventoryProperties" ->
              serialize_inventory_properties(field, fragments)
            "productTags" ->
              serialize_string_connection(product_tags(store), field, variables)
            "productTypes" ->
              serialize_string_connection(
                product_types(store),
                field,
                variables,
              )
            "productVendors" ->
              serialize_string_connection(
                product_vendors(store),
                field,
                variables,
              )
            "collections" | "productFeeds" | "productSavedSearches" ->
              serialize_empty_connection(
                field,
                default_selected_field_options(),
              )
            "productsCount" | "productVariantsCount" ->
              serialize_exact_count(field, case name.value {
                "productsCount" ->
                  product_count_for_field(store, field, variables)
                _ -> store.get_effective_product_variant_count(store)
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
            product_source_with_store(store, product),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_product_by_identifier_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_identifier_argument(field, variables) {
    Some(identifier) ->
      case product_by_identifier(store, identifier) {
        Some(product) ->
          project_graphql_value(
            product_source_with_store(store, product),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn product_by_identifier(
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

fn serialize_product_variant_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_variant_by_id(store, id) {
        Some(variant) ->
          project_graphql_value(
            product_variant_source(store, variant),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_product_variant_by_identifier_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_identifier_argument(field, variables) {
    Some(identifier) ->
      case read_string_field(identifier, "id") {
        Some(id) ->
          case store.get_effective_variant_by_id(store, id) {
            Some(variant) ->
              project_graphql_value(
                product_variant_source(store, variant),
                get_selected_child_fields(
                  field,
                  default_selected_field_options(),
                ),
                fragments,
              )
            None -> json.null()
          }
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_inventory_item_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.find_effective_variant_by_inventory_item_id(store, id) {
        Some(variant) ->
          project_graphql_value(
            inventory_item_source(store, variant),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_inventory_level_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.find_effective_inventory_level_by_id(store, id) {
        Some(level) ->
          project_graphql_value(
            inventory_level_source(level),
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
  let products = filtered_products(store, field, variables)
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
      let count = product_count_for_field(store, field, variables)
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
              product_source_with_store(store, product),
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

fn filtered_products(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductRecord) {
  search_query_parser.apply_search_query(
    store.list_effective_products(store),
    read_string_argument(field, variables, "query"),
    product_search_parse_options(),
    fn(product, term) {
      product_matches_positive_query_term(store, product, term)
    },
  )
}

fn product_search_parse_options() -> search_query_parser.SearchQueryParseOptions {
  search_query_parser.SearchQueryParseOptions(
    ..search_query_parser.default_parse_options(),
    recognize_not_keyword: True,
  )
}

fn product_string_match_options() -> search_query_parser.SearchQueryStringMatchOptions {
  search_query_parser.SearchQueryStringMatchOptions(word_prefix: True)
}

fn product_count_for_field(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Int {
  case read_string_argument(field, variables, "query") {
    Some(_) -> list.length(filtered_products(store, field, variables))
    None -> store.get_effective_product_count(store)
  }
}

fn product_matches_positive_query_term(
  store: Store,
  product: ProductRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  case option.map(term.field, string.lowercase) {
    None -> product_matches_search_text(product, term.value)
    Some("id") -> product_id_matches(product, term.value)
    Some("title") ->
      search_query_parser.matches_search_query_string(
        Some(product.title),
        search_query_parser.search_query_term_value(term),
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
    Some("handle") ->
      search_query_parser.matches_search_query_string(
        Some(product.handle),
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("tag") ->
      list.any(product.tags, fn(tag) {
        search_query_parser.matches_search_query_string(
          Some(tag),
          search_query_parser.search_query_term_value(term),
          search_query_parser.ExactMatch,
          product_string_match_options(),
        )
      })
    Some("product_type") ->
      search_query_parser.matches_search_query_string(
        product.product_type,
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("vendor") ->
      search_query_parser.matches_search_query_string(
        product.vendor,
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("status") ->
      search_query_parser.matches_search_query_string(
        Some(product.status),
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("sku") ->
      store.get_effective_variants_by_product_id(store, product.id)
      |> list.any(fn(variant) {
        search_query_parser.matches_search_query_string(
          variant.sku,
          search_query_parser.search_query_term_value(term),
          search_query_parser.ExactMatch,
          product_string_match_options(),
        )
      })
    Some("inventory_total") ->
      search_query_parser.matches_search_query_number(
        option.map(product.total_inventory, int.to_float),
        term,
      )
    _ -> True
  }
}

fn product_matches_search_text(
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

fn product_id_matches(product: ProductRecord, raw_value: String) -> Bool {
  resource_id_matches(product.id, product.legacy_resource_id, raw_value)
}

fn resource_id_matches(
  resource_id: String,
  legacy_resource_id: Option(String),
  raw_value: String,
) -> Bool {
  let normalized =
    search_query_parser.strip_search_query_value_quotes(raw_value)
    |> string.trim
  case normalized {
    "" -> True
    _ -> {
      resource_id == normalized
      || option.unwrap(legacy_resource_id, "") == normalized
      || resource_tail(resource_id) == normalized
      || resource_tail(normalized) == resource_tail(resource_id)
    }
  }
}

fn resource_tail(id: String) -> String {
  case list.last(string.split(id, "/")) {
    Ok(tail) -> tail
    Error(_) -> id
  }
}

fn serialize_inventory_items_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let variants =
    filtered_inventory_item_variants(store, field, variables)
    |> reverse_inventory_item_variants(field, variables)
  let window =
    paginate_connection_items(
      variants,
      field,
      variables,
      inventory_item_variant_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: inventory_item_variant_cursor,
      serialize_node: fn(variant, node_field, _index) {
        project_graphql_value(
          inventory_item_source(store, variant),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn filtered_inventory_item_variants(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductVariantRecord) {
  search_query_parser.apply_search_query(
    inventory_item_variants(store),
    read_string_argument(field, variables, "query"),
    product_search_parse_options(),
    inventory_item_variant_matches_positive_query_term,
  )
}

fn inventory_item_variants(store: Store) -> List(ProductVariantRecord) {
  store.list_effective_product_variants(store)
  |> list.filter(fn(variant) {
    case variant.inventory_item {
      Some(_) -> True
      None -> False
    }
  })
  |> list.sort(fn(left, right) {
    string.compare(
      inventory_item_variant_cursor(left, 0),
      inventory_item_variant_cursor(right, 0),
    )
  })
}

fn reverse_inventory_item_variants(
  variants: List(ProductVariantRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductVariantRecord) {
  case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(variants)
    _ -> variants
  }
}

fn inventory_item_variant_matches_positive_query_term(
  variant: ProductVariantRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  case variant.inventory_item {
    Some(item) -> {
      let value = search_query_parser.search_query_term_value(term)
      case option.map(term.field, string.lowercase) {
        None ->
          list.any(
            [item.id, option.unwrap(variant.sku, ""), variant.id],
            fn(candidate) {
              search_query_parser.matches_search_query_string(
                Some(candidate),
                value,
                search_query_parser.IncludesMatch,
                product_string_match_options(),
              )
            },
          )
        Some("id") -> resource_id_matches(item.id, None, value)
        Some("sku") ->
          search_query_parser.matches_search_query_string(
            variant.sku,
            value,
            search_query_parser.ExactMatch,
            product_string_match_options(),
          )
        Some("tracked") ->
          bool_string(option.unwrap(item.tracked, False))
          == string.lowercase(value)
        _ -> True
      }
    }
    None -> False
  }
}

fn bool_string(value: Bool) -> String {
  case value {
    True -> "true"
    False -> "false"
  }
}

fn inventory_item_variant_cursor(
  variant: ProductVariantRecord,
  _index: Int,
) -> String {
  case variant.inventory_item {
    Some(item) -> item.id
    None -> variant.id
  }
}

fn serialize_product_variants_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let variants =
    store.list_effective_product_variants(store)
    |> list.sort(fn(left, right) {
      resource_ids.compare_shopify_resource_ids(left.id, right.id)
    })
  let ordered_variants = case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(variants)
    _ -> variants
  }
  let window =
    paginate_connection_items(
      ordered_variants,
      field,
      variables,
      product_variant_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: product_variant_cursor,
      serialize_node: fn(variant, node_field, _index) {
        project_graphql_value(
          product_variant_source(store, variant),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_inventory_properties(
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    inventory_properties_source(),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_properties_source() -> SourceValue {
  src_object([
    #(
      "quantityNames",
      SrcList(list.map(
        inventory_quantity_name_definitions(),
        inventory_quantity_name_source,
      )),
    ),
  ])
}

fn inventory_quantity_name_definitions() -> List(
  #(String, String, Bool, List(String), List(String)),
) {
  [
    #("available", "Available", True, ["on_hand"], []),
    #("committed", "Committed", True, ["on_hand"], []),
    #("damaged", "Damaged", False, ["on_hand"], []),
    #("incoming", "Incoming", False, [], []),
    #("on_hand", "On hand", True, [], [
      "available",
      "committed",
      "damaged",
      "quality_control",
      "reserved",
      "safety_stock",
    ]),
    #("quality_control", "Quality control", False, ["on_hand"], []),
    #("reserved", "Reserved", True, ["on_hand"], []),
    #("safety_stock", "Safety stock", False, ["on_hand"], []),
  ]
}

fn inventory_quantity_name_source(
  definition: #(String, String, Bool, List(String), List(String)),
) -> SourceValue {
  let #(name, display_name, is_in_use, belongs_to, comprises) = definition
  src_object([
    #("name", SrcString(name)),
    #("displayName", SrcString(display_name)),
    #("isInUse", SrcBool(is_in_use)),
    #("belongsTo", SrcList(list.map(belongs_to, SrcString))),
    #("comprises", SrcList(list.map(comprises, SrcString))),
  ])
}

fn serialize_string_connection(
  values: List(String),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let sorted_values = normalize_string_catalog(values)
  let ordered_values = case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(sorted_values)
    _ -> sorted_values
  }
  let window =
    paginate_connection_items(
      ordered_values,
      field,
      variables,
      string_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: string_cursor,
      serialize_node: fn(value, _node_field, _index) { json.string(value) },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn product_tags(store: Store) -> List(String) {
  store.list_effective_products(store)
  |> list.flat_map(fn(product) { product.tags })
}

fn product_types(store: Store) -> List(String) {
  store.list_effective_products(store)
  |> list.filter_map(fn(product) {
    case product.product_type {
      Some(value) -> Ok(value)
      None -> Error(Nil)
    }
  })
}

fn product_vendors(store: Store) -> List(String) {
  store.list_effective_products(store)
  |> list.filter_map(fn(product) {
    case product.vendor {
      Some(value) -> Ok(value)
      None -> Error(Nil)
    }
  })
}

fn normalize_string_catalog(values: List(String)) -> List(String) {
  values
  |> list.filter(fn(value) { string.length(string.trim(value)) > 0 })
  |> list.fold(dict.new(), fn(seen, value) { dict.insert(seen, value, True) })
  |> dict.keys()
  |> list.sort(string.compare)
}

fn string_cursor(value: String, _index: Int) -> String {
  value
}

fn product_variant_cursor(
  variant: ProductVariantRecord,
  _index: Int,
) -> String {
  case variant.cursor {
    Some(cursor) -> cursor
    None -> variant.id
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

fn read_identifier_argument(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(Dict(String, ResolvedValue)) {
  case get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, "identifier") {
        Ok(ObjectVal(identifier)) -> Some(identifier)
        _ -> None
      }
    Error(_) -> None
  }
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

fn read_bool_argument(
  field: Selection,
  variables: Dict(String, ResolvedValue),
  name: String,
) -> Option(Bool) {
  case get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(BoolVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

fn read_string_field(
  fields: Dict(String, ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(fields, name) {
    Ok(StringVal(value)) -> Some(value)
    _ -> None
  }
}

fn optional_string(value: Option(String)) -> Json {
  case value {
    Some(value) -> json.string(value)
    None -> json.null()
  }
}

pub fn product_source(product: ProductRecord) -> SourceValue {
  product_source_with_relationships(
    product,
    empty_connection_source(),
    SrcList([]),
  )
}

fn product_source_with_store(
  store: Store,
  product: ProductRecord,
) -> SourceValue {
  product_source_with_relationships(
    product,
    product_variants_connection_source(store, product),
    product_options_source(store.get_effective_options_by_product_id(
      store,
      product.id,
    )),
  )
}

fn product_source_with_relationships(
  product: ProductRecord,
  variants: SourceValue,
  options: SourceValue,
) -> SourceValue {
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
    #("options", options),
    #("variants", variants),
  ])
}

fn product_options_source(options: List(ProductOptionRecord)) -> SourceValue {
  SrcList(list.map(options, product_option_source))
}

fn product_option_source(option: ProductOptionRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductOption")),
    #("id", SrcString(option.id)),
    #("name", SrcString(option.name)),
    #("position", SrcInt(option.position)),
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

fn product_option_value_source(
  option_value: ProductOptionValueRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductOptionValue")),
    #("id", SrcString(option_value.id)),
    #("name", SrcString(option_value.name)),
    #("hasVariants", SrcBool(option_value.has_variants)),
  ])
}

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

fn product_variants_connection_source(
  store: Store,
  product: ProductRecord,
) -> SourceValue {
  let variants = store.get_effective_variants_by_product_id(store, product.id)
  let edges =
    variants
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(variant, index) = pair
      src_object([
        #("cursor", SrcString(product_variant_cursor(variant, index))),
        #("node", product_variant_source(store, variant)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(variants, fn(variant) {
          product_variant_source(store, variant)
        }),
      ),
    ),
    #("pageInfo", connection_page_info_source(variants, product_variant_cursor)),
  ])
}

pub fn product_variant_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  product_variant_source_with_inventory(
    store,
    variant,
    variant_inventory_item_source(variant),
  )
}

fn product_variant_source_without_inventory(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  product_variant_source_with_inventory(store, variant, SrcNull)
}

fn product_variant_source_with_inventory(
  store: Store,
  variant: ProductVariantRecord,
  inventory_item: SourceValue,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductVariant")),
    #("id", SrcString(variant.id)),
    #("title", SrcString(variant.title)),
    #("sku", optional_string_source(variant.sku)),
    #("barcode", optional_string_source(variant.barcode)),
    #("price", optional_string_source(variant.price)),
    #("compareAtPrice", optional_string_source(variant.compare_at_price)),
    #("taxable", optional_bool_source(variant.taxable)),
    #("inventoryPolicy", optional_string_source(variant.inventory_policy)),
    #("inventoryQuantity", optional_int_source(variant.inventory_quantity)),
    #(
      "selectedOptions",
      SrcList(list.map(variant.selected_options, selected_option_source)),
    ),
    #("inventoryItem", inventory_item),
    #("product", variant_product_source(store, variant.product_id)),
  ])
}

fn variant_inventory_item_source(variant: ProductVariantRecord) -> SourceValue {
  case variant.inventory_item {
    Some(item) -> inventory_item_source_without_variant(item)
    None -> SrcNull
  }
}

fn inventory_item_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  case variant.inventory_item {
    Some(item) ->
      inventory_item_source_with_variant(
        item,
        product_variant_source_without_inventory(store, variant),
      )
    None -> SrcNull
  }
}

fn inventory_item_source_without_variant(
  item: InventoryItemRecord,
) -> SourceValue {
  inventory_item_source_with_variant(item, SrcNull)
}

fn inventory_item_source_with_variant(
  item: InventoryItemRecord,
  variant: SourceValue,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("InventoryItem")),
    #("id", SrcString(item.id)),
    #("tracked", optional_bool_source(item.tracked)),
    #("requiresShipping", optional_bool_source(item.requires_shipping)),
    #("measurement", optional_measurement_source(item.measurement)),
    #(
      "countryCodeOfOrigin",
      optional_string_source(item.country_code_of_origin),
    ),
    #(
      "provinceCodeOfOrigin",
      optional_string_source(item.province_code_of_origin),
    ),
    #(
      "harmonizedSystemCode",
      optional_string_source(item.harmonized_system_code),
    ),
    #(
      "inventoryLevels",
      inventory_levels_connection_source(item.inventory_levels),
    ),
    #("variant", variant),
  ])
}

fn inventory_levels_connection_source(
  levels: List(InventoryLevelRecord),
) -> SourceValue {
  let edges =
    levels
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(level, index) = pair
      src_object([
        #("cursor", SrcString(inventory_level_cursor(level, index))),
        #("node", inventory_level_source(level)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #("nodes", SrcList(list.map(levels, inventory_level_source))),
    #("pageInfo", connection_page_info_source(levels, inventory_level_cursor)),
  ])
}

fn inventory_level_source(level: InventoryLevelRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("InventoryLevel")),
    #("id", SrcString(level.id)),
    #(
      "location",
      src_object([
        #("__typename", SrcString("Location")),
        #("id", SrcString(level.location.id)),
        #("name", SrcString(level.location.name)),
      ]),
    ),
    #("quantities", SrcList(list.map(level.quantities, quantity_source))),
  ])
}

fn quantity_source(quantity: InventoryQuantityRecord) -> SourceValue {
  src_object([
    #("name", SrcString(quantity.name)),
    #("quantity", SrcInt(quantity.quantity)),
    #("updatedAt", optional_string_source(quantity.updated_at)),
  ])
}

fn optional_measurement_source(
  measurement: Option(InventoryMeasurementRecord),
) -> SourceValue {
  case measurement {
    Some(value) ->
      src_object([#("weight", optional_weight_source(value.weight))])
    None -> SrcNull
  }
}

fn optional_weight_source(
  weight: Option(InventoryWeightRecord),
) -> SourceValue {
  case weight {
    Some(value) ->
      src_object([
        #("unit", SrcString(value.unit)),
        #("value", inventory_weight_value_source(value.value)),
      ])
    None -> SrcNull
  }
}

fn inventory_weight_value_source(value) -> SourceValue {
  case value {
    InventoryWeightInt(value) -> SrcInt(value)
    InventoryWeightFloat(value) -> SrcFloat(value)
  }
}

fn connection_page_info_source(
  items: List(a),
  get_cursor: fn(a, Int) -> String,
) -> SourceValue {
  src_object([
    #("hasNextPage", SrcBool(False)),
    #("hasPreviousPage", SrcBool(False)),
    #("startCursor", connection_start_cursor(items, get_cursor)),
    #("endCursor", connection_end_cursor(items, get_cursor)),
  ])
}

fn connection_start_cursor(
  items: List(a),
  get_cursor: fn(a, Int) -> String,
) -> SourceValue {
  case items {
    [first, ..] -> SrcString(get_cursor(first, 0))
    [] -> SrcNull
  }
}

fn connection_end_cursor(
  items: List(a),
  get_cursor: fn(a, Int) -> String,
) -> SourceValue {
  case list.last(items) {
    Ok(last) -> SrcString(get_cursor(last, list.length(items) - 1))
    Error(_) -> SrcNull
  }
}

fn inventory_level_cursor(level: InventoryLevelRecord, _index: Int) -> String {
  case level.cursor {
    Some(cursor) -> cursor
    None -> level.id
  }
}

fn enumerate_items(items: List(a)) -> List(#(a, Int)) {
  enumerate_items_loop(items, 0, [])
}

fn enumerate_items_loop(
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

fn selected_option_source(
  selected_option: ProductVariantSelectedOptionRecord,
) -> SourceValue {
  src_object([
    #("name", SrcString(selected_option.name)),
    #("value", SrcString(selected_option.value)),
  ])
}

fn variant_product_source(store: Store, product_id: String) -> SourceValue {
  case store.get_effective_product_by_id(store, product_id) {
    Some(product) -> product_source(product)
    None -> SrcNull
  }
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

pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

type ProductUserError {
  ProductUserError(field: List(String), message: String, code: Option(String))
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
  )
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, ResolvedValue),
) -> Result(MutationOutcome, ProductsError) {
  case get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_mutation_fields(store, identity, fields, fragments, variables))
    }
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationOutcome {
  let initial = #([], store, identity, [], [])
  let #(data_entries, final_store, final_identity, all_staged, all_drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids, drafts) = acc
      case field {
        Field(name: name, ..) ->
          case name.value {
            "productOptionsCreate" -> {
              let result =
                handle_product_options_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store.Staged,
                  "products",
                  "stage-locally",
                  Some("Gleam staged productOptionsCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            _ -> acc
          }
        _ -> acc
      }
    })
  MutationOutcome(
    data: json.object([#("data", json.object(data_entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: all_staged,
    log_drafts: all_drafts,
  )
}

fn handle_product_options_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  case read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_options_create_payload(
          store,
          None,
          [ProductUserError(["productId"], "Product id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_options_create_payload(
              store,
              None,
              [
                ProductUserError(["productId"], "Product does not exist", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) ->
          stage_product_options_create(
            store,
            identity,
            key,
            product_id,
            read_option_create_inputs(args),
            read_arg_string(args, "variantStrategy") == Some("CREATE"),
            field,
            fragments,
          )
      }
  }
}

fn stage_product_options_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  option_inputs: List(Dict(String, ResolvedValue)),
  should_create_option_variants: Bool,
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let existing_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  let replacing_default =
    product_uses_only_default_option_state(existing_options, existing_variants)
  let starting_options = case replacing_default {
    True -> []
    False -> existing_options
  }
  let #(created_options, identity_after_options) =
    make_created_option_records(identity, product_id, option_inputs)
  let next_options =
    list.append(starting_options, created_options)
    |> sort_and_position_options
  let next_variants = case replacing_default, existing_variants {
    True, [first_variant, ..] -> [
      remap_variant_to_first_option_values(first_variant, next_options),
    ]
    _, _ ->
      map_variants_to_first_new_option_values(
        existing_variants,
        created_options,
      )
  }
  let #(next_variants, final_identity) = case should_create_option_variants {
    True ->
      create_variants_for_option_value_combinations(
        identity_after_options,
        product_id,
        next_options,
        created_options,
        next_variants,
      )
    False -> #(next_variants, identity_after_options)
  }
  let synced_options =
    sync_product_options_with_variants(next_options, next_variants)
  let next_store =
    store
    |> store.replace_staged_variants_for_product(product_id, next_variants)
    |> store.replace_staged_options_for_product(product_id, synced_options)
  let staged_ids =
    list.append(
      list.map(created_options, fn(option) { option.id }),
      list.flat_map(created_options, fn(option) {
        list.map(option.option_values, fn(value) { value.id })
      }),
    )
  mutation_result(
    key,
    product_options_create_payload(
      next_store,
      store.get_effective_product_by_id(next_store, product_id),
      [],
      field,
      fragments,
    ),
    next_store,
    final_identity,
    staged_ids,
  )
}

fn mutation_result(
  key: String,
  payload: Json,
  store: Store,
  identity: SyntheticIdentityRegistry,
  staged_resource_ids: List(String),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: payload,
    store: store,
    identity: identity,
    staged_resource_ids: staged_resource_ids,
  )
}

fn product_options_create_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductOptionsCreatePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn user_errors_source(errors: List(ProductUserError)) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let ProductUserError(field: field, message: message, code: code) = error
      src_object([
        #("field", SrcList(list.map(field, SrcString))),
        #("message", SrcString(message)),
        #("code", optional_string_source(code)),
      ])
    }),
  )
}

fn field_args(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Dict(String, ResolvedValue) {
  case get_field_arguments(field, variables) {
    Ok(args) -> args
    Error(_) -> dict.new()
  }
}

fn read_arg_string(
  args: Dict(String, ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(StringVal(value)) -> Some(value)
    Ok(NullVal) -> None
    _ -> None
  }
}

fn read_option_create_inputs(
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

fn make_created_option_records(
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

fn make_created_option_record(
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

fn make_created_option_value_records(
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

fn read_option_value_names(input: Dict(String, ResolvedValue)) -> List(String) {
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

fn read_int_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(Int) {
  case dict.get(input, name) {
    Ok(IntVal(value)) -> Some(value)
    _ -> None
  }
}

fn product_uses_only_default_option_state(
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

fn option_values_are_default(values: List(ProductOptionValueRecord)) -> Bool {
  case values {
    [value] -> value.name == "Default Title"
    _ -> False
  }
}

fn sort_and_position_options(
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

fn position_options(
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

fn remap_variant_to_first_option_values(
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

fn map_variants_to_first_new_option_values(
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

fn first_option_value_name(option: ProductOptionRecord) -> String {
  case option.option_values {
    [value, ..] -> value.name
    [] -> "Default Title"
  }
}

fn variant_title(
  selected_options: List(ProductVariantSelectedOptionRecord),
) -> String {
  selected_options
  |> list.map(fn(selected) { selected.value })
  |> string.join(" / ")
}

fn sync_product_options_with_variants(
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

fn variants_use_option_value(
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

type VariantCombination =
  List(ProductVariantSelectedOptionRecord)

fn create_variants_for_option_value_combinations(
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

fn create_variants_for_single_new_option(
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

fn remaining_option_values(
  option: ProductOptionRecord,
) -> List(ProductOptionValueRecord) {
  case option.option_values {
    [] -> []
    [_, ..rest] -> rest
  }
}

fn variant_selected_options_with_value(
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

fn create_variants_for_all_combinations(
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

fn option_value_combinations(
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

fn find_variant_for_combination(
  variants: List(ProductVariantRecord),
  combination: VariantCombination,
) -> Option(ProductVariantRecord) {
  variants
  |> list.find(fn(variant) {
    selected_options_equal(variant.selected_options, combination)
  })
  |> option.from_result
}

fn selected_options_equal(
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

fn variant_for_combination(
  variant: ProductVariantRecord,
  combination: VariantCombination,
) -> ProductVariantRecord {
  ProductVariantRecord(
    ..variant,
    title: variant_title(combination),
    selected_options: combination,
  )
}

fn make_variant_for_combination(
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
      inventory_item: inventory_item,
      cursor: None,
    ),
    final_identity,
  )
}

fn make_inventory_item_for_variant(
  identity: SyntheticIdentityRegistry,
  template: Option(ProductVariantRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry) {
  let template_item =
    option.then(template, fn(variant) { variant.inventory_item })
  case template_item {
    Some(item) -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        Some(InventoryItemRecord(..item, id: id, inventory_levels: [])),
        next_identity,
      )
    }
    None -> #(None, identity)
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
