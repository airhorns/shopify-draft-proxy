//// Products-domain submodule: publications_core.
//// Combines layered files: publications_l00, publications_l01.

import gleam/dict.{type Dict}

import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import shopify_draft_proxy/graphql/ast.{
  type Selection, Field, InlineFragment, SelectionSet,
}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SerializeConnectionConfig, SrcNull,
  SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_selected_child_fields, paginate_connection_items, project_graphql_value,
  serialize_connection, src_object,
}

import shopify_draft_proxy/proxy/products/shared.{
  count_source, dedupe_preserving_order, read_arg_object_list,
  read_object_list_field, read_string_argument, read_string_field,
}
import shopify_draft_proxy/proxy/products/variants_helpers.{option_to_result}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type ChannelRecord, type ProductFeedRecord, type ProductRecord,
  type PublicationRecord, CollectionRecord, ProductRecord, PublicationRecord,
}

// ===== from publications_l00 =====
@internal
pub fn publication_cursor(
  publication: PublicationRecord,
  _index: Int,
) -> String {
  publication.cursor |> option.unwrap("cursor:" <> publication.id)
}

@internal
pub fn channel_cursor(channel: ChannelRecord, _index: Int) -> String {
  channel.cursor |> option.unwrap("cursor:" <> channel.id)
}

@internal
pub fn publication_catalog_source(catalog_id: Option(String)) -> SourceValue {
  case catalog_id {
    Some(id) ->
      src_object([
        #("__typename", SrcString("MarketCatalog")),
        #("id", SrcString(id)),
      ])
    None -> SrcNull
  }
}

@internal
pub fn optional_publication_source(
  publication: Option(PublicationRecord),
) -> SourceValue {
  case publication {
    Some(publication) ->
      src_object([
        #("__typename", SrcString("Publication")),
        #("id", SrcString(publication.id)),
        #("name", graphql_helpers.option_string_source(publication.name)),
      ])
    None -> SrcNull
  }
}

@internal
pub fn products_published_to_publication(
  store: Store,
  publication_id: String,
) -> List(ProductRecord) {
  store.list_effective_products(store)
  |> list.filter(fn(product) {
    product.status == "ACTIVE"
    && list.contains(product.publication_ids, publication_id)
  })
}

@internal
pub fn product_feed_cursor(feed: ProductFeedRecord, _index: Int) -> String {
  feed.id
}

@internal
pub fn product_feed_source(feed: ProductFeedRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductFeed")),
    #("id", SrcString(feed.id)),
    #("country", graphql_helpers.option_string_source(feed.country)),
    #("language", graphql_helpers.option_string_source(feed.language)),
    #("status", SrcString(feed.status)),
  ])
}

@internal
pub fn ensure_default_publication_baseline(store: Store) -> Store {
  case
    store.get_effective_publication_by_id(store, "gid://shopify/Publication/1")
  {
    Some(_) -> store
    None -> {
      let publication =
        PublicationRecord(
          id: "gid://shopify/Publication/1",
          name: Some("Online Store"),
          auto_publish: Some(True),
          supports_future_publishing: Some(False),
          catalog_id: None,
          channel_id: None,
          cursor: Some("cursor:gid://shopify/Publication/1"),
        )
      store.upsert_base_publications(store, [publication])
    }
  }
}

@internal
pub fn make_unique_publication_gid(
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> #(String, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "Publication")
  case store.get_effective_publication_by_id(store, id) {
    Some(_) -> make_unique_publication_gid(store, next_identity)
    None -> #(id, next_identity)
  }
}

@internal
pub fn remove_publication_targets(
  current: List(String),
  targets: List(String),
) -> List(String) {
  current
  |> list.filter(fn(id) { !list.contains(targets, id) })
}

@internal
pub fn is_valid_feedback_state(state: String) -> Bool {
  state == "ACCEPTED" || state == "REQUIRES_ACTION"
}

// ===== from publications_l01 =====
@internal
pub fn channel_source(store: Store, channel: ChannelRecord) -> SourceValue {
  let publication = case channel.publication_id {
    Some(id) -> store.get_effective_publication_by_id(store, id)
    None -> None
  }
  let product_count = case channel.publication_id {
    Some(id) -> list.length(products_published_to_publication(store, id))
    None -> 0
  }
  src_object([
    #("__typename", SrcString("Channel")),
    #("id", SrcString(channel.id)),
    #("name", graphql_helpers.option_string_source(channel.name)),
    #("handle", graphql_helpers.option_string_source(channel.handle)),
    #("publication", optional_publication_source(publication)),
    #("productsCount", count_source(product_count)),
  ])
}

