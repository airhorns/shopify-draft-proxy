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
import shopify_draft_proxy/proxy/products/selling_plans_l01.{
  product_variant_count_for_selling_plan_group,
  serialize_selling_plans_connection,
}
import shopify_draft_proxy/proxy/products/selling_plans_l10.{
  serialize_selling_plan_group_variants_connection,
}
import shopify_draft_proxy/proxy/products/selling_plans_l13.{
  serialize_selling_plan_group_products_connection,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_string_argument, serialize_exact_count,
}
import shopify_draft_proxy/proxy/products/variants_l00.{
  optional_int_json, optional_string,
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
pub fn serialize_selling_plan_group_object(
  store: Store,
  group: SellingPlanGroupRecord,
  selections: List(Selection),
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "__typename" -> json.string("SellingPlanGroup")
            "id" -> json.string(group.id)
            "appId" -> optional_string(group.app_id)
            "name" -> json.string(group.name)
            "merchantCode" -> json.string(group.merchant_code)
            "description" -> optional_string(group.description)
            "options" -> json.array(group.options, json.string)
            "position" -> optional_int_json(group.position)
            "summary" -> optional_string(group.summary)
            "createdAt" -> optional_string(group.created_at)
            "productsCount" ->
              serialize_exact_count(selection, list.length(group.product_ids))
            "productVariantsCount" ->
              serialize_exact_count(
                selection,
                product_variant_count_for_selling_plan_group(
                  store,
                  group,
                  selection,
                  variables,
                ),
              )
            "appliesToProduct" ->
              json.bool(
                case read_string_argument(selection, variables, "productId") {
                  Some(product_id) ->
                    list.contains(group.product_ids, product_id)
                  None -> False
                },
              )
            "appliesToProductVariant" ->
              json.bool(
                case
                  read_string_argument(selection, variables, "productVariantId")
                {
                  Some(variant_id) ->
                    list.contains(group.product_variant_ids, variant_id)
                  None -> False
                },
              )
            "appliesToProductVariants" ->
              json.bool(
                case read_string_argument(selection, variables, "productId") {
                  Some(product_id) ->
                    list.any(group.product_variant_ids, fn(variant_id) {
                      case
                        store.get_effective_variant_by_id(store, variant_id)
                      {
                        Some(variant) -> variant.product_id == product_id
                        None -> False
                      }
                    })
                  None -> False
                },
              )
            "products" ->
              serialize_selling_plan_group_products_connection(
                store,
                group,
                selection,
                variables,
                fragments,
              )
            "productVariants" ->
              serialize_selling_plan_group_variants_connection(
                store,
                group,
                selection,
                variables,
                fragments,
              )
            "sellingPlans" ->
              serialize_selling_plans_connection(
                group.selling_plans,
                selection,
                variables,
                fragments,
              )
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}
