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
import shopify_draft_proxy/proxy/products/products_l00.{
  existing_group_app_id, existing_group_description,
  existing_group_merchant_code, existing_group_name, existing_group_position,
}
import shopify_draft_proxy/proxy/products/selling_plans_l00.{
  find_selling_plan, replace_selling_plan,
}
import shopify_draft_proxy/proxy/products/selling_plans_l03.{
  make_selling_plan_record, summarize_selling_plan_group,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  dedupe_preserving_order, read_int_field, read_object_list_field,
  read_string_field, read_string_list_field,
}
import shopify_draft_proxy/proxy/products/variants_l00.{existing_group_options}
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
pub fn make_selling_plan_group_record(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  existing: Option(SellingPlanGroupRecord),
  resources: Dict(String, ResolvedValue),
) -> #(SellingPlanGroupRecord, SyntheticIdentityRegistry) {
  let current_plans = case existing {
    Some(group) -> group.selling_plans
    None -> []
  }
  let #(created_plans, identity_after_creates) =
    read_object_list_field(input, "sellingPlansToCreate")
    |> list.fold(#(current_plans, identity), fn(acc, plan_input) {
      let #(plans, current_identity) = acc
      let #(plan, next_identity) =
        make_selling_plan_record(current_identity, plan_input, None)
      #(list.append(plans, [plan]), next_identity)
    })
  let #(updated_plans, identity_after_updates) =
    read_object_list_field(input, "sellingPlansToUpdate")
    |> list.fold(#(created_plans, identity_after_creates), fn(acc, plan_input) {
      let #(plans, current_identity) = acc
      case read_string_field(plan_input, "id") {
        None -> acc
        Some(plan_id) ->
          case find_selling_plan(plans, plan_id) {
            None -> acc
            Some(existing_plan) -> {
              let #(next_plan, next_identity) =
                make_selling_plan_record(
                  current_identity,
                  plan_input,
                  Some(existing_plan),
                )
              #(replace_selling_plan(plans, next_plan), next_identity)
            }
          }
      }
    })
  let deleted_plan_ids =
    read_string_list_field(input, "sellingPlansToDelete") |> option.unwrap([])
  let plans =
    updated_plans
    |> list.filter(fn(plan) { !list.contains(deleted_plan_ids, plan.id) })
  let #(created_at, identity_after_timestamp) = case existing {
    Some(group) -> #(group.created_at, identity_after_updates)
    None -> {
      let #(timestamp, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity_after_updates)
      #(Some(timestamp), next_identity)
    }
  }
  let #(id, next_identity) = case existing {
    Some(group) -> #(group.id, identity_after_timestamp)
    None ->
      synthetic_identity.make_proxy_synthetic_gid(
        identity_after_timestamp,
        "SellingPlanGroup",
      )
  }
  let existing_product_ids = case existing {
    Some(group) -> group.product_ids
    None -> []
  }
  let existing_variant_ids = case existing {
    Some(group) -> group.product_variant_ids
    None -> []
  }
  let group =
    SellingPlanGroupRecord(
      id: id,
      app_id: read_string_field(input, "appId")
        |> option.or(existing_group_app_id(existing)),
      name: read_string_field(input, "name")
        |> option.unwrap(existing_group_name(existing)),
      merchant_code: read_string_field(input, "merchantCode")
        |> option.unwrap(existing_group_merchant_code(existing)),
      description: read_string_field(input, "description")
        |> option.or(existing_group_description(existing)),
      options: read_string_list_field(input, "options")
        |> option.unwrap(existing_group_options(existing)),
      position: read_int_field(input, "position")
        |> option.or(existing_group_position(existing)),
      summary: summarize_selling_plan_group(plans),
      created_at: created_at,
      product_ids: dedupe_preserving_order(list.append(
        existing_product_ids,
        read_string_list_field(resources, "productIds") |> option.unwrap([]),
      )),
      product_variant_ids: dedupe_preserving_order(list.append(
        existing_variant_ids,
        read_string_list_field(resources, "productVariantIds")
          |> option.unwrap([]),
      )),
      selling_plans: plans,
      cursor: case existing {
        Some(group) -> group.cursor
        None -> None
      },
    )
  #(group, next_identity)
}
