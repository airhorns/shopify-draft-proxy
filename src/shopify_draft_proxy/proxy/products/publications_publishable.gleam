//// Products-domain submodule: publications_publishable.
//// Combines layered files: publications_l06, publications_l07, publications_l08, publications_l09, publications_l10, publications_l11.

import gleam/dict.{type Dict}

import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcList, SrcNull, SrcObject, SrcString,
  default_connection_window_options, default_selected_field_options,
  get_field_response_key, get_selected_child_fields, paginate_connection_items,
  project_graphql_value, serialize_connection, src_object,
}

import shopify_draft_proxy/proxy/products/collections_core.{
  collection_source_with_store_and_publication,
  collections_published_to_publication, product_collections_connection_source,
}
import shopify_draft_proxy/proxy/products/media_core.{
  product_media_connection_source,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type MutationFieldResult, type ProductUserError, ProductUserError,
}
import shopify_draft_proxy/proxy/products/products_core.{
  enumerate_items, product_currency_code, product_cursor,
}
import shopify_draft_proxy/proxy/products/products_records.{product_source}
import shopify_draft_proxy/proxy/products/publications_core.{
  ensure_default_publication_baseline, make_unique_publication_gid,
  products_published_to_publication, publication_catalog_source,
  publication_cursor, remove_publication_from_publishables,
  selected_publication_id,
}
import shopify_draft_proxy/proxy/products/publications_feeds.{
  optional_channel_source, product_source_with_relationships,
}
import shopify_draft_proxy/proxy/products/selling_plans_core.{
  selling_plan_group_connection_source,
}
import shopify_draft_proxy/proxy/products/shared.{
  connection_page_info_source, count_source, mutation_rejected_result,
  mutation_result, read_bool_field, read_string_argument, read_string_field,
  user_errors_source,
}
import shopify_draft_proxy/proxy/products/variants_options.{
  product_options_source,
}
import shopify_draft_proxy/proxy/products/variants_sources.{
  product_variants_connection_source,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type ProductRecord, type PublicationRecord, PublicationRecord,
}

// ===== from publications_l06 =====
@internal
pub fn publication_products_connection_source(
  products: List(ProductRecord),
) -> SourceValue {
  let edges =
    products
    |> enumerate_items()
    |> list.map(fn(pair) {
      let #(product, index) = pair
      src_object([
        #("cursor", SrcString(product_cursor(product, index))),
        #("node", product_source(product)),
      ])
    })
  src_object([
    #("edges", SrcList(edges)),
    #("nodes", SrcList(list.map(products, product_source))),
    #("pageInfo", connection_page_info_source(products, product_cursor)),
  ])
}

// ===== from publications_l07 =====
@internal
pub fn publication_source(
  store: Store,
  publication: PublicationRecord,
) -> SourceValue {
  let channel =
    store.list_effective_channels(store)
    |> list.find(fn(channel) { channel.publication_id == Some(publication.id) })
    |> option.from_result
  let published_products =
    products_published_to_publication(store, publication.id)
  let published_collections =
    collections_published_to_publication(store, publication.id)
  src_object([
    #("__typename", SrcString("Publication")),
    #("id", SrcString(publication.id)),
    #("name", graphql_helpers.option_string_source(publication.name)),
    #("autoPublish", SrcBool(publication.auto_publish |> option.unwrap(False))),
    #(
      "supportsFuturePublishing",
      SrcBool(publication.supports_future_publishing |> option.unwrap(False)),
    ),
    #("catalog", publication_catalog_source(publication.catalog_id)),
    #("channel", optional_channel_source(store, channel)),
    #("products", publication_products_connection_source(published_products)),
    #("productsCount", count_source(list.length(published_products))),
    #("publishedProductsCount", count_source(list.length(published_products))),
    #("collectionsCount", count_source(list.length(published_collections))),
  ])
}

