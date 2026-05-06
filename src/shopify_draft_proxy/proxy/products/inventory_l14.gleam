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
import shopify_draft_proxy/proxy/products/inventory_l01.{
  read_inventory_adjustment_change_inputs,
}
import shopify_draft_proxy/proxy/products/inventory_l02.{
  read_inventory_move_quantity_inputs, validate_inventory_adjust_inputs,
}
import shopify_draft_proxy/proxy/products/inventory_l03.{
  inventory_adjust_202604_contract_error, validate_inventory_move_inputs,
}
import shopify_draft_proxy/proxy/products/inventory_l07.{
  apply_inventory_adjust_quantities, apply_inventory_move_quantities,
  apply_inventory_set_quantities,
}
import shopify_draft_proxy/proxy/products/inventory_l13.{
  inventory_quantity_mutation_result,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  mutation_error_with_null_data_result, read_non_empty_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type InventorySetQuantityInput, type MutationFieldResult,
  type ProductUserError, InventorySetQuantityInput, MutationFieldResult,
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
pub fn handle_inventory_adjust_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  uses_202604_contract: Bool,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case
    inventory_adjust_202604_contract_error(
      uses_202604_contract,
      input,
      field,
      variables,
    )
  {
    Some(error) ->
      mutation_error_with_null_data_result(key, store, identity, [
        error,
      ])
    None -> {
      let quantity_name = read_non_empty_string_field(input, "name")
      let reason = read_non_empty_string_field(input, "reason")
      let changes = read_inventory_adjustment_change_inputs(input)
      case quantity_name, reason, changes {
        None, _, _ ->
          inventory_quantity_mutation_result(
            key,
            "InventoryAdjustQuantitiesPayload",
            store,
            identity,
            None,
            [
              ProductUserError(
                ["input", "name"],
                "Inventory quantity name is required",
                None,
              ),
            ],
            field,
            fragments,
            [],
          )
        _, None, _ ->
          inventory_quantity_mutation_result(
            key,
            "InventoryAdjustQuantitiesPayload",
            store,
            identity,
            None,
            [
              ProductUserError(
                ["input", "reason"],
                "Inventory adjustment reason is required",
                None,
              ),
            ],
            field,
            fragments,
            [],
          )
        _, _, [] ->
          inventory_quantity_mutation_result(
            key,
            "InventoryAdjustQuantitiesPayload",
            store,
            identity,
            None,
            [
              ProductUserError(
                ["input", "changes"],
                "At least one inventory adjustment is required",
                None,
              ),
            ],
            field,
            fragments,
            [],
          )
        Some(name), Some(reason), changes -> {
          case validate_inventory_adjust_inputs(name, changes) {
            [_, ..] as errors ->
              inventory_quantity_mutation_result(
                key,
                "InventoryAdjustQuantitiesPayload",
                store,
                identity,
                None,
                errors,
                field,
                fragments,
                [],
              )
            [] -> {
              let result =
                apply_inventory_adjust_quantities(
                  store,
                  identity,
                  input,
                  name,
                  reason,
                  changes,
                  uses_202604_contract,
                )
              case result {
                Error(errors) ->
                  inventory_quantity_mutation_result(
                    key,
                    "InventoryAdjustQuantitiesPayload",
                    store,
                    identity,
                    None,
                    errors,
                    field,
                    fragments,
                    [],
                  )
                Ok(applied) -> {
                  let #(next_store, next_identity, group, staged_ids) = applied
                  inventory_quantity_mutation_result(
                    key,
                    "InventoryAdjustQuantitiesPayload",
                    next_store,
                    next_identity,
                    Some(group),
                    [],
                    field,
                    fragments,
                    staged_ids,
                  )
                }
              }
            }
          }
        }
      }
    }
  }
}

@internal
pub fn handle_valid_inventory_set_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  input: Dict(String, ResolvedValue),
  name: String,
  reason: Option(String),
  quantities: List(InventorySetQuantityInput),
  ignore_compare_quantity: Bool,
  uses_202604_contract: Bool,
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  case reason, quantities {
    None, _ ->
      inventory_quantity_mutation_result(
        key,
        "InventorySetQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "reason"],
            "Inventory adjustment reason is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    _, [] ->
      inventory_quantity_mutation_result(
        key,
        "InventorySetQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "quantities"],
            "At least one inventory quantity is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    Some(reason), quantities -> {
      case
        !uses_202604_contract
        && !ignore_compare_quantity
        && list.any(quantities, fn(quantity) {
          quantity.compare_quantity == None
        })
      {
        True ->
          inventory_quantity_mutation_result(
            key,
            "InventorySetQuantitiesPayload",
            store,
            identity,
            None,
            [
              ProductUserError(
                ["input", "ignoreCompareQuantity"],
                "The compareQuantity argument must be given to each quantity or ignored using ignoreCompareQuantity.",
                None,
              ),
            ],
            field,
            fragments,
            [],
          )
        False -> {
          let result =
            apply_inventory_set_quantities(
              store,
              identity,
              input,
              name,
              reason,
              quantities,
              ignore_compare_quantity,
              uses_202604_contract,
            )
          case result {
            Error(errors) ->
              inventory_quantity_mutation_result(
                key,
                "InventorySetQuantitiesPayload",
                store,
                identity,
                None,
                errors,
                field,
                fragments,
                [],
              )
            Ok(applied) -> {
              let #(next_store, next_identity, group, staged_ids) = applied
              inventory_quantity_mutation_result(
                key,
                "InventorySetQuantitiesPayload",
                next_store,
                next_identity,
                Some(group),
                [],
                field,
                fragments,
                staged_ids,
              )
            }
          }
        }
      }
    }
  }
}

@internal
pub fn handle_inventory_move_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let reason = read_non_empty_string_field(input, "reason")
  let changes = read_inventory_move_quantity_inputs(input)
  case reason, changes {
    None, _ ->
      inventory_quantity_mutation_result(
        key,
        "InventoryMoveQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "reason"],
            "Inventory adjustment reason is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    _, [] ->
      inventory_quantity_mutation_result(
        key,
        "InventoryMoveQuantitiesPayload",
        store,
        identity,
        None,
        [
          ProductUserError(
            ["input", "changes"],
            "At least one inventory quantity move is required",
            None,
          ),
        ],
        field,
        fragments,
        [],
      )
    Some(reason), changes -> {
      case validate_inventory_move_inputs(changes) {
        [_, ..] as errors ->
          inventory_quantity_mutation_result(
            key,
            "InventoryMoveQuantitiesPayload",
            store,
            identity,
            None,
            errors,
            field,
            fragments,
            [],
          )
        [] -> {
          let result =
            apply_inventory_move_quantities(
              store,
              identity,
              input,
              reason,
              changes,
            )
          case result {
            Error(errors) ->
              inventory_quantity_mutation_result(
                key,
                "InventoryMoveQuantitiesPayload",
                store,
                identity,
                None,
                errors,
                field,
                fragments,
                [],
              )
            Ok(applied) -> {
              let #(next_store, next_identity, group, staged_ids) = applied
              inventory_quantity_mutation_result(
                key,
                "InventoryMoveQuantitiesPayload",
                next_store,
                next_identity,
                Some(group),
                [],
                field,
                fragments,
                staged_ids,
              )
            }
          }
        }
      }
    }
  }
}
