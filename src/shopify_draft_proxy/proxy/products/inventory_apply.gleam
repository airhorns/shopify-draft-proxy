//// Products-domain submodule: inventory_apply.
//// Combines layered files: inventory_l05, inventory_l06.

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
import shopify_draft_proxy/proxy/products/inventory_core.{
  active_inventory_levels, add_inventory_quantity_amount, find_inventory_level,
  find_inventory_level_target, inventory_compare_field_name,
  inventory_deactivate_only_state_error, inventory_quantity_amount,
  is_on_hand_component_quantity_name, replace_first_inventory_level,
  replace_inventory_level, variant_inventory_levels,
  write_inventory_quantity_amount, write_inventory_quantity_delta,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_helpers.{
  default_shipment_inventory_level,
}
import shopify_draft_proxy/proxy/products/inventory_validation.{
  inventory_deactivate_missing_target_errors, inventory_deactivate_payload,
  inventory_item_source_without_variant, stage_variant_inventory_levels,
}
import shopify_draft_proxy/proxy/products/products_core.{
  add_on_hand_move_delta, maybe_add_available_for_on_hand_delta,
  maybe_add_on_hand_component_delta,
}
import shopify_draft_proxy/proxy/products/products_validation.{
  product_derived_summary,
}
import shopify_draft_proxy/proxy/products/shared.{
  dedupe_preserving_order, mutation_result, read_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type InventoryAdjustmentChange, type InventoryAdjustmentChangeInput,
  type InventoryAdjustmentGroup, type InventoryMoveQuantityInput,
  type MutationFieldResult, type ProductTotalInventorySync,
  type ProductUserError, InventoryAdjustmentChange,
  InventoryAdjustmentChangeInput, InventoryAdjustmentGroup,
  InventoryMoveQuantityInput, MutationFieldResult, PreserveProductTotalInventory,
  ProductUserError, RecomputeProductTotalInventory,
} as product_types
import shopify_draft_proxy/proxy/products/variants_helpers.{
  has_only_default_variant,
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

// ===== from inventory_l05 =====
@internal
pub fn variant_inventory_item_source(
  variant: ProductVariantRecord,
) -> SourceValue {
  case variant.inventory_item {
    Some(item) -> inventory_item_source_without_variant(item)
    None -> SrcNull
  }
}

@internal
pub fn sync_product_set_inventory_summary(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
  _previous_product: Option(ProductRecord),
) -> #(Option(ProductRecord), Store, SyntheticIdentityRegistry) {
  case store.get_effective_product_by_id(store, product_id) {
    None -> #(None, store, identity)
    Some(product) -> {
      let #(updated_at, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let variants =
        store.get_effective_variants_by_product_id(store, product_id)
      let summary = product_derived_summary(variants)
      let next_product =
        ProductRecord(
          ..product,
          price_range_min: summary.price_range_min,
          price_range_max: summary.price_range_max,
          total_variants: summary.total_variants,
          has_only_default_variant: summary.has_only_default_variant,
          has_out_of_stock_variants: summary.has_out_of_stock_variants,
          total_inventory: summary.total_inventory,
          tracks_inventory: summary.tracks_inventory,
          updated_at: Some(updated_at),
        )
      let #(_, next_store) = store.upsert_staged_product(store, next_product)
      #(Some(next_product), next_store, next_identity)
    }
  }
}

@internal
pub fn adjust_inventory_item_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inventory_item_id: String,
  incoming_delta: Int,
  available_delta: Option(Int),
) -> #(Store, SyntheticIdentityRegistry) {
  case
    store.find_effective_variant_by_inventory_item_id(store, inventory_item_id)
  {
    None -> #(store, identity)
    Some(variant) -> {
      let levels = case variant_inventory_levels(variant) {
        [] -> [default_shipment_inventory_level(variant, inventory_item_id)]
        levels -> levels
      }
      let target = case levels {
        [first, ..] -> first
        [] -> default_shipment_inventory_level(variant, inventory_item_id)
      }
      let #(target, identity_after_incoming) =
        write_inventory_quantity_delta(
          target,
          identity,
          "incoming",
          incoming_delta,
        )
      let #(target, next_identity) = case available_delta {
        Some(delta) -> {
          let #(with_available, identity_after_available) =
            write_inventory_quantity_delta(
              target,
              identity_after_incoming,
              "available",
              delta,
            )
          write_inventory_quantity_delta(
            with_available,
            identity_after_available,
            "on_hand",
            delta,
          )
        }
        None -> #(target, identity_after_incoming)
      }
      let next_levels = replace_first_inventory_level(levels, target)
      #(
        stage_variant_inventory_levels(store, variant, next_levels),
        next_identity,
      )
    }
  }
}

