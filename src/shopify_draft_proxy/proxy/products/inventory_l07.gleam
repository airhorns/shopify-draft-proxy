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
  inventory_adjust_product_total_inventory_sync, inventory_set_quantity_changes,
  quantity_compare_quantity,
}
import shopify_draft_proxy/proxy/products/inventory_l02.{
  inventory_adjustment_staged_ids, make_inventory_adjustment_group,
}
import shopify_draft_proxy/proxy/products/inventory_l04.{
  validate_inventory_set_quantity_inputs,
}
import shopify_draft_proxy/proxy/products/inventory_l05.{
  stage_inventory_quantity_adjust, stage_inventory_quantity_move,
  stage_inventory_quantity_set,
}
import shopify_draft_proxy/proxy/products/inventory_l06.{
  sync_product_summaries_for_inventory_group,
}
import shopify_draft_proxy/proxy/products/media_l03.{
  variant_media_connection_source,
}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/selling_plans_l01.{
  selling_plan_group_connection_source,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  count_source, read_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type InventoryAdjustmentChange, type InventoryAdjustmentChangeInput,
  type InventoryAdjustmentGroup, type InventoryMoveQuantityInput,
  type InventorySetQuantityInput, type ProductUserError,
  InventoryAdjustmentChange, InventoryAdjustmentChangeInput,
  InventoryAdjustmentGroup, InventoryMoveQuantityInput,
  InventorySetQuantityInput, ProductUserError, RecomputeProductTotalInventory,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{selected_option_source}
import shopify_draft_proxy/proxy/products/variants_l01.{
  optional_captured_json_source,
}
import shopify_draft_proxy/proxy/products/variants_l06.{variant_product_source}
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
pub fn product_variant_source_with_inventory(
  store: Store,
  variant: ProductVariantRecord,
  inventory_item: SourceValue,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ProductVariant")),
    #("id", SrcString(variant.id)),
    #("title", SrcString(variant.title)),
    #("sku", graphql_helpers.option_string_source(variant.sku)),
    #("barcode", graphql_helpers.option_string_source(variant.barcode)),
    #("price", graphql_helpers.option_string_source(variant.price)),
    #(
      "compareAtPrice",
      graphql_helpers.option_string_source(variant.compare_at_price),
    ),
    #("taxable", graphql_helpers.option_bool_source(variant.taxable)),
    #(
      "inventoryPolicy",
      graphql_helpers.option_string_source(variant.inventory_policy),
    ),
    #(
      "inventoryQuantity",
      graphql_helpers.option_int_source(variant.inventory_quantity),
    ),
    #(
      "selectedOptions",
      SrcList(list.map(variant.selected_options, selected_option_source)),
    ),
    #("inventoryItem", inventory_item),
    #("product", variant_product_source(store, variant.product_id)),
    #("media", variant_media_connection_source(store, variant)),
    #(
      "sellingPlanGroups",
      selling_plan_group_connection_source(
        store.list_effective_selling_plan_groups_visible_for_product_variant(
          store,
          variant.id,
        ),
      ),
    ),
    #(
      "sellingPlanGroupsCount",
      count_source(
        list.length(
          store.list_effective_selling_plan_groups_for_product_variant(
            store,
            variant.id,
          ),
        ),
      ),
    ),
    #(
      "contextualPricing",
      optional_captured_json_source(variant.contextual_pricing),
    ),
  ])
}

@internal
pub fn apply_inventory_adjust_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  name: String,
  reason: String,
  changes: List(InventoryAdjustmentChangeInput),
  require_change_from_quantity: Bool,
) -> Result(
  #(Store, SyntheticIdentityRegistry, InventoryAdjustmentGroup, List(String)),
  List(ProductUserError),
) {
  let reference_document_uri = read_string_field(input, "referenceDocumentUri")
  let result =
    changes
    |> enumerate_items()
    |> list.try_fold(#([], [], store), fn(acc, pair) {
      let #(change, index) = pair
      let #(adjusted_changes, mirrored_changes, current_store) = acc
      case
        stage_inventory_quantity_adjust(
          current_store,
          name,
          change,
          index,
          require_change_from_quantity,
        )
      {
        Error(error) -> Error([error])
        Ok(applied) -> {
          let #(next_store, adjusted_change, mirrored) = applied
          Ok(#(
            list.append(adjusted_changes, [adjusted_change]),
            list.append(mirrored_changes, mirrored),
            next_store,
          ))
        }
      }
    })
  case result {
    Error(errors) -> Error(errors)
    Ok(done) -> {
      let #(adjusted_changes, mirrored_changes, next_store) = done
      let #(group, next_identity) =
        make_inventory_adjustment_group(
          identity,
          reason,
          reference_document_uri,
          list.append(adjusted_changes, mirrored_changes),
        )
      let #(synced_store, synced_identity, product_ids) =
        sync_product_summaries_for_inventory_group(
          next_store,
          next_identity,
          group,
          inventory_adjust_product_total_inventory_sync(
            require_change_from_quantity,
          ),
        )
      Ok(#(
        synced_store,
        synced_identity,
        group,
        list.append(inventory_adjustment_staged_ids(group), product_ids),
      ))
    }
  }
}

