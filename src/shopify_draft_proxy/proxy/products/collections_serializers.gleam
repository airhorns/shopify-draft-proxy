//// Products-domain submodule: collections_serializers.
//// Combines layered files: collections_l12, collections_l13, collections_l14, collections_l15, collections_l16.

import gleam/dict.{type Dict}

import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, ConnectionPageInfoOptions, SerializeConnectionConfig,
  default_connection_window_options, default_selected_field_options,
  get_field_response_key, get_selected_child_fields, paginate_connection_items,
  project_graphql_value, serialize_connection, serialize_empty_connection,
}

import shopify_draft_proxy/proxy/products/collections_core.{
  collection_by_identifier, collection_create_validation_errors,
  collection_cursor_for_field, collection_has_product, collection_product_cursor,
  collection_source_with_store, collection_source_with_store_and_publication,
  created_collection_record, filtered_collections, read_collection_product_ids,
  serialize_collection_image, serialize_collection_rule_set, sort_collections,
  stage_collection_product_memberships,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type MutationFieldResult, type ProductUserError, AppendProducts,
  ProductUserError,
}
import shopify_draft_proxy/proxy/products/products_core.{
  product_seo_source, serialize_product_metafield,
  serialize_product_metafields_connection,
}
import shopify_draft_proxy/proxy/products/products_handlers.{
  product_source_with_store,
}
import shopify_draft_proxy/proxy/products/publications_core.{
  merge_publication_targets, remove_publication_targets, selected_publication_id,
}
import shopify_draft_proxy/proxy/products/publications_publishable.{
  publishable_mutation_payload,
}
import shopify_draft_proxy/proxy/products/shared.{
  legacy_resource_id_from_gid, mutation_result, read_identifier_argument,
  read_string_argument, serialize_exact_count, user_errors_source,
}
import shopify_draft_proxy/proxy/products/variants_helpers.{optional_string_json}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{type CollectionRecord, CollectionRecord}

// ===== from collections_l12 =====
@internal
pub fn serialize_collection_products_connection(
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

@internal
pub fn publishable_collection_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  collection: CollectionRecord,
  publication_targets: List(String),
  is_publish: Bool,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> MutationFieldResult {
  case publication_targets {
    [] ->
      mutation_result(
        key,
        publishable_mutation_payload(
          store,
          Some(collection_source_with_store(store, collection)),
          [ProductUserError(["input"], "Publication target is required", None)],
          field,
          variables,
          fragments,
        ),
        store,
        identity,
        [],
      )
    _ -> {
      let next_publication_ids = case is_publish {
        True ->
          merge_publication_targets(
            collection.publication_ids,
            publication_targets,
          )
        False ->
          remove_publication_targets(
            collection.publication_ids,
            publication_targets,
          )
      }
      let next_collection =
        CollectionRecord(..collection, publication_ids: next_publication_ids)
      let next_store = store.upsert_staged_collections(store, [next_collection])
      mutation_result(
        key,
        publishable_mutation_payload(
          next_store,
          Some(collection_source_with_store_and_publication(
            next_store,
            next_collection,
            selected_publication_id(
              get_selected_child_fields(field, default_selected_field_options()),
              variables,
            ),
          )),
          [],
          field,
          variables,
          fragments,
        ),
        next_store,
        identity,
        [next_collection.id],
      )
    }
  }
}

// ===== from collections_l13 =====
@internal
pub fn serialize_collection_field(
  store: Store,
  collection: CollectionRecord,
  field: Selection,
  field_name: String,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
  products_count_override: Option(Int),
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
    "publishedOnCurrentPublication" | "publishedOnCurrentChannel" ->
      json.bool(collection.publication_ids != [])
    "publishedOnPublication" ->
      json.bool(case read_string_argument(field, variables, "publicationId") {
        Some(id) -> list.contains(collection.publication_ids, id)
        None -> False
      })
    "availablePublicationsCount"
    | "resourcePublicationsCount"
    | "publicationCount" ->
      serialize_exact_count(field, list.length(collection.publication_ids))
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
        products_count_override
          |> option.unwrap(
            collection.products_count
            |> option.unwrap(
              list.length(store.list_effective_products_for_collection(
                store,
                collection.id,
              )),
            ),
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
    "metafield" ->
      serialize_product_metafield(store, collection.id, field, variables)
    "metafields" ->
      serialize_product_metafields_connection(
        store,
        collection.id,
        field,
        variables,
      )
    _ -> json.null()
  }
}

// ===== from collections_l14 =====
@internal
pub fn serialize_collection_object_with_options(
  store: Store,
  collection: CollectionRecord,
  selections: List(Selection),
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
  products_count_override: Option(Int),
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
            products_count_override,
          )
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

// ===== from collections_l15 =====
@internal
pub fn serialize_collection_object(
  store: Store,
  collection: CollectionRecord,
  selections: List(Selection),
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  serialize_collection_object_with_options(
    store,
    collection,
    selections,
    variables,
    fragments,
    None,
  )
}

@internal
pub fn collection_create_payload(
  store: Store,
  collection: Option(CollectionRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
  products_count_override: Option(Int),
) -> Json {
  json.object(
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "__typename" -> json.string("CollectionCreatePayload")
              "collection" ->
                case collection {
                  Some(record) ->
                    serialize_collection_object_with_options(
                      store,
                      record,
                      get_selected_child_fields(
                        selection,
                        default_selected_field_options(),
                      ),
                      variables,
                      fragments,
                      products_count_override,
                    )
                  None -> json.null()
                }
              "userErrors" ->
                project_graphql_value(
                  user_errors_source(user_errors),
                  get_selected_child_fields(
                    selection,
                    default_selected_field_options(),
                  ),
                  fragments,
                )
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      },
    ),
  )
}

// ===== from collections_l16 =====
@internal
pub fn serialize_collection_root(
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

@internal
pub fn serialize_collection_by_identifier_root(
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

@internal
pub fn serialize_collection_by_handle_root(
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

@internal
pub fn serialize_collections_connection(
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

@internal
pub fn handle_collection_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case collection_create_validation_errors(input) {
    [_, ..] as user_errors ->
      mutation_result(
        key,
        collection_create_payload(
          store,
          None,
          user_errors,
          field,
          fragments,
          variables,
          None,
        ),
        store,
        identity,
        [],
      )
    [] -> {
      let #(collection, next_identity) =
        created_collection_record(store, identity, input)
      let product_ids = read_collection_product_ids(input)
      let result = case product_ids {
        [] -> #(store, Some(collection), [])
        _ ->
          stage_collection_product_memberships(
            store,
            collection,
            product_ids,
            AppendProducts,
          )
      }
      let #(membership_store, result_collection, user_errors) = result
      case user_errors {
        [] -> {
          let staged_collection =
            result_collection
            |> option.unwrap(collection)
          let next_store =
            store.upsert_staged_collections(membership_store, [
              staged_collection,
            ])
          mutation_result(
            key,
            collection_create_payload(
              next_store,
              Some(staged_collection),
              [],
              field,
              fragments,
              variables,
              Some(0),
            ),
            next_store,
            next_identity,
            [staged_collection.id],
          )
        }
        _ ->
          mutation_result(
            key,
            collection_create_payload(
              store,
              None,
              user_errors,
              field,
              fragments,
              variables,
              None,
            ),
            store,
            next_identity,
            [],
          )
      }
    }
  }
}
