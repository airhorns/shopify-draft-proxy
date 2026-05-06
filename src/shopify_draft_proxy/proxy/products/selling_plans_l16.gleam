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
  selling_plan_group_staged_ids,
}
import shopify_draft_proxy/proxy/products/selling_plans_l01.{
  selling_plan_group_does_not_exist_error,
}
import shopify_draft_proxy/proxy/products/selling_plans_l04.{
  make_selling_plan_group_record,
}
import shopify_draft_proxy/proxy/products/selling_plans_l15.{
  selling_plan_group_mutation_payload,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  dedupe_preserving_order, read_arg_string_list, read_string_list_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{mutation_result}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, MutationFieldResult,
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
pub fn handle_selling_plan_group_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case root_name {
    "sellingPlanGroupCreate" -> {
      let input =
        graphql_helpers.read_arg_object(args, "input")
        |> option.unwrap(dict.new())
      let resources =
        graphql_helpers.read_arg_object(args, "resources")
        |> option.unwrap(dict.new())
      let #(group, next_identity) =
        make_selling_plan_group_record(identity, input, None, resources)
      let #(_, next_store) =
        store.upsert_staged_selling_plan_group(store, group)
      mutation_result(
        key,
        selling_plan_group_mutation_payload(
          next_store,
          field,
          variables,
          fragments,
          Some(group),
          [],
          None,
          None,
          None,
          None,
        ),
        next_store,
        next_identity,
        selling_plan_group_staged_ids(group),
      )
    }
    "sellingPlanGroupUpdate" -> {
      let id = graphql_helpers.read_arg_string(args, "id")
      case
        id
        |> option.then(fn(id) {
          store.get_effective_selling_plan_group_by_id(store, id)
        })
      {
        None ->
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              [selling_plan_group_does_not_exist_error()],
              Some(None),
              None,
              None,
              None,
            ),
            store,
            identity,
            [],
          )
        Some(existing) -> {
          let input =
            graphql_helpers.read_arg_object(args, "input")
            |> option.unwrap(dict.new())
          let deleted_plan_ids =
            read_string_list_field(input, "sellingPlansToDelete")
            |> option.unwrap([])
            |> list.filter(fn(plan_id) {
              list.any(existing.selling_plans, fn(plan) { plan.id == plan_id })
            })
          let #(group, next_identity) =
            make_selling_plan_group_record(
              identity,
              input,
              Some(existing),
              dict.new(),
            )
          let #(_, next_store) =
            store.upsert_staged_selling_plan_group(store, group)
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              next_store,
              field,
              variables,
              fragments,
              Some(group),
              [],
              Some(Some(deleted_plan_ids)),
              None,
              None,
              None,
            ),
            next_store,
            next_identity,
            selling_plan_group_staged_ids(group),
          )
        }
      }
    }
    "sellingPlanGroupDelete" -> {
      let id = graphql_helpers.read_arg_string(args, "id")
      case
        id
        |> option.then(fn(id) {
          store.get_effective_selling_plan_group_by_id(store, id)
        })
      {
        None ->
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              [selling_plan_group_does_not_exist_error()],
              None,
              Some(None),
              None,
              None,
            ),
            store,
            identity,
            [],
          )
        Some(group) -> {
          let next_store =
            store.delete_staged_selling_plan_group(store, group.id)
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              next_store,
              field,
              variables,
              fragments,
              None,
              [],
              None,
              Some(Some(group.id)),
              None,
              None,
            ),
            next_store,
            identity,
            [group.id],
          )
        }
      }
    }
    "sellingPlanGroupAddProducts" | "sellingPlanGroupAddProductVariants" -> {
      let id = graphql_helpers.read_arg_string(args, "id")
      case
        id
        |> option.then(fn(id) {
          store.get_effective_selling_plan_group_by_id(store, id)
        })
      {
        None ->
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              [selling_plan_group_does_not_exist_error()],
              None,
              None,
              None,
              None,
            ),
            store,
            identity,
            [],
          )
        Some(group) -> {
          let next_group = case root_name {
            "sellingPlanGroupAddProducts" ->
              SellingPlanGroupRecord(
                ..group,
                product_ids: dedupe_preserving_order(list.append(
                  group.product_ids,
                  read_arg_string_list(args, "productIds"),
                )),
              )
            _ ->
              SellingPlanGroupRecord(
                ..group,
                product_variant_ids: dedupe_preserving_order(list.append(
                  group.product_variant_ids,
                  read_arg_string_list(args, "productVariantIds"),
                )),
              )
          }
          let #(_, next_store) =
            store.upsert_staged_selling_plan_group(store, next_group)
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              next_store,
              field,
              variables,
              fragments,
              Some(next_group),
              [],
              None,
              None,
              None,
              None,
            ),
            next_store,
            identity,
            [next_group.id],
          )
        }
      }
    }
    "sellingPlanGroupRemoveProducts"
    | "sellingPlanGroupRemoveProductVariants" -> {
      let id = graphql_helpers.read_arg_string(args, "id")
      case
        id
        |> option.then(fn(id) {
          store.get_effective_selling_plan_group_by_id(store, id)
        })
      {
        None ->
          mutation_result(
            key,
            selling_plan_group_mutation_payload(
              store,
              field,
              variables,
              fragments,
              None,
              [selling_plan_group_does_not_exist_error()],
              None,
              None,
              case root_name {
                "sellingPlanGroupRemoveProducts" -> Some(None)
                _ -> None
              },
              case root_name {
                "sellingPlanGroupRemoveProductVariants" -> Some(None)
                _ -> None
              },
            ),
            store,
            identity,
            [],
          )
        Some(group) -> {
          case root_name {
            "sellingPlanGroupRemoveProducts" -> {
              let requested = read_arg_string_list(args, "productIds")
              let removed =
                group.product_ids
                |> list.filter(fn(product_id) {
                  list.contains(requested, product_id)
                })
              let next_group =
                SellingPlanGroupRecord(
                  ..group,
                  product_ids: group.product_ids
                    |> list.filter(fn(product_id) {
                      !list.contains(requested, product_id)
                    }),
                )
              let #(_, next_store) =
                store.upsert_staged_selling_plan_group(store, next_group)
              mutation_result(
                key,
                selling_plan_group_mutation_payload(
                  next_store,
                  field,
                  variables,
                  fragments,
                  None,
                  [],
                  None,
                  None,
                  Some(Some(removed)),
                  None,
                ),
                next_store,
                identity,
                [next_group.id],
              )
            }
            _ -> {
              let requested = read_arg_string_list(args, "productVariantIds")
              let removed =
                group.product_variant_ids
                |> list.filter(fn(variant_id) {
                  list.contains(requested, variant_id)
                })
              let next_group =
                SellingPlanGroupRecord(
                  ..group,
                  product_variant_ids: group.product_variant_ids
                    |> list.filter(fn(variant_id) {
                      !list.contains(requested, variant_id)
                    }),
                )
              let #(_, next_store) =
                store.upsert_staged_selling_plan_group(store, next_group)
              mutation_result(
                key,
                selling_plan_group_mutation_payload(
                  next_store,
                  field,
                  variables,
                  fragments,
                  None,
                  [],
                  None,
                  None,
                  None,
                  Some(Some(removed)),
                ),
                next_store,
                identity,
                [next_group.id],
              )
            }
          }
        }
      }
    }
    _ -> mutation_result(key, json.null(), store, identity, [])
  }
}
