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
import shopify_draft_proxy/graphql/ast.{
  type Definition, type Location, type ObjectField, type Selection,
  type VariableDefinition, Field, NullValue, ObjectField, ObjectValue,
  OperationDefinition, VariableDefinition, VariableValue,
}
import shopify_draft_proxy/graphql/location as graphql_location
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/parser
import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, type RootFieldError, BoolVal, FloatVal, IntVal, ListVal,
  NullVal, ObjectVal, StringVal, get_field_arguments, get_root_fields,
}
import shopify_draft_proxy/graphql/source as graphql_source
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
  type LogDraft, build_null_argument_error, find_argument, single_root_log_draft,
}
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CollectionImageRecord, type CollectionRecord, type CollectionRuleRecord,
  type CollectionRuleSetRecord, type InventoryItemRecord,
  type InventoryLevelRecord, type InventoryLocationRecord,
  type InventoryMeasurementRecord, type InventoryQuantityRecord,
  type InventoryWeightRecord, type InventoryWeightValue,
  type ProductCategoryRecord, type ProductCollectionRecord,
  type ProductOptionRecord, type ProductOptionValueRecord, type ProductRecord,
  type ProductSeoRecord, type ProductVariantRecord,
  type ProductVariantSelectedOptionRecord, CollectionRecord, InventoryItemRecord,
  InventoryLevelRecord, InventoryLocationRecord, InventoryMeasurementRecord,
  InventoryQuantityRecord, InventoryWeightFloat, InventoryWeightInt,
  InventoryWeightRecord, ProductCollectionRecord, ProductOptionRecord,
  ProductOptionValueRecord, ProductRecord, ProductSeoRecord,
  ProductVariantRecord, ProductVariantSelectedOptionRecord,
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
    | "collectionByIdentifier"
    | "collectionByHandle"
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
    "productOptionsCreate"
    | "productCreate"
    | "productOptionUpdate"
    | "productOptionsDelete"
    | "productOptionsReorder"
    | "productChangeStatus"
    | "productDelete"
    | "productUpdate"
    | "productVariantCreate"
    | "productVariantUpdate"
    | "productVariantDelete"
    | "productVariantsBulkCreate"
    | "productVariantsBulkUpdate"
    | "productVariantsBulkDelete"
    | "productVariantsBulkReorder"
    | "inventoryAdjustQuantities"
    | "inventoryActivate"
    | "inventoryDeactivate"
    | "inventoryBulkToggleActivation"
    | "inventoryItemUpdate"
    | "inventorySetQuantities"
    | "inventoryMoveQuantities"
    | "collectionAddProducts"
    | "collectionRemoveProducts"
    | "collectionReorderProducts"
    | "collectionUpdate"
    | "collectionDelete"
    | "collectionCreate"
    | "tagsAdd"
    | "tagsRemove" -> True
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
            "collection" ->
              serialize_collection_root(store, field, variables, fragments)
            "collectionByIdentifier" ->
              serialize_collection_by_identifier_root(
                store,
                field,
                variables,
                fragments,
              )
            "collectionByHandle" ->
              serialize_collection_by_handle_root(
                store,
                field,
                variables,
                fragments,
              )
            "productFeed" | "productResourceFeedback" | "productOperation" ->
              json.null()
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
            "collections" ->
              serialize_collections_connection(
                store,
                field,
                variables,
                fragments,
              )
            "productFeeds" | "productSavedSearches" ->
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

