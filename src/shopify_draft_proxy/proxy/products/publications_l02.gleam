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
import shopify_draft_proxy/proxy/products/products_l00.{
  duplicate_product_metafields,
}
import shopify_draft_proxy/proxy/products/publications_l00.{
  channel_cursor, is_valid_feedback_state, product_feed_source,
}
import shopify_draft_proxy/proxy/products/publications_l01.{
  channel_source, feedback_generated_at,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_string_argument, read_string_field, read_string_list_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  nullable_field_user_errors_source, user_errors_source,
}
import shopify_draft_proxy/proxy/products/types.{
  type NullableFieldUserError, type ProductUserError, NullableFieldUserError,
  ProductUserError,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l01.{
  duplicate_product_options, duplicate_product_variants,
}
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
pub fn serialize_channel_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_channel_by_id(store, id) {
        Some(channel) ->
          project_graphql_value(
            channel_source(store, channel),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_channels_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let channels = store.list_effective_channels(store)
  let window =
    paginate_connection_items(
      channels,
      field,
      variables,
      channel_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: channel_cursor,
      serialize_node: fn(channel, node_field, _index) {
        project_graphql_value(
          channel_source(store, channel),
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
pub fn optional_channel_source(
  store: Store,
  channel: Option(ChannelRecord),
) -> SourceValue {
  case channel {
    Some(channel) -> channel_source(store, channel)
    None -> SrcNull
  }
}

@internal
pub fn product_resource_feedback_source(
  feedback: ProductResourceFeedbackRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductResourceFeedback")),
    #("productId", SrcString(feedback.product_id)),
    #("state", SrcString(feedback.state)),
    #("messages", SrcList(list.map(feedback.messages, SrcString))),
    #("feedbackGeneratedAt", SrcString(feedback.feedback_generated_at)),
    #("productUpdatedAt", SrcString(feedback.product_updated_at)),
  ])
}

@internal
pub fn shop_resource_feedback_source(
  feedback: ShopResourceFeedbackRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("AppFeedback")),
    #("state", SrcString(feedback.state)),
    #("feedbackGeneratedAt", SrcString(feedback.feedback_generated_at)),
    #(
      "messages",
      SrcList(
        list.map(feedback.messages, fn(message) {
          src_object([#("message", SrcString(message))])
        }),
      ),
    ),
    #("app", SrcNull),
    #("link", SrcNull),
  ])
}

@internal
pub fn make_product_resource_feedback_record(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
) -> #(Option(ProductResourceFeedbackRecord), SyntheticIdentityRegistry) {
  let product_id = read_string_field(input, "productId")
  let state = read_string_field(input, "state")
  let #(feedback_generated_at, next_identity) =
    feedback_generated_at(input, identity)
  let product_updated_at =
    read_string_field(input, "productUpdatedAt")
    |> option.unwrap(feedback_generated_at)
  case product_id, state {
    Some(product_id), Some(state) ->
      case is_valid_feedback_state(state) {
        True -> #(
          Some(ProductResourceFeedbackRecord(
            product_id: product_id,
            state: state,
            feedback_generated_at: feedback_generated_at,
            product_updated_at: product_updated_at,
            messages: read_string_list_field(input, "messages")
              |> option.unwrap([]),
          )),
          next_identity,
        )
        False -> #(None, next_identity)
      }
    _, _ -> #(None, next_identity)
  }
}

@internal
pub fn make_shop_resource_feedback_record(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
) -> #(Option(ShopResourceFeedbackRecord), SyntheticIdentityRegistry) {
  let state = read_string_field(input, "state")
  case state {
    Some(state) ->
      case is_valid_feedback_state(state) {
        True -> {
          let #(id, identity_after_id) =
            synthetic_identity.make_synthetic_gid(identity, "AppFeedback")
          let #(feedback_generated_at, next_identity) =
            feedback_generated_at(input, identity_after_id)
          #(
            Some(ShopResourceFeedbackRecord(
              id: id,
              state: state,
              feedback_generated_at: feedback_generated_at,
              messages: read_string_list_field(input, "messages")
                |> option.unwrap([]),
            )),
            next_identity,
          )
        }
        False -> #(None, identity)
      }
    None -> #(None, identity)
  }
}

@internal
pub fn product_feed_create_payload(
  feed: ProductFeedRecord,
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductFeedCreatePayload")),
      #("productFeed", product_feed_source(feed)),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_feed_delete_payload(
  deleted_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductFeedDeletePayload")),
      #("deletedId", graphql_helpers.option_string_source(deleted_id)),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_full_sync_payload(
  id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductFullSyncPayload")),
      #("id", graphql_helpers.option_string_source(id)),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_bundle_mutation_payload(
  root_name: String,
  operation: Option(ProductOperationRecord),
  user_errors: List(NullableFieldUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let typename = case root_name {
    "productBundleUpdate" -> "ProductBundleUpdatePayload"
    _ -> "ProductBundleCreatePayload"
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("productBundleOperation", case operation {
        Some(operation) -> product_bundle_operation_source(operation)
        None -> SrcNull
      }),
      #("userErrors", nullable_field_user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

fn product_bundle_operation_source(
  operation: ProductOperationRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString(operation.type_name)),
    #("id", SrcString(operation.id)),
    #("status", SrcString(operation.status)),
    #("product", SrcNull),
  ])
}

@internal
pub fn combined_listing_update_payload(
  product: SourceValue,
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CombinedListingUpdatePayload")),
      #("product", product),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_variant_relationship_bulk_update_payload(
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let parent_product_variants = case user_errors {
    [] -> SrcList([])
    _ -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductVariantRelationshipBulkUpdatePayload")),
      #("parentProductVariants", parent_product_variants),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn duplicate_product_relationships(
  store: Store,
  identity: SyntheticIdentityRegistry,
  source_product_id: String,
  duplicate_product_id: String,
) -> #(Store, SyntheticIdentityRegistry, List(String)) {
  let #(options, identity_after_options, option_ids) =
    duplicate_product_options(
      identity,
      duplicate_product_id,
      store.get_effective_options_by_product_id(store, source_product_id),
    )
  let #(variants, identity_after_variants, variant_ids) =
    duplicate_product_variants(
      identity_after_options,
      duplicate_product_id,
      store.get_effective_variants_by_product_id(store, source_product_id),
    )
  let #(metafields, next_identity, metafield_ids) =
    duplicate_product_metafields(
      identity_after_variants,
      duplicate_product_id,
      store.get_effective_metafields_by_owner_id(store, source_product_id),
    )
  let memberships =
    store.list_effective_collections_for_product(store, source_product_id)
    |> list.map(fn(entry) {
      let #(_, membership) = entry
      ProductCollectionRecord(..membership, product_id: duplicate_product_id)
    })
  let next_store =
    store
    |> store.replace_staged_options_for_product(duplicate_product_id, options)
    |> store.replace_staged_variants_for_product(duplicate_product_id, variants)
    |> store.upsert_staged_product_collections(memberships)
    |> store.replace_staged_media_for_product(duplicate_product_id, [])
    |> store.replace_staged_metafields_for_owner(
      duplicate_product_id,
      metafields,
    )
  #(
    next_store,
    next_identity,
    list.append(option_ids, list.append(variant_ids, metafield_ids)),
  )
}
