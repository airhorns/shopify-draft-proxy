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
  selling_plan_group_does_not_exist_error, selling_plan_policy_value,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  captured_object_field, dedupe_preserving_order, read_int_field,
  read_object_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  captured_number_string_field, captured_object_or_null, captured_string_field,
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
pub fn update_product_selling_plan_group_membership(
  store: Store,
  product_id: String,
  group_ids: List(String),
  join: Bool,
) -> #(Store, List(ProductUserError), List(String)) {
  let #(next_store, errors, staged_ids) =
    list.fold(group_ids, #(store, [], []), fn(acc, group_id) {
      let #(current_store, current_errors, current_ids) = acc
      case
        store.get_effective_selling_plan_group_by_id(current_store, group_id)
      {
        None -> #(
          current_store,
          list.append(current_errors, [
            selling_plan_group_does_not_exist_error(),
          ]),
          current_ids,
        )
        Some(group) -> {
          let next_product_ids = case join {
            True ->
              dedupe_preserving_order(
                list.append(group.product_ids, [
                  product_id,
                ]),
              )
            False ->
              group.product_ids
              |> list.filter(fn(existing_id) { existing_id != product_id })
          }
          let next_group =
            SellingPlanGroupRecord(..group, product_ids: next_product_ids)
          let #(_, updated_store) =
            store.upsert_staged_selling_plan_group(current_store, next_group)
          #(updated_store, current_errors, list.append(current_ids, [group_id]))
        }
      }
    })
  #(next_store, errors, staged_ids)
}

@internal
pub fn update_variant_selling_plan_group_membership(
  store: Store,
  variant_id: String,
  group_ids: List(String),
  join: Bool,
) -> #(Store, List(ProductUserError), List(String)) {
  let #(next_store, errors, staged_ids) =
    list.fold(group_ids, #(store, [], []), fn(acc, group_id) {
      let #(current_store, current_errors, current_ids) = acc
      case
        store.get_effective_selling_plan_group_by_id(current_store, group_id)
      {
        None -> #(
          current_store,
          list.append(current_errors, [
            selling_plan_group_does_not_exist_error(),
          ]),
          current_ids,
        )
        Some(group) -> {
          let next_variant_ids = case join {
            True ->
              dedupe_preserving_order(
                list.append(group.product_variant_ids, [
                  variant_id,
                ]),
              )
            False ->
              group.product_variant_ids
              |> list.filter(fn(existing_id) { existing_id != variant_id })
          }
          let next_group =
            SellingPlanGroupRecord(
              ..group,
              product_variant_ids: next_variant_ids,
            )
          let #(_, updated_store) =
            store.upsert_staged_selling_plan_group(current_store, next_group)
          #(updated_store, current_errors, list.append(current_ids, [group_id]))
        }
      }
    })
  #(next_store, errors, staged_ids)
}

@internal
pub fn selling_plan_billing_policy(
  input: Dict(String, ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  case read_object_field(input, "recurring") {
    Some(recurring) ->
      CapturedObject([
        #("__typename", CapturedString("SellingPlanRecurringBillingPolicy")),
        #(
          "interval",
          optional_captured_string(read_string_field(recurring, "interval")),
        ),
        #(
          "intervalCount",
          optional_captured_int(read_int_field(recurring, "intervalCount")),
        ),
        #(
          "minCycles",
          optional_captured_int(read_int_field(recurring, "minCycles")),
        ),
        #(
          "maxCycles",
          optional_captured_int(read_int_field(recurring, "maxCycles")),
        ),
      ])
    None ->
      case read_object_field(input, "fixed") {
        Some(fixed) ->
          CapturedObject([
            #("__typename", CapturedString("SellingPlanFixedBillingPolicy")),
            #(
              "checkoutCharge",
              captured_object_or_null(read_object_field(fixed, "checkoutCharge")),
            ),
            #(
              "remainingBalanceChargeTrigger",
              optional_captured_string(read_string_field(
                fixed,
                "remainingBalanceChargeTrigger",
              )),
            ),
            #(
              "remainingBalanceChargeExactTime",
              optional_captured_string(read_string_field(
                fixed,
                "remainingBalanceChargeExactTime",
              )),
            ),
            #(
              "remainingBalanceChargeTimeAfterCheckout",
              optional_captured_string(read_string_field(
                fixed,
                "remainingBalanceChargeTimeAfterCheckout",
              )),
            ),
          ])
        None ->
          existing
          |> option.unwrap(
            CapturedObject([
              #(
                "__typename",
                CapturedString("SellingPlanRecurringBillingPolicy"),
              ),
            ]),
          )
      }
  }
}

@internal
pub fn selling_plan_inventory_policy(
  input: Dict(String, ResolvedValue),
  existing: Option(CapturedJsonValue),
) -> CapturedJsonValue {
  CapturedObject([
    #(
      "reserve",
      optional_captured_string(
        read_string_field(input, "reserve")
        |> option.or(
          option.then(existing, fn(value) {
            captured_string_field(value, "reserve")
          }),
        ),
      ),
    ),
  ])
}

@internal
pub fn selling_plan_pricing_policy(
  input: Dict(String, ResolvedValue),
) -> CapturedJsonValue {
  case read_object_field(input, "fixed") {
    Some(fixed) ->
      CapturedObject([
        #("__typename", CapturedString("SellingPlanFixedPricingPolicy")),
        #(
          "adjustmentType",
          optional_captured_string(read_string_field(fixed, "adjustmentType")),
        ),
        #(
          "adjustmentValue",
          selling_plan_policy_value(
            read_object_field(fixed, "adjustmentValue")
            |> option.unwrap(dict.new()),
          ),
        ),
      ])
    None -> {
      let recurring =
        read_object_field(input, "recurring") |> option.unwrap(dict.new())
      CapturedObject([
        #("__typename", CapturedString("SellingPlanRecurringPricingPolicy")),
        #(
          "adjustmentType",
          optional_captured_string(read_string_field(
            recurring,
            "adjustmentType",
          )),
        ),
        #(
          "adjustmentValue",
          selling_plan_policy_value(
            read_object_field(recurring, "adjustmentValue")
            |> option.unwrap(dict.new()),
          ),
        ),
        #(
          "afterCycle",
          optional_captured_int(read_int_field(recurring, "afterCycle")),
        ),
      ])
    }
  }
}

@internal
pub fn first_selling_plan_percentage(
  value: CapturedJsonValue,
) -> Option(String) {
  case captured_object_field(value, "pricingPolicies") {
    Some(CapturedArray(policies)) ->
      policies
      |> list.find_map(fn(policy) {
        case
          captured_object_field(policy, "adjustmentValue")
          |> option.then(fn(adjustment) {
            captured_number_string_field(adjustment, "percentage")
          })
        {
          Some(value) -> Ok(value <> "%")
          None -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}