fn serialize_collection_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_collection_by_id(store, id) {
        Some(collection) ->
          serialize_collection_object(
            store,
            collection,
            get_selected_child_fields(field, default_selected_field_options()),
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_collection_by_identifier_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_identifier_argument(field, variables) {
    Some(identifier) ->
      case collection_by_identifier(store, identifier) {
        Some(collection) ->
          serialize_collection_object(
            store,
            collection,
            get_selected_child_fields(field, default_selected_field_options()),
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_collection_by_handle_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "handle") {
    Some(handle) ->
      case store.get_effective_collection_by_handle(store, handle) {
        Some(collection) ->
          serialize_collection_object(
            store,
            collection,
            get_selected_child_fields(field, default_selected_field_options()),
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn collection_by_identifier(
  store: Store,
  identifier: Dict(String, ResolvedValue),
) -> Option(CollectionRecord) {
  case read_string_field(identifier, "id") {
    Some(id) -> store.get_effective_collection_by_id(store, id)
    None ->
      case read_string_field(identifier, "handle") {
        Some(handle) -> store.get_effective_collection_by_handle(store, handle)
        None -> None
      }
  }
}

fn serialize_collection_object(
  store: Store,
  collection: CollectionRecord,
  selections: List(Selection),
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          serialize_collection_field(
            store,
            collection,
            selection,
            name.value,
            variables,
            fragments,
          )
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

fn serialize_collection_field(
  store: Store,
  collection: CollectionRecord,
  field: Selection,
  field_name: String,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case field_name {
    "__typename" -> json.string("Collection")
    "id" -> json.string(collection.id)
    "legacyResourceId" ->
      json.string(
        collection.legacy_resource_id
        |> option.unwrap(legacy_resource_id_from_gid(collection.id)),
      )
    "title" -> json.string(collection.title)
    "handle" -> json.string(collection.handle)
    "updatedAt" -> optional_string_json(collection.updated_at)
    "description" -> optional_string_json(collection.description)
    "descriptionHtml" -> optional_string_json(collection.description_html)
    "image" ->
      serialize_collection_image(
        collection.image,
        get_selected_child_fields(field, default_selected_field_options()),
      )
    "productsCount" ->
      serialize_exact_count(
        field,
        collection.products_count
          |> option.unwrap(
            list.length(store.list_effective_products_for_collection(
              store,
              collection.id,
            )),
          ),
      )
    "hasProduct" ->
      json.bool(collection_has_product(store, collection.id, field, variables))
    "sortOrder" -> optional_string_json(collection.sort_order)
    "templateSuffix" -> optional_string_json(collection.template_suffix)
    "seo" ->
      project_graphql_value(
        product_seo_source(collection.seo),
        get_selected_child_fields(field, default_selected_field_options()),
        fragments,
      )
    "ruleSet" ->
      serialize_collection_rule_set(
        collection.rule_set,
        get_selected_child_fields(field, default_selected_field_options()),
      )
    "products" ->
      serialize_collection_products_connection(
        store,
        collection,
        field,
        variables,
        fragments,
      )
    _ -> json.null()
  }
}

fn serialize_collection_image(
  image: Option(CollectionImageRecord),
  selections: List(Selection),
) -> Json {
  case image {
    None -> json.null()
    Some(image) ->
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          let value = case selection {
            Field(name: name, ..) ->
              case name.value {
                "id" -> optional_string_json(image.id)
                "altText" -> optional_string_json(image.alt_text)
                "url" | "src" | "originalSrc" | "transformedSrc" ->
                  optional_string_json(image.url)
                "width" -> optional_int_json(image.width)
                "height" -> optional_int_json(image.height)
                _ -> json.null()
              }
            _ -> json.null()
          }
          #(key, value)
        }),
      )
  }
}

fn serialize_collection_rule_set(
  rule_set: Option(CollectionRuleSetRecord),
  selections: List(Selection),
) -> Json {
  case rule_set {
    None -> json.null()
    Some(rule_set) ->
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          let value = case selection {
            Field(name: name, ..) ->
              case name.value {
                "appliedDisjunctively" ->
                  json.bool(rule_set.applied_disjunctively)
                "rules" ->
                  json.array(rule_set.rules, fn(rule) {
                    serialize_collection_rule(
                      rule,
                      get_selected_child_fields(
                        selection,
                        default_selected_field_options(),
                      ),
                    )
                  })
                _ -> json.null()
              }
            _ -> json.null()
          }
          #(key, value)
        }),
      )
  }
}

fn serialize_collection_rule(
  rule: CollectionRuleRecord,
  selections: List(Selection),
) -> Json {
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "column" -> json.string(rule.column)
            "relation" -> json.string(rule.relation)
            "condition" -> json.string(rule.condition)
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

fn serialize_collection_products_connection(
  store: Store,
  collection: CollectionRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let entries =
    store.list_effective_products_for_collection(store, collection.id)
  let window =
    paginate_connection_items(
      entries,
      field,
      variables,
      collection_product_cursor,
      default_connection_window_options(),
    )
  let has_next_page = case collection.products_count {
    Some(count) -> window.has_next_page || count > list.length(window.items)
    None -> window.has_next_page
  }
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: collection_product_cursor,
      serialize_node: fn(entry, node_field, _index) {
        let #(product, _) = entry
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

fn serialize_collections_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let collections =
    filtered_collections(store, field, variables)
    |> sort_collections(field, variables)
  case collections {
    [] -> serialize_empty_connection(field, default_selected_field_options())
    _ -> {
      let get_cursor = fn(collection, _index) {
        collection_cursor_for_field(collection, field, variables)
      }
      let window =
        paginate_connection_items(
          collections,
          field,
          variables,
          get_cursor,
          default_connection_window_options(),
        )
      serialize_connection(
        field,
        SerializeConnectionConfig(
          items: window.items,
          has_next_page: window.has_next_page,
          has_previous_page: window.has_previous_page,
          get_cursor_value: get_cursor,
          serialize_node: fn(collection, node_field, _index) {
            serialize_collection_object(
              store,
              collection,
              get_selected_child_fields(
                node_field,
                default_selected_field_options(),
              ),
              variables,
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

fn filtered_collections(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(CollectionRecord) {
  search_query_parser.apply_search_query(
    store.list_effective_collections(store),
    read_string_argument(field, variables, "query"),
    product_search_parse_options(),
    fn(collection, term) {
      collection_matches_positive_query_term(store, collection, term)
    },
  )
}

fn collection_matches_positive_query_term(
  store: Store,
  collection: CollectionRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  let value = search_query_parser.search_query_term_value(term)
  case option.map(term.field, string.lowercase) {
    None ->
      search_query_parser.matches_search_query_string(
        Some(collection.title),
        value,
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
      || search_query_parser.matches_search_query_string(
        Some(collection.handle),
        value,
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
    Some("title") ->
      search_query_parser.matches_search_query_string(
        Some(collection.title),
        value,
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
    Some("handle") ->
      search_query_parser.matches_search_query_string(
        Some(collection.handle),
        value,
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("collection_type") -> {
      let normalized =
        search_query_parser.strip_search_query_value_quotes(value)
        |> string.trim
        |> string.lowercase
      case normalized {
        "smart" -> collection.is_smart
        "custom" -> !collection.is_smart
        _ -> True
      }
    }
    Some("id") ->
      resource_id_matches(collection.id, collection.legacy_resource_id, value)
    Some("product_id") -> collection_has_product_id(store, collection.id, value)
    Some("updated_at") -> True
    Some("product_publication_status")
    | Some("publishable_status")
    | Some("published_at")
    | Some("published_status") -> True
    _ -> True
  }
}

fn collection_has_product_id(
  store: Store,
  collection_id: String,
  raw_value: String,
) -> Bool {
  let normalized =
    search_query_parser.strip_search_query_value_quotes(raw_value)
    |> string.trim
  store.list_effective_products_for_collection(store, collection_id)
  |> list.any(fn(entry) {
    let #(product, _) = entry
    product.id == normalized
    || product.legacy_resource_id == Some(normalized)
    || resource_tail(product.id) == normalized
    || resource_tail(normalized) == resource_tail(product.id)
  })
}

fn sort_collections(
  collections: List(CollectionRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(CollectionRecord) {
  let sort_key = read_string_argument(field, variables, "sortKey")
  let sorted =
    list.sort(collections, fn(left, right) {
      compare_collections_by_sort_key(left, right, sort_key)
    })
  case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(sorted)
    _ -> sorted
  }
}

fn compare_collections_by_sort_key(
  left: CollectionRecord,
  right: CollectionRecord,
  sort_key: Option(String),
) -> order.Order {
  case sort_key {
    Some("TITLE") ->
      case string.compare(left.title, right.title) {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    Some("UPDATED_AT") ->
      case
        resource_ids.compare_nullable_strings(left.updated_at, right.updated_at)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    _ -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
  }
}

fn collection_cursor_for_field(
  collection: CollectionRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> String {
  case read_string_argument(field, variables, "sortKey") {
    Some("TITLE") ->
      collection.title_cursor
      |> option.unwrap(option.unwrap(collection.cursor, collection.id))
    Some("UPDATED_AT") ->
      collection.updated_at_cursor
      |> option.unwrap(option.unwrap(collection.cursor, collection.id))
    _ -> option.unwrap(collection.cursor, collection.id)
  }
}

fn collection_product_cursor(
  entry: #(ProductRecord, ProductCollectionRecord),
  _index: Int,
) -> String {
  let #(_, membership) = entry
  membership.cursor |> option.unwrap(membership.product_id)
}

fn product_collection_cursor(
  entry: #(CollectionRecord, ProductCollectionRecord),
  _index: Int,
) -> String {
  let #(collection, membership) = entry
  membership.cursor |> option.unwrap(collection.id)
}

fn collection_has_product(
  store: Store,
  collection_id: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Bool {
  case read_string_argument(field, variables, "id") {
    Some(product_id) ->
      store.list_effective_products_for_collection(store, collection_id)
      |> list.any(fn(entry) {
        let #(product, _) = entry
        product.id == product_id
      })
    None -> False
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
      list.any(product_searchable_tags(store, product), fn(tag) {
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
        Some(product_searchable_status(store, product)),
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("sku") ->
      product_searchable_variants(store, product.id)
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

fn product_searchable_variants(
  store: Store,
  product_id: String,
) -> List(ProductVariantRecord) {
  let base_variants = store.get_base_variants_by_product_id(store, product_id)
  let effective_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  case base_variants {
    [] -> effective_variants
    _ ->
      case variants_search_equal(base_variants, effective_variants) {
        True -> effective_variants
        False -> base_variants
      }
  }
}

fn variants_search_equal(
  left: List(ProductVariantRecord),
  right: List(ProductVariantRecord),
) -> Bool {
  list.length(left) == list.length(right)
  && list.all(left, fn(variant) {
    list.any(right, fn(other) {
      variant.id == other.id && variant.sku == other.sku
    })
  })
}

fn product_searchable_status(store: Store, product: ProductRecord) -> String {
  case dict.get(store.base_state.products, product.id) {
    Ok(base_product) ->
      case base_product.status == product.status {
        True -> product.status
        False -> base_product.status
      }
    Error(_) -> product.status
  }
}

fn product_searchable_tags(
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
    product_collections_connection_source(store, product),
    product_variants_connection_source(store, product),
    product_options_source(store.get_effective_options_by_product_id(
      store,
      product.id,
    )),
  )
}

fn product_source_with_relationships(
  product: ProductRecord,
  collections: SourceValue,
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
    #("collections", collections),
    #("media", empty_connection_source()),
    #("options", options),
    #("variants", variants),
  ])
}

fn product_collections_connection_source(
  store: Store,
  product: ProductRecord,
) -> SourceValue {
  let collections =
    store.list_effective_collections_for_product(store, product.id)
  let edges =
    collections
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(entry, index) = pair
      let #(collection, _) = entry
      src_object([
        #("cursor", SrcString(product_collection_cursor(entry, index))),
        #("node", collection_source_with_store(store, collection)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(collections, fn(entry) {
          let #(collection, _) = entry
          collection_source_with_store(store, collection)
        }),
      ),
    ),
    #(
      "pageInfo",
      connection_page_info_source(collections, product_collection_cursor),
    ),
  ])
}

fn collection_source_with_store(
  store: Store,
  collection: CollectionRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("Collection")),
    #("id", SrcString(collection.id)),
    #("legacyResourceId", optional_string_source(collection.legacy_resource_id)),
    #("title", SrcString(collection.title)),
    #("handle", SrcString(collection.handle)),
    #("updatedAt", optional_string_source(collection.updated_at)),
    #("description", optional_string_source(collection.description)),
    #("descriptionHtml", optional_string_source(collection.description_html)),
    #("sortOrder", optional_string_source(collection.sort_order)),
    #("templateSuffix", optional_string_source(collection.template_suffix)),
    #("products", collection_products_connection_source(store, collection)),
  ])
}

fn collection_products_connection_source(
  store: Store,
  collection: CollectionRecord,
) -> SourceValue {
  let products =
    store.list_effective_products_for_collection(store, collection.id)
  let edges =
    products
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(entry, index) = pair
      let #(product, _) = entry
      src_object([
        #("cursor", SrcString(collection_product_cursor(entry, index))),
        #("node", product_source(product)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #(
      "nodes",
      SrcList(
        list.map(products, fn(entry) {
          let #(product, _) = entry
          product_source(product)
        }),
      ),
    ),
    #(
      "pageInfo",
      connection_page_info_source(products, collection_product_cursor),
    ),
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

fn inventory_level_source_with_item(
  store: Store,
  variant: ProductVariantRecord,
  level: InventoryLevelRecord,
) -> SourceValue {
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
    #("item", inventory_item_source(store, variant)),
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

fn optional_string_json(value: Option(String)) -> Json {
  case value {
    Some(value) -> json.string(value)
    None -> json.null()
  }
}

fn optional_int_json(value: Option(Int)) -> Json {
  case value {
    Some(value) -> json.int(value)
    None -> json.null()
  }
}

fn legacy_resource_id_from_gid(id: String) -> String {
  case string.split(id, "/") |> list.last {
    Ok(tail_with_query) ->
      case string.split(tail_with_query, "?") {
        [tail, ..] -> tail
        [] -> id
      }
    Error(_) -> id
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

type CollectionProductMove {
  CollectionProductMove(id: String, new_position: Int)
}

type NullableFieldUserError {
  NullableFieldUserError(field: Option(List(String)), message: String)
}

type InventoryAdjustmentChange {
  InventoryAdjustmentChange(
    inventory_item_id: String,
    location_id: String,
    name: String,
    delta: Int,
    quantity_after_change: Option(Int),
    ledger_document_uri: Option(String),
  )
}

type InventoryAdjustmentChangeInput {
  InventoryAdjustmentChangeInput(
    inventory_item_id: Option(String),
    location_id: Option(String),
    ledger_document_uri: Option(String),
    delta: Option(Int),
    change_from_quantity: Option(Int),
  )
}

type InventoryAdjustmentGroup {
  InventoryAdjustmentGroup(
    id: String,
    created_at: String,
    reason: String,
    reference_document_uri: Option(String),
    changes: List(InventoryAdjustmentChange),
  )
}

type InventorySetQuantityInput {
  InventorySetQuantityInput(
    inventory_item_id: Option(String),
    location_id: Option(String),
    quantity: Option(Int),
    compare_quantity: Option(Int),
  )
}

type InventoryMoveTerminalInput {
  InventoryMoveTerminalInput(
    location_id: Option(String),
    name: Option(String),
    ledger_document_uri: Option(String),
  )
}

type InventoryMoveQuantityInput {
  InventoryMoveQuantityInput(
    inventory_item_id: Option(String),
    quantity: Option(Int),
    from: InventoryMoveTerminalInput,
    to: InventoryMoveTerminalInput,
  )
}

type ProductVariantPositionInput {
  ProductVariantPositionInput(id: String, position: Int)
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
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
      let operation_path = get_operation_path_label(document)
      Ok(handle_mutation_fields(
        store,
        identity,
        document,
        operation_path,
        fields,
        fragments,
        variables,
      ))
    }
  }
}

fn get_operation_path_label(document: String) -> String {
  case parse_operation.parse_operation(document) {
    Ok(parsed) -> {
      let kind = case parsed.type_ {
        parse_operation.QueryOperation -> "query"
        parse_operation.MutationOperation -> "mutation"
      }
      case parsed.name {
        Some(name) -> kind <> " " <> name
        None -> kind
      }
    }
    Error(_) -> "mutation"
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationOutcome {
  let initial = #([], [], store, identity, [], [])
  let #(
    data_entries,
    all_errors,
    final_store,
    final_identity,
    all_staged,
    all_drafts,
  ) =
    list.fold(fields, initial, fn(acc, field) {
      let #(
        entries,
        errors,
        current_store,
        current_identity,
        staged_ids,
        drafts,
      ) = acc
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
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productCreate" -> {
              let result =
                handle_product_create(
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
                  Some("Gleam staged productCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productOptionUpdate" -> {
              let result =
                handle_product_option_update(
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
                  Some("Gleam staged productOptionUpdate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productOptionsDelete" -> {
              let result =
                handle_product_options_delete(
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
                  Some("Gleam staged productOptionsDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productOptionsReorder" -> {
              let result =
                handle_product_options_reorder(
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
                  Some("Gleam staged productOptionsReorder locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productChangeStatus" -> {
              let result =
                handle_product_change_status(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
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
                  Some("Gleam staged productChangeStatus locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> entries
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productDelete" -> {
              let result =
                handle_product_delete(
                  current_store,
                  current_identity,
                  document,
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
                  Some("Gleam staged productDelete locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> entries
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productUpdate" -> {
              let result =
                handle_product_update(
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
                  Some("Gleam staged productUpdate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantCreate" -> {
              let result =
                handle_product_variant_create(
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
                  Some("Gleam staged productVariantCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantUpdate" -> {
              let result =
                handle_product_variant_update(
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
                  Some("Gleam staged productVariantUpdate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantDelete" -> {
              let result =
                handle_product_variant_delete(
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
                  Some("Gleam staged productVariantDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantsBulkCreate" -> {
              let result =
                handle_product_variants_bulk_create(
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
                  Some("Gleam staged productVariantsBulkCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantsBulkUpdate" -> {
              let result =
                handle_product_variants_bulk_update(
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
                  Some("Gleam staged productVariantsBulkUpdate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantsBulkDelete" -> {
              let result =
                handle_product_variants_bulk_delete(
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
                  Some("Gleam staged productVariantsBulkDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantsBulkReorder" -> {
              let result =
                handle_product_variants_bulk_reorder(
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
                  Some("Gleam staged productVariantsBulkReorder locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryAdjustQuantities" -> {
              let result =
                handle_inventory_adjust_quantities(
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
                  Some("Gleam staged inventoryAdjustQuantities locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryActivate" -> {
              let result =
                handle_inventory_activate(
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
                  Some("Gleam staged inventoryActivate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryDeactivate" -> {
              let result =
                handle_inventory_deactivate(
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
                  Some("Gleam staged inventoryDeactivate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryBulkToggleActivation" -> {
              let result =
                handle_inventory_bulk_toggle_activation(
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
                  Some("Gleam staged inventoryBulkToggleActivation locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryItemUpdate" -> {
              let result =
                handle_inventory_item_update(
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
                  Some("Gleam staged inventoryItemUpdate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventorySetQuantities" -> {
              let result =
                handle_inventory_set_quantities(
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
                  Some("Gleam staged inventorySetQuantities locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryMoveQuantities" -> {
              let result =
                handle_inventory_move_quantities(
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
                  Some("Gleam staged inventoryMoveQuantities locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionAddProducts" -> {
              let result =
                handle_collection_add_products(
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
                  Some("Gleam staged collectionAddProducts locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionCreate" -> {
              let result =
                handle_collection_create(
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
                  Some("Gleam staged collectionCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionRemoveProducts" -> {
              let result =
                handle_collection_remove_products(
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
                  Some("Gleam staged collectionRemoveProducts locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionReorderProducts" -> {
              let result =
                handle_collection_reorder_products(
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
                  Some("Gleam staged collectionReorderProducts locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionUpdate" -> {
              let result =
                handle_collection_update(
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
                  Some("Gleam staged collectionUpdate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionDelete" -> {
              let result =
                handle_collection_delete(
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
                  Some("Gleam staged collectionDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "tagsAdd" -> {
              let result =
                handle_tags_update(
                  current_store,
                  current_identity,
                  True,
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
                  Some("Gleam staged tagsAdd locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "tagsRemove" -> {
              let result =
                handle_tags_update(
                  current_store,
                  current_identity,
                  False,
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
                  Some("Gleam staged tagsRemove locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
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
  let envelope = case all_errors {
    [] -> json.object([#("data", json.object(data_entries))])
    _ -> json.object([#("errors", json.preprocessed_array(all_errors))])
  }
  let final_staged_ids = case all_errors {
    [] -> all_staged
    _ -> []
  }
  MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: final_staged_ids,
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

fn handle_product_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let input = read_arg_object(args, "product")
  case input {
    None ->
      mutation_result(
        key,
        product_create_payload(
          store,
          None,
          [ProductUserError(["title"], "Title can't be blank", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(input) ->
      case product_create_validation_error(input) {
        Some(error) ->
          mutation_result(
            key,
            product_create_payload(store, None, [error], field, fragments),
            store,
            identity,
            [],
          )
        None -> {
          let #(product, identity_after_product) =
            created_product_record(store, identity, input)
          let #(default_option, identity_after_option, option_ids) =
            make_default_option_record(identity_after_product, product)
          let #(default_variant, final_identity, variant_ids) =
            make_default_variant_record(identity_after_option, product)
          let #(_, next_store) = store.upsert_staged_product(store, product)
          let next_store =
            next_store
            |> store.replace_staged_options_for_product(product.id, [
              default_option,
            ])
            |> store.replace_staged_variants_for_product(product.id, [
              default_variant,
            ])
          let synced_product =
            ProductRecord(
              ..product,
              total_inventory: Some(0),
              tracks_inventory: Some(False),
            )
          let #(_, next_store) =
            store.upsert_staged_product(next_store, synced_product)
          mutation_result(
            key,
            product_create_payload(
              next_store,
              Some(synced_product),
              [],
              field,
              fragments,
            ),
            next_store,
            final_identity,
            list.append(
              [synced_product.id],
              list.append(option_ids, variant_ids),
            ),
          )
        }
      }
  }
}

fn handle_product_options_delete(
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
        product_options_delete_payload(
          store,
          [],
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
            product_options_delete_payload(
              store,
              [],
              None,
              [ProductUserError(["productId"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) ->
          stage_product_options_delete(
            store,
            identity,
            key,
            product,
            read_arg_string_list(args, "options"),
            field,
            fragments,
          )
      }
  }
}

fn handle_product_options_reorder(
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
        product_options_reorder_payload(
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
            product_options_reorder_payload(
              store,
              None,
              [ProductUserError(["productId"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) ->
          stage_product_options_reorder(
            store,
            identity,
            key,
            product,
            read_arg_object_list(args, "options"),
            field,
            fragments,
          )
      }
  }
}

fn handle_product_change_status(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  case
    product_change_status_null_product_id_error(document, operation_path, field)
  {
    Some(error) -> mutation_error_result(key, store, identity, [error])
    None -> {
      let args = field_args(field, variables)
      case read_arg_string(args, "productId") {
        None ->
          mutation_result(
            key,
            product_change_status_payload(
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
        Some(product_id) -> {
          let status = read_arg_string(args, "status")
          case is_valid_product_status(status) {
            False ->
              mutation_result(
                key,
                product_change_status_payload(
                  store,
                  None,
                  [
                    ProductUserError(
                      ["status"],
                      "Product status is required",
                      None,
                    ),
                  ],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            True ->
              case store.get_effective_product_by_id(store, product_id) {
                None ->
                  mutation_result(
                    key,
                    product_change_status_payload(
                      store,
                      None,
                      [
                        ProductUserError(
                          ["productId"],
                          "Product does not exist",
                          None,
                        ),
                      ],
                      field,
                      fragments,
                    ),
                    store,
                    identity,
                    [],
                  )
                Some(product) -> {
                  let assert Some(next_status) = status
                  let #(updated_at, next_identity) =
                    synthetic_identity.make_synthetic_timestamp(identity)
                  let next_product =
                    ProductRecord(
                      ..product,
                      status: next_status,
                      updated_at: Some(updated_at),
                    )
                  let #(_, next_store) =
                    store.upsert_staged_product(store, next_product)
                  mutation_result(
                    key,
                    product_change_status_payload(
                      next_store,
                      Some(next_product),
                      [],
                      field,
                      fragments,
                    ),
                    next_store,
                    next_identity,
                    [next_product.id],
                  )
                }
              }
          }
        }
      }
    }
  }
}

fn handle_product_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  case product_delete_input_error(document, field, variables) {
    Some(error) -> mutation_error_result(key, store, identity, [error])
    None -> {
      let args = field_args(field, variables)
      let input = read_arg_object(args, "input")
      let id = case input {
        Some(input) -> read_arg_string(input, "id")
        None -> read_arg_string(args, "id")
      }
      case id {
        None ->
          mutation_error_result(key, store, identity, [
            build_product_delete_invalid_variable_error(
              "input",
              json.object([]),
              None,
              document,
            ),
          ])
        Some(product_id) ->
          case store.get_effective_product_by_id(store, product_id) {
            None ->
              mutation_result(
                key,
                product_delete_payload(
                  None,
                  [ProductUserError(["id"], "Product does not exist", None)],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(_) -> {
              let next_store = store.delete_staged_product(store, product_id)
              mutation_result(
                key,
                product_delete_payload(Some(product_id), [], field, fragments),
                next_store,
                identity,
                [product_id],
              )
            }
          }
      }
    }
  }
}

fn handle_product_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let input = read_arg_object(args, "product")
  let id = case input {
    Some(input) -> read_arg_string(input, "id")
    None -> None
  }
  case id {
    None ->
      mutation_result(
        key,
        product_update_payload(
          store,
          None,
          [ProductUserError(["id"], "Product does not exist", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id), input {
        None, _ ->
          mutation_result(
            key,
            product_update_payload(
              store,
              None,
              [ProductUserError(["id"], "Product does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product), Some(input) ->
          case product_update_validation_error(input) {
            Some(error) ->
              mutation_result(
                key,
                product_update_payload(
                  store,
                  Some(product),
                  [error],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            None -> {
              let #(next_product, next_identity) =
                updated_product_record(identity, product, input)
              let #(_, next_store) =
                store.upsert_staged_product(store, next_product)
              mutation_result(
                key,
                product_update_payload(
                  next_store,
                  Some(next_product),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                [next_product.id],
              )
            }
          }
        Some(_), None ->
          mutation_result(
            key,
            product_update_payload(
              store,
              None,
              [ProductUserError(["id"], "Product does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
  }
}

fn handle_product_variant_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let input = read_arg_object(args, "input")
  let product_id = case input {
    Some(input) -> read_arg_string(input, "productId")
    None -> None
  }
  case product_id, input {
    None, _ ->
      mutation_result(
        key,
        product_variant_payload(
          store,
          None,
          None,
          [
            ProductUserError(
              ["input", "productId"],
              "Product id is required",
              None,
            ),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id), Some(input) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_variant_payload(
              store,
              None,
              None,
              [
                ProductUserError(
                  ["input", "productId"],
                  "Product not found",
                  None,
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          let defaults = list.first(effective_variants) |> option.from_result
          let #(created_variant, identity_after_variant) =
            make_created_variant_record(identity, product_id, input, defaults)
          let next_variants = list.append(effective_variants, [created_variant])
          let next_store =
            store.replace_staged_variants_for_product(
              store,
              product_id,
              next_variants,
            )
          let #(product, next_store, final_identity) =
            sync_product_inventory_summary(
              next_store,
              identity_after_variant,
              product_id,
            )
          mutation_result(
            key,
            product_variant_payload(
              next_store,
              product,
              Some(created_variant),
              [],
              field,
              fragments,
            ),
            next_store,
            final_identity,
            variant_staged_ids(created_variant),
          )
        }
      }
    Some(_), None ->
      mutation_result(
        key,
        product_variant_payload(
          store,
          None,
          None,
          [
            ProductUserError(
              ["input", "productId"],
              "Product id is required",
              None,
            ),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
  }
}

fn handle_product_variant_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let input = read_arg_object(args, "input")
  let variant_id = case input {
    Some(input) -> read_arg_string(input, "id")
    None -> None
  }
  case variant_id, input {
    None, _ ->
      mutation_result(
        key,
        product_variant_payload(
          store,
          None,
          None,
          [ProductUserError(["input", "id"], "Variant id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(variant_id), Some(input) ->
      case store.get_effective_variant_by_id(store, variant_id) {
        None ->
          mutation_result(
            key,
            product_variant_payload(
              store,
              None,
              None,
              [ProductUserError(["input", "id"], "Variant not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(existing_variant) -> {
          let #(updated_variant, identity_after_variant) =
            update_variant_record(identity, existing_variant, input)
          let next_variants =
            store.get_effective_variants_by_product_id(
              store,
              existing_variant.product_id,
            )
            |> list.map(fn(variant) {
              case variant.id == variant_id {
                True -> updated_variant
                False -> variant
              }
            })
          let next_store =
            store.replace_staged_variants_for_product(
              store,
              existing_variant.product_id,
              next_variants,
            )
          let #(product, next_store, final_identity) =
            sync_product_inventory_summary(
              next_store,
              identity_after_variant,
              existing_variant.product_id,
            )
          mutation_result(
            key,
            product_variant_payload(
              next_store,
              product,
              Some(updated_variant),
              [],
              field,
              fragments,
            ),
            next_store,
            final_identity,
            variant_staged_ids(updated_variant),
          )
        }
      }
    Some(_), None ->
      mutation_result(
        key,
        product_variant_payload(
          store,
          None,
          None,
          [ProductUserError(["input", "id"], "Variant id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
  }
}

fn handle_product_variant_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        product_variant_delete_payload(
          None,
          [ProductUserError(["id"], "Variant id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(variant_id) ->
      case store.get_effective_variant_by_id(store, variant_id) {
        None ->
          mutation_result(
            key,
            product_variant_delete_payload(
              None,
              [ProductUserError(["id"], "Variant not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(existing_variant) -> {
          let next_variants =
            store.get_effective_variants_by_product_id(
              store,
              existing_variant.product_id,
            )
            |> list.filter(fn(variant) { variant.id != variant_id })
          let next_store =
            store.replace_staged_variants_for_product(
              store,
              existing_variant.product_id,
              next_variants,
            )
          let #(_, next_store, final_identity) =
            sync_product_inventory_summary(
              next_store,
              identity,
              existing_variant.product_id,
            )
          mutation_result(
            key,
            product_variant_delete_payload(
              Some(variant_id),
              [],
              field,
              fragments,
            ),
            next_store,
            final_identity,
            [variant_id],
          )
        }
      }
  }
}

fn handle_product_variants_bulk_create(
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
        product_variants_bulk_payload(
          "ProductVariantsBulkCreatePayload",
          store,
          None,
          [],
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
            product_variants_bulk_payload(
              "ProductVariantsBulkCreatePayload",
              store,
              None,
              [],
              [ProductUserError(["productId"], "Product does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          let effective_options =
            store.get_effective_options_by_product_id(store, product_id)
          let defaults = list.first(effective_variants) |> option.from_result
          let variant_inputs = read_arg_object_list(args, "variants")
          let #(created_variants, identity_after_variants) =
            make_created_variant_records(
              identity,
              product_id,
              variant_inputs,
              defaults,
            )
          let should_remove_standalone_variant =
            !list.is_empty(variant_inputs)
            && list.length(effective_variants) == 1
            && {
              read_arg_string(args, "strategy")
              == Some("REMOVE_STANDALONE_VARIANT")
              || product_has_standalone_default_variant(
                effective_options,
                effective_variants,
              )
            }
          let retained_variants = case should_remove_standalone_variant {
            True -> []
            False -> effective_variants
          }
          let next_variants = list.append(retained_variants, created_variants)
          let #(synced_options, identity_after_options) = case
            should_remove_standalone_variant
          {
            True ->
              make_options_from_variant_selections(
                identity_after_variants,
                product_id,
                next_variants,
              )
            False -> {
              let #(next_options, identity_after_options) =
                upsert_variant_selections_into_options(
                  identity_after_variants,
                  product_id,
                  effective_options,
                  next_variants,
                )
              #(
                sync_product_options_with_variants(next_options, next_variants),
                identity_after_options,
              )
            }
          }
          let next_store =
            store.replace_staged_variants_for_product(
              store,
              product_id,
              next_variants,
            )
            |> store.replace_staged_options_for_product(
              product_id,
              synced_options,
            )
          let #(product, next_store, final_identity) =
            sync_product_inventory_summary(
              next_store,
              identity_after_options,
              product_id,
            )
          mutation_result(
            key,
            product_variants_bulk_payload(
              "ProductVariantsBulkCreatePayload",
              next_store,
              product,
              created_variants,
              [],
              field,
              fragments,
            ),
            next_store,
            final_identity,
            list.flat_map(created_variants, variant_staged_ids),
          )
        }
      }
  }
}

fn handle_product_variants_bulk_update(
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
        product_variants_bulk_payload(
          "ProductVariantsBulkUpdatePayload",
          store,
          None,
          [],
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
            product_variants_bulk_payload(
              "ProductVariantsBulkUpdatePayload",
              store,
              None,
              [],
              [ProductUserError(["productId"], "Product does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let updates = read_arg_object_list(args, "variants")
          let #(next_variants, updated_variants, identity_after_variants) =
            update_variant_records(
              identity,
              store.get_effective_variants_by_product_id(store, product_id),
              updates,
            )
          let synced_options =
            sync_product_options_with_variants(
              store.get_effective_options_by_product_id(store, product_id),
              next_variants,
            )
          let next_store =
            store.replace_staged_variants_for_product(
              store,
              product_id,
              next_variants,
            )
            |> store.replace_staged_options_for_product(
              product_id,
              synced_options,
            )
          let #(product, next_store, final_identity) =
            sync_product_inventory_summary(
              next_store,
              identity_after_variants,
              product_id,
            )
          mutation_result(
            key,
            product_variants_bulk_payload(
              "ProductVariantsBulkUpdatePayload",
              next_store,
              product,
              updated_variants,
              [],
              field,
              fragments,
            ),
            next_store,
            final_identity,
            list.flat_map(updated_variants, variant_staged_ids),
          )
        }
      }
  }
}

fn handle_product_variants_bulk_delete(
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
        product_variants_bulk_delete_payload(
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
            product_variants_bulk_delete_payload(
              store,
              None,
              [ProductUserError(["productId"], "Product does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let variant_ids = read_arg_string_list(args, "variantsIds")
          let next_variants =
            store.get_effective_variants_by_product_id(store, product_id)
            |> list.filter(fn(variant) {
              !list.contains(variant_ids, variant.id)
            })
          let synced_options =
            sync_product_options_with_variants(
              store.get_effective_options_by_product_id(store, product_id),
              next_variants,
            )
          let next_store =
            store.replace_staged_variants_for_product(
              store,
              product_id,
              next_variants,
            )
            |> store.replace_staged_options_for_product(
              product_id,
              synced_options,
            )
          let #(product, next_store, final_identity) =
            sync_product_inventory_summary(next_store, identity, product_id)
          mutation_result(
            key,
            product_variants_bulk_delete_payload(
              next_store,
              product,
              [],
              field,
              fragments,
            ),
            next_store,
            final_identity,
            variant_ids,
          )
        }
      }
  }
}

fn handle_product_variants_bulk_reorder(
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
        product_variants_bulk_reorder_payload(
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
            product_variants_bulk_reorder_payload(
              store,
              None,
              [ProductUserError(["productId"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let #(positions, user_errors) =
            read_product_variant_positions(read_arg_object_list(
              args,
              "positions",
            ))
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          let missing_errors =
            validate_product_variant_positions(effective_variants, positions)
          let all_errors = list.append(user_errors, missing_errors)
          case all_errors {
            [_, ..] ->
              mutation_result(
                key,
                product_variants_bulk_reorder_payload(
                  store,
                  None,
                  all_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            [] -> {
              let next_variants =
                apply_sequential_variant_reorder(effective_variants, positions)
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  product_id,
                  next_variants,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(next_store, identity, product_id)
              mutation_result(
                key,
                product_variants_bulk_reorder_payload(
                  next_store,
                  product,
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                list.map(next_variants, fn(variant) { variant.id }),
              )
            }
          }
        }
      }
  }
}

fn handle_inventory_adjust_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let input = read_arg_object(args, "input") |> option.unwrap(dict.new())
  let quantity_name = read_non_empty_string_field(input, "name")
  let reason = read_non_empty_string_field(input, "reason")
  let changes = read_inventory_adjustment_change_inputs(input)
  case quantity_name, reason, changes {
    None, _, _ ->
      inventory_quantity_mutation_result(
        key,
        "InventoryAdjustQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "name"],
            "Inventory quantity name is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    _, None, _ ->
      inventory_quantity_mutation_result(
        key,
        "InventoryAdjustQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "reason"],
            "Inventory adjustment reason is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    _, _, [] ->
      inventory_quantity_mutation_result(
        key,
        "InventoryAdjustQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "changes"],
            "At least one inventory adjustment is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    Some(name), Some(reason), changes -> {
      case validate_inventory_adjust_inputs(name, changes) {
        [_, ..] as errors ->
          inventory_quantity_mutation_result(
            key,
            "InventoryAdjustQuantitiesPayload",
            store,
            identity,
            None,
            errors,
            field,
            fragments,
            [],
          )
        [] -> {
          let result =
            apply_inventory_adjust_quantities(
              store,
              identity,
              input,
              name,
              reason,
              changes,
            )
          case result {
            Error(errors) ->
              inventory_quantity_mutation_result(
                key,
                "InventoryAdjustQuantitiesPayload",
                store,
                identity,
                None,
                errors,
                field,
                fragments,
                [],
              )
            Ok(applied) -> {
              let #(next_store, next_identity, group, staged_ids) = applied
              inventory_quantity_mutation_result(
                key,
                "InventoryAdjustQuantitiesPayload",
                next_store,
                next_identity,
                Some(group),
                [],
                field,
                fragments,
                staged_ids,
              )
            }
          }
        }
      }
    }
  }
}

fn handle_inventory_activate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let inventory_item_id = read_string_field(args, "inventoryItemId")
  let location_id = read_string_field(args, "locationId")
  let user_errors = case dict.get(args, "available") {
    Ok(_) -> [
      ProductUserError(
        ["available"],
        "Not allowed to set available quantity when the item is already active at the location.",
        None,
      ),
    ]
    Error(_) -> []
  }
  let resolved = case inventory_item_id, location_id, user_errors {
    Some(inventory_item_id), Some(location_id), [] -> {
      case
        store.find_effective_variant_by_inventory_item_id(
          store,
          inventory_item_id,
        )
      {
        Some(variant) ->
          case
            find_inventory_level(variant_inventory_levels(variant), location_id)
          {
            Some(level) -> Some(#(variant, level))
            None -> None
          }
        None -> None
      }
    }
    _, _, _ -> None
  }
  let staged_ids = case resolved {
    Some(#(variant, level)) ->
      case variant.inventory_item {
        Some(item) -> [level.id, item.id]
        None -> [level.id]
      }
    None -> []
  }
  mutation_result(
    key,
    inventory_activate_payload(store, resolved, user_errors, field, fragments),
    store,
    identity,
    staged_ids,
  )
}

fn handle_inventory_deactivate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let inventory_level_id = read_string_field(args, "inventoryLevelId")
  let target = case inventory_level_id {
    Some(inventory_level_id) ->
      find_inventory_level_target(store, inventory_level_id)
    None -> None
  }
  let user_errors = case target {
    Some(#(variant, level)) -> {
      let active_levels = variant_inventory_levels(variant)
      case list.length(active_levels) <= 1 {
        True -> [
          NullableFieldUserError(
            None,
            "The product couldn't be unstocked from "
              <> level.location.name
              <> " because products need to be stocked at a minimum of 1 location.",
          ),
        ]
        False -> []
      }
    }
    None -> []
  }
  let next_store = case target, user_errors {
    Some(#(variant, level)), [] -> {
      let next_levels =
        variant_inventory_levels(variant)
        |> list.filter(fn(candidate) { candidate.id != level.id })
      stage_variant_inventory_levels(store, variant, next_levels)
    }
    _, _ -> store
  }
  let staged_ids = case target, user_errors {
    Some(#(_variant, level)), [] -> [level.id]
    _, _ -> []
  }
  mutation_result(
    key,
    inventory_deactivate_payload(user_errors, field, fragments),
    next_store,
    identity,
    staged_ids,
  )
}

fn handle_inventory_bulk_toggle_activation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let inventory_item_id = read_string_field(args, "inventoryItemId")
  let first_update =
    read_arg_object_list(args, "inventoryItemUpdates")
    |> list.first
    |> option.from_result
  let location_id = case first_update {
    Some(update) -> read_string_field(update, "locationId")
    None -> None
  }
  let activate = case first_update {
    Some(update) -> read_bool_field(update, "activate")
    None -> None
  }
  let variant = case inventory_item_id {
    Some(inventory_item_id) ->
      store.find_effective_variant_by_inventory_item_id(
        store,
        inventory_item_id,
      )
    None -> None
  }
  let target = case variant, location_id {
    Some(variant), Some(location_id) ->
      case
        find_inventory_level(variant_inventory_levels(variant), location_id)
      {
        Some(level) -> Some(#(variant, level))
        None -> None
      }
    _, _ -> None
  }
  let user_errors = case variant, location_id, target, activate {
    Some(_variant), Some(_location_id), None, _ -> [
      ProductUserError(
        ["inventoryItemUpdates", "0", "locationId"],
        "The quantity couldn't be updated because the location was not found.",
        Some("LOCATION_NOT_FOUND"),
      ),
    ]
    Some(variant),
      Some(_location_id),
      Some(#(_target_variant, level)),
      Some(False)
    -> {
      case list.length(variant_inventory_levels(variant)) <= 1 {
        True -> [
          ProductUserError(
            ["inventoryItemUpdates", "0", "locationId"],
            "The variant couldn't be unstocked from "
              <> level.location.name
              <> " because products need to be stocked at a minimum of 1 location.",
            Some("CANNOT_DEACTIVATE_FROM_ONLY_LOCATION"),
          ),
        ]
        False -> []
      }
    }
    _, _, _, _ -> []
  }
  let outcome = case target, activate, user_errors {
    Some(#(variant, level)), Some(False), [] -> {
      let next_levels =
        variant_inventory_levels(variant)
        |> list.filter(fn(candidate) { candidate.id != level.id })
      let next_store =
        stage_variant_inventory_levels(store, variant, next_levels)
      #(
        next_store,
        store.find_effective_variant_by_inventory_item_id(
          next_store,
          option.unwrap(inventory_item_id, ""),
        ),
        Some([]),
        [level.id],
      )
    }
    Some(#(variant, level)), _, [] -> #(
      store,
      Some(variant),
      Some([#(variant, level)]),
      case variant.inventory_item {
        Some(item) -> [level.id, item.id]
        None -> [level.id]
      },
    )
    _, _, [] -> #(store, variant, None, [])
    _, _, _ -> #(store, None, None, [])
  }
  let #(next_store, payload_variant, response_levels, staged_ids) = outcome
  mutation_result(
    key,
    inventory_bulk_toggle_activation_payload(
      next_store,
      payload_variant,
      response_levels,
      user_errors,
      field,
      fragments,
    ),
    next_store,
    identity,
    staged_ids,
  )
}

fn handle_inventory_item_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let inventory_item_id = read_string_field(args, "id")
  let input = read_arg_object(args, "input") |> option.unwrap(dict.new())
  let existing_variant = case inventory_item_id {
    Some(inventory_item_id) ->
      store.find_effective_variant_by_inventory_item_id(
        store,
        inventory_item_id,
      )
    None -> None
  }
  case existing_variant {
    Some(variant) ->
      case variant.inventory_item {
        Some(existing_item) -> {
          let #(next_item, _) =
            read_variant_inventory_item(
              identity,
              Some(input),
              Some(existing_item),
            )
          case next_item {
            Some(next_item) -> {
              let next_variant =
                ProductVariantRecord(..variant, inventory_item: Some(next_item))
              let next_variants =
                store.get_effective_variants_by_product_id(
                  store,
                  variant.product_id,
                )
                |> list.map(fn(candidate) {
                  case candidate.id == variant.id {
                    True -> next_variant
                    False -> candidate
                  }
                })
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  variant.product_id,
                  next_variants,
                )
              let #(_, synced_store, synced_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity,
                  variant.product_id,
                )
              let updated_variant =
                store.get_effective_variant_by_id(synced_store, variant.id)
                |> option.unwrap(next_variant)
              mutation_result(
                key,
                inventory_item_update_payload(
                  synced_store,
                  Some(updated_variant),
                  [],
                  field,
                  fragments,
                ),
                synced_store,
                synced_identity,
                variant_staged_ids(updated_variant),
              )
            }
            None ->
              inventory_item_update_missing_result(
                key,
                store,
                identity,
                field,
                fragments,
              )
          }
        }
        None ->
          inventory_item_update_missing_result(
            key,
            store,
            identity,
            field,
            fragments,
          )
      }
    None ->
      inventory_item_update_missing_result(
        key,
        store,
        identity,
        field,
        fragments,
      )
  }
}

fn handle_inventory_set_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let input = read_arg_object(args, "input") |> option.unwrap(dict.new())
  let quantity_name = read_non_empty_string_field(input, "name")
  let reason = read_non_empty_string_field(input, "reason")
  let quantities = read_inventory_set_quantity_inputs(input)
  let ignore_compare_quantity =
    read_bool_field(input, "ignoreCompareQuantity") == Some(True)
  case quantity_name, reason, quantities {
    None, _, _ ->
      inventory_quantity_mutation_result(
        key,
        "InventorySetQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "name"],
            "Inventory quantity name is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    Some(name), _, _ -> {
      case valid_staged_inventory_quantity_name(name) {
        False ->
          inventory_quantity_mutation_result(
            key,
            "InventorySetQuantitiesPayload",
            store,
            identity,
            None,
            [invalid_inventory_quantity_name_error(["input", "name"])],
            field,
            fragments,
            [],
          )
        True ->
          handle_valid_inventory_set_quantities(
            store,
            identity,
            key,
            input,
            name,
            reason,
            quantities,
            ignore_compare_quantity,
            field,
            fragments,
          )
      }
    }
  }
}

fn handle_valid_inventory_set_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  input: Dict(String, ResolvedValue),
  name: String,
  reason: Option(String),
  quantities: List(InventorySetQuantityInput),
  ignore_compare_quantity: Bool,
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  case reason, quantities {
    None, _ ->
      inventory_quantity_mutation_result(
        key,
        "InventorySetQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "reason"],
            "Inventory adjustment reason is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    _, [] ->
      inventory_quantity_mutation_result(
        key,
        "InventorySetQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "quantities"],
            "At least one inventory quantity is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    Some(reason), quantities -> {
      case
        !ignore_compare_quantity
        && list.any(quantities, fn(quantity) {
          quantity.compare_quantity == None
        })
      {
        True ->
          inventory_quantity_mutation_result(
            key,
            "InventorySetQuantitiesPayload",
            store,
            identity,
            None,
            [
              ProductUserError(
                ["input", "ignoreCompareQuantity"],
                "The compareQuantity argument must be given to each quantity or ignored using ignoreCompareQuantity.",
                None,
              ),
            ],
            field,
            fragments,
            [],
          )
        False -> {
          let result =
            apply_inventory_set_quantities(
              store,
              identity,
              input,
              name,
              reason,
              quantities,
              ignore_compare_quantity,
            )
          case result {
            Error(errors) ->
              inventory_quantity_mutation_result(
                key,
                "InventorySetQuantitiesPayload",
                store,
                identity,
                None,
                errors,
                field,
                fragments,
                [],
              )
            Ok(applied) -> {
              let #(next_store, next_identity, group, staged_ids) = applied
              inventory_quantity_mutation_result(
                key,
                "InventorySetQuantitiesPayload",
                next_store,
                next_identity,
                Some(group),
                [],
                field,
                fragments,
                staged_ids,
              )
            }
          }
        }
      }
    }
  }
}

fn handle_inventory_move_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let input = read_arg_object(args, "input") |> option.unwrap(dict.new())
  let reason = read_non_empty_string_field(input, "reason")
  let changes = read_inventory_move_quantity_inputs(input)
  case reason, changes {
    None, _ ->
      inventory_quantity_mutation_result(
        key,
        "InventoryMoveQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "reason"],
            "Inventory adjustment reason is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    _, [] ->
      inventory_quantity_mutation_result(
        key,
        "InventoryMoveQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "changes"],
            "At least one inventory quantity move is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    Some(reason), changes -> {
      case validate_inventory_move_inputs(changes) {
        [_, ..] as errors ->
          inventory_quantity_mutation_result(
            key,
            "InventoryMoveQuantitiesPayload",
            store,
            identity,
            None,
            errors,
            field,
            fragments,
            [],
          )
        [] -> {
          let result =
            apply_inventory_move_quantities(
              store,
              identity,
              input,
              reason,
              changes,
            )
          case result {
            Error(errors) ->
              inventory_quantity_mutation_result(
                key,
                "InventoryMoveQuantitiesPayload",
                store,
                identity,
                None,
                errors,
                field,
                fragments,
                [],
              )
            Ok(applied) -> {
              let #(next_store, next_identity, group, staged_ids) = applied
              inventory_quantity_mutation_result(
                key,
                "InventoryMoveQuantitiesPayload",
                next_store,
                next_identity,
                Some(group),
                [],
                field,
                fragments,
                staged_ids,
              )
            }
          }
        }
      }
    }
  }
}

fn product_update_validation_error(
  input: Dict(String, ResolvedValue),
) -> Option(ProductUserError) {
  case read_string_field(input, "title") {
    Some(title) ->
      case string.length(string.trim(title)) == 0 {
        True -> Some(ProductUserError(["title"], "Title can't be blank", None))
        False -> product_update_handle_validation_error(input)
      }
    None -> product_update_handle_validation_error(input)
  }
}

fn product_create_validation_error(
  input: Dict(String, ResolvedValue),
) -> Option(ProductUserError) {
  case read_string_field(input, "title") {
    Some(title) ->
      case string.length(string.trim(title)) == 0 {
        True -> Some(ProductUserError(["title"], "Title can't be blank", None))
        False -> product_create_handle_validation_error(input)
      }
    None -> Some(ProductUserError(["title"], "Title can't be blank", None))
  }
}

fn product_create_handle_validation_error(
  input: Dict(String, ResolvedValue),
) -> Option(ProductUserError) {
  case read_explicit_product_handle(input) {
    Some(handle) ->
      case string.length(handle) > 255 {
        True ->
          Some(ProductUserError(
            ["handle"],
            "Handle is too long (maximum is 255 characters)",
            None,
          ))
        False -> None
      }
    None -> None
  }
}

fn product_update_handle_validation_error(
  input: Dict(String, ResolvedValue),
) -> Option(ProductUserError) {
  case read_string_field(input, "handle") {
    Some(handle) ->
      case string.length(handle) > 255 {
        True ->
          Some(ProductUserError(
            ["handle"],
            "Handle is too long (maximum is 255 characters)",
            None,
          ))
        False -> None
      }
    None -> None
  }
}

fn handle_collection_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let input = read_arg_object(args, "input")
  let collection_id = case input {
    Some(input) -> read_arg_string(input, "id")
    None -> None
  }
  case collection_id {
    None ->
      mutation_result(
        key,
        collection_update_payload(
          store,
          None,
          [
            ProductUserError(["input", "id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id), input {
        None, _ ->
          mutation_result(
            key,
            collection_update_payload(
              store,
              None,
              [
                ProductUserError(["input", "id"], "Collection not found", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection), Some(input) -> {
          let #(next_collection, next_identity) =
            updated_collection_record(identity, collection, input)
          let next_store =
            store.upsert_staged_collections(store, [next_collection])
          mutation_result(
            key,
            collection_update_payload(
              next_store,
              Some(next_collection),
              [],
              field,
              fragments,
            ),
            next_store,
            next_identity,
            [next_collection.id],
          )
        }
        Some(_), None ->
          mutation_result(
            key,
            collection_update_payload(
              store,
              None,
              [
                ProductUserError(
                  ["input", "id"],
                  "Collection id is required",
                  None,
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
  }
}

fn handle_collection_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let input = read_arg_object(args, "input") |> option.unwrap(dict.new())
  case read_non_empty_string_field(input, "title") {
    None ->
      mutation_result(
        key,
        collection_create_payload(
          store,
          None,
          [
            ProductUserError(
              ["input", "title"],
              "Collection title is required",
              None,
            ),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(_) -> {
      let #(collection, next_identity) =
        created_collection_record(store, identity, input)
      let next_store = store.upsert_staged_collections(store, [collection])
      mutation_result(
        key,
        collection_create_payload(
          next_store,
          Some(collection),
          [],
          field,
          fragments,
        ),
        next_store,
        next_identity,
        [collection.id],
      )
    }
  }
}

fn handle_collection_add_products(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        collection_add_products_payload(
          store,
          None,
          [
            ProductUserError(["id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_add_products_payload(
              store,
              None,
              [
                ProductUserError(["id"], "Collection does not exist", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection) -> {
          let result =
            add_products_to_collection(
              store,
              collection,
              read_arg_string_list(args, "productIds"),
            )
          let #(next_store, result_collection, user_errors) = result
          let staged_ids = case user_errors, result_collection {
            [], Some(record) -> [record.id]
            _, _ -> []
          }
          mutation_result(
            key,
            collection_add_products_payload(
              next_store,
              result_collection,
              user_errors,
              field,
              fragments,
            ),
            next_store,
            identity,
            staged_ids,
          )
        }
      }
  }
}

fn add_products_to_collection(
  store: Store,
  collection: CollectionRecord,
  product_ids: List(String),
) -> #(Store, Option(CollectionRecord), List(ProductUserError)) {
  let normalized_product_ids = dedupe_preserving_order(product_ids)
  case normalized_product_ids {
    [] -> #(store, None, [
      ProductUserError(
        ["productIds"],
        "At least one product id is required",
        None,
      ),
    ])
    _ ->
      case collection.is_smart {
        True -> #(store, None, [
          ProductUserError(
            ["id"],
            "Can't manually add products to a smart collection",
            None,
          ),
        ])
        False ->
          case
            list.find(normalized_product_ids, fn(product_id) {
              product_already_in_collection(store, collection.id, product_id)
            })
          {
            Ok(_) -> #(store, None, [
              ProductUserError(
                ["productIds"],
                "Product is already in the collection",
                None,
              ),
            ])
            Error(_) ->
              stage_collection_product_memberships(
                store,
                collection,
                normalized_product_ids,
              )
          }
      }
  }
}

fn stage_collection_product_memberships(
  store: Store,
  collection: CollectionRecord,
  product_ids: List(String),
) -> #(Store, Option(CollectionRecord), List(ProductUserError)) {
  let existing_product_ids =
    product_ids
    |> list.filter(fn(product_id) {
      case store.get_effective_product_by_id(store, product_id) {
        Some(_) -> True
        None -> False
      }
    })
  case existing_product_ids {
    [] -> #(store, Some(collection), [])
    _ -> {
      let max_position =
        store.list_effective_products_for_collection(store, collection.id)
        |> list.fold(-1, fn(max_position, entry) {
          let #(_, membership) = entry
          case int.compare(membership.position, max_position) {
            order.Gt -> membership.position
            _ -> max_position
          }
        })
      let existing_memberships =
        store.list_effective_products_for_collection(store, collection.id)
      let first_position = max_position + 1
      let memberships =
        existing_product_ids
        |> enumerate_strings()
        |> list.map(fn(entry) {
          let #(product_id, index) = entry
          ProductCollectionRecord(
            collection_id: collection.id,
            product_id: product_id,
            position: first_position + index,
            cursor: None,
          )
        })
      let next_count =
        list.length(existing_memberships) + list.length(memberships)
      let next_collection =
        CollectionRecord(..collection, products_count: Some(next_count))
      let next_store =
        store
        |> store.upsert_staged_collections([next_collection])
        |> store.upsert_staged_product_collections(memberships)
      #(next_store, Some(next_collection), [])
    }
  }
}

fn handle_collection_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  let input = read_arg_object(args, "input")
  let collection_id = case input {
    Some(input) -> read_arg_string(input, "id")
    None -> None
  }
  case collection_id {
    None ->
      mutation_result(
        key,
        collection_delete_payload(
          None,
          [
            ProductUserError(["input", "id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_delete_payload(
              None,
              [
                ProductUserError(["input", "id"], "Collection not found", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let next_store = store.delete_staged_collection(store, collection_id)
          mutation_result(
            key,
            collection_delete_payload(Some(collection_id), [], field, fragments),
            next_store,
            identity,
            [collection_id],
          )
        }
      }
  }
}

fn handle_collection_remove_products(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        collection_remove_products_payload(
          None,
          [
            ProductUserError(["id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_remove_products_payload(
              None,
              [
                ProductUserError(["id"], "Collection not found", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection) -> {
          let next_store =
            remove_products_from_collection(
              store,
              collection,
              read_arg_string_list(args, "productIds"),
            )
          let next_count =
            store.list_effective_products_for_collection(
              next_store,
              collection.id,
            )
            |> list.length
          let next_collection =
            CollectionRecord(..collection, products_count: Some(next_count))
          let next_store =
            store.upsert_staged_collections(next_store, [next_collection])
          let #(job_id, next_identity) =
            synthetic_identity.make_synthetic_gid(identity, "Job")
          mutation_result(
            key,
            collection_remove_products_payload(
              Some(job_id),
              [],
              field,
              fragments,
            ),
            next_store,
            next_identity,
            [collection.id],
          )
        }
      }
  }
}

fn remove_products_from_collection(
  store: Store,
  collection: CollectionRecord,
  product_ids: List(String),
) -> Store {
  let normalized_product_ids = dedupe_preserving_order(product_ids)
  list.fold(normalized_product_ids, store, fn(current_store, product_id) {
    case store.get_effective_product_by_id(current_store, product_id) {
      None -> current_store
      Some(_) -> {
        let next_memberships =
          store.list_effective_collections_for_product(
            current_store,
            product_id,
          )
          |> list.filter(fn(entry) {
            let #(existing_collection, _) = entry
            existing_collection.id != collection.id
          })
          |> list.map(fn(entry) {
            let #(_, membership) = entry
            membership
          })
        store.replace_staged_collections_for_product(
          current_store,
          product_id,
          next_memberships,
        )
      }
    }
  })
}

fn handle_collection_reorder_products(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        collection_reorder_products_payload(
          None,
          [
            ProductUserError(["id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_reorder_products_payload(
              None,
              [
                ProductUserError(
                  ["id"],
                  "Collection not found",
                  Some("COLLECTION_NOT_FOUND"),
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection) -> {
          let result =
            reorder_collection_products(
              store,
              collection,
              read_arg_object_list(args, "moves"),
            )
          let #(next_store, user_errors) = result
          case user_errors {
            [] -> {
              let #(job_id, next_identity) =
                synthetic_identity.make_synthetic_gid(identity, "Job")
              mutation_result(
                key,
                collection_reorder_products_payload(
                  Some(job_id),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                [collection.id],
              )
            }
            _ ->
              mutation_result(
                key,
                collection_reorder_products_payload(
                  None,
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
          }
        }
      }
  }
}

fn reorder_collection_products(
  store: Store,
  collection: CollectionRecord,
  raw_moves: List(Dict(String, ResolvedValue)),
) -> #(Store, List(ProductUserError)) {
  case
    collection.is_smart
    || case collection.sort_order {
      Some(sort_order) -> sort_order != "MANUAL"
      None -> False
    }
  {
    True -> #(store, [
      ProductUserError(
        ["id"],
        "Can't reorder products unless collection is manually sorted",
        Some("MANUALLY_SORTED_COLLECTION"),
      ),
    ])
    False -> {
      let #(moves, user_errors) = read_collection_product_moves(raw_moves)
      let ordered_entries =
        store.list_effective_products_for_collection(store, collection.id)
      let product_ids_in_collection =
        ordered_entries
        |> list.map(fn(entry) {
          let #(product, _) = entry
          product.id
        })
      let user_errors =
        list.fold(enumerate_items(moves), user_errors, fn(errors, entry) {
          let #(move, index) = entry
          let CollectionProductMove(id: product_id, new_position: _) = move
          case store.get_effective_product_by_id(store, product_id) {
            None ->
              list.append(errors, [
                ProductUserError(
                  ["moves", int.to_string(index), "id"],
                  "Product does not exist",
                  Some("INVALID_MOVE"),
                ),
              ])
            Some(_) ->
              case list.contains(product_ids_in_collection, product_id) {
                True -> errors
                False ->
                  list.append(errors, [
                    ProductUserError(
                      ["moves", int.to_string(index), "id"],
                      "Product is not in the collection",
                      Some("INVALID_MOVE"),
                    ),
                  ])
              }
          }
        })
      case user_errors {
        [] -> {
          let reordered_entries =
            apply_collection_product_moves(ordered_entries, moves)
          let next_store =
            reordered_entries
            |> enumerate_items()
            |> list.fold(store, fn(current_store, entry) {
              let #(#(product, _), position) = entry
              let next_memberships =
                store.list_effective_collections_for_product(
                  current_store,
                  product.id,
                )
                |> list.map(fn(collection_entry) {
                  let #(existing_collection, membership) = collection_entry
                  case existing_collection.id == collection.id {
                    True ->
                      ProductCollectionRecord(..membership, position: position)
                    False -> membership
                  }
                })
              store.replace_staged_collections_for_product(
                current_store,
                product.id,
                next_memberships,
              )
            })
          #(next_store, [])
        }
        _ -> #(store, user_errors)
      }
    }
  }
}

fn read_collection_product_moves(
  raw_moves: List(Dict(String, ResolvedValue)),
) -> #(List(CollectionProductMove), List(ProductUserError)) {
  case raw_moves {
    [] -> #([], [
      ProductUserError(
        ["moves"],
        "At least one move is required",
        Some("INVALID_MOVE"),
      ),
    ])
    _ -> {
      let too_many_errors = case list.length(raw_moves) > 250 {
        True -> [
          ProductUserError(
            ["moves"],
            "Too many moves were provided",
            Some("INVALID_MOVE"),
          ),
        ]
        False -> []
      }
      let result =
        raw_moves
        |> enumerate_items()
        |> list.fold(#([], too_many_errors), fn(acc, entry) {
          let #(moves, errors) = acc
          let #(raw_move, index) = entry
          let product_id = read_string_field(raw_move, "id")
          let new_position = read_collection_reorder_position(raw_move)
          let errors = case product_id {
            Some(_) -> errors
            None ->
              list.append(errors, [
                ProductUserError(
                  ["moves", int.to_string(index), "id"],
                  "Product id is required",
                  Some("INVALID_MOVE"),
                ),
              ])
          }
          let errors = case new_position {
            Some(_) -> errors
            None ->
              list.append(errors, [
                ProductUserError(
                  ["moves", int.to_string(index), "newPosition"],
                  "Position is invalid",
                  Some("INVALID_MOVE"),
                ),
              ])
          }
          case product_id, new_position {
            Some(id), Some(position) -> #(
              list.append(moves, [
                CollectionProductMove(id: id, new_position: position),
              ]),
              errors,
            )
            _, _ -> #(moves, errors)
          }
        })
      result
    }
  }
}

fn read_collection_reorder_position(
  fields: Dict(String, ResolvedValue),
) -> Option(Int) {
  case dict.get(fields, "newPosition") {
    Ok(IntVal(value)) -> Some(int.max(0, value))
    Ok(StringVal(value)) -> parse_unsigned_int_string(value)
    _ -> None
  }
}

fn parse_unsigned_int_string(value: String) -> Option(Int) {
  let trimmed = string.trim(value)
  case
    string.length(trimmed) > 0
    && list.all(string.to_graphemes(trimmed), is_decimal_digit)
  {
    False -> None
    True ->
      case int.parse(trimmed) {
        Ok(parsed) -> Some(parsed)
        Error(_) -> None
      }
  }
}

fn is_decimal_digit(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    _ -> False
  }
}

fn apply_collection_product_moves(
  entries: List(#(ProductRecord, ProductCollectionRecord)),
  moves: List(CollectionProductMove),
) -> List(#(ProductRecord, ProductCollectionRecord)) {
  list.fold(moves, entries, fn(current_entries, move) {
    let CollectionProductMove(id: product_id, new_position: new_position) = move
    case
      list.find(current_entries, fn(entry) {
        let #(product, _) = entry
        product.id == product_id
      })
    {
      Error(_) -> current_entries
      Ok(entry) -> {
        let without_entry =
          current_entries
          |> list.filter(fn(candidate) {
            let #(product, _) = candidate
            product.id != product_id
          })
        insert_collection_entry(without_entry, entry, new_position)
      }
    }
  })
}

fn insert_collection_entry(
  entries: List(#(ProductRecord, ProductCollectionRecord)),
  entry: #(ProductRecord, ProductCollectionRecord),
  position: Int,
) -> List(#(ProductRecord, ProductCollectionRecord)) {
  let insertion_index = int.min(position, list.length(entries))
  let before = list.take(entries, insertion_index)
  let after = list.drop(entries, insertion_index)
  list.append(before, [entry, ..after])
}

fn product_already_in_collection(
  store: Store,
  collection_id: String,
  product_id: String,
) -> Bool {
  store.list_effective_collections_for_product(store, product_id)
  |> list.any(fn(entry) {
    let #(collection, _) = entry
    collection.id == collection_id
  })
}

fn enumerate_strings(values: List(String)) -> List(#(String, Int)) {
  values
  |> enumerate_items()
}

fn handle_tags_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  is_add: Bool,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = field_args(field, variables)
  case read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        tags_update_payload(
          store,
          is_add,
          None,
          [ProductUserError(["id"], "Product id is required", None)],
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
            tags_update_payload(
              store,
              is_add,
              None,
              [ProductUserError(["id"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) -> {
          let tags = read_tag_inputs(args, is_add)
          case tags {
            [] ->
              mutation_result(
                key,
                tags_update_payload(
                  store,
                  is_add,
                  None,
                  [
                    ProductUserError(
                      ["tags"],
                      "At least one tag is required",
                      None,
                    ),
                  ],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            _ -> {
              let next_tags = case is_add {
                True -> normalize_product_tags(list.append(product.tags, tags))
                False ->
                  normalize_product_tags(
                    list.filter(product.tags, fn(tag) {
                      !list.contains(tags, tag)
                    }),
                  )
              }
              let next_product = ProductRecord(..product, tags: next_tags)
              let #(_, next_store) =
                store.upsert_staged_product(store, next_product)
              mutation_result(
                key,
                tags_update_payload(
                  next_store,
                  is_add,
                  Some(next_product),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                identity,
                [next_product.id],
              )
            }
          }
        }
      }
  }
}

fn product_delete_input_error(
  document: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(Json) {
  let arguments = case field {
    Field(arguments: args, ..) -> args
    _ -> []
  }
  case find_argument(arguments, "input") {
    Some(argument) ->
      case argument.value {
        ObjectValue(fields: fields, loc: loc) ->
          case find_object_field(fields, "id") {
            None ->
              Some(build_product_delete_missing_input_id_error(loc, document))
            Some(ObjectField(value: NullValue(..), ..)) ->
              Some(build_product_delete_null_input_id_error(loc, document))
            Some(_) -> None
          }
        VariableValue(variable: variable) -> {
          let args = field_args(field, variables)
          let input = read_arg_object(args, "input")
          let invalid = case input {
            Some(input) ->
              case dict.get(input, "id") {
                Ok(StringVal(_)) -> False
                _ -> True
              }
            None -> True
          }
          case invalid {
            False -> None
            True ->
              Some(build_product_delete_invalid_variable_error(
                variable.name.value,
                resolved_input_to_json(input),
                find_variable_definition_location(document, variable.name.value),
                document,
              ))
          }
        }
        _ -> None
      }
    None -> None
  }
}

fn find_object_field(
  fields: List(ObjectField),
  name: String,
) -> Option(ObjectField) {
  case fields {
    [] -> None
    [first, ..rest] -> {
      let ObjectField(name: field_name, ..) = first
      case field_name.value == name {
        True -> Some(first)
        False -> find_object_field(rest, name)
      }
    }
  }
}

fn build_product_delete_missing_input_id_error(
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

fn build_product_delete_null_input_id_error(
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

fn product_delete_input_object_error(
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

fn build_product_delete_invalid_variable_error(
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

fn find_variable_definition_location(
  document: String,
  variable_name: String,
) -> Option(Location) {
  case parser.parse(graphql_source.new(document)) {
    Ok(parsed) ->
      find_variable_definition_location_in_definitions(
        parsed.definitions,
        variable_name,
      )
    Error(_) -> None
  }
}

fn find_variable_definition_location_in_definitions(
  definitions: List(Definition),
  variable_name: String,
) -> Option(Location) {
  case definitions {
    [] -> None
    [definition, ..rest] ->
      case definition {
        OperationDefinition(variable_definitions: definitions, ..) ->
          case find_variable_definition(definitions, variable_name) {
            Some(location) -> Some(location)
            None ->
              find_variable_definition_location_in_definitions(
                rest,
                variable_name,
              )
          }
        _ ->
          find_variable_definition_location_in_definitions(rest, variable_name)
      }
  }
}

fn find_variable_definition(
  definitions: List(VariableDefinition),
  variable_name: String,
) -> Option(Location) {
  case definitions {
    [] -> None
    [definition, ..rest] -> {
      let VariableDefinition(variable: variable, loc: loc, ..) = definition
      case variable.name.value == variable_name {
        True -> loc
        False -> find_variable_definition(rest, variable_name)
      }
    }
  }
}

fn locations_payload(loc: Option(Location), document: String) -> Option(Json) {
  case loc {
    None -> None
    Some(loc) -> {
      let source = graphql_source.new(document)
      let computed = graphql_location.get_location(source, position: loc.start)
      Some(
        json.preprocessed_array([
          json.object([
            #("line", json.int(computed.line)),
            #("column", json.int(computed.column)),
          ]),
        ]),
      )
    }
  }
}

fn resolved_input_to_json(input: Option(Dict(String, ResolvedValue))) -> Json {
  case input {
    Some(fields) ->
      json.object(
        list.map(dict.to_list(fields), fn(entry) {
          let #(key, value) = entry
          #(key, resolved_value_to_json(value))
        }),
      )
    None -> json.null()
  }
}

fn resolved_value_to_json(value: ResolvedValue) -> Json {
  case value {
    StringVal(value) -> json.string(value)
    IntVal(value) -> json.int(value)
    FloatVal(value) -> json.float(value)
    BoolVal(value) -> json.bool(value)
    NullVal -> json.null()
    ListVal(values) -> json.array(values, resolved_value_to_json)
    ObjectVal(fields) ->
      json.object(
        list.map(dict.to_list(fields), fn(entry) {
          let #(key, value) = entry
          #(key, resolved_value_to_json(value))
        }),
      )
  }
}

fn product_change_status_null_product_id_error(
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

fn is_valid_product_status(status: Option(String)) -> Bool {
  case status {
    Some("ACTIVE") | Some("ARCHIVED") | Some("DRAFT") -> True
    _ -> False
  }
}

fn stage_product_options_reorder(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
  option_inputs: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let product_id = product.id
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let #(next_options, user_errors) =
    reorder_product_options(existing_options, option_inputs)
  case user_errors {
    [_, ..] ->
      mutation_result(
        key,
        product_options_reorder_payload(
          store,
          Some(product),
          user_errors,
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    [] -> {
      let next_variants =
        store.get_effective_variants_by_product_id(store, product_id)
        |> reorder_variant_selections_for_options(next_options)
      let synced_options =
        sync_product_options_with_variants(next_options, next_variants)
      let next_store =
        store
        |> store.replace_staged_options_for_product(product_id, synced_options)
        |> store.replace_staged_variants_for_product(product_id, next_variants)
      mutation_result(
        key,
        product_options_reorder_payload(
          next_store,
          store.get_effective_product_by_id(next_store, product_id),
          [],
          field,
          fragments,
        ),
        next_store,
        identity,
        list.map(next_options, fn(option) { option.id }),
      )
    }
  }
}

fn stage_product_options_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
  option_ids: List(String),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let product_id = product.id
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let existing_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  let existing_ids = list.map(existing_options, fn(option) { option.id })
  let unknown_errors = unknown_option_errors(option_ids, existing_ids)
  case unknown_errors {
    [_, ..] ->
      mutation_result(
        key,
        product_options_delete_payload(
          store,
          [],
          Some(product),
          unknown_errors,
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    [] -> {
      let deleted_ids =
        existing_options
        |> list.filter(fn(option) { list.contains(option_ids, option.id) })
        |> list.map(fn(option) { option.id })
      let remaining_options =
        existing_options
        |> list.filter(fn(option) { !list.contains(option_ids, option.id) })
        |> position_options(1, [])
      let #(next_options, next_variants, final_identity, restored_ids) = case
        remaining_options
      {
        [] -> restore_default_option_state(identity, product, existing_variants)
        [_, ..] -> #(remaining_options, existing_variants, identity, [])
      }
      let synced_options =
        sync_product_options_with_variants(next_options, next_variants)
      let next_store =
        store
        |> store.replace_staged_variants_for_product(product_id, next_variants)
        |> store.replace_staged_options_for_product(product_id, synced_options)
      mutation_result(
        key,
        product_options_delete_payload(
          next_store,
          deleted_ids,
          store.get_effective_product_by_id(next_store, product_id),
          [],
          field,
          fragments,
        ),
        next_store,
        final_identity,
        list.append(deleted_ids, restored_ids),
      )
    }
  }
}

fn handle_product_option_update(
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
        product_option_update_payload(
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
            product_option_update_payload(
              store,
              None,
              [ProductUserError(["productId"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) ->
          case read_arg_object(args, "option") {
            None ->
              mutation_result(
                key,
                product_option_update_payload(
                  store,
                  None,
                  [
                    ProductUserError(
                      ["option", "id"],
                      "Option id is required",
                      None,
                    ),
                  ],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(option_input) ->
              stage_product_option_update(
                store,
                identity,
                key,
                product,
                option_input,
                args,
                field,
                fragments,
              )
          }
      }
  }
}

fn stage_product_option_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
  option_input: Dict(String, ResolvedValue),
  args: Dict(String, ResolvedValue),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let product_id = product.id
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let existing_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  case read_string_field(option_input, "id") {
    None ->
      mutation_result(
        key,
        product_option_update_payload(
          store,
          None,
          [
            ProductUserError(["option", "id"], "Option id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(option_id) ->
      case find_product_option(existing_options, product_id, option_id) {
        None ->
          mutation_result(
            key,
            product_option_update_payload(
              store,
              Some(product),
              [ProductUserError(["option"], "Option does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(target_option) -> {
          let #(updated_option, renamed_values, identity_after_values, new_ids) =
            update_product_option_record(
              identity,
              target_option,
              option_input,
              read_arg_object_list(args, "optionValuesToAdd"),
              read_arg_object_list(args, "optionValuesToUpdate"),
              read_arg_string_list(args, "optionValuesToDelete"),
            )
          let next_options =
            existing_options
            |> list.filter(fn(option) { option.id != option_id })
            |> insert_option_at_position(
              updated_option,
              read_int_field(option_input, "position"),
            )
          let next_variants =
            existing_variants
            |> remap_variant_selections_for_option_update(
              target_option.name,
              updated_option.name,
              renamed_values,
            )
            |> reorder_variant_selections_for_options(next_options)
          let synced_options =
            sync_product_options_with_variants(next_options, next_variants)
          let next_store =
            store
            |> store.replace_staged_variants_for_product(
              product_id,
              next_variants,
            )
            |> store.replace_staged_options_for_product(
              product_id,
              synced_options,
            )
          mutation_result(
            key,
            product_option_update_payload(
              next_store,
              store.get_effective_product_by_id(next_store, product_id),
              [],
              field,
              fragments,
            ),
            next_store,
            identity_after_values,
            list.append([option_id], new_ids),
          )
        }
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
    top_level_errors: [],
  )
}

fn mutation_error_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  errors: List(Json),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: json.null(),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: errors,
  )
}

fn product_delete_payload(
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

fn product_update_payload(
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
      #("__typename", SrcString("ProductUpdatePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_create_payload(
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
      #("__typename", SrcString("ProductCreatePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_variant_payload(
  store: Store,
  product: Option(ProductRecord),
  variant: Option(ProductVariantRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  let variant_value = case variant {
    Some(record) -> product_variant_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductVariantPayload")),
      #("product", product_value),
      #("productVariant", variant_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_variant_delete_payload(
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

fn product_variants_bulk_payload(
  typename: String,
  store: Store,
  product: Option(ProductRecord),
  variants: List(ProductVariantRecord),
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
      #("__typename", SrcString(typename)),
      #("product", product_value),
      #(
        "productVariants",
        SrcList(
          list.map(variants, fn(variant) {
            product_variant_source(store, variant)
          }),
        ),
      ),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_variants_bulk_delete_payload(
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
      #("__typename", SrcString("ProductVariantsBulkDeletePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_variants_bulk_reorder_payload(
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
      #("__typename", SrcString("ProductVariantsBulkReorderPayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_quantity_mutation_result(
  key: String,
  typename: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  group: Option(InventoryAdjustmentGroup),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
  staged_ids: List(String),
) -> MutationFieldResult {
  mutation_result(
    key,
    inventory_quantity_payload(
      typename,
      store,
      group,
      user_errors,
      field,
      fragments,
    ),
    store,
    identity,
    staged_ids,
  )
}

fn inventory_quantity_payload(
  typename: String,
  store: Store,
  group: Option(InventoryAdjustmentGroup),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #(
        "inventoryAdjustmentGroup",
        inventory_adjustment_group_source(store, group),
      ),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_activate_payload(
  store: Store,
  resolved: Option(#(ProductVariantRecord, InventoryLevelRecord)),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let inventory_level = case resolved {
    Some(#(variant, level)) ->
      inventory_level_source_with_item(store, variant, level)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryActivatePayload")),
      #("inventoryLevel", inventory_level),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_deactivate_payload(
  user_errors: List(NullableFieldUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryDeactivatePayload")),
      #("userErrors", nullable_field_user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_bulk_toggle_activation_payload(
  store: Store,
  variant: Option(ProductVariantRecord),
  levels: Option(List(#(ProductVariantRecord, InventoryLevelRecord))),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let inventory_item = case variant {
    Some(variant) -> inventory_item_source(store, variant)
    None -> SrcNull
  }
  let inventory_levels = case levels {
    Some(levels) ->
      SrcList(
        list.map(levels, fn(level) {
          let #(variant, level) = level
          inventory_level_source_with_item(store, variant, level)
        }),
      )
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryBulkToggleActivationPayload")),
      #("inventoryItem", inventory_item),
      #("inventoryLevels", inventory_levels),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn inventory_item_update_missing_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  mutation_result(
    key,
    inventory_item_update_payload(
      store,
      None,
      [
        ProductUserError(
          ["id"],
          "The product couldn't be updated because it does not exist.",
          None,
        ),
      ],
      field,
      fragments,
    ),
    store,
    identity,
    [],
  )
}

fn inventory_item_update_payload(
  store: Store,
  variant: Option(ProductVariantRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let inventory_item = case variant {
    Some(variant) -> inventory_item_source(store, variant)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryItemUpdatePayload")),
      #("inventoryItem", inventory_item),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn collection_add_products_payload(
  store: Store,
  collection: Option(CollectionRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let collection_value = case collection {
    Some(record) -> collection_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionAddProductsPayload")),
      #("collection", collection_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn collection_create_payload(
  store: Store,
  collection: Option(CollectionRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let collection_value = case collection {
    Some(record) -> collection_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionCreatePayload")),
      #("collection", collection_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn collection_update_payload(
  store: Store,
  collection: Option(CollectionRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let collection_value = case collection {
    Some(record) -> collection_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionUpdatePayload")),
      #("collection", collection_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn collection_delete_payload(
  deleted_collection_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let deleted_value = case deleted_collection_id {
    Some(id) -> SrcString(id)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionDeletePayload")),
      #("deletedCollectionId", deleted_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn collection_remove_products_payload(
  job_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let job_value = case job_id {
    Some(id) -> job_source(id, False)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionRemoveProductsPayload")),
      #("job", job_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn collection_reorder_products_payload(
  job_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let job_value = case job_id {
    Some(id) -> job_source(id, False)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionReorderProductsPayload")),
      #("job", job_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn job_source(id: String, done: Bool) -> SourceValue {
  src_object([
    #("__typename", SrcString("Job")),
    #("id", SrcString(id)),
    #("done", SrcBool(done)),
  ])
}

fn inventory_adjustment_group_source(
  store: Store,
  group: Option(InventoryAdjustmentGroup),
) -> SourceValue {
  case group {
    None -> SrcNull
    Some(group) ->
      src_object([
        #("__typename", SrcString("InventoryAdjustmentGroup")),
        #("id", SrcString(group.id)),
        #("createdAt", SrcString(group.created_at)),
        #("reason", SrcString(group.reason)),
        #(
          "referenceDocumentUri",
          optional_string_source(group.reference_document_uri),
        ),
        #("app", inventory_adjustment_app_source()),
        #(
          "changes",
          SrcList(
            list.map(group.changes, fn(change) {
              inventory_adjustment_change_source(store, change)
            }),
          ),
        ),
      ])
  }
}

fn inventory_adjustment_change_source(
  store: Store,
  change: InventoryAdjustmentChange,
) -> SourceValue {
  let location = inventory_change_location(store, change)
  let item = case
    store.find_effective_variant_by_inventory_item_id(
      store,
      change.inventory_item_id,
    )
  {
    Some(variant) -> inventory_item_source(store, variant)
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("InventoryChange")),
    #("name", SrcString(change.name)),
    #("delta", SrcInt(change.delta)),
    #("quantityAfterChange", optional_int_source(change.quantity_after_change)),
    #("ledgerDocumentUri", optional_string_source(change.ledger_document_uri)),
    #("item", item),
    #(
      "location",
      src_object([
        #("__typename", SrcString("Location")),
        #("id", SrcString(location.id)),
        #("name", SrcString(location.name)),
      ]),
    ),
  ])
}

fn inventory_adjustment_app_source() -> SourceValue {
  src_object([
    #("__typename", SrcString("App")),
    #("id", SrcNull),
    #("title", SrcString("hermes-conformance-products")),
    #("apiKey", SrcNull),
    #("handle", SrcString("hermes-conformance-products")),
  ])
}

fn tags_update_payload(
  store: Store,
  is_add: Bool,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  let typename = case is_add {
    True -> "TagsAddPayload"
    False -> "TagsRemovePayload"
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("node", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_options_delete_payload(
  store: Store,
  deleted_option_ids: List(String),
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
      #("__typename", SrcString("ProductOptionsDeletePayload")),
      #("deletedOptionsIds", SrcList(list.map(deleted_option_ids, SrcString))),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_change_status_payload(
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
      #("__typename", SrcString("ProductChangeStatusPayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_options_reorder_payload(
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
      #("__typename", SrcString("ProductOptionsReorderPayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_option_update_payload(
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
      #("__typename", SrcString("ProductOptionUpdatePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
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

fn nullable_field_user_errors_source(
  errors: List(NullableFieldUserError),
) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let NullableFieldUserError(field: field, message: message) = error
      let field_value = case field {
        Some(field) -> SrcList(list.map(field, SrcString))
        None -> SrcNull
      }
      src_object([
        #("field", field_value),
        #("message", SrcString(message)),
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

fn read_arg_object(
  args: Dict(String, ResolvedValue),
  name: String,
) -> Option(Dict(String, ResolvedValue)) {
  case dict.get(args, name) {
    Ok(ObjectVal(input)) -> Some(input)
    _ -> None
  }
}

fn read_arg_object_list(
  args: Dict(String, ResolvedValue),
  name: String,
) -> List(Dict(String, ResolvedValue)) {
  case dict.get(args, name) {
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

fn read_arg_string_list(
  args: Dict(String, ResolvedValue),
  name: String,
) -> List(String) {
  case dict.get(args, name) {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          StringVal(input) -> Ok(input)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_inventory_set_quantity_inputs(
  input: Dict(String, ResolvedValue),
) -> List(InventorySetQuantityInput) {
  case dict.get(input, "quantities") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) ->
            Ok(InventorySetQuantityInput(
              inventory_item_id: read_string_field(fields, "inventoryItemId"),
              location_id: read_string_field(fields, "locationId"),
              quantity: read_int_field(fields, "quantity"),
              compare_quantity: read_int_field(fields, "compareQuantity"),
            ))
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_inventory_adjustment_change_inputs(
  input: Dict(String, ResolvedValue),
) -> List(InventoryAdjustmentChangeInput) {
  case dict.get(input, "changes") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) ->
            Ok(InventoryAdjustmentChangeInput(
              inventory_item_id: read_string_field(fields, "inventoryItemId"),
              location_id: read_string_field(fields, "locationId"),
              ledger_document_uri: read_string_field(
                fields,
                "ledgerDocumentUri",
              ),
              delta: read_int_field(fields, "delta"),
              change_from_quantity: read_int_field(fields, "changeFromQuantity"),
            ))
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_inventory_move_quantity_inputs(
  input: Dict(String, ResolvedValue),
) -> List(InventoryMoveQuantityInput) {
  case dict.get(input, "changes") {
    Ok(ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          ObjectVal(fields) ->
            Ok(InventoryMoveQuantityInput(
              inventory_item_id: read_string_field(fields, "inventoryItemId"),
              quantity: read_int_field(fields, "quantity"),
              from: read_inventory_move_terminal(fields, "from"),
              to: read_inventory_move_terminal(fields, "to"),
            ))
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn read_inventory_move_terminal(
  input: Dict(String, ResolvedValue),
  name: String,
) -> InventoryMoveTerminalInput {
  case read_object_field(input, name) {
    Some(fields) ->
      InventoryMoveTerminalInput(
        location_id: read_string_field(fields, "locationId"),
        name: read_string_field(fields, "name"),
        ledger_document_uri: read_string_field(fields, "ledgerDocumentUri"),
      )
    None ->
      InventoryMoveTerminalInput(
        location_id: None,
        name: None,
        ledger_document_uri: None,
      )
  }
}

fn read_tag_inputs(
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

fn trimmed_non_empty(value: String) -> Result(String, Nil) {
  let trimmed = string.trim(value)
  case string.length(trimmed) > 0 {
    True -> Ok(trimmed)
    False -> Error(Nil)
  }
}

fn normalize_product_tags(tags: List(String)) -> List(String) {
  tags
  |> list.filter_map(trimmed_non_empty)
  |> list.fold(dict.new(), fn(seen, tag) { dict.insert(seen, tag, True) })
  |> dict.keys()
  |> list.sort(string.compare)
}

fn dedupe_preserving_order(values: List(String)) -> List(String) {
  let #(reversed, _) =
    list.fold(values, #([], dict.new()), fn(acc, value) {
      let #(items, seen) = acc
      case dict.has_key(seen, value) {
        True -> #(items, seen)
        False -> #([value, ..items], dict.insert(seen, value, True))
      }
    })
  list.reverse(reversed)
}

fn updated_product_record(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
  input: Dict(String, ResolvedValue),
) -> #(ProductRecord, SyntheticIdentityRegistry) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  #(
    ProductRecord(
      ..product,
      title: read_non_empty_string_field(input, "title")
        |> option.unwrap(product.title),
      handle: read_non_empty_string_field(input, "handle")
        |> option.unwrap(product.handle),
      status: read_product_status_field(input) |> option.unwrap(product.status),
      vendor: read_string_field(input, "vendor") |> option.or(product.vendor),
      product_type: read_string_field(input, "productType")
        |> option.or(product.product_type),
      tags: read_string_list_field(input, "tags")
        |> option.map(normalize_product_tags)
        |> option.unwrap(product.tags),
      description_html: read_string_field(input, "descriptionHtml")
        |> option.unwrap(product.description_html),
      template_suffix: read_string_field(input, "templateSuffix")
        |> option.or(product.template_suffix),
      seo: updated_product_seo(product.seo, input),
      updated_at: Some(updated_at),
    ),
    next_identity,
  )
}

fn created_product_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
) -> #(ProductRecord, SyntheticIdentityRegistry) {
  let title =
    read_non_empty_string_field(input, "title")
    |> option.unwrap("Untitled product")
  let #(created_at, identity_after_timestamp) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(id, next_identity) =
    synthetic_identity.make_proxy_synthetic_gid(
      identity_after_timestamp,
      "Product",
    )
  let base_handle = case read_explicit_product_handle(input) {
    Some(handle) -> handle
    None -> slugify_product_handle(title)
  }
  #(
    ProductRecord(
      id: id,
      legacy_resource_id: None,
      title: title,
      handle: ensure_unique_product_handle(store, base_handle),
      status: read_product_status_field(input) |> option.unwrap("ACTIVE"),
      vendor: read_string_field(input, "vendor"),
      product_type: read_string_field(input, "productType"),
      tags: read_string_list_field(input, "tags")
        |> option.map(normalize_product_tags)
        |> option.unwrap([]),
      total_inventory: Some(0),
      tracks_inventory: Some(False),
      created_at: Some(created_at),
      updated_at: Some(created_at),
      description_html: read_string_field(input, "descriptionHtml")
        |> option.unwrap(""),
      online_store_preview_url: None,
      template_suffix: read_string_field(input, "templateSuffix"),
      seo: updated_product_seo(
        ProductSeoRecord(title: None, description: None),
        input,
      ),
      category: None,
      cursor: None,
    ),
    next_identity,
  )
}

fn updated_collection_record(
  identity: SyntheticIdentityRegistry,
  collection: CollectionRecord,
  input: Dict(String, ResolvedValue),
) -> #(CollectionRecord, SyntheticIdentityRegistry) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let title =
    read_non_empty_string_field(input, "title")
    |> option.unwrap(collection.title)
  let handle =
    read_non_empty_string_field(input, "handle")
    |> option.unwrap(collection.handle)
  #(
    CollectionRecord(
      ..collection,
      title: title,
      handle: handle,
      updated_at: Some(updated_at),
      description: read_string_field(input, "description")
        |> option.or(collection.description),
      description_html: read_string_field(input, "descriptionHtml")
        |> option.or(collection.description_html),
      sort_order: read_string_field(input, "sortOrder")
        |> option.or(collection.sort_order),
      template_suffix: read_string_field(input, "templateSuffix")
        |> option.or(collection.template_suffix),
      seo: updated_product_seo(collection.seo, input),
    ),
    next_identity,
  )
}

fn created_collection_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
) -> #(CollectionRecord, SyntheticIdentityRegistry) {
  let title =
    read_non_empty_string_field(input, "title")
    |> option.unwrap("Untitled collection")
  let #(updated_at, identity_after_timestamp) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_timestamp,
      "Collection",
    )
  let handle = case read_non_empty_string_field(input, "handle") {
    Some(handle) -> normalize_product_handle(handle)
    None -> slugify_collection_handle(title)
  }
  #(
    CollectionRecord(
      id: id,
      legacy_resource_id: None,
      title: title,
      handle: ensure_unique_collection_handle(store, handle),
      publication_ids: [],
      updated_at: Some(updated_at),
      description: read_string_field(input, "description"),
      description_html: read_string_field(input, "descriptionHtml")
        |> option.or(Some("")),
      image: None,
      sort_order: read_string_field(input, "sortOrder")
        |> option.or(Some("MANUAL")),
      template_suffix: read_string_field(input, "templateSuffix"),
      seo: updated_product_seo(
        ProductSeoRecord(title: None, description: None),
        input,
      ),
      rule_set: None,
      products_count: Some(0),
      is_smart: False,
      cursor: None,
      title_cursor: None,
      updated_at_cursor: None,
    ),
    next_identity,
  )
}

fn slugify_collection_handle(title: String) -> String {
  let normalized = normalize_product_handle(title)
  let handle = case normalized {
    "" -> "untitled-collection"
    _ -> normalized
  }
  case string.ends_with(handle, "product") {
    True -> string.drop_end(handle, 7) <> "collection"
    False -> handle
  }
}

fn ensure_unique_collection_handle(store: Store, handle: String) -> String {
  case store.get_effective_collection_by_handle(store, handle) {
    Some(_) -> ensure_unique_collection_handle(store, handle <> "-1")
    None -> handle
  }
}

fn read_explicit_product_handle(
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

fn slugify_product_handle(title: String) -> String {
  let normalized = normalize_product_handle(title)
  case normalized {
    "" -> "untitled-product"
    _ -> normalized
  }
}

fn normalize_product_handle(value: String) -> String {
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

fn finish_handle_parts(parts_state: #(List(String), String)) -> String {
  let #(parts, current) = parts_state
  let parts = case current {
    "" -> parts
    _ -> [current, ..parts]
  }
  parts
  |> list.reverse
  |> string.join("-")
}

fn is_handle_grapheme(grapheme: String) -> Bool {
  case grapheme {
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" -> True
    "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" -> True
    "u" | "v" | "w" | "x" | "y" | "z" -> True
    _ -> False
  }
}

fn ensure_unique_product_handle(store: Store, handle: String) -> String {
  case product_handle_in_use(store, handle) {
    True -> ensure_unique_product_handle(store, handle <> "-1")
    False -> handle
  }
}

fn product_handle_in_use(store: Store, handle: String) -> Bool {
  store.list_effective_products(store)
  |> list.any(fn(product) { product.handle == handle })
}

fn make_default_option_record(
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

fn make_default_variant_record(
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
      price: None,
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
      cursor: None,
    ),
    next_identity,
    [variant_id, inventory_item_id],
  )
}

fn make_created_variant_record(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  input: Dict(String, ResolvedValue),
  defaults: Option(ProductVariantRecord),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let selected_options = read_variant_selected_options(input, [])
  let #(variant_id, identity_after_variant) =
    synthetic_identity.make_synthetic_gid(identity, "ProductVariant")
  let #(inventory_item, final_identity) = case
    read_object_field(input, "inventoryItem")
  {
    Some(inventory_item_input) ->
      read_variant_inventory_item(
        identity_after_variant,
        Some(inventory_item_input),
        None,
      )
    None ->
      clone_default_inventory_item(
        identity_after_variant,
        option.then(defaults, fn(variant) { variant.inventory_item }),
      )
  }
  #(
    ProductVariantRecord(
      id: variant_id,
      product_id: product_id,
      title: read_non_empty_string_field(input, "title")
        |> option.unwrap(variant_title_with_fallback(
          selected_options,
          "Default Title",
        )),
      sku: read_variant_sku(input, None),
      barcode: read_string_field(input, "barcode"),
      price: read_string_field(input, "price")
        |> option.or(option.then(defaults, fn(variant) { variant.price })),
      compare_at_price: read_string_field(input, "compareAtPrice")
        |> option.or(
          option.then(defaults, fn(variant) { variant.compare_at_price }),
        ),
      taxable: read_bool_field(input, "taxable")
        |> option.or(option.then(defaults, fn(variant) { variant.taxable })),
      inventory_policy: read_string_field(input, "inventoryPolicy")
        |> option.or(
          option.then(defaults, fn(variant) { variant.inventory_policy }),
        ),
      inventory_quantity: read_variant_inventory_quantity(input, Some(0)),
      selected_options: selected_options,
      inventory_item: inventory_item,
      cursor: None,
    ),
    final_identity,
  )
}

fn make_created_variant_records(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
  defaults: Option(ProductVariantRecord),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry) {
  let #(reversed, final_identity) =
    list.fold(inputs, #([], identity), fn(acc, input) {
      let #(variants, current_identity) = acc
      let #(variant, next_identity) =
        make_created_variant_record(
          current_identity,
          product_id,
          input,
          defaults,
        )
      #([variant, ..variants], next_identity)
    })
  #(list.reverse(reversed), final_identity)
}

fn update_variant_records(
  identity: SyntheticIdentityRegistry,
  variants: List(ProductVariantRecord),
  updates: List(Dict(String, ResolvedValue)),
) -> #(
  List(ProductVariantRecord),
  List(ProductVariantRecord),
  SyntheticIdentityRegistry,
) {
  let #(reversed_variants, reversed_updated, final_identity) =
    list.fold(variants, #([], [], identity), fn(acc, variant) {
      let #(next_variants, updated_variants, current_identity) = acc
      case find_variant_update(updates, variant.id) {
        Some(input) -> {
          let #(updated, next_identity) =
            update_variant_record(current_identity, variant, input)
          #(
            [updated, ..next_variants],
            [updated, ..updated_variants],
            next_identity,
          )
        }
        None -> #(
          [variant, ..next_variants],
          updated_variants,
          current_identity,
        )
      }
    })
  #(
    list.reverse(reversed_variants),
    list.reverse(reversed_updated),
    final_identity,
  )
}

fn read_product_variant_positions(
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
          let variant_id = read_arg_string(input, "id")
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

fn validate_product_variant_positions(
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

fn apply_sequential_variant_reorder(
  variants: List(ProductVariantRecord),
  positions: List(ProductVariantPositionInput),
) -> List(ProductVariantRecord) {
  list.fold(positions, variants, fn(current, position) {
    move_variant_to_position(current, position.id, position.position)
  })
}

fn move_variant_to_position(
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

fn remove_variant_by_id(
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

fn insert_variant_at_position(
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

fn find_variant_update(
  updates: List(Dict(String, ResolvedValue)),
  variant_id: String,
) -> Option(Dict(String, ResolvedValue)) {
  updates
  |> list.find(fn(input) { read_arg_string(input, "id") == Some(variant_id) })
  |> option.from_result
}

fn update_variant_record(
  identity: SyntheticIdentityRegistry,
  existing: ProductVariantRecord,
  input: Dict(String, ResolvedValue),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let selected_options =
    read_variant_selected_options(input, existing.selected_options)
  let #(inventory_item, next_identity) =
    read_variant_inventory_item(
      identity,
      read_object_field(input, "inventoryItem"),
      existing.inventory_item,
    )
  #(
    ProductVariantRecord(
      ..existing,
      title: read_non_empty_string_field(input, "title")
        |> option.unwrap(variant_title_with_fallback(
          selected_options,
          existing.title,
        )),
      sku: read_variant_sku(input, existing.sku),
      barcode: read_string_field(input, "barcode")
        |> option.or(existing.barcode),
      price: read_string_field(input, "price") |> option.or(existing.price),
      compare_at_price: read_string_field(input, "compareAtPrice")
        |> option.or(existing.compare_at_price),
      taxable: read_bool_field(input, "taxable") |> option.or(existing.taxable),
      inventory_policy: read_string_field(input, "inventoryPolicy")
        |> option.or(existing.inventory_policy),
      inventory_quantity: read_variant_inventory_quantity(
        input,
        existing.inventory_quantity,
      ),
      selected_options: selected_options,
      inventory_item: inventory_item,
    ),
    next_identity,
  )
}

fn read_variant_sku(
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

fn read_variant_selected_options(
  input: Dict(String, ResolvedValue),
  fallback: List(ProductVariantSelectedOptionRecord),
) -> List(ProductVariantSelectedOptionRecord) {
  case dict.get(input, "selectedOptions") {
    Ok(ListVal(values)) -> {
      let selected =
        list.filter_map(values, fn(value) {
          case value {
            ObjectVal(fields) -> read_variant_selected_option(fields)
            _ -> Error(Nil)
          }
        })
      case selected {
        [] -> fallback
        _ -> selected
      }
    }
    _ -> read_variant_option_values(input, fallback)
  }
}

fn read_variant_option_values(
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

fn read_variant_option_value(
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

fn read_variant_selected_option(
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

fn read_variant_inventory_quantity(
  input: Dict(String, ResolvedValue),
  fallback: Option(Int),
) -> Option(Int) {
  case read_int_field(input, "inventoryQuantity") {
    Some(quantity) -> Some(quantity)
    None ->
      read_inventory_quantities_available_total(input) |> option.or(fallback)
  }
}

fn read_inventory_quantities_available_total(
  input: Dict(String, ResolvedValue),
) -> Option(Int) {
  case dict.get(input, "inventoryQuantities") {
    Ok(ListVal(values)) -> {
      let quantities =
        list.filter_map(values, fn(value) {
          case value {
            ObjectVal(fields) ->
              case read_int_field(fields, "availableQuantity") {
                Some(quantity) -> Ok(quantity)
                None -> Error(Nil)
              }
            _ -> Error(Nil)
          }
        })
      case quantities {
        [] -> None
        _ ->
          Some(
            list.fold(quantities, 0, fn(total, quantity) { total + quantity }),
          )
      }
    }
    _ -> None
  }
}

fn read_variant_inventory_item(
  identity: SyntheticIdentityRegistry,
  input: Option(Dict(String, ResolvedValue)),
  existing: Option(InventoryItemRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry) {
  case input {
    None -> #(existing, identity)
    Some(input) -> {
      let #(id, next_identity) = case existing {
        Some(item) -> #(item.id, identity)
        None -> synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      }
      let current_levels = case existing {
        Some(item) -> item.inventory_levels
        None -> []
      }
      #(
        Some(InventoryItemRecord(
          id: id,
          tracked: read_bool_field(input, "tracked")
            |> option.or(option.then(existing, fn(item) { item.tracked })),
          requires_shipping: read_bool_field(input, "requiresShipping")
            |> option.or(
              option.then(existing, fn(item) { item.requires_shipping }),
            ),
          measurement: read_inventory_measurement_input(
            input,
            option.then(existing, fn(item) { item.measurement }),
          ),
          country_code_of_origin: read_string_field(
            input,
            "countryCodeOfOrigin",
          )
            |> option.or(
              option.then(existing, fn(item) { item.country_code_of_origin }),
            ),
          province_code_of_origin: read_string_field(
            input,
            "provinceCodeOfOrigin",
          )
            |> option.or(
              option.then(existing, fn(item) { item.province_code_of_origin }),
            ),
          harmonized_system_code: read_string_field(
            input,
            "harmonizedSystemCode",
          )
            |> option.or(
              option.then(existing, fn(item) { item.harmonized_system_code }),
            ),
          inventory_levels: current_levels,
        )),
        next_identity,
      )
    }
  }
}

fn read_inventory_measurement_input(
  input: Dict(String, ResolvedValue),
  fallback: Option(InventoryMeasurementRecord),
) -> Option(InventoryMeasurementRecord) {
  case read_object_field(input, "measurement") {
    Some(measurement) ->
      Some(
        InventoryMeasurementRecord(weight: read_inventory_weight_input(
          measurement,
          option.then(fallback, fn(measurement) { measurement.weight }),
        )),
      )
    None -> fallback
  }
}

fn read_inventory_weight_input(
  input: Dict(String, ResolvedValue),
  fallback: Option(InventoryWeightRecord),
) -> Option(InventoryWeightRecord) {
  case read_object_field(input, "weight") {
    Some(weight) ->
      case
        read_string_field(weight, "unit"),
        read_inventory_weight_value_input(weight)
      {
        Some(unit), Some(value) ->
          Some(InventoryWeightRecord(unit: unit, value: value))
        _, _ -> fallback
      }
    None -> fallback
  }
}

fn read_inventory_weight_value_input(
  input: Dict(String, ResolvedValue),
) -> Option(InventoryWeightValue) {
  case dict.get(input, "value") {
    Ok(IntVal(value)) -> Some(InventoryWeightInt(value))
    Ok(FloatVal(value)) -> Some(InventoryWeightFloat(value))
    _ -> None
  }
}

fn clone_default_inventory_item(
  identity: SyntheticIdentityRegistry,
  item: Option(InventoryItemRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry) {
  case item {
    None -> #(None, identity)
    Some(item) -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(Some(InventoryItemRecord(..item, id: id)), next_identity)
    }
  }
}

fn apply_inventory_adjust_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  name: String,
  reason: String,
  changes: List(InventoryAdjustmentChangeInput),
) -> Result(
  #(Store, SyntheticIdentityRegistry, InventoryAdjustmentGroup, List(String)),
  List(ProductUserError),
) {
  let reference_document_uri = read_string_field(input, "referenceDocumentUri")
  let result =
    changes
    |> enumerate_items()
    |> list.try_fold(#([], [], store), fn(acc, pair) {
      let #(change, index) = pair
      let #(adjusted_changes, mirrored_changes, current_store) = acc
      case stage_inventory_quantity_adjust(current_store, name, change, index) {
        Error(error) -> Error([error])
        Ok(applied) -> {
          let #(next_store, adjusted_change, mirrored) = applied
          Ok(#(
            list.append(adjusted_changes, [adjusted_change]),
            list.append(mirrored_changes, mirrored),
            next_store,
          ))
        }
      }
    })
  case result {
    Error(errors) -> Error(errors)
    Ok(done) -> {
      let #(adjusted_changes, mirrored_changes, next_store) = done
      let #(group, next_identity) =
        make_inventory_adjustment_group(
          identity,
          reason,
          reference_document_uri,
          list.append(adjusted_changes, mirrored_changes),
        )
      Ok(#(
        next_store,
        next_identity,
        group,
        inventory_adjustment_staged_ids(group),
      ))
    }
  }
}

fn apply_inventory_set_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  name: String,
  reason: String,
  quantities: List(InventorySetQuantityInput),
  ignore_compare_quantity: Bool,
) -> Result(
  #(Store, SyntheticIdentityRegistry, InventoryAdjustmentGroup, List(String)),
  List(ProductUserError),
) {
  let reference_document_uri = read_string_field(input, "referenceDocumentUri")
  let initial = #([], [], store)
  let result =
    quantities
    |> enumerate_items()
    |> list.try_fold(initial, fn(acc, pair) {
      let #(quantity, index) = pair
      let #(changes, mirrored_changes, current_store) = acc
      case validate_inventory_set_quantity(quantity, index) {
        Some(error) -> Error([error])
        None -> {
          let assert Some(inventory_item_id) = quantity.inventory_item_id
          let assert Some(location_id) = quantity.location_id
          let assert Some(next_quantity) = quantity.quantity
          case
            stage_inventory_quantity_set(
              current_store,
              inventory_item_id,
              location_id,
              name,
              next_quantity,
              ignore_compare_quantity,
              quantity.compare_quantity,
              index,
            )
          {
            Error(error) -> Error([error])
            Ok(applied) -> {
              let #(next_store, delta) = applied
              let change =
                InventoryAdjustmentChange(
                  inventory_item_id: inventory_item_id,
                  location_id: location_id,
                  name: name,
                  delta: delta,
                  quantity_after_change: None,
                  ledger_document_uri: None,
                )
              let mirrored = case is_on_hand_component_quantity_name(name) {
                True -> [
                  InventoryAdjustmentChange(
                    inventory_item_id: inventory_item_id,
                    location_id: location_id,
                    name: "on_hand",
                    delta: delta,
                    quantity_after_change: None,
                    ledger_document_uri: None,
                  ),
                ]
                False -> []
              }
              Ok(#(
                list.append(changes, [change]),
                list.append(mirrored_changes, mirrored),
                next_store,
              ))
            }
          }
        }
      }
    })
  case result {
    Error(errors) -> Error(errors)
    Ok(done) -> {
      let #(changes, mirrored_changes, next_store) = done
      let #(group, next_identity) =
        make_inventory_adjustment_group(
          identity,
          reason,
          reference_document_uri,
          list.append(changes, mirrored_changes),
        )
      Ok(#(
        next_store,
        next_identity,
        group,
        inventory_adjustment_staged_ids(group),
      ))
    }
  }
}

fn validate_inventory_adjust_inputs(
  name: String,
  changes: List(InventoryAdjustmentChangeInput),
) -> List(ProductUserError) {
  let name_errors = case valid_inventory_adjust_quantity_name(name) {
    True -> []
    False -> [invalid_inventory_quantity_name_error(["input", "name"])]
  }
  let ledger_errors = case name {
    "available" -> []
    _ ->
      changes
      |> enumerate_items()
      |> list.filter_map(fn(pair) {
        let #(change, index) = pair
        case change.ledger_document_uri {
          Some(_) -> Error(Nil)
          None ->
            Ok(ProductUserError(
              ["input", "changes", int.to_string(index), "ledgerDocumentUri"],
              "A ledger document URI is required except when adjusting available.",
              None,
            ))
        }
      })
  }
  list.append(name_errors, ledger_errors)
}

fn stage_inventory_quantity_adjust(
  store: Store,
  name: String,
  change: InventoryAdjustmentChangeInput,
  index: Int,
) -> Result(
  #(Store, InventoryAdjustmentChange, List(InventoryAdjustmentChange)),
  ProductUserError,
) {
  let path = ["input", "changes", int.to_string(index)]
  case change.inventory_item_id, change.location_id, change.delta {
    None, _, _ ->
      Error(ProductUserError(
        list.append(path, ["inventoryItemId"]),
        "Inventory item id is required",
        None,
      ))
    _, None, _ ->
      Error(ProductUserError(
        list.append(path, ["locationId"]),
        "Inventory location id is required",
        None,
      ))
    _, _, None ->
      Error(ProductUserError(
        list.append(path, ["delta"]),
        "Inventory delta is required",
        None,
      ))
    Some(inventory_item_id), Some(location_id), Some(delta) -> {
      case
        store.find_effective_variant_by_inventory_item_id(
          store,
          inventory_item_id,
        )
      {
        None ->
          Error(ProductUserError(
            list.append(path, ["inventoryItemId"]),
            "The specified inventory item could not be found.",
            None,
          ))
        Some(variant) -> {
          let current_levels = variant_inventory_levels(variant)
          case find_inventory_level(current_levels, location_id) {
            None ->
              Error(ProductUserError(
                list.append(path, ["locationId"]),
                "The specified location could not be found.",
                None,
              ))
            Some(level) -> {
              let quantities =
                level.quantities
                |> add_inventory_quantity_amount(name, delta)
                |> maybe_add_on_hand_component_delta(name, delta)
              let next_store =
                stage_variant_inventory_levels(
                  store,
                  variant,
                  replace_inventory_level(
                    current_levels,
                    location_id,
                    InventoryLevelRecord(..level, quantities: quantities),
                  ),
                )
              let adjusted =
                InventoryAdjustmentChange(
                  inventory_item_id: inventory_item_id,
                  location_id: location_id,
                  name: name,
                  delta: delta,
                  quantity_after_change: None,
                  ledger_document_uri: change.ledger_document_uri,
                )
              let mirrored = case is_on_hand_component_quantity_name(name) {
                True -> [
                  InventoryAdjustmentChange(
                    inventory_item_id: inventory_item_id,
                    location_id: location_id,
                    name: "on_hand",
                    delta: delta,
                    quantity_after_change: None,
                    ledger_document_uri: None,
                  ),
                ]
                False -> []
              }
              Ok(#(next_store, adjusted, mirrored))
            }
          }
        }
      }
    }
  }
}

fn validate_inventory_set_quantity(
  quantity: InventorySetQuantityInput,
  index: Int,
) -> Option(ProductUserError) {
  let path = ["input", "quantities", int.to_string(index)]
  case quantity.inventory_item_id, quantity.location_id, quantity.quantity {
    None, _, _ ->
      Some(ProductUserError(
        list.append(path, ["inventoryItemId"]),
        "Inventory item id is required",
        None,
      ))
    _, None, _ ->
      Some(ProductUserError(
        list.append(path, ["locationId"]),
        "Inventory location id is required",
        None,
      ))
    _, _, None ->
      Some(ProductUserError(
        list.append(path, ["quantity"]),
        "Inventory quantity is required",
        None,
      ))
    _, _, _ -> None
  }
}

fn stage_inventory_quantity_set(
  store: Store,
  inventory_item_id: String,
  location_id: String,
  name: String,
  next_quantity: Int,
  ignore_compare_quantity: Bool,
  compare_quantity: Option(Int),
  index: Int,
) -> Result(#(Store, Int), ProductUserError) {
  case
    store.find_effective_variant_by_inventory_item_id(store, inventory_item_id)
  {
    None ->
      Error(ProductUserError(
        ["input", "quantities", int.to_string(index), "inventoryItemId"],
        "The specified inventory item could not be found.",
        None,
      ))
    Some(variant) -> {
      let current_levels = variant_inventory_levels(variant)
      case find_inventory_level(current_levels, location_id) {
        None ->
          Error(ProductUserError(
            ["input", "quantities", int.to_string(index), "locationId"],
            "The specified location could not be found.",
            None,
          ))
        Some(level) -> {
          let previous = inventory_quantity_amount(level.quantities, name)
          case !ignore_compare_quantity && compare_quantity != Some(previous) {
            True ->
              Error(ProductUserError(
                ["input", "quantities", int.to_string(index), "compareQuantity"],
                "The specified compare quantity does not match the current quantity.",
                None,
              ))
            False -> {
              let delta = next_quantity - previous
              let quantities =
                write_inventory_quantity_amount(
                  level.quantities,
                  name,
                  next_quantity,
                )
                |> maybe_add_on_hand_component_delta(name, delta)
              let next_store =
                stage_variant_inventory_levels(
                  store,
                  variant,
                  replace_inventory_level(
                    current_levels,
                    location_id,
                    InventoryLevelRecord(..level, quantities: quantities),
                  ),
                )
              Ok(#(next_store, delta))
            }
          }
        }
      }
    }
  }
}

fn apply_inventory_move_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  reason: String,
  changes: List(InventoryMoveQuantityInput),
) -> Result(
  #(Store, SyntheticIdentityRegistry, InventoryAdjustmentGroup, List(String)),
  List(ProductUserError),
) {
  let reference_document_uri = read_string_field(input, "referenceDocumentUri")
  let result =
    changes
    |> enumerate_items()
    |> list.try_fold(#([], store), fn(acc, pair) {
      let #(change, index) = pair
      let #(adjustment_changes, current_store) = acc
      case stage_inventory_quantity_move(current_store, change, index) {
        Error(error) -> Error([error])
        Ok(applied) -> {
          let #(next_store, from_change, to_change) = applied
          Ok(#(
            list.append(adjustment_changes, [from_change, to_change]),
            next_store,
          ))
        }
      }
    })
  case result {
    Error(errors) -> Error(errors)
    Ok(done) -> {
      let #(adjustment_changes, next_store) = done
      let #(group, next_identity) =
        make_inventory_adjustment_group(
          identity,
          reason,
          reference_document_uri,
          adjustment_changes,
        )
      Ok(#(
        next_store,
        next_identity,
        group,
        inventory_adjustment_staged_ids(group),
      ))
    }
  }
}

fn validate_inventory_move_inputs(
  changes: List(InventoryMoveQuantityInput),
) -> List(ProductUserError) {
  changes
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(change, index) = pair
    validate_inventory_move_input(change, index)
  })
}

fn validate_inventory_move_input(
  change: InventoryMoveQuantityInput,
  index: Int,
) -> List(ProductUserError) {
  let path = ["input", "changes", int.to_string(index)]
  let name_errors =
    list.filter_map(
      [
        #(change.from.name, list.append(path, ["from", "name"])),
        #(change.to.name, list.append(path, ["to", "name"])),
      ],
      fn(candidate) {
        let #(name, field_path) = candidate
        case name {
          Some(name) ->
            case valid_staged_inventory_quantity_name(name) {
              True -> Error(Nil)
              False -> Ok(invalid_inventory_quantity_name_error(field_path))
            }
          None -> Error(Nil)
        }
      },
    )
  let location_error = case change.from.location_id, change.to.location_id {
    Some(from), Some(to) if from != to -> [
      ProductUserError(
        path,
        "The quantities can't be moved between different locations.",
        None,
      ),
    ]
    _, _ -> []
  }
  let same_name_error = case change.from.name, change.to.name {
    Some(from), Some(to) if from == to -> [
      ProductUserError(
        path,
        "The quantity names for each change can't be the same.",
        None,
      ),
    ]
    _, _ -> []
  }
  let ledger_errors =
    list.append(
      validate_inventory_move_ledger_document_uri(
        change.from.name,
        change.from.ledger_document_uri,
        list.append(path, ["from", "ledgerDocumentUri"]),
      ),
      validate_inventory_move_ledger_document_uri(
        change.to.name,
        change.to.ledger_document_uri,
        list.append(path, ["to", "ledgerDocumentUri"]),
      ),
    )
  list.append(
    name_errors,
    list.append(location_error, list.append(same_name_error, ledger_errors)),
  )
}

fn validate_inventory_move_ledger_document_uri(
  quantity_name: Option(String),
  ledger_document_uri: Option(String),
  path: List(String),
) -> List(ProductUserError) {
  case quantity_name, ledger_document_uri {
    Some("available"), Some(_) -> [
      ProductUserError(
        path,
        "A ledger document URI is not allowed when adjusting available.",
        None,
      ),
    ]
    Some(name), None if name != "available" -> [
      ProductUserError(
        path,
        "A ledger document URI is required except when adjusting available.",
        None,
      ),
    ]
    _, _ -> []
  }
}

fn stage_inventory_quantity_move(
  store: Store,
  change: InventoryMoveQuantityInput,
  index: Int,
) -> Result(
  #(Store, InventoryAdjustmentChange, InventoryAdjustmentChange),
  ProductUserError,
) {
  let path = ["input", "changes", int.to_string(index)]
  case
    change.inventory_item_id,
    change.quantity,
    change.from.location_id,
    change.from.name,
    change.to.location_id,
    change.to.name
  {
    None, _, _, _, _, _ ->
      Error(ProductUserError(
        list.append(path, ["inventoryItemId"]),
        "Inventory item id is required",
        None,
      ))
    _, None, _, _, _, _ ->
      Error(ProductUserError(
        list.append(path, ["quantity"]),
        "Inventory move quantity is required",
        None,
      ))
    _, _, None, _, _, _ ->
      Error(ProductUserError(
        path,
        "Inventory move terminals are required",
        None,
      ))
    _, _, _, None, _, _ ->
      Error(ProductUserError(
        path,
        "Inventory move terminals are required",
        None,
      ))
    _, _, _, _, None, _ ->
      Error(ProductUserError(
        path,
        "Inventory move terminals are required",
        None,
      ))
    _, _, _, _, _, None ->
      Error(ProductUserError(
        path,
        "Inventory move terminals are required",
        None,
      ))
    Some(inventory_item_id),
      Some(quantity),
      Some(location_id),
      Some(from_name),
      _,
      Some(to_name)
    -> {
      case
        store.find_effective_variant_by_inventory_item_id(
          store,
          inventory_item_id,
        )
      {
        None ->
          Error(ProductUserError(
            list.append(path, ["inventoryItemId"]),
            "The specified inventory item could not be found.",
            None,
          ))
        Some(variant) -> {
          let current_levels = variant_inventory_levels(variant)
          case find_inventory_level(current_levels, location_id) {
            None ->
              Error(ProductUserError(
                list.append(path, ["from", "locationId"]),
                "The specified inventory item is not stocked at the location.",
                None,
              ))
            Some(level) -> {
              let quantities =
                level.quantities
                |> add_inventory_quantity_amount(from_name, 0 - quantity)
                |> add_inventory_quantity_amount(to_name, quantity)
                |> add_on_hand_move_delta(from_name, to_name, quantity)
              let next_store =
                stage_variant_inventory_levels(
                  store,
                  variant,
                  replace_inventory_level(
                    current_levels,
                    location_id,
                    InventoryLevelRecord(..level, quantities: quantities),
                  ),
                )
              Ok(#(
                next_store,
                InventoryAdjustmentChange(
                  inventory_item_id: inventory_item_id,
                  location_id: location_id,
                  name: from_name,
                  delta: 0 - quantity,
                  quantity_after_change: None,
                  ledger_document_uri: change.from.ledger_document_uri,
                ),
                InventoryAdjustmentChange(
                  inventory_item_id: inventory_item_id,
                  location_id: location_id,
                  name: to_name,
                  delta: quantity,
                  quantity_after_change: None,
                  ledger_document_uri: change.to.ledger_document_uri,
                ),
              ))
            }
          }
        }
      }
    }
  }
}

fn make_inventory_adjustment_group(
  identity: SyntheticIdentityRegistry,
  reason: String,
  reference_document_uri: Option(String),
  changes: List(InventoryAdjustmentChange),
) -> #(InventoryAdjustmentGroup, SyntheticIdentityRegistry) {
  let #(id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "InventoryAdjustmentGroup")
  let #(created_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  #(
    InventoryAdjustmentGroup(
      id: id,
      created_at: created_at,
      reason: reason,
      reference_document_uri: reference_document_uri,
      changes: changes,
    ),
    next_identity,
  )
}

fn inventory_adjustment_staged_ids(
  group: InventoryAdjustmentGroup,
) -> List(String) {
  [
    group.id,
    ..dedupe_preserving_order(
      list.map(group.changes, fn(change) { change.inventory_item_id }),
    )
  ]
}

fn variant_inventory_levels(
  variant: ProductVariantRecord,
) -> List(InventoryLevelRecord) {
  case variant.inventory_item {
    Some(item) -> item.inventory_levels
    None -> []
  }
}

fn find_inventory_level(
  levels: List(InventoryLevelRecord),
  location_id: String,
) -> Option(InventoryLevelRecord) {
  levels
  |> list.find(fn(level) { level.location.id == location_id })
  |> option.from_result
}

fn find_inventory_level_target(
  store: Store,
  inventory_level_id: String,
) -> Option(#(ProductVariantRecord, InventoryLevelRecord)) {
  store.list_effective_product_variants(store)
  |> list.filter_map(fn(variant) {
    case
      list.find(variant_inventory_levels(variant), fn(level) {
        level.id == inventory_level_id
      })
    {
      Ok(level) -> Ok(#(variant, level))
      Error(_) -> Error(Nil)
    }
  })
  |> list.first
  |> option.from_result
}

fn replace_inventory_level(
  levels: List(InventoryLevelRecord),
  location_id: String,
  next_level: InventoryLevelRecord,
) -> List(InventoryLevelRecord) {
  list.map(levels, fn(level) {
    case level.location.id == location_id {
      True -> next_level
      False -> level
    }
  })
}

fn stage_variant_inventory_levels(
  store: Store,
  variant: ProductVariantRecord,
  next_levels: List(InventoryLevelRecord),
) -> Store {
  let next_variant =
    ProductVariantRecord(
      ..variant,
      inventory_quantity: sum_inventory_level_available(next_levels),
      inventory_item: option.map(variant.inventory_item, fn(item) {
        InventoryItemRecord(..item, inventory_levels: next_levels)
      }),
    )
  let next_variants =
    store.get_effective_variants_by_product_id(store, variant.product_id)
    |> list.map(fn(candidate) {
      case candidate.id == variant.id {
        True -> next_variant
        False -> candidate
      }
    })
  store.replace_staged_variants_for_product(
    store,
    variant.product_id,
    next_variants,
  )
}

fn sum_inventory_level_available(
  levels: List(InventoryLevelRecord),
) -> Option(Int) {
  Some(
    list.fold(levels, 0, fn(total, level) {
      total + inventory_quantity_amount(level.quantities, "available")
    }),
  )
}

fn inventory_quantity_amount(
  quantities: List(InventoryQuantityRecord),
  name: String,
) -> Int {
  case list.find(quantities, fn(quantity) { quantity.name == name }) {
    Ok(quantity) -> quantity.quantity
    Error(_) -> 0
  }
}

fn write_inventory_quantity_amount(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True ->
      list.map(quantities, fn(quantity) {
        case quantity.name == name {
          True -> InventoryQuantityRecord(..quantity, quantity: amount)
          False -> quantity
        }
      })
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(name: name, quantity: amount, updated_at: None),
      ])
  }
}

fn add_inventory_quantity_amount(
  quantities: List(InventoryQuantityRecord),
  name: String,
  delta: Int,
) -> List(InventoryQuantityRecord) {
  write_inventory_quantity_amount(
    quantities,
    name,
    inventory_quantity_amount(quantities, name) + delta,
  )
}

fn maybe_add_on_hand_component_delta(
  quantities: List(InventoryQuantityRecord),
  name: String,
  delta: Int,
) -> List(InventoryQuantityRecord) {
  case is_on_hand_component_quantity_name(name) {
    True -> add_inventory_quantity_amount(quantities, "on_hand", delta)
    False -> quantities
  }
}

fn add_on_hand_move_delta(
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

fn on_hand_component_delta(name: String, delta: Int) -> Int {
  case is_on_hand_component_quantity_name(name) {
    True -> delta
    False -> 0
  }
}

fn is_on_hand_component_quantity_name(name: String) -> Bool {
  case name {
    "available"
    | "committed"
    | "damaged"
    | "quality_control"
    | "reserved"
    | "safety_stock" -> True
    _ -> False
  }
}

fn valid_staged_inventory_quantity_name(name: String) -> Bool {
  case name {
    "available"
    | "committed"
    | "damaged"
    | "incoming"
    | "quality_control"
    | "reserved"
    | "safety_stock" -> True
    _ -> False
  }
}

fn valid_inventory_adjust_quantity_name(name: String) -> Bool {
  case name {
    "available"
    | "damaged"
    | "incoming"
    | "quality_control"
    | "reserved"
    | "safety_stock" -> True
    _ -> False
  }
}

fn invalid_inventory_quantity_name_error(
  field: List(String),
) -> ProductUserError {
  ProductUserError(
    field,
    "The specified quantity name is invalid. Valid values are: available, damaged, incoming, quality_control, reserved, safety_stock.",
    None,
  )
}

fn inventory_change_location(
  store: Store,
  change: InventoryAdjustmentChange,
) -> InventoryLocationRecord {
  case
    store.find_effective_variant_by_inventory_item_id(
      store,
      change.inventory_item_id,
    )
  {
    Some(variant) ->
      case
        find_inventory_level(
          variant_inventory_levels(variant),
          change.location_id,
        )
      {
        Some(level) -> level.location
        None -> InventoryLocationRecord(id: change.location_id, name: "")
      }
    None -> InventoryLocationRecord(id: change.location_id, name: "")
  }
}

fn read_object_field(
  fields: Dict(String, ResolvedValue),
  name: String,
) -> Option(Dict(String, ResolvedValue)) {
  case dict.get(fields, name) {
    Ok(ObjectVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_bool_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(Bool) {
  case dict.get(input, name) {
    Ok(BoolVal(value)) -> Some(value)
    _ -> None
  }
}

fn variant_title_with_fallback(
  selected_options: List(ProductVariantSelectedOptionRecord),
  fallback: String,
) -> String {
  case selected_options {
    [] -> fallback
    _ -> variant_title(selected_options)
  }
}

fn variant_staged_ids(variant: ProductVariantRecord) -> List(String) {
  case variant.inventory_item {
    Some(item) -> [variant.id, item.id]
    None -> [variant.id]
  }
}

fn sync_product_inventory_summary(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
) -> #(Option(ProductRecord), Store, SyntheticIdentityRegistry) {
  case store.get_effective_product_by_id(store, product_id) {
    None -> #(None, store, identity)
    Some(product) -> {
      let #(updated_at, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let variants =
        store.get_effective_variants_by_product_id(store, product_id)
      let next_product =
        ProductRecord(
          ..product,
          total_inventory: sum_variant_inventory(variants),
          tracks_inventory: derive_tracks_inventory(variants),
          updated_at: Some(updated_at),
        )
      let #(_, next_store) = store.upsert_staged_product(store, next_product)
      #(Some(next_product), next_store, next_identity)
    }
  }
}

fn sum_variant_inventory(variants: List(ProductVariantRecord)) -> Option(Int) {
  let quantities =
    list.filter_map(variants, fn(variant) {
      case variant.inventory_quantity {
        Some(quantity) -> Ok(quantity)
        None -> Error(Nil)
      }
    })
  case quantities {
    [] -> None
    _ ->
      Some(list.fold(quantities, 0, fn(total, quantity) { total + quantity }))
  }
}

fn derive_tracks_inventory(
  variants: List(ProductVariantRecord),
) -> Option(Bool) {
  let tracked_values =
    list.filter_map(variants, fn(variant) {
      case variant.inventory_item {
        Some(item) ->
          case item.tracked {
            Some(tracked) -> Ok(tracked)
            None -> Error(Nil)
          }
        None -> Error(Nil)
      }
    })
  case tracked_values {
    [] ->
      case
        list.any(variants, fn(variant) { variant.inventory_quantity != None })
      {
        True -> Some(True)
        False -> None
      }
    _ -> Some(list.any(tracked_values, fn(tracked) { tracked }))
  }
}

fn read_non_empty_string_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(String) {
  case read_string_field(input, name) {
    Some(value) ->
      case string.length(string.trim(value)) > 0 {
        True -> Some(value)
        False -> None
      }
    None -> None
  }
}

fn read_product_status_field(
  input: Dict(String, ResolvedValue),
) -> Option(String) {
  case read_string_field(input, "status") {
    Some("ACTIVE") -> Some("ACTIVE")
    Some("ARCHIVED") -> Some("ARCHIVED")
    Some("DRAFT") -> Some("DRAFT")
    _ -> None
  }
}

fn read_string_list_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(List(String)) {
  case dict.get(input, name) {
    Ok(ListVal(values)) ->
      Some(
        list.filter_map(values, fn(value) {
          case value {
            StringVal(item) -> Ok(item)
            _ -> Error(Nil)
          }
        }),
      )
    _ -> None
  }
}

fn updated_product_seo(
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

fn find_product_option(
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

type RenamedOptionValue =
  #(String, String)

fn update_product_option_record(
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

fn update_option_values(
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

fn find_option_value_update(
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

fn read_option_value_create_names(
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

fn insert_option_at_position(
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

fn remap_variant_selections_for_option_update(
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

fn renamed_value_name(
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

fn reorder_variant_selections_for_options(
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

fn find_selected_option(
  selected_options: List(ProductVariantSelectedOptionRecord),
  name: String,
) -> Result(ProductVariantSelectedOptionRecord, Nil) {
  selected_options
  |> list.find(fn(selected) { selected.name == name })
}

fn unknown_option_errors(
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
          None,
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

fn reorder_product_options(
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

fn take_matching_option(
  options: List(ProductOptionRecord),
  input: Dict(String, ResolvedValue),
) -> #(Option(ProductOptionRecord), List(ProductOptionRecord)) {
  let option_id = read_string_field(input, "id")
  let option_name = read_string_field(input, "name")
  take_matching_option_loop(options, option_id, option_name, [])
}

fn take_matching_option_loop(
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

fn restore_default_option_state(
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

fn product_has_standalone_default_variant(
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

fn make_options_from_variant_selections(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  variants: List(ProductVariantRecord),
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry) {
  let #(options, next_identity) =
    upsert_variant_selections_into_options(identity, product_id, [], variants)
  #(sync_product_options_with_variants(options, variants), next_identity)
}

fn upsert_variant_selections_into_options(
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

fn upsert_option_selection(
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

fn upsert_option_selection_loop(
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

fn upsert_option_value(
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
