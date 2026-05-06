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
import shopify_draft_proxy/proxy/products/collections_l01.{
  collection_by_identifier, collection_cursor_for_field,
  read_collection_product_ids, sort_collections,
}
import shopify_draft_proxy/proxy/products/collections_l03.{
  collection_create_validation_errors, filtered_collections,
  stage_collection_product_memberships,
}
import shopify_draft_proxy/proxy/products/collections_l04.{
  created_collection_record,
}
import shopify_draft_proxy/proxy/products/collections_l15.{
  collection_create_payload, serialize_collection_object,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_identifier_argument, read_string_argument,
}
import shopify_draft_proxy/proxy/products/shared_l01.{mutation_result}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, AppendProducts, MutationFieldResult,
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
