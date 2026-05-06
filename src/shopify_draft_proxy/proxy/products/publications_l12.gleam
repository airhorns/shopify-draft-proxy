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
import shopify_draft_proxy/proxy/products/products_l11.{
  product_source_with_store,
}
import shopify_draft_proxy/proxy/products/publications_l00.{
  remove_publication_targets,
}
import shopify_draft_proxy/proxy/products/publications_l01.{
  merge_publication_targets, selected_publication_id,
}
import shopify_draft_proxy/proxy/products/publications_l02.{
  combined_listing_update_payload,
}
import shopify_draft_proxy/proxy/products/publications_l10.{
  product_source_with_store_and_publication,
}
import shopify_draft_proxy/proxy/products/publications_l11.{
  publishable_mutation_payload,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  mutation_result, user_errors_source,
}
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
pub fn publishable_product_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
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
          Some(product_source_with_store(store, product)),
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
            product.publication_ids,
            publication_targets,
          )
        False ->
          remove_publication_targets(
            product.publication_ids,
            publication_targets,
          )
      }
      let next_product =
        ProductRecord(..product, publication_ids: next_publication_ids)
      let #(_, next_store) = store.upsert_staged_product(store, next_product)
      mutation_result(
        key,
        publishable_mutation_payload(
          next_store,
          Some(product_source_with_store_and_publication(
            next_store,
            next_product,
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
        [next_product.id],
      )
    }
  }
}

@internal
pub fn handle_combined_listing_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let parent_product_id =
    graphql_helpers.read_arg_string(args, "parentProductId")
  let parent_product = case parent_product_id {
    Some(id) -> store.get_effective_product_by_id(store, id)
    None -> None
  }
  case parent_product {
    None ->
      mutation_result(
        key,
        combined_listing_update_payload(
          SrcNull,
          [
            ProductUserError(
              ["parentProductId"],
              "Product does not exist",
              Some("PARENT_PRODUCT_NOT_FOUND"),
            ),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product) ->
      mutation_result(
        key,
        combined_listing_update_payload(
          product_source_with_store(store, product),
          [],
          field,
          fragments,
        ),
        store,
        identity,
        [product.id],
      )
  }
}

@internal
pub fn product_publication_payload(
  typename: String,
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}
