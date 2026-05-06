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