// ===== from publications_l08 =====
@internal
pub fn serialize_publications_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let publications = store.list_effective_publications(store)
  let window =
    paginate_connection_items(
      publications,
      field,
      variables,
      publication_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: publication_cursor,
      serialize_node: fn(publication, node_field, _index) {
        project_graphql_value(
          publication_source(store, publication),
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
pub fn serialize_publication_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_publication_by_id(store, id) {
        Some(publication) ->
          project_graphql_value(
            publication_source(store, publication),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn publication_mutation_payload(
  store: Store,
  typename: String,
  publication: Option(PublicationRecord),
  deleted_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let publication_value = case publication {
    Some(record) -> publication_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("publication", publication_value),
      #("deletedId", graphql_helpers.option_string_source(deleted_id)),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

// ===== from publications_l09 =====
@internal
pub fn handle_publication_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case root_name {
    "publicationCreate" -> {
      let store = ensure_default_publication_baseline(store)
      let input =
        graphql_helpers.read_arg_object(args, "input")
        |> option.unwrap(dict.new())
      let validation_errors = publication_target_errors(store, input, True)
      case validation_errors {
        [] -> {
          let #(publication_id, next_identity) =
            make_unique_publication_gid(store, identity)
          let name = read_string_field(input, "name")
          let publication =
            PublicationRecord(
              id: publication_id,
              name: name,
              auto_publish: read_bool_field(input, "autoPublish"),
              supports_future_publishing: Some(False),
              catalog_id: read_string_field(input, "catalogId"),
              channel_id: read_string_field(input, "channelId"),
              cursor: None,
            )
          let #(staged, next_store) =
            store.upsert_staged_publication(store, publication)
          mutation_result(
            key,
            publication_mutation_payload(
              next_store,
              "PublicationCreatePayload",
              Some(staged),
              None,
              [],
              field,
              fragments,
            ),
            next_store,
            next_identity,
            [staged.id],
          )
        }
        _ ->
          mutation_rejected_result(
            key,
            publication_mutation_payload(
              store,
              "PublicationCreatePayload",
              None,
              None,
              validation_errors,
              field,
              fragments,
            ),
            store,
            identity,
          )
      }
    }
    "publicationUpdate" -> {
      let input =
        graphql_helpers.read_arg_object(args, "input")
        |> option.unwrap(dict.new())
      let publication_id =
        graphql_helpers.read_arg_string(args, "id")
        |> option.or(read_string_field(input, "id"))
      case publication_id {
        None ->
          mutation_result(
            key,
            publication_mutation_payload(
              store,
              "PublicationUpdatePayload",
              None,
              None,
              [ProductUserError(["id"], "Publication id is required", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(id) ->
          case store.get_effective_publication_by_id(store, id) {
            None ->
              mutation_result(
                key,
                publication_mutation_payload(
                  store,
                  "PublicationUpdatePayload",
                  None,
                  None,
                  [ProductUserError(["id"], "Publication not found", None)],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(existing) -> {
              let validation_errors =
                publication_target_errors(store, input, False)
              case validation_errors {
                [] -> {
                  let publication =
                    PublicationRecord(
                      ..existing,
                      name: read_string_field(input, "name")
                        |> option.or(existing.name),
                      auto_publish: read_bool_field(input, "autoPublish")
                        |> option.or(existing.auto_publish),
                      supports_future_publishing: read_bool_field(
                          input,
                          "supportsFuturePublishing",
                        )
                        |> option.or(existing.supports_future_publishing),
                      catalog_id: read_string_field(input, "catalogId")
                        |> option.or(existing.catalog_id),
                      channel_id: read_string_field(input, "channelId")
                        |> option.or(existing.channel_id),
                    )
                  let #(staged, next_store) =
                    store.upsert_staged_publication(store, publication)
                  mutation_result(
                    key,
                    publication_mutation_payload(
                      next_store,
                      "PublicationUpdatePayload",
                      Some(staged),
                      None,
                      [],
                      field,
                      fragments,
                    ),
                    next_store,
                    identity,
                    [staged.id],
                  )
                }
                _ ->
                  mutation_rejected_result(
                    key,
                    publication_mutation_payload(
                      store,
                      "PublicationUpdatePayload",
                      None,
                      None,
                      validation_errors,
                      field,
                      fragments,
                    ),
                    store,
                    identity,
                  )
              }
            }
          }
      }
    }
    "publicationDelete" -> {
      let store = ensure_default_publication_baseline(store)
      case graphql_helpers.read_arg_string(args, "id") {
        None ->
          mutation_result(
            key,
            publication_mutation_payload(
              store,
              "PublicationDeletePayload",
              None,
              None,
              [ProductUserError(["id"], "Publication id is required", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(id) ->
          case store.get_effective_publication_by_id(store, id) {
            None ->
              mutation_result(
                key,
                publication_mutation_payload(
                  store,
                  "PublicationDeletePayload",
                  None,
                  None,
                  [ProductUserError(["id"], "Publication not found", None)],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(existing) -> {
              case is_default_online_store_publication(existing) {
                True ->
                  mutation_rejected_result(
                    key,
                    publication_mutation_payload(
                      store,
                      "PublicationDeletePayload",
                      None,
                      None,
                      [
                        ProductUserError(
                          ["id"],
                          "Cannot delete the default publication",
                          Some("CANNOT_DELETE_DEFAULT_PUBLICATION"),
                        ),
                      ],
                      field,
                      fragments,
                    ),
                    store,
                    identity,
                  )
                False -> {
                  let next_store =
                    store
                    |> remove_publication_from_publishables(id)
                    |> store.delete_staged_publication(id)
                  mutation_result(
                    key,
                    publication_mutation_payload(
                      next_store,
                      "PublicationDeletePayload",
                      Some(existing),
                      Some(id),
                      [],
                      field,
                      fragments,
                    ),
                    next_store,
                    identity,
                    [id],
                  )
                }
              }
            }
          }
      }
    }
    _ -> mutation_result(key, json.null(), store, identity, [])
  }
}

fn publication_target_errors(
  store: Store,
  input: Dict(String, ResolvedValue),
  require_target: Bool,
) -> List(ProductUserError) {
  let catalog_id = read_string_field(input, "catalogId")
  let channel_id = read_string_field(input, "channelId")
  case catalog_id, channel_id {
    Some(_), Some(_) -> [
      ProductUserError(
        ["input"],
        "Only one of catalog or channel can be provided",
        Some("INVALID"),
      ),
    ]
    None, None -> {
      let has_legacy_name = option.is_some(read_string_field(input, "name"))
      case require_target && !has_legacy_name {
        True -> [
          ProductUserError(
            ["input", "catalogId"],
            "Catalog can't be blank",
            Some("BLANK"),
          ),
        ]
        False -> []
      }
    }
    Some(id), None ->
      case store.get_effective_catalog_by_id(store, id) {
        Some(_) -> []
        None -> [
          ProductUserError(
            ["input", "catalogId"],
            "Catalog not found",
            Some("NOT_FOUND"),
          ),
        ]
      }
    None, Some(id) ->
      case store.get_effective_channel_by_id(store, id) {
        Some(_) -> []
        None -> [
          ProductUserError(
            ["input", "channelId"],
            "Channel not found",
            Some("NOT_FOUND"),
          ),
        ]
      }
  }
}

fn is_default_online_store_publication(publication: PublicationRecord) -> Bool {
  publication.id == "gid://shopify/Publication/1"
  || publication.name == Some("Online Store")
}

// ===== from publications_l10 =====
@internal
pub fn product_source_with_store_and_publication(
  store: Store,
  product: ProductRecord,
  publication_id: Option(String),
) -> SourceValue {
  product_source_with_relationships(
    product,
    product_collections_connection_source(store, product),
    product_variants_connection_source(store, product),
    product_media_connection_source(store, product),
    product_options_source(store.get_effective_options_by_product_id(
      store,
      product.id,
    )),
    selling_plan_group_connection_source(
      store.list_effective_selling_plan_groups_visible_for_product(
        store,
        product.id,
      ),
    ),
    count_source(
      list.length(store.list_effective_selling_plan_groups_for_product(
        store,
        product.id,
      )),
    ),
    product_currency_code(store),
    publication_id,
  )
}

// ===== from publications_l11 =====
@internal
pub fn publishable_mutation_payload(
  store: Store,
  publishable: Option(SourceValue),
  user_errors: List(ProductUserError),
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let selected_publication =
    selected_publication_id(
      get_selected_child_fields(field, default_selected_field_options()),
      variables,
    )
  let publishable_value = case publishable {
    Some(SrcObject(source)) ->
      case dict.get(source, "__typename") {
        Ok(SrcString("Product")) ->
          case dict.get(source, "id") {
            Ok(SrcString(id)) ->
              case store.get_effective_product_by_id(store, id) {
                Some(product) ->
                  product_source_with_store_and_publication(
                    store,
                    product,
                    selected_publication,
                  )
                None -> SrcObject(source)
              }
            _ -> SrcObject(source)
          }
        Ok(SrcString("Collection")) ->
          case dict.get(source, "id") {
            Ok(SrcString(id)) ->
              case store.get_effective_collection_by_id(store, id) {
                Some(collection) ->
                  collection_source_with_store_and_publication(
                    store,
                    collection,
                    selected_publication,
                  )
                None -> SrcObject(source)
              }
            _ -> SrcObject(source)
          }
        _ -> SrcObject(source)
      }
    Some(value) -> value
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("PublishablePublishPayload")),
      #("publishable", publishable_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}
