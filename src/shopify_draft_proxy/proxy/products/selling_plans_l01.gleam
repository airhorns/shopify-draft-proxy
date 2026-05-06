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
import shopify_draft_proxy/proxy/products/selling_plans_l00.{
  selling_plan_cursor, selling_plan_group_summary_source,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  captured_json_source, read_int_field, read_number_captured_field,
  read_object_field, read_string_argument, read_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type ProductUserError, ProductUserError,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  optional_captured_int, optional_captured_string,
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
pub fn serialize_selling_plans_connection(
  plans: List(SellingPlanRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let window =
    paginate_connection_items(
      plans,
      field,
      variables,
      selling_plan_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: selling_plan_cursor,
      serialize_node: fn(plan, node_field, _index) {
        project_graphql_value(
          captured_json_source(plan.data),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

@internal
pub fn product_variant_count_for_selling_plan_group(
  store: Store,
  group: SellingPlanGroupRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Int {
  case read_string_argument(field, variables, "productId") {
    Some(product_id) ->
      group.product_variant_ids
      |> list.filter(fn(variant_id) {
        case store.get_effective_variant_by_id(store, variant_id) {
          Some(variant) -> variant.product_id == product_id
          None -> False
        }
      })
      |> list.length
    None -> list.length(group.product_variant_ids)
  }
}

@internal
pub fn serialize_selling_plan_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  let plan =
    store.list_effective_selling_plan_groups(store)
    |> list.flat_map(fn(group) { group.selling_plans })
    |> list.find(fn(plan) { plan.id == id })
  case plan {
    Ok(plan) ->
      project_graphql_value(
        captured_json_source(plan.data),
        selections,
        fragments,
      )
    Error(_) -> json.null()
  }
}

@internal
pub fn selling_plan_group_connection_source(
  groups: List(SellingPlanGroupRecord),
) -> SourceValue {
  src_object([
    #("nodes", SrcList(list.map(groups, selling_plan_group_summary_source))),
  ])
}

@internal
pub fn selling_plan_delivery_policy(
  input: Dict(String, ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  case read_object_field(input, "recurring") {
    Some(recurring) ->
      CapturedObject([
        #("__typename", CapturedString("SellingPlanRecurringDeliveryPolicy")),
        #(
          "interval",
          optional_captured_string(read_string_field(recurring, "interval")),
        ),
        #(
          "intervalCount",
          optional_captured_int(read_int_field(recurring, "intervalCount")),
        ),
        #("cutoff", optional_captured_int(read_int_field(recurring, "cutoff"))),
        #(
          "intent",
          CapturedString(
            read_string_field(recurring, "intent")
            |> option.unwrap("FULFILLMENT_BEGIN"),
          ),
        ),
        #(
          "preAnchorBehavior",
          CapturedString(
            read_string_field(recurring, "preAnchorBehavior")
            |> option.unwrap("ASAP"),
          ),
        ),
      ])
    None ->
      case read_object_field(input, "fixed") {
        Some(fixed) ->
          CapturedObject([
            #("__typename", CapturedString("SellingPlanFixedDeliveryPolicy")),
            #("cutoff", optional_captured_int(read_int_field(fixed, "cutoff"))),
            #(
              "fulfillmentTrigger",
              optional_captured_string(read_string_field(
                fixed,
                "fulfillmentTrigger",
              )),
            ),
            #(
              "fulfillmentExactTime",
              optional_captured_string(read_string_field(
                fixed,
                "fulfillmentExactTime",
              )),
            ),
            #(
              "intent",
              optional_captured_string(read_string_field(fixed, "intent")),
            ),
            #(
              "preAnchorBehavior",
              optional_captured_string(read_string_field(
                fixed,
                "preAnchorBehavior",
              )),
            ),
          ])
        None ->
          existing
          |> option.unwrap(
            CapturedObject([
              #(
                "__typename",
                CapturedString("SellingPlanRecurringDeliveryPolicy"),
              ),
            ]),
          )
      }
  }
}

@internal
pub fn selling_plan_policy_value(
  input: Dict(String, ResolvedValue),
) -> CapturedJsonValue {
  case read_number_captured_field(input, "fixedValue") {
    Some(value) ->
      CapturedObject([
        #("__typename", CapturedString("SellingPlanPricingPolicyFixedValue")),
        #("fixedValue", value),
      ])
    None ->
      case read_string_field(input, "fixedValue") {
        Some(value) ->
          CapturedObject([
            #(
              "__typename",
              CapturedString("SellingPlanPricingPolicyFixedValue"),
            ),
            #("fixedValue", CapturedString(value)),
          ])
        None ->
          CapturedObject([
            #(
              "__typename",
              CapturedString("SellingPlanPricingPolicyPercentageValue"),
            ),
            #(
              "percentage",
              read_number_captured_field(input, "percentage")
                |> option.unwrap(CapturedNull),
            ),
          ])
      }
  }
}

@internal
pub fn selling_plan_group_does_not_exist_error() -> ProductUserError {
  ProductUserError(
    ["id"],
    "Selling plan group does not exist.",
    Some("GROUP_DOES_NOT_EXIST"),
  )
}