@internal
pub fn apply_inventory_set_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  name: String,
  reason: String,
  quantities: List(InventorySetQuantityInput),
  ignore_compare_quantity: Bool,
  use_change_from_quantity: Bool,
) -> Result(
  #(Store, SyntheticIdentityRegistry, InventoryAdjustmentGroup, List(String)),
  List(ProductUserError),
) {
  let reference_document_uri = read_string_field(input, "referenceDocumentUri")
  case validate_inventory_set_quantity_inputs(quantities) {
    [_, ..] as errors -> Error(errors)
    [] -> {
      let initial = #([], [], store)
      let result =
        quantities
        |> enumerate_items()
        |> list.try_fold(initial, fn(acc, pair) {
          let #(quantity, index) = pair
          let #(changes, mirrored_changes, current_store) = acc
          let assert Some(inventory_item_id) = quantity.inventory_item_id
          let assert Some(location_id) = quantity.location_id
          let assert Some(next_quantity) = quantity.quantity
          case
            stage_inventory_quantity_set(
              current_store,
              inventory_item_id,
              location_id,
              name,
              next_quantity,
              ignore_compare_quantity,
              quantity_compare_quantity(quantity, use_change_from_quantity),
              use_change_from_quantity,
              index,
            )
          {
            Error(error) -> Error([error])
            Ok(applied) -> {
              let #(next_store, delta) = applied
              let change =
                InventoryAdjustmentChange(
                  inventory_item_id: inventory_item_id,
                  location_id: location_id,
                  name: name,
                  delta: delta,
                  quantity_after_change: None,
                  ledger_document_uri: None,
                )
              let #(changes_to_append, mirrored) =
                inventory_set_quantity_changes(
                  inventory_item_id,
                  location_id,
                  name,
                  delta,
                  change,
                )
              Ok(#(
                list.append(changes, changes_to_append),
                list.append(mirrored_changes, mirrored),
                next_store,
              ))
            }
          }
        })
      case result {
        Error(errors) -> Error(errors)
        Ok(done) -> {
          let #(changes, mirrored_changes, next_store) = done
          let #(group, next_identity) =
            make_inventory_adjustment_group(
              identity,
              reason,
              reference_document_uri,
              list.append(changes, mirrored_changes),
            )
          let #(synced_store, synced_identity, product_ids) =
            sync_product_summaries_for_inventory_group(
              next_store,
              next_identity,
              group,
              RecomputeProductTotalInventory,
            )
          Ok(#(
            synced_store,
            synced_identity,
            group,
            list.append(inventory_adjustment_staged_ids(group), product_ids),
          ))
        }
      }
    }
  }
}

@internal
pub fn apply_inventory_move_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
  reason: String,
  changes: List(InventoryMoveQuantityInput),
) -> Result(
  #(Store, SyntheticIdentityRegistry, InventoryAdjustmentGroup, List(String)),
  List(ProductUserError),
) {
  let reference_document_uri = read_string_field(input, "referenceDocumentUri")
  let result =
    changes
    |> enumerate_items()
    |> list.try_fold(#([], store), fn(acc, pair) {
      let #(change, index) = pair
      let #(adjustment_changes, current_store) = acc
      case stage_inventory_quantity_move(current_store, change, index) {
        Error(error) -> Error([error])
        Ok(applied) -> {
          let #(next_store, from_change, to_change) = applied
          Ok(#(
            list.append(adjustment_changes, [from_change, to_change]),
            next_store,
          ))
        }
      }
    })
  case result {
    Error(errors) -> Error(errors)
    Ok(done) -> {
      let #(adjustment_changes, next_store) = done
      let #(group, next_identity) =
        make_inventory_adjustment_group(
          identity,
          reason,
          reference_document_uri,
          adjustment_changes,
        )
      let #(synced_store, synced_identity, product_ids) =
        sync_product_summaries_for_inventory_group(
          next_store,
          next_identity,
          group,
          RecomputeProductTotalInventory,
        )
      Ok(#(
        synced_store,
        synced_identity,
        group,
        list.append(inventory_adjustment_staged_ids(group), product_ids),
      ))
    }
  }
}