@internal
pub fn handle_inventory_deactivate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let inventory_level_id = read_string_field(args, "inventoryLevelId")
  let target = case inventory_level_id {
    Some(inventory_level_id) ->
      find_inventory_level_target(store, inventory_level_id)
    None -> None
  }
  let user_errors = case target {
    Some(#(variant, level)) -> {
      let active_levels =
        variant_inventory_levels(variant)
        |> active_inventory_levels
      case list.length(active_levels) <= 1 {
        True -> [inventory_deactivate_only_state_error(level)]
        False -> []
      }
    }
    None ->
      inventory_deactivate_missing_target_errors(store, inventory_level_id)
  }
  let next_store = case target, user_errors {
    Some(#(variant, level)), [] -> {
      let next_level = InventoryLevelRecord(..level, is_active: Some(False))
      let next_levels =
        variant_inventory_levels(variant)
        |> replace_inventory_level(level.location.id, next_level)
      stage_variant_inventory_levels(store, variant, next_levels)
    }
    _, _ -> store
  }
  let staged_ids = case target, user_errors {
    Some(#(_variant, level)), [] -> [level.id]
    _, _ -> []
  }
  mutation_result(
    key,
    inventory_deactivate_payload(user_errors, field, fragments),
    next_store,
    identity,
    staged_ids,
  )
}

