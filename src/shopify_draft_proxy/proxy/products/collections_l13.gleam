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
  collection_has_product, serialize_collection_image,
  serialize_collection_rule_set,
}
import shopify_draft_proxy/proxy/products/collections_l12.{
  serialize_collection_products_connection,
}
import shopify_draft_proxy/proxy/products/products_l00.{product_seo_source}
import shopify_draft_proxy/proxy/products/products_l01.{
  serialize_product_metafield, serialize_product_metafields_connection,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  legacy_resource_id_from_gid, read_string_argument, serialize_exact_count,
}
import shopify_draft_proxy/proxy/products/variants_l00.{optional_string_json}
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
pub fn serialize_collection_field(
  store: Store,
  collection: CollectionRecord,
  field: Selection,
  field_name: String,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
  products_count_override: Option(Int),
) -> Json {
  case field_name {
    "__typename" -> json.string("Collection")
    "id" -> json.string(collection.id)
    "legacyResourceId" ->
      json.string(
        collection.legacy_resource_id
        |> option.unwrap(legacy_resource_id_from_gid(collection.id)),
      )
    "title" -> json.string(collection.title)
    "handle" -> json.string(collection.handle)
    "publishedOnCurrentPublication" | "publishedOnCurrentChannel" ->
      json.bool(collection.publication_ids != [])
    "publishedOnPublication" ->
      json.bool(case read_string_argument(field, variables, "publicationId") {
        Some(id) -> list.contains(collection.publication_ids, id)
        None -> False
      })
    "availablePublicationsCount"
    | "resourcePublicationsCount"
    | "publicationCount" ->
      serialize_exact_count(field, list.length(collection.publication_ids))
    "updatedAt" -> optional_string_json(collection.updated_at)
    "description" -> optional_string_json(collection.description)
    "descriptionHtml" -> optional_string_json(collection.description_html)
    "image" ->
      serialize_collection_image(
        collection.image,
        get_selected_child_fields(field, default_selected_field_options()),
      )
    "productsCount" ->
      serialize_exact_count(
        field,
        products_count_override
          |> option.unwrap(
            collection.products_count
            |> option.unwrap(
              list.length(store.list_effective_products_for_collection(
                store,
                collection.id,
              )),
            ),
          ),
      )
    "hasProduct" ->
      json.bool(collection_has_product(store, collection.id, field, variables))
    "sortOrder" -> optional_string_json(collection.sort_order)
    "templateSuffix" -> optional_string_json(collection.template_suffix)
    "seo" ->
      project_graphql_value(
        product_seo_source(collection.seo),
        get_selected_child_fields(field, default_selected_field_options()),
        fragments,
      )
    "ruleSet" ->
      serialize_collection_rule_set(
        collection.rule_set,
        get_selected_child_fields(field, default_selected_field_options()),
      )
    "products" ->
      serialize_collection_products_connection(
        store,
        collection,
        field,
        variables,
        fragments,
      )
    "metafield" ->
      serialize_product_metafield(store, collection.id, field, variables)
    "metafields" ->
      serialize_product_metafields_connection(
        store,
        collection.id,
        field,
        variables,
      )
    _ -> json.null()
  }
}
