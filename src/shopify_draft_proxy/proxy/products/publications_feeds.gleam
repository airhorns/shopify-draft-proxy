//// Products-domain submodule: publications_feeds.
//// Combines layered files: publications_l02, publications_l03, publications_l04.

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
import shopify_draft_proxy/proxy/products/products_core.{
  duplicate_product_metafields, enumerate_items, json_string_array_literal,
  product_seo_source,
}
import shopify_draft_proxy/proxy/products/products_validation.{
  product_price_range_source,
}
import shopify_draft_proxy/proxy/products/publications_core.{
  channel_cursor, channel_source, feedback_generated_at, is_valid_feedback_state,
  missing_variant_relationship_ids, product_feed_source,
}
import shopify_draft_proxy/proxy/products/shared.{
  count_source, empty_connection_source, mutation_rejected_result,
  mutation_result, nullable_field_user_errors_source, read_arg_object_list,
  read_int_field, read_object_field, read_object_list_field,
  read_string_argument, read_string_field, read_string_list_field,
  user_errors_source,
}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type NullableFieldUserError, type ProductUserError,
  MutationFieldResult, NullableFieldUserError, ProductUserError,
} as product_types
import shopify_draft_proxy/proxy/products/variants_helpers.{
  has_only_default_variant, optional_product_category_source,
}
import shopify_draft_proxy/proxy/products/variants_options_core.{
  duplicate_product_options, duplicate_product_variants,
  optional_captured_json_source,
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
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid, make_synthetic_gid,
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

// ===== from publications_l02 =====
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

// ===== from publications_l03 =====
@internal
pub fn serialize_product_resource_feedback_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_product_resource_feedback(store, id) {
        Some(feedback) ->
          project_graphql_value(
            product_resource_feedback_source(feedback),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn handle_product_feed_create(
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
  // The captured local-runtime fixture comes from the TS path where the
  // mutation-log entry consumes the first synthetic id before the feed is
  // minted, so preserve that observable id sequence for this staged root.
  let #(_, identity_after_log_slot) =
    synthetic_identity.make_synthetic_gid(identity, "MutationLogEntry")
  let #(feed_id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_log_slot,
      "ProductFeed",
    )
  let feed =
    ProductFeedRecord(
      id: feed_id,
      country: read_string_field(input, "country"),
      language: read_string_field(input, "language"),
      status: "ACTIVE",
    )
  let #(staged_feed, next_store) = store.upsert_staged_product_feed(store, feed)
  mutation_result(
    key,
    product_feed_create_payload(staged_feed, [], field, fragments),
    next_store,
    next_identity,
    [staged_feed.id],
  )
}

@internal
pub fn handle_product_feed_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  case id {
    Some(feed_id) ->
      case store.get_effective_product_feed_by_id(store, feed_id) {
        Some(_) -> {
          let next_store = store.delete_staged_product_feed(store, feed_id)
          mutation_result(
            key,
            product_feed_delete_payload(Some(feed_id), [], field, fragments),
            next_store,
            identity,
            [feed_id],
          )
        }
        None ->
          mutation_result(
            key,
            product_feed_delete_payload(
              None,
              [
                ProductUserError(["id"], "ProductFeed does not exist", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      mutation_result(
        key,
        product_feed_delete_payload(
          None,
          [ProductUserError(["id"], "ProductFeed does not exist", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn handle_product_full_sync(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = graphql_helpers.read_arg_string(args, "id")
  case id {
    Some(feed_id) ->
      case store.get_effective_product_feed_by_id(store, feed_id) {
        Some(_) ->
          mutation_result(
            key,
            product_full_sync_payload(Some(feed_id), [], field, fragments),
            store,
            identity,
            [feed_id],
          )
        None ->
          mutation_result(
            key,
            product_full_sync_payload(
              None,
              [
                ProductUserError(["id"], "ProductFeed does not exist", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
      }
    None ->
      mutation_result(
        key,
        product_full_sync_payload(
          None,
          [ProductUserError(["id"], "ProductFeed does not exist", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn handle_product_bundle_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let product_id = read_string_field(input, "productId")
  let existing_product = case product_id {
    Some(id) -> store.get_effective_product_by_id(store, id)
    None -> None
  }
  let components = read_object_list_field(input, "components")
  let user_errors = case root_name, product_id, existing_product {
    "productBundleUpdate", _, None -> [
      NullableFieldUserError(None, "Product does not exist"),
    ]
    _, _, _ -> {
      case components {
        [] -> [
          NullableFieldUserError(None, "At least one component is required."),
        ]
        _ -> validate_product_bundle_components(store, input, components)
      }
    }
  }
  case user_errors {
    [] -> {
      let #(operation_id, next_identity) =
        make_synthetic_gid(identity, "ProductBundleOperation")
      let completed_operation =
        ProductOperationRecord(
          id: operation_id,
          type_name: "ProductBundleOperation",
          product_id: None,
          new_product_id: None,
          status: "ACTIVE",
          user_errors: [],
        )
      let #(staged_operation, next_store) =
        store.stage_product_operation(store, completed_operation)
      let initial_operation =
        ProductOperationRecord(..staged_operation, status: "CREATED")
      mutation_result(
        key,
        product_bundle_mutation_payload(
          root_name,
          Some(initial_operation),
          [],
          field,
          fragments,
        ),
        next_store,
        next_identity,
        [operation_id],
      )
    }
    _ ->
      mutation_rejected_result(
        key,
        product_bundle_mutation_payload(
          root_name,
          None,
          user_errors,
          field,
          fragments,
        ),
        store,
        identity,
      )
  }
}

const product_bundle_quantity_max = 2000

fn validate_product_bundle_components(
  store: Store,
  input: Dict(String, ResolvedValue),
  components: List(Dict(String, ResolvedValue)),
) -> List(NullableFieldUserError) {
  let missing_product_tails =
    components
    |> list.filter_map(fn(component) {
      case read_string_field(component, "productId") {
        Some(id) ->
          case store.get_effective_product_by_id(store, id) {
            Some(_) -> Error(Nil)
            None -> Ok(resource_id_tail(id))
          }
        None -> Error(Nil)
      }
    })
  case missing_product_tails {
    [] -> {
      list.append(
        product_bundle_option_mapping_errors(store, components),
        list.append(
          product_bundle_quantity_errors(components),
          list.append(
            product_bundle_quantity_option_errors(components),
            product_bundle_consolidated_option_errors(input, components),
          ),
        ),
      )
    }
    _ -> [
      NullableFieldUserError(
        None,
        "Failed to locate the following products: "
          <> numeric_id_array_literal(missing_product_tails),
      ),
    ]
  }
}

fn product_bundle_option_mapping_errors(
  store: Store,
  components: List(Dict(String, ResolvedValue)),
) -> List(NullableFieldUserError) {
  let invalid_product_tails =
    components
    |> list.filter_map(fn(component) {
      case read_string_field(component, "productId") {
        Some(id) ->
          case store.get_effective_product_by_id(store, id) {
            Some(_) -> {
              let options = store.get_effective_options_by_product_id(store, id)
              case product_bundle_component_options_valid(component, options) {
                True -> Error(Nil)
                False -> Ok(resource_id_tail(id))
              }
            }
            None -> Error(Nil)
          }
        None -> Error(Nil)
      }
    })
  case invalid_product_tails {
    [] -> []
    _ -> [
      NullableFieldUserError(
        None,
        "Mapping of components targeting products need to map all of the options of the product. Missing or invalid options found for components targeting product_ids "
          <> numeric_id_array_literal(invalid_product_tails)
          <> ".",
      ),
    ]
  }
}

fn product_bundle_component_options_valid(
  component: Dict(String, ResolvedValue),
  options: List(ProductOptionRecord),
) -> Bool {
  let selections = read_object_list_field(component, "optionSelections")
  list.length(selections) == list.length(options)
  && list.all(options, fn(option) {
    case product_bundle_selection_for_option(selections, option.id) {
      Some(selection) -> {
        let values =
          read_string_list_field(selection, "values") |> option.unwrap([])
        let valid_values =
          list.map(option.option_values, fn(value) { value.name })
        values != []
        && list.all(values, fn(value) { list.contains(valid_values, value) })
        && read_string_field(selection, "name") == Some(option.name)
      }
      None -> False
    }
  })
}

fn product_bundle_selection_for_option(
  selections: List(Dict(String, ResolvedValue)),
  option_id: String,
) -> Option(Dict(String, ResolvedValue)) {
  case
    selections
    |> list.filter(fn(selection) {
      read_string_field(selection, "componentOptionId") == Some(option_id)
    })
  {
    [selection] -> Some(selection)
    _ -> None
  }
}

fn product_bundle_quantity_errors(
  components: List(Dict(String, ResolvedValue)),
) -> List(NullableFieldUserError) {
  let exceeding_product_tails =
    components
    |> list.filter_map(fn(component) {
      case
        read_int_field(component, "quantity"),
        read_string_field(component, "productId")
      {
        Some(quantity), Some(product_id)
          if quantity > product_bundle_quantity_max
        -> Ok(resource_id_tail(product_id))
        _, _ -> Error(Nil)
      }
    })
  case exceeding_product_tails {
    [] -> []
    _ -> [
      NullableFieldUserError(
        None,
        "Quantity cannot be greater than "
          <> int.to_string(product_bundle_quantity_max)
          <> ". The following products have a quantity that exceeds the maximum: "
          <> numeric_id_array_literal(exceeding_product_tails),
      ),
    ]
  }
}

fn product_bundle_quantity_option_errors(
  components: List(Dict(String, ResolvedValue)),
) -> List(NullableFieldUserError) {
  let invalid_product_tails =
    components
    |> list.filter_map(fn(component) {
      case
        read_object_field(component, "quantityOption"),
        read_string_field(component, "productId")
      {
        Some(quantity_option), Some(product_id) -> {
          case read_object_list_field(quantity_option, "values") {
            [_] -> Ok(resource_id_tail(product_id))
            [] -> Ok(resource_id_tail(product_id))
            _ -> Error(Nil)
          }
        }
        _, _ -> Error(Nil)
      }
    })
  case invalid_product_tails {
    [] -> []
    _ -> [
      NullableFieldUserError(
        None,
        "Quantity options must have at least two values. Invalid quantity options found for components targeting product_ids "
          <> numeric_id_array_literal(invalid_product_tails)
          <> ".",
      ),
    ]
  }
}

fn product_bundle_consolidated_option_errors(
  input: Dict(String, ResolvedValue),
  components: List(Dict(String, ResolvedValue)),
) -> List(NullableFieldUserError) {
  let component_options =
    components
    |> list.flat_map(fn(component) {
      read_object_list_field(component, "optionSelections")
      |> list.filter_map(fn(selection) {
        case
          read_string_field(selection, "componentOptionId"),
          read_string_list_field(selection, "values")
        {
          Some(id), Some(values) -> Ok(#(id, values))
          _, _ -> Error(Nil)
        }
      })
    })
  let invalid =
    read_object_list_field(input, "consolidatedOptions")
    |> list.any(fn(component) {
      read_string_field(component, "optionName") == Some("")
      || {
        read_object_list_field(component, "optionSelections")
        |> list.any(fn(selection) {
          read_object_list_field(selection, "components")
          |> list.any(fn(selection_component) {
            case read_string_field(selection_component, "componentOptionId") {
              Some(component_option_id) ->
                !component_option_value_exists(
                  component_options,
                  component_option_id,
                  read_string_field(selection_component, "componentOptionValue"),
                )
              None -> False
            }
          })
        })
      }
    })
  case invalid {
    True -> [
      NullableFieldUserError(
        None,
        "Consolidated option selections are invalid.",
      ),
    ]
    False -> []
  }
}

fn component_option_value_exists(
  component_options: List(#(String, List(String))),
  component_option_id: String,
  component_option_value: Option(String),
) -> Bool {
  case component_option_value {
    Some(value) ->
      component_options
      |> list.any(fn(option) {
        let #(option_id, values) = option
        option_id == component_option_id && list.contains(values, value)
      })
    None -> False
  }
}

fn numeric_id_array_literal(values: List(String)) -> String {
  "[" <> string.join(values, ",") <> "]"
}

fn resource_id_tail(id: String) -> String {
  case list.last(string.split(id, "/")) {
    Ok(tail) -> tail
    Error(_) -> id
  }
}

@internal
pub fn handle_product_variant_relationship_bulk_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let inputs = read_arg_object_list(args, "input")
  let missing_ids =
    inputs
    |> list.flat_map(missing_variant_relationship_ids(store))
  let user_errors = case missing_ids {
    [] -> []
    _ -> [
      ProductUserError(
        ["input"],
        "The product variants with ID(s) "
          <> json_string_array_literal(missing_ids)
          <> " could not be found.",
        Some("PRODUCT_VARIANTS_NOT_FOUND"),
      ),
    ]
  }
  let user_errors = case user_errors {
    [] -> variant_relationship_semantics_errors(store, inputs)
    errors -> errors
  }
  let payload =
    product_variant_relationship_bulk_update_payload(
      user_errors,
      field,
      fragments,
    )
  case user_errors {
    [] -> mutation_result(key, payload, store, identity, [])
    _ -> mutation_rejected_result(key, payload, store, identity)
  }
}

const product_variant_relationship_component_quantity_max: Int = 9999

fn variant_relationship_semantics_errors(
  store: Store,
  inputs: List(Dict(String, ResolvedValue)),
) -> List(ProductUserError) {
  list.append(
    duplicate_parent_variant_errors(store, inputs),
    inputs
      |> enumerate_items
      |> list.flat_map(fn(pair) {
        let #(input, input_index) = pair
        variant_relationship_input_errors(store, input, input_index)
      }),
  )
}

fn duplicate_parent_variant_errors(
  store: Store,
  inputs: List(Dict(String, ResolvedValue)),
) -> List(ProductUserError) {
  let parent_ids =
    inputs
    |> enumerate_items
    |> list.filter_map(fn(pair) {
      let #(input, input_index) = pair
      case parent_variant_id_for_relationship_input(store, input) {
        Some(id) -> Ok(#(id, input_index))
        None -> Error(Nil)
      }
    })
  duplicate_parent_variant_errors_loop(parent_ids, [], [])
}

fn duplicate_parent_variant_errors_loop(
  parent_ids: List(#(String, Int)),
  seen: List(String),
  errors: List(ProductUserError),
) -> List(ProductUserError) {
  case parent_ids {
    [] -> list.reverse(errors)
    [#(id, input_index), ..rest] -> {
      case list.contains(seen, id) {
        True ->
          duplicate_parent_variant_errors_loop(rest, seen, [
            duplicated_products_error(["input", int.to_string(input_index)]),
            ..errors
          ])
        False ->
          duplicate_parent_variant_errors_loop(rest, [id, ..seen], errors)
      }
    }
  }
}

fn variant_relationship_input_errors(
  store: Store,
  input: Dict(String, ResolvedValue),
  input_index: Int,
) -> List(ProductUserError) {
  let parent_id = parent_variant_id_for_relationship_input(store, input)
  list.append(
    both_parent_ids_errors(input, input_index),
    list.append(
      create_relationship_errors(input, input_index, parent_id),
      list.append(
        update_relationship_errors(input, input_index),
        remove_relationship_errors(input, input_index),
      ),
    ),
  )
}

fn both_parent_ids_errors(
  input: Dict(String, ResolvedValue),
  input_index: Int,
) -> List(ProductUserError) {
  case
    read_string_field(input, "parentProductId"),
    read_string_field(input, "parentProductVariantId")
  {
    Some(_), Some(_) -> [
      ProductUserError(
        ["input", int.to_string(input_index)],
        "Only one of parentProductId or parentProductVariantId can be specified.",
        Some("INVALID_INPUT"),
      ),
    ]
    _, _ -> []
  }
}

fn create_relationship_errors(
  input: Dict(String, ResolvedValue),
  input_index: Int,
  parent_id: Option(String),
) -> List(ProductUserError) {
  let relationships =
    read_object_list_field(input, "productVariantRelationshipsToCreate")
  let quantity_errors =
    relationship_quantity_errors(
      relationships,
      input_index,
      "productVariantRelationshipsToCreate",
    )
  let duplicate_errors =
    duplicate_child_errors(
      relationships,
      input_index,
      "productVariantRelationshipsToCreate",
    )
  let self_errors = case parent_id {
    Some(parent_id) ->
      relationships
      |> enumerate_items
      |> list.filter_map(fn(pair) {
        let #(relationship, _relationship_index) = pair
        case read_string_field(relationship, "id") {
          Some(id) if id == parent_id ->
            Ok(ProductUserError(
              ["input"],
              "A parent product variant cannot contain itself as a component.",
              Some("CIRCULAR_REFERENCE"),
            ))
          _ -> Error(Nil)
        }
      })
    None -> []
  }
  list.append(quantity_errors, list.append(duplicate_errors, self_errors))
}

fn update_relationship_errors(
  input: Dict(String, ResolvedValue),
  input_index: Int,
) -> List(ProductUserError) {
  let relationships =
    read_object_list_field(input, "productVariantRelationshipsToUpdate")
  list.append(
    relationship_quantity_errors(
      relationships,
      input_index,
      "productVariantRelationshipsToUpdate",
    ),
    not_a_child_relationship_errors(
      relationships,
      input_index,
      "productVariantRelationshipsToUpdate",
    ),
  )
}

fn remove_relationship_errors(
  input: Dict(String, ResolvedValue),
  input_index: Int,
) -> List(ProductUserError) {
  read_string_list_field(input, "productVariantRelationshipsToRemove")
  |> option.unwrap([])
  |> enumerate_items
  |> list.map(fn(pair) {
    let #(_, relationship_index) = pair
    not_a_child_error([
      "input",
      int.to_string(input_index),
      "productVariantRelationshipsToRemove",
      int.to_string(relationship_index),
    ])
  })
}

fn relationship_quantity_errors(
  relationships: List(Dict(String, ResolvedValue)),
  input_index: Int,
  field_name: String,
) -> List(ProductUserError) {
  relationships
  |> enumerate_items
  |> list.filter_map(fn(pair) {
    let #(relationship, relationship_index) = pair
    case read_int_field(relationship, "quantity") {
      Some(quantity) if quantity < 1 ->
        Ok(ProductUserError(
          [
            "input",
            int.to_string(input_index),
            field_name,
            int.to_string(relationship_index),
            "quantity",
          ],
          "Quantity must be greater than or equal to 1",
          Some("INVALID"),
        ))
      Some(quantity)
        if quantity > product_variant_relationship_component_quantity_max
      ->
        Ok(ProductUserError(
          [
            "input",
            int.to_string(input_index),
            field_name,
            int.to_string(relationship_index),
            "quantity",
          ],
          "Quantity must be less than or equal to "
            <> int.to_string(
            product_variant_relationship_component_quantity_max,
          ),
          Some("INVALID"),
        ))
      _ -> Error(Nil)
    }
  })
}

fn duplicate_child_errors(
  relationships: List(Dict(String, ResolvedValue)),
  input_index: Int,
  field_name: String,
) -> List(ProductUserError) {
  let child_ids =
    relationships
    |> enumerate_items
    |> list.filter_map(fn(pair) {
      let #(relationship, relationship_index) = pair
      case read_string_field(relationship, "id") {
        Some(id) -> Ok(#(id, relationship_index))
        None -> Error(Nil)
      }
    })
  duplicate_child_errors_loop(child_ids, [], [], input_index, field_name)
}

fn duplicate_child_errors_loop(
  child_ids: List(#(String, Int)),
  seen: List(String),
  errors: List(ProductUserError),
  input_index: Int,
  field_name: String,
) -> List(ProductUserError) {
  case child_ids {
    [] -> list.reverse(errors)
    [#(id, relationship_index), ..rest] -> {
      case list.contains(seen, id) {
        True ->
          duplicate_child_errors_loop(
            rest,
            seen,
            [
              duplicated_products_error([
                "input",
                int.to_string(input_index),
                field_name,
                int.to_string(relationship_index),
                "id",
              ]),
              ..errors
            ],
            input_index,
            field_name,
          )
        False ->
          duplicate_child_errors_loop(
            rest,
            [id, ..seen],
            errors,
            input_index,
            field_name,
          )
      }
    }
  }
}

fn not_a_child_relationship_errors(
  relationships: List(Dict(String, ResolvedValue)),
  input_index: Int,
  field_name: String,
) -> List(ProductUserError) {
  relationships
  |> enumerate_items
  |> list.map(fn(pair) {
    let #(_, relationship_index) = pair
    not_a_child_error([
      "input",
      int.to_string(input_index),
      field_name,
      int.to_string(relationship_index),
      "id",
    ])
  })
}

fn parent_variant_id_for_relationship_input(
  store: Store,
  input: Dict(String, ResolvedValue),
) -> Option(String) {
  case read_string_field(input, "parentProductVariantId") {
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
}

fn duplicated_products_error(field: List(String)) -> ProductUserError {
  ProductUserError(
    field,
    "cannot_have_duplicated_products",
    Some("CANNOT_HAVE_DUPLICATED_PRODUCTS"),
  )
}

fn not_a_child_error(field: List(String)) -> ProductUserError {
  ProductUserError(field, "not_a_child", Some("NOT_A_CHILD"))
}

@internal
pub fn bulk_product_resource_feedback_create_payload(
  feedback: List(ProductResourceFeedbackRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString("BulkProductResourceFeedbackCreatePayload")),
      #(
        "feedback",
        SrcList(list.map(feedback, product_resource_feedback_source)),
      ),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn shop_resource_feedback_create_payload(
  feedback: Option(ShopResourceFeedbackRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let feedback_value = case feedback {
    Some(record) -> shop_resource_feedback_source(record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ShopResourceFeedbackCreatePayload")),
      #("feedback", feedback_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

// ===== from publications_l04 =====
@internal
pub fn product_source_with_relationships(
  product: ProductRecord,
  collections: SourceValue,
  variants: SourceValue,
  media: SourceValue,
  options: SourceValue,
  selling_plan_groups: SourceValue,
  selling_plan_groups_count: SourceValue,
  currency_code: String,
  publication_id: Option(String),
) -> SourceValue {
  let visible_publication_count = case product.status == "ACTIVE" {
    True -> list.length(product.publication_ids)
    False -> 0
  }
  let published_on_publication = case publication_id, product.status {
    Some(id), "ACTIVE" -> list.contains(product.publication_ids, id)
    _, _ -> False
  }
  src_object([
    #("__typename", SrcString("Product")),
    #("id", SrcString(product.id)),
    #(
      "legacyResourceId",
      graphql_helpers.option_string_source(product.legacy_resource_id),
    ),
    #("title", SrcString(product.title)),
    #("handle", SrcString(product.handle)),
    #("status", SrcString(product.status)),
    #("vendor", graphql_helpers.option_string_source(product.vendor)),
    #("productType", graphql_helpers.option_string_source(product.product_type)),
    #("tags", SrcList(list.map(product.tags, SrcString))),
    #("priceRangeV2", product_price_range_source(product, currency_code)),
    #("priceRange", product_price_range_source(product, currency_code)),
    #(
      "totalVariants",
      graphql_helpers.option_int_source(product.total_variants),
    ),
    #(
      "hasOnlyDefaultVariant",
      graphql_helpers.option_bool_source(product.has_only_default_variant),
    ),
    #(
      "hasOutOfStockVariants",
      graphql_helpers.option_bool_source(product.has_out_of_stock_variants),
    ),
    #(
      "totalInventory",
      graphql_helpers.option_int_source(product.total_inventory),
    ),
    #(
      "tracksInventory",
      graphql_helpers.option_bool_source(product.tracks_inventory),
    ),
    #("createdAt", graphql_helpers.option_string_source(product.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(product.updated_at)),
    #("publishedAt", graphql_helpers.option_string_source(product.published_at)),
    #("descriptionHtml", SrcString(product.description_html)),
    #(
      "onlineStorePreviewUrl",
      graphql_helpers.option_string_source(product.online_store_preview_url),
    ),
    #(
      "templateSuffix",
      graphql_helpers.option_string_source(product.template_suffix),
    ),
    #("seo", product_seo_source(product.seo)),
    #("category", optional_product_category_source(product.category)),
    #(
      "contextualPricing",
      optional_captured_json_source(product.contextual_pricing),
    ),
    #("publishedOnCurrentPublication", SrcBool(visible_publication_count > 0)),
    #("publishedOnCurrentChannel", SrcBool(visible_publication_count > 0)),
    #("publishedOnPublication", SrcBool(published_on_publication)),
    #(
      "combinedListingRole",
      graphql_helpers.option_string_source(product.combined_listing_role),
    ),
    #("availablePublicationsCount", count_source(visible_publication_count)),
    #("resourcePublicationsCount", count_source(visible_publication_count)),
    #("collections", collections),
    #("media", media),
    #("images", empty_connection_source()),
    #("options", options),
    #("variants", variants),
    #("requiresSellingPlan", SrcBool(False)),
    #("sellingPlanGroups", selling_plan_groups),
    #("sellingPlanGroupsCount", selling_plan_groups_count),
  ])
}

@internal
pub fn handle_bulk_product_resource_feedback_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let initial = #(store, identity, [], [], [])
  let #(next_store, next_identity, feedback, user_errors, staged_ids) =
    read_arg_object_list(args, "feedbackInput")
    |> enumerate_items()
    |> list.fold(initial, fn(acc, entry) {
      let #(current_store, current_identity, records, errors, ids) = acc
      let #(input, index) = entry
      let #(record, identity_after_record) =
        make_product_resource_feedback_record(current_identity, input)
      case record {
        Some(feedback_record) ->
          case
            store.get_effective_product_by_id(
              current_store,
              feedback_record.product_id,
            )
          {
            Some(_) -> {
              let #(staged, staged_store) =
                store.upsert_staged_product_resource_feedback(
                  current_store,
                  feedback_record,
                )
              #(
                staged_store,
                identity_after_record,
                list.append(records, [staged]),
                errors,
                list.append(ids, [staged.product_id]),
              )
            }
            None -> #(
              current_store,
              identity_after_record,
              records,
              list.append(errors, [
                ProductUserError(
                  ["feedbackInput", int.to_string(index), "productId"],
                  "Product does not exist",
                  None,
                ),
              ]),
              ids,
            )
          }
        None -> #(
          current_store,
          identity_after_record,
          records,
          list.append(errors, [
            ProductUserError(
              ["feedbackInput", int.to_string(index), "productId"],
              "Product does not exist",
              None,
            ),
          ]),
          ids,
        )
      }
    })
  mutation_result(
    key,
    bulk_product_resource_feedback_create_payload(
      feedback,
      user_errors,
      field,
      fragments,
    ),
    next_store,
    next_identity,
    staged_ids,
  )
}

@internal
pub fn handle_shop_resource_feedback_create(
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
  let #(record, next_identity) =
    make_shop_resource_feedback_record(identity, input)
  case record {
    Some(feedback) -> {
      let #(staged, next_store) =
        store.upsert_staged_shop_resource_feedback(store, feedback)
      mutation_result(
        key,
        shop_resource_feedback_create_payload(
          Some(staged),
          [],
          field,
          fragments,
        ),
        next_store,
        next_identity,
        [staged.id],
      )
    }
    None ->
      mutation_result(
        key,
        shop_resource_feedback_create_payload(
          None,
          [ProductUserError(["input", "state"], "State is invalid", None)],
          field,
          fragments,
        ),
        store,
        next_identity,
        [],
      )
  }
}