@internal
pub fn serialize_product_feed_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_product_feed_by_id(store, id) {
        Some(feed) ->
          project_graphql_value(
            product_feed_source(feed),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_product_feeds_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let feeds = store.list_effective_product_feeds(store)
  let window =
    paginate_connection_items(
      feeds,
      field,
      variables,
      product_feed_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: product_feed_cursor,
      serialize_node: fn(feed, node_field, _index) {
        project_graphql_value(
          product_feed_source(feed),
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

@internal
pub fn selected_publication_id(
  selections: List(Selection),
  variables: Dict(String, ResolvedValue),
) -> Option(String) {
  selections
  |> list.find_map(fn(selection) {
    case selection {
      Field(name: name, ..) if name.value == "publishedOnPublication" ->
        read_string_argument(selection, variables, "publicationId")
        |> option_to_result
      Field(selection_set: Some(SelectionSet(selections: inner, ..)), ..)
      | InlineFragment(selection_set: SelectionSet(selections: inner, ..), ..) ->
        selected_publication_id(inner, variables) |> option_to_result
      _ -> Error(Nil)
    }
  })
  |> option.from_result
}

@internal
pub fn read_publication_targets(
  args: Dict(String, ResolvedValue),
) -> List(String) {
  read_arg_object_list(args, "input")
  |> list.filter_map(fn(input) {
    read_string_field(input, "publicationId") |> option_to_result
  })
}

@internal
pub fn merge_publication_targets(
  current: List(String),
  targets: List(String),
) -> List(String) {
  list.append(current, targets) |> dedupe_preserving_order
}

@internal
pub fn remove_publication_from_publishables(
  store: Store,
  publication_id: String,
) -> Store {
  let next_store =
    store.list_effective_products(store)
    |> list.filter(fn(product) {
      list.contains(product.publication_ids, publication_id)
    })
    |> list.fold(store, fn(acc, product) {
      let next_product =
        ProductRecord(
          ..product,
          publication_ids: remove_publication_targets(product.publication_ids, [
            publication_id,
          ]),
        )
      let #(_, staged_store) = store.upsert_staged_product(acc, next_product)
      staged_store
    })
  store.list_effective_collections(next_store)
  |> list.filter(fn(collection) {
    list.contains(collection.publication_ids, publication_id)
  })
  |> list.fold(next_store, fn(acc, collection) {
    let next_collection =
      CollectionRecord(
        ..collection,
        publication_ids: remove_publication_targets(collection.publication_ids, [
          publication_id,
        ]),
      )
    store.upsert_staged_collections(acc, [next_collection])
  })
}

@internal
pub fn missing_variant_relationship_ids(
  store: Store,
) -> fn(Dict(String, ResolvedValue)) -> List(String) {
  fn(input) {
    let parent_variant_id = case
      read_string_field(input, "parentProductVariantId")
    {
      Some(id) -> Some(id)
      None ->
        case read_string_field(input, "parentProductId") {
          Some(product_id) ->
            store.get_effective_variants_by_product_id(store, product_id)
            |> list.first
            |> option.from_result
            |> option.map(fn(variant) { variant.id })
          None -> None
        }
    }
    let parent_missing = case parent_variant_id {
      Some(id) ->
        case store.get_effective_variant_by_id(store, id) {
          Some(_) -> []
          None -> [id]
        }
      None -> []
    }
    let relationship_ids =
      list.append(
        read_object_list_field(input, "productVariantRelationshipsToCreate"),
        read_object_list_field(input, "productVariantRelationshipsToUpdate"),
      )
      |> list.filter_map(fn(relationship) {
        read_string_field(relationship, "id") |> option_to_result
      })
    let relationship_missing =
      relationship_ids
      |> list.filter(fn(id) {
        case store.get_effective_variant_by_id(store, id) {
          Some(_) -> False
          None -> True
        }
      })
    list.append(parent_missing, relationship_missing)
  }
}

@internal
pub fn feedback_generated_at(
  input: Dict(String, ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(String, SyntheticIdentityRegistry) {
  case read_string_field(input, "feedbackGeneratedAt") {
    Some(value) -> #(value, identity)
    None -> synthetic_identity.make_synthetic_timestamp(identity)
  }
}
