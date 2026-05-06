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
import shopify_draft_proxy/proxy/products/inventory_l00.{
  find_inventory_level, inventory_activate_staged_ids, inventory_level_is_active,
  replace_inventory_level, variant_inventory_levels,
}
import shopify_draft_proxy/proxy/products/inventory_l01.{
  reactivate_inventory_level,
}
import shopify_draft_proxy/proxy/products/inventory_l03.{
  read_variant_inventory_item, variant_with_inventory_levels,
}
import shopify_draft_proxy/proxy/products/inventory_l04.{
  stage_variant_inventory_levels,
}
import shopify_draft_proxy/proxy/products/inventory_l05.{
  sync_product_inventory_summary,
}
import shopify_draft_proxy/proxy/products/inventory_l10.{
  inventory_item_update_payload,
}
import shopify_draft_proxy/proxy/products/inventory_l11.{
  inventory_activate_payload, inventory_adjustment_group_source,
  inventory_bulk_toggle_activation_payload, inventory_item_update_missing_result,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_arg_object_list, read_bool_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  mutation_result, user_errors_source,
}
import shopify_draft_proxy/proxy/products/types.{
  type InventoryAdjustmentGroup, type MutationFieldResult, type ProductUserError,
  InventoryAdjustmentGroup, MutationFieldResult, ProductUserError,
  RecomputeProductTotalInventory, product_user_error,
  product_user_error_code_invalid_inventory_item,
  product_user_error_code_invalid_location,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{variant_staged_ids}
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
pub fn handle_inventory_activate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let inventory_item_id = read_string_field(args, "inventoryItemId")
  let location_id = read_string_field(args, "locationId")
  let available_supplied = case dict.get(args, "available") {
    Ok(_) -> True
    Error(_) -> False
  }
  let variant = case inventory_item_id {
    Some(inventory_item_id) ->
      store.find_effective_variant_by_inventory_item_id(
        store,
        inventory_item_id,
      )
    None -> None
  }
  let resolved = case inventory_item_id, location_id, variant {
    Some(_), Some(location_id), Some(variant) ->
      case
        find_inventory_level(variant_inventory_levels(variant), location_id)
      {
        Some(level) -> Some(#(variant, level))
        None -> None
      }
    _, _, _ -> None
  }
  let user_errors = case inventory_item_id, location_id, variant {
    None, _, _ -> [
      product_user_error(
        ["inventoryItemId"],
        "Inventory item does not exist",
        product_user_error_code_invalid_inventory_item,
      ),
    ]
    Some(_), _, None -> [
      product_user_error(
        ["inventoryItemId"],
        "Inventory item does not exist",
        product_user_error_code_invalid_inventory_item,
      ),
    ]
    _, None, _ -> [
      product_user_error(
        ["locationId"],
        "Location does not exist",
        product_user_error_code_invalid_location,
      ),
    ]
    _, _, _ ->
      case resolved, available_supplied {
        Some(#(_, level)), True ->
          case inventory_level_is_active(level) {
            True -> [
              ProductUserError(
                ["available"],
                "Not allowed to set available quantity when the item is already active at the location.",
                None,
              ),
            ]
            False -> []
          }
        _, _ -> []
      }
  }
  let activation_result = case resolved, user_errors {
    Some(#(variant, level)), [] -> {
      case inventory_level_is_active(level) {
        True -> #(store, identity, resolved)
        False -> {
          let #(next_level, next_identity) =
            reactivate_inventory_level(level, identity)
          let next_levels =
            replace_inventory_level(
              variant_inventory_levels(variant),
              level.location.id,
              next_level,
            )
          let next_variant = variant_with_inventory_levels(variant, next_levels)
          #(
            stage_variant_inventory_levels(store, variant, next_levels),
            next_identity,
            Some(#(next_variant, next_level)),
          )
        }
      }
    }
    _, _ -> #(store, identity, resolved)
  }
  let #(next_store, next_identity, next_resolved) = activation_result
  let staged_ids = inventory_activate_staged_ids(next_resolved)
  mutation_result(
    key,
    inventory_activate_payload(
      next_store,
      next_resolved,
      user_errors,
      field,
      fragments,
    ),
    next_store,
    next_identity,
    staged_ids,
  )
}

@internal
pub fn handle_inventory_bulk_toggle_activation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let inventory_item_id = read_string_field(args, "inventoryItemId")
  let first_update =
    read_arg_object_list(args, "inventoryItemUpdates")
    |> list.first
    |> option.from_result
  let location_id = case first_update {
    Some(update) -> read_string_field(update, "locationId")
    None -> None
  }
  let activate = case first_update {
    Some(update) -> read_bool_field(update, "activate")
    None -> None
  }
  let variant = case inventory_item_id {
    Some(inventory_item_id) ->
      store.find_effective_variant_by_inventory_item_id(
        store,
        inventory_item_id,
      )
    None -> None
  }
  let target = case variant, location_id {
    Some(variant), Some(location_id) ->
      case
        find_inventory_level(variant_inventory_levels(variant), location_id)
      {
        Some(level) -> Some(#(variant, level))
        None -> None
      }
    _, _ -> None
  }
  let user_errors = case variant, location_id, target, activate {
    Some(_variant), Some(_location_id), None, _ -> [
      ProductUserError(
        ["inventoryItemUpdates", "0", "locationId"],
        "The quantity couldn't be updated because the location was not found.",
        Some("LOCATION_NOT_FOUND"),
      ),
    ]
    Some(variant),
      Some(_location_id),
      Some(#(_target_variant, level)),
      Some(False)
    -> {
      case list.length(variant_inventory_levels(variant)) <= 1 {
        True -> [
          ProductUserError(
            ["inventoryItemUpdates", "0", "locationId"],
            "The variant couldn't be unstocked from "
              <> level.location.name
              <> " because products need to be stocked at a minimum of 1 location.",
            Some("CANNOT_DEACTIVATE_FROM_ONLY_LOCATION"),
          ),
        ]
        False -> []
      }
    }
    _, _, _, _ -> []
  }
  let outcome = case target, activate, user_errors {
    Some(#(variant, level)), Some(False), [] -> {
      let next_levels =
        variant_inventory_levels(variant)
        |> list.filter(fn(candidate) { candidate.id != level.id })
      let next_store =
        stage_variant_inventory_levels(store, variant, next_levels)
      #(
        next_store,
        store.find_effective_variant_by_inventory_item_id(
          next_store,
          option.unwrap(inventory_item_id, ""),
        ),
        Some([]),
        [level.id],
      )
    }
    Some(#(variant, level)), _, [] -> #(
      store,
      Some(variant),
      Some([#(variant, level)]),
      case variant.inventory_item {
        Some(item) -> [level.id, item.id]
        None -> [level.id]
      },
    )
    _, _, [] -> #(store, variant, None, [])
    _, _, _ -> #(store, None, None, [])
  }
  let #(next_store, payload_variant, response_levels, staged_ids) = outcome
  mutation_result(
    key,
    inventory_bulk_toggle_activation_payload(
      next_store,
      payload_variant,
      response_levels,
      user_errors,
      field,
      fragments,
    ),
    next_store,
    identity,
    staged_ids,
  )
}

@internal
pub fn handle_inventory_item_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let inventory_item_id = read_string_field(args, "id")
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let existing_variant = case inventory_item_id {
    Some(inventory_item_id) ->
      store.find_effective_variant_by_inventory_item_id(
        store,
        inventory_item_id,
      )
    None -> None
  }
  case existing_variant {
    Some(variant) ->
      case variant.inventory_item {
        Some(existing_item) -> {
          let #(next_item, _) =
            read_variant_inventory_item(
              identity,
              Some(input),
              Some(existing_item),
            )
          case next_item {
            Some(next_item) -> {
              let next_variant =
                ProductVariantRecord(..variant, inventory_item: Some(next_item))
              let next_variants =
                store.get_effective_variants_by_product_id(
                  store,
                  variant.product_id,
                )
                |> list.map(fn(candidate) {
                  case candidate.id == variant.id {
                    True -> next_variant
                    False -> candidate
                  }
                })
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  variant.product_id,
                  next_variants,
                )
              let #(_, synced_store, synced_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity,
                  variant.product_id,
                  RecomputeProductTotalInventory,
                )
              let updated_variant =
                store.get_effective_variant_by_id(synced_store, variant.id)
                |> option.unwrap(next_variant)
              mutation_result(
                key,
                inventory_item_update_payload(
                  synced_store,
                  Some(updated_variant),
                  [],
                  field,
                  fragments,
                ),
                synced_store,
                synced_identity,
                variant_staged_ids(updated_variant),
              )
            }
            None ->
              inventory_item_update_missing_result(
                key,
                store,
                identity,
                field,
                fragments,
              )
          }
        }
        None ->
          inventory_item_update_missing_result(
            key,
            store,
            identity,
            field,
            fragments,
          )
      }
    None ->
      inventory_item_update_missing_result(
        key,
        store,
        identity,
        field,
        fragments,
      )
  }
}

@internal
pub fn inventory_quantity_payload(
  typename: String,
  store: Store,
  group: Option(InventoryAdjustmentGroup),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #(
        "inventoryAdjustmentGroup",
        inventory_adjustment_group_source(store, group),
      ),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}
