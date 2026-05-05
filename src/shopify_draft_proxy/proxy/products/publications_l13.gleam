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
import shopify_draft_proxy/proxy/products/collections_l12.{
  publishable_collection_result,
}
import shopify_draft_proxy/proxy/products/publications_l01.{
  read_publication_targets,
}
import shopify_draft_proxy/proxy/products/publications_l11.{
  publishable_mutation_payload,
}
import shopify_draft_proxy/proxy/products/publications_l12.{
  product_publication_payload, publishable_product_result,
}
import shopify_draft_proxy/proxy/products/shared_l01.{mutation_result}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type ProductUserError, MutationFieldResult,
  ProductUserError,
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
pub fn handle_product_publication_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  typename: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "input")
  let id = case input {
    Some(input) -> graphql_helpers.read_arg_string(input, "id")
    None -> None
  }
  case id {
    None ->
      mutation_result(
        key,
        product_publication_payload(
          typename,
          store,
          None,
          [
            ProductUserError(["input", "id"], "Product id is required", None),
          ],
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
            product_publication_payload(
              typename,
              store,
              None,
              [
                ProductUserError(["input", "id"], "Product not found", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) -> {
          let #(_, next_store) = store.upsert_staged_product(store, product)
          mutation_result(
            key,
            product_publication_payload(
              typename,
              next_store,
              Some(product),
              [],
              field,
              fragments,
            ),
            next_store,
            identity,
            [product.id],
          )
        }
      }
  }
}

@internal
pub fn handle_publishable_publication_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  is_publish: Bool,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let publishable_id = graphql_helpers.read_arg_string(args, "id")
  let publication_targets = read_publication_targets(args)
  case publishable_id {
    None ->
      mutation_result(
        key,
        publishable_mutation_payload(
          store,
          None,
          [ProductUserError(["id"], "Publishable id is required", None)],
          field,
          variables,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(id) ->
      case store.get_effective_product_by_id(store, id) {
        Some(product) ->
          publishable_product_result(
            store,
            identity,
            key,
            product,
            publication_targets,
            is_publish,
            field,
            variables,
            fragments,
          )
        None ->
          case store.get_effective_collection_by_id(store, id) {
            Some(collection) ->
              publishable_collection_result(
                store,
                identity,
                key,
                collection,
                publication_targets,
                is_publish,
                field,
                variables,
                fragments,
              )
            None ->
              mutation_result(
                key,
                publishable_mutation_payload(
                  store,
                  None,
                  [
                    ProductUserError(
                      ["id"],
                      "Only Product and Collection publishable IDs are supported locally",
                      None,
                    ),
                  ],
                  field,
                  variables,
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