@internal
pub fn stage_inventory_quantity_adjust(
  store: Store,
  name: String,
  change: InventoryAdjustmentChangeInput,
  index: Int,
  require_change_from_quantity: Bool,
) -> Result(
  #(Store, InventoryAdjustmentChange, List(InventoryAdjustmentChange)),
  ProductUserError,
) {
  let path = ["input", "changes", int.to_string(index)]
  case change.inventory_item_id, change.location_id, change.delta {
    None, _, _ ->
      Error(ProductUserError(
        list.append(path, ["inventoryItemId"]),
        "Inventory item id is required",
        None,
      ))
    _, None, _ ->
      Error(ProductUserError(
        list.append(path, ["locationId"]),
        "Inventory location id is required",
        None,
      ))
    _, _, None ->
      Error(ProductUserError(
        list.append(path, ["delta"]),
        "Inventory delta is required",
        None,
      ))
    Some(inventory_item_id), Some(location_id), Some(delta) -> {
      case
        store.find_effective_variant_by_inventory_item_id(
          store,
          inventory_item_id,
        )
      {
        None ->
          Error(ProductUserError(
            list.append(path, ["inventoryItemId"]),
            "The specified inventory item could not be found.",
            None,
          ))
        Some(variant) -> {
          let current_levels = variant_inventory_levels(variant)
          case find_inventory_level(current_levels, location_id) {
            None ->
              Error(ProductUserError(
                list.append(path, ["locationId"]),
                "The specified location could not be found.",
                None,
              ))
            Some(level) -> {
              let previous = inventory_quantity_amount(level.quantities, name)
              case
                require_change_from_quantity
                && change.change_from_quantity != Some(previous)
              {
                True ->
                  Error(ProductUserError(
                    list.append(path, ["changeFromQuantity"]),
                    "The specified compare quantity does not match the current quantity.",
                    None,
                  ))
                False -> {
                  let quantities =
                    level.quantities
                    |> add_inventory_quantity_amount(name, delta)
                    |> maybe_add_on_hand_component_delta(name, delta)
                  let next_store =
                    stage_variant_inventory_levels(
                      store,
                      variant,
                      replace_inventory_level(
                        current_levels,
                        location_id,
                        InventoryLevelRecord(..level, quantities: quantities),
                      ),
                    )
                  let adjusted =
                    InventoryAdjustmentChange(
                      inventory_item_id: inventory_item_id,
                      location_id: location_id,
                      name: name,
                      delta: delta,
                      quantity_after_change: None,
                      ledger_document_uri: change.ledger_document_uri,
                    )
                  let mirrored = case is_on_hand_component_quantity_name(name) {
                    True -> [
                      InventoryAdjustmentChange(
                        inventory_item_id: inventory_item_id,
                        location_id: location_id,
                        name: "on_hand",
                        delta: delta,
                        quantity_after_change: None,
                        ledger_document_uri: None,
                      ),
                    ]
                    False -> []
                  }
                  Ok(#(next_store, adjusted, mirrored))
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
pub fn stage_inventory_quantity_set(
  store: Store,
  inventory_item_id: String,
  location_id: String,
  name: String,
  next_quantity: Int,
  ignore_compare_quantity: Bool,
  compare_quantity: Option(Int),
  use_change_from_quantity: Bool,
  index: Int,
) -> Result(#(Store, Int), ProductUserError) {
  case
    store.find_effective_variant_by_inventory_item_id(store, inventory_item_id)
  {
    None ->
      Error(ProductUserError(
        ["input", "quantities", int.to_string(index), "inventoryItemId"],
        "The specified inventory item could not be found.",
        None,
      ))
    Some(variant) -> {
      let current_levels = variant_inventory_levels(variant)
      case find_inventory_level(current_levels, location_id) {
        None ->
          Error(ProductUserError(
            ["input", "quantities", int.to_string(index), "locationId"],
            "The specified location could not be found.",
            None,
          ))
        Some(level) -> {
          let previous = inventory_quantity_amount(level.quantities, name)
          case !ignore_compare_quantity && compare_quantity != Some(previous) {
            True ->
              Error(ProductUserError(
                [
                  "input",
                  "quantities",
                  int.to_string(index),
                  inventory_compare_field_name(use_change_from_quantity),
                ],
                "The specified compare quantity does not match the current quantity.",
                None,
              ))
            False -> {
              let delta = next_quantity - previous
              let quantities =
                write_inventory_quantity_amount(
                  level.quantities,
                  name,
                  next_quantity,
                )
                |> maybe_add_on_hand_component_delta(name, delta)
                |> maybe_add_available_for_on_hand_delta(name, delta)
              let next_store =
                stage_variant_inventory_levels(
                  store,
                  variant,
                  replace_inventory_level(
                    current_levels,
                    location_id,
                    InventoryLevelRecord(..level, quantities: quantities),
                  ),
                )
              Ok(#(next_store, delta))
            }
          }
        }
      }
    }
  }
}

@internal
pub fn stage_inventory_quantity_move(
  store: Store,
  change: InventoryMoveQuantityInput,
  index: Int,
) -> Result(
  #(Store, InventoryAdjustmentChange, InventoryAdjustmentChange),
  ProductUserError,
) {
  let path = ["input", "changes", int.to_string(index)]
  case
    change.inventory_item_id,
    change.quantity,
    change.from.location_id,
    change.from.name,
    change.to.location_id,
    change.to.name
  {
    None, _, _, _, _, _ ->
      Error(ProductUserError(
        list.append(path, ["inventoryItemId"]),
        "Inventory item id is required",
        None,
      ))
    _, None, _, _, _, _ ->
      Error(ProductUserError(
        list.append(path, ["quantity"]),
        "Inventory move quantity is required",
        None,
      ))
    _, _, None, _, _, _ ->
      Error(ProductUserError(
        path,
        "Inventory move terminals are required",
        None,
      ))
    _, _, _, None, _, _ ->
      Error(ProductUserError(
        path,
        "Inventory move terminals are required",
        None,
      ))
    _, _, _, _, None, _ ->
      Error(ProductUserError(
        path,
        "Inventory move terminals are required",
        None,
      ))
    _, _, _, _, _, None ->
      Error(ProductUserError(
        path,
        "Inventory move terminals are required",
        None,
      ))
    Some(inventory_item_id),
      Some(quantity),
      Some(location_id),
      Some(from_name),
      _,
      Some(to_name)
    -> {
      case
        store.find_effective_variant_by_inventory_item_id(
          store,
          inventory_item_id,
        )
      {
        None ->
          Error(ProductUserError(
            list.append(path, ["inventoryItemId"]),
            "The specified inventory item could not be found.",
            None,
          ))
        Some(variant) -> {
          let current_levels = variant_inventory_levels(variant)
          case find_inventory_level(current_levels, location_id) {
            None ->
              Error(ProductUserError(
                list.append(path, ["from", "locationId"]),
                "The specified inventory item is not stocked at the location.",
                None,
              ))
            Some(level) -> {
              let quantities =
                level.quantities
                |> add_inventory_quantity_amount(from_name, 0 - quantity)
                |> add_inventory_quantity_amount(to_name, quantity)
                |> add_on_hand_move_delta(from_name, to_name, quantity)
              let next_store =
                stage_variant_inventory_levels(
                  store,
                  variant,
                  replace_inventory_level(
                    current_levels,
                    location_id,
                    InventoryLevelRecord(..level, quantities: quantities),
                  ),
                )
              Ok(#(
                next_store,
                InventoryAdjustmentChange(
                  inventory_item_id: inventory_item_id,
                  location_id: location_id,
                  name: from_name,
                  delta: 0 - quantity,
                  quantity_after_change: None,
                  ledger_document_uri: change.from.ledger_document_uri,
                ),
                InventoryAdjustmentChange(
                  inventory_item_id: inventory_item_id,
                  location_id: location_id,
                  name: to_name,
                  delta: quantity,
                  quantity_after_change: None,
                  ledger_document_uri: change.to.ledger_document_uri,
                ),
              ))
            }
          }
        }
      }
    }
  }
}

@internal
pub fn sync_product_inventory_summary(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
  total_inventory_sync: ProductTotalInventorySync,
) -> #(Option(ProductRecord), Store, SyntheticIdentityRegistry) {
  case store.get_effective_product_by_id(store, product_id) {
    None -> #(None, store, identity)
    Some(product) -> {
      let #(updated_at, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let variants =
        store.get_effective_variants_by_product_id(store, product_id)
      let summary = product_derived_summary(variants)
      let next_product =
        ProductRecord(
          ..product,
          price_range_min: summary.price_range_min,
          price_range_max: summary.price_range_max,
          total_variants: summary.total_variants,
          has_only_default_variant: summary.has_only_default_variant,
          has_out_of_stock_variants: summary.has_out_of_stock_variants,
          total_inventory: case total_inventory_sync {
            PreserveProductTotalInventory -> product.total_inventory
            RecomputeProductTotalInventory -> summary.total_inventory
          },
          tracks_inventory: summary.tracks_inventory,
          updated_at: Some(updated_at),
        )
      let #(_, next_store) = store.upsert_staged_product(store, next_product)
      #(Some(next_product), next_store, next_identity)
    }
  }
}

// ===== from inventory_l06 =====
@internal
pub fn sync_product_summaries_for_inventory_group(
  store: Store,
  identity: SyntheticIdentityRegistry,
  group: InventoryAdjustmentGroup,
  total_inventory_sync: ProductTotalInventorySync,
) -> #(Store, SyntheticIdentityRegistry, List(String)) {
  let product_ids =
    group.changes
    |> list.filter(fn(change) { change.name == "available" })
    |> list.filter_map(fn(change) {
      store.find_effective_variant_by_inventory_item_id(
        store,
        change.inventory_item_id,
      )
      |> option.to_result(Nil)
      |> result.map(fn(variant) { variant.product_id })
    })
    |> dedupe_preserving_order
  let #(next_store, next_identity) =
    list.fold(product_ids, #(store, identity), fn(acc, product_id) {
      let #(current_store, current_identity) = acc
      let #(_, synced_store, synced_identity) =
        sync_product_inventory_summary(
          current_store,
          current_identity,
          product_id,
          total_inventory_sync,
        )
      #(synced_store, synced_identity)
    })
  #(next_store, next_identity, product_ids)
}
