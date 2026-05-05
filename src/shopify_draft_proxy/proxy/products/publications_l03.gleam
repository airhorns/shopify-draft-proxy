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
  json_string_array_literal,
}
import shopify_draft_proxy/proxy/products/publications_l01.{
  missing_variant_relationship_ids,
}
import shopify_draft_proxy/proxy/products/publications_l02.{
  product_bundle_mutation_payload, product_feed_create_payload,
  product_feed_delete_payload, product_full_sync_payload,
  product_resource_feedback_source,
  product_variant_relationship_bulk_update_payload,
  shop_resource_feedback_source,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_arg_object_list, read_object_list_field, read_string_argument,
  read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  mutation_result, user_errors_source,
}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type NullableFieldUserError, type ProductUserError,
  MutationFieldResult, NullableFieldUserError, ProductUserError,
} as product_types
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
  let user_errors = case root_name, product_id, existing_product {
    "productBundleUpdate", _, None -> [
      NullableFieldUserError(None, "Product does not exist"),
    ]
    _, _, _ -> {
      case read_object_list_field(input, "components") {
        [] -> [
          NullableFieldUserError(None, "At least one component is required."),
        ]
        _ -> []
      }
    }
  }
  mutation_result(
    key,
    product_bundle_mutation_payload(root_name, user_errors, field, fragments),
    store,
    identity,
    [],
  )
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
  mutation_result(
    key,
    product_variant_relationship_bulk_update_payload(
      user_errors,
      field,
      fragments,
    ),
    store,
    identity,
    [],
  )
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
