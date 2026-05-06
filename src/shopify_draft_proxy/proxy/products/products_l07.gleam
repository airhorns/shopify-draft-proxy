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
import shopify_draft_proxy/proxy/products/products_l01.{
  product_set_metafield_records,
}
import shopify_draft_proxy/proxy/products/products_l05.{
  product_set_product_field_errors,
}
import shopify_draft_proxy/proxy/products/shared_l00.{read_object_list_field}
import shopify_draft_proxy/proxy/products/variants_l00.{
  make_default_option_record, make_default_variant_record,
}
import shopify_draft_proxy/proxy/products/variants_l01.{
  product_set_requires_variants_for_options_errors,
  sync_product_options_with_variants,
}
import shopify_draft_proxy/proxy/products/variants_l02.{
  product_set_duplicate_variant_errors,
}
import shopify_draft_proxy/proxy/products/variants_l03.{
  product_set_option_records,
}
import shopify_draft_proxy/proxy/products/variants_l06.{
  product_set_scalar_variant_errors, product_set_variant_records,
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
pub fn product_set_validation_errors(
  store: Store,
  input: Dict(String, ResolvedValue),
  existing: Option(ProductRecord),
) -> List(ProductOperationUserErrorRecord) {
  list.append(
    product_set_product_field_errors(store, input, existing),
    list.append(
      product_set_requires_variants_for_options_errors(input),
      list.append(
        product_set_duplicate_variant_errors(input),
        product_set_scalar_variant_errors(input),
      ),
    ),
  )
}

@internal
pub fn apply_product_set_graph(
  store: Store,
  identity: SyntheticIdentityRegistry,
  existing: Option(ProductRecord),
  product_id: String,
  input: Dict(String, ResolvedValue),
) -> #(Store, SyntheticIdentityRegistry, List(String)) {
  let #(store, identity, option_ids) = case
    dict.has_key(input, "productOptions")
  {
    True -> {
      let #(options, next_identity, ids) =
        product_set_option_records(
          store,
          identity,
          product_id,
          read_object_list_field(input, "productOptions"),
        )
      let next_store =
        store.replace_staged_options_for_product(store, product_id, options)
      #(next_store, next_identity, ids)
    }
    False ->
      case existing {
        Some(_) -> #(store, identity, [])
        None ->
          case store.get_effective_options_by_product_id(store, product_id) {
            [] -> {
              let assert Some(product) =
                store.get_effective_product_by_id(store, product_id)
              let #(option, next_identity, ids) =
                make_default_option_record(identity, product)
              let next_store =
                store.replace_staged_options_for_product(store, product_id, [
                  option,
                ])
              #(next_store, next_identity, ids)
            }
            _ -> #(store, identity, [])
          }
      }
  }
  let #(store, identity, variant_ids) = case dict.has_key(input, "variants") {
    True -> {
      let #(variants, next_identity, ids) =
        product_set_variant_records(
          store,
          identity,
          product_id,
          read_object_list_field(input, "variants"),
        )
      let synced_options =
        sync_product_options_with_variants(
          store.get_effective_options_by_product_id(store, product_id),
          variants,
        )
      let next_store =
        store
        |> store.replace_staged_variants_for_product(product_id, variants)
        |> store.replace_staged_options_for_product(product_id, synced_options)
      #(next_store, next_identity, ids)
    }
    False ->
      case existing {
        Some(_) -> #(store, identity, [])
        None ->
          case store.get_effective_variants_by_product_id(store, product_id) {
            [] -> {
              let assert Some(product) =
                store.get_effective_product_by_id(store, product_id)
              let #(variant, next_identity, ids) =
                make_default_variant_record(identity, product)
              let next_store =
                store.replace_staged_variants_for_product(store, product_id, [
                  variant,
                ])
              #(next_store, next_identity, ids)
            }
            _ -> #(store, identity, [])
          }
      }
  }
  let #(store, identity, metafield_ids) = case
    dict.has_key(input, "metafields")
  {
    True -> {
      let #(metafields, next_identity, ids) =
        product_set_metafield_records(
          store,
          identity,
          product_id,
          read_object_list_field(input, "metafields"),
        )
      let next_store =
        store.replace_staged_metafields_for_owner(store, product_id, metafields)
      #(next_store, next_identity, ids)
    }
    False -> #(store, identity, [])
  }
  #(
    store,
    identity,
    list.append(option_ids, list.append(variant_ids, metafield_ids)),
  )
}
