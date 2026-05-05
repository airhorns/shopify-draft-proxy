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
  serialize_product_user_errors_json,
}
import shopify_draft_proxy/proxy/products/selling_plans_l00.{
  selling_plan_group_cursor,
}
import shopify_draft_proxy/proxy/products/selling_plans_l14.{
  serialize_selling_plan_group_object,
}
import shopify_draft_proxy/proxy/products/shared_l00.{read_string_argument}
import shopify_draft_proxy/proxy/products/types.{
  type ProductUserError, ProductUserError,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  optional_string_list_json, optional_string_value_json,
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
pub fn serialize_selling_plan_group_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.get_effective_selling_plan_group_by_id(store, id) {
        Some(group) ->
          serialize_selling_plan_group_object(
            store,
            group,
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
pub fn serialize_selling_plan_groups_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let groups = store.list_effective_selling_plan_groups(store)
  case groups {
    [] -> serialize_empty_connection(field, default_selected_field_options())
    _ -> {
      let window =
        paginate_connection_items(
          groups,
          field,
          variables,
          selling_plan_group_cursor,
          default_connection_window_options(),
        )
      serialize_connection(
        field,
        SerializeConnectionConfig(
          items: window.items,
          has_next_page: window.has_next_page,
          has_previous_page: window.has_previous_page,
          get_cursor_value: selling_plan_group_cursor,
          serialize_node: fn(group, node_field, _index) {
            serialize_selling_plan_group_object(
              store,
              group,
              get_selected_child_fields(
                node_field,
                default_selected_field_options(),
              ),
              variables,
              fragments,
            )
          },
          selected_field_options: default_selected_field_options(),
          page_info_options: default_connection_page_info_options(),
        ),
      )
    }
  }
}

@internal
pub fn selling_plan_group_mutation_payload(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
  group: Option(SellingPlanGroupRecord),
  user_errors: List(ProductUserError),
  deleted_selling_plan_ids: Option(Option(List(String))),
  deleted_selling_plan_group_id: Option(Option(String)),
  removed_product_ids: Option(Option(List(String))),
  removed_product_variant_ids: Option(Option(List(String))),
) -> Json {
  json.object(
    get_selected_child_fields(field, default_selected_field_options())
    |> list.map(fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "sellingPlanGroup" ->
              case group {
                Some(group) ->
                  serialize_selling_plan_group_object(
                    store,
                    group,
                    get_selected_child_fields(
                      selection,
                      default_selected_field_options(),
                    ),
                    variables,
                    fragments,
                  )
                None -> json.null()
              }
            "userErrors" ->
              serialize_product_user_errors_json(user_errors, selection)
            "deletedSellingPlanIds" ->
              optional_string_list_json(deleted_selling_plan_ids)
            "deletedSellingPlanGroupId" ->
              optional_string_value_json(deleted_selling_plan_group_id)
            "removedProductIds" ->
              optional_string_list_json(removed_product_ids)
            "removedProductVariantIds" ->
              optional_string_list_json(removed_product_variant_ids)
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}
