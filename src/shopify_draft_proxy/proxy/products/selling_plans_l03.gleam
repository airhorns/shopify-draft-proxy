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
  selling_plan_delivery_policy,
}
import shopify_draft_proxy/proxy/products/selling_plans_l02.{
  first_selling_plan_percentage, selling_plan_billing_policy,
  selling_plan_inventory_policy, selling_plan_pricing_policy,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  captured_object_field, read_int_field, read_object_field,
  read_object_list_field, read_string_field, read_string_list_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  captured_int_field, captured_string_array_field, captured_string_field,
}
import shopify_draft_proxy/proxy/products/variants_l00.{
  option_to_result, optional_captured_int, optional_captured_string,
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
pub fn make_selling_plan_record(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  existing: Option(SellingPlanRecord),
) -> #(SellingPlanRecord, SyntheticIdentityRegistry) {
  let #(id, identity_after_id) = case read_string_field(input, "id"), existing {
    Some(id), _ -> #(id, identity)
    None, Some(plan) -> #(plan.id, identity)
    None, None ->
      synthetic_identity.make_proxy_synthetic_gid(identity, "SellingPlan")
  }
  let previous = case existing {
    Some(plan) -> plan.data
    None -> CapturedObject([])
  }
  let #(created_at, next_identity) = case
    captured_string_field(previous, "createdAt")
  {
    Some(value) -> #(value, identity_after_id)
    None -> synthetic_identity.make_synthetic_timestamp(identity_after_id)
  }
  let data =
    CapturedObject([
      #("__typename", CapturedString("SellingPlan")),
      #("id", CapturedString(id)),
      #(
        "name",
        CapturedString(
          read_string_field(input, "name")
          |> option.or(captured_string_field(previous, "name"))
          |> option.unwrap("Selling plan"),
        ),
      ),
      #(
        "description",
        optional_captured_string(
          read_string_field(input, "description")
          |> option.or(captured_string_field(previous, "description")),
        ),
      ),
      #(
        "options",
        CapturedArray(
          read_string_list_field(input, "options")
          |> option.or(captured_string_array_field(previous, "options"))
          |> option.unwrap([])
          |> list.map(CapturedString),
        ),
      ),
      #(
        "position",
        optional_captured_int(
          read_int_field(input, "position")
          |> option.or(captured_int_field(previous, "position")),
        ),
      ),
      #(
        "category",
        optional_captured_string(
          read_string_field(input, "category")
          |> option.or(captured_string_field(previous, "category")),
        ),
      ),
      #("createdAt", CapturedString(created_at)),
      #(
        "billingPolicy",
        selling_plan_billing_policy(
          read_object_field(input, "billingPolicy") |> option.unwrap(dict.new()),
          captured_object_field(previous, "billingPolicy"),
        ),
      ),
      #(
        "deliveryPolicy",
        selling_plan_delivery_policy(
          read_object_field(input, "deliveryPolicy")
            |> option.unwrap(dict.new()),
          captured_object_field(previous, "deliveryPolicy"),
        ),
      ),
      #(
        "inventoryPolicy",
        selling_plan_inventory_policy(
          read_object_field(input, "inventoryPolicy")
            |> option.unwrap(dict.new()),
          captured_object_field(previous, "inventoryPolicy"),
        ),
      ),
      #("pricingPolicies", case dict.has_key(input, "pricingPolicies") {
        True ->
          CapturedArray(list.map(
            read_object_list_field(input, "pricingPolicies"),
            selling_plan_pricing_policy,
          ))
        False -> CapturedArray([])
      }),
    ])
  #(SellingPlanRecord(id: id, data: data), next_identity)
}

@internal
pub fn summarize_selling_plan_group(
  plans: List(SellingPlanRecord),
) -> Option(String) {
  let percentage =
    plans
    |> list.find_map(fn(plan) {
      first_selling_plan_percentage(plan.data) |> option_to_result
    })
    |> option.from_result
    |> option.unwrap("")
  Some(
    int.to_string(list.length(plans))
    <> " delivery frequency, "
    <> percentage
    <> " discount",
  )
}
