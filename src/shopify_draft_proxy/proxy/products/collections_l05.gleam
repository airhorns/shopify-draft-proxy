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
import shopify_draft_proxy/proxy/products/collections_l02.{
  collection_add_products_v2_payload, collection_reorder_products_payload,
}
import shopify_draft_proxy/proxy/products/collections_l04.{
  add_products_to_collection, reorder_collection_products,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_arg_object_list, read_arg_string_list,
}
import shopify_draft_proxy/proxy/products/shared_l01.{mutation_result}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type ProductUserError, AppendProducts,
  MutationFieldResult, PrependReverseProducts, ProductUserError,
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
pub fn handle_collection_add_products_v2(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        collection_add_products_v2_payload(
          None,
          [ProductUserError(["id"], "Collection id is required", None)],
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
            collection_add_products_v2_payload(
              None,
              [ProductUserError(["id"], "Collection does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection) -> {
          let placement = case collection.sort_order {
            Some("MANUAL") -> AppendProducts
            _ -> PrependReverseProducts
          }
          let #(next_store, result_collection, user_errors) =
            add_products_to_collection(
              store,
              collection,
              read_arg_string_list(args, "productIds"),
              placement,
            )
          case user_errors, result_collection {
            [], Some(record) -> {
              let #(job_id, next_identity) =
                synthetic_identity.make_synthetic_gid(identity, "Job")
              mutation_result(
                key,
                collection_add_products_v2_payload(
                  Some(job_id),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                [record.id],
              )
            }
            _, _ ->
              mutation_result(
                key,
                collection_add_products_v2_payload(
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

@internal
pub fn handle_collection_reorder_products(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
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
