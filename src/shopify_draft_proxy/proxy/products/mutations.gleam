//// Internal products-domain implementation split from proxy/products.gleam.

import gleam/dict.{type Dict}

import gleam/json
import gleam/list
import gleam/option.{None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/parse_operation

import shopify_draft_proxy/graphql/root_field.{
  type ResolvedValue, get_root_fields,
}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments,
}

import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, single_root_log_draft,
}

import shopify_draft_proxy/proxy/products/collections_core.{
  handle_collection_add_products, handle_collection_add_products_v2,
  handle_collection_delete, handle_collection_remove_products,
  handle_collection_reorder_products, handle_collection_update,
}
import shopify_draft_proxy/proxy/products/collections_serializers.{
  handle_collection_create,
}
import shopify_draft_proxy/proxy/products/hydration.{
  hydrate_products_for_live_hybrid_mutation,
}
import shopify_draft_proxy/proxy/products/inventory_apply.{
  handle_inventory_deactivate,
}
import shopify_draft_proxy/proxy/products/inventory_handlers.{
  handle_inventory_activate, handle_inventory_adjust_quantities,
  handle_inventory_bulk_toggle_activation, handle_inventory_item_update,
  handle_inventory_move_quantities, handle_inventory_set_quantities,
}
import shopify_draft_proxy/proxy/products/inventory_shipments_handlers.{
  handle_inventory_shipment_add_items, handle_inventory_shipment_create,
  handle_inventory_shipment_delete, handle_inventory_shipment_mark_in_transit,
  handle_inventory_shipment_receive, handle_inventory_shipment_remove_items,
  handle_inventory_shipment_set_tracking,
  handle_inventory_shipment_update_item_quantities,
}
import shopify_draft_proxy/proxy/products/inventory_transfers.{
  handle_inventory_transfer_mutation,
}
import shopify_draft_proxy/proxy/products/media_handlers.{
  handle_product_media_mutation, handle_product_variant_media_mutation,
}
import shopify_draft_proxy/proxy/products/products_handlers.{
  handle_product_change_status, handle_product_create, handle_product_duplicate,
  handle_product_set, handle_product_update, handle_tags_update,
}
import shopify_draft_proxy/proxy/products/products_validation.{
  handle_product_delete,
}
import shopify_draft_proxy/proxy/products/publications_feeds.{
  handle_bulk_product_resource_feedback_create, handle_product_bundle_mutation,
  handle_product_feed_create, handle_product_feed_delete,
  handle_product_full_sync, handle_product_variant_relationship_bulk_update,
  handle_shop_resource_feedback_create,
}
import shopify_draft_proxy/proxy/products/publications_handlers.{
  handle_combined_listing_update, handle_product_publication_mutation,
  handle_publishable_publication_mutation,
}
import shopify_draft_proxy/proxy/products/publications_publishable.{
  handle_publication_mutation,
}
import shopify_draft_proxy/proxy/products/selling_plans_handlers.{
  handle_product_selling_plan_group_mutation, handle_selling_plan_group_mutation,
}
import shopify_draft_proxy/proxy/products/shared_money.{
  admin_api_version_at_least,
}
import shopify_draft_proxy/proxy/products/variants_handlers.{
  handle_product_option_update, handle_product_options_create,
  handle_product_options_delete, handle_product_options_reorder,
  handle_product_variant_create, handle_product_variant_update,
  handle_product_variants_bulk_create, handle_product_variants_bulk_delete,
  handle_product_variants_bulk_reorder, handle_product_variants_bulk_update,
}
import shopify_draft_proxy/proxy/products/variants_validation.{
  handle_product_variant_delete,
}

import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

@internal
pub fn is_products_mutation_root(name: String) -> Bool {
  case name {
    "productOptionsCreate"
    | "productCreate"
    | "productOptionUpdate"
    | "productOptionsDelete"
    | "productOptionsReorder"
    | "productChangeStatus"
    | "productDelete"
    | "productUpdate"
    | "productDuplicate"
    | "productSet"
    | "productVariantCreate"
    | "productVariantUpdate"
    | "productVariantDelete"
    | "productVariantsBulkCreate"
    | "productVariantsBulkUpdate"
    | "productVariantsBulkDelete"
    | "productVariantsBulkReorder"
    | "inventoryAdjustQuantities"
    | "inventoryActivate"
    | "inventoryDeactivate"
    | "inventoryBulkToggleActivation"
    | "inventoryItemUpdate"
    | "inventorySetQuantities"
    | "inventoryMoveQuantities"
    | "collectionAddProducts"
    | "collectionAddProductsV2"
    | "collectionRemoveProducts"
    | "collectionReorderProducts"
    | "collectionUpdate"
    | "collectionDelete"
    | "collectionCreate"
    | "productPublish"
    | "productUnpublish"
    | "publicationCreate"
    | "publicationUpdate"
    | "publicationDelete"
    | "publishablePublish"
    | "publishableUnpublish"
    | "productFeedCreate"
    | "productFeedDelete"
    | "productFullSync"
    | "productBundleCreate"
    | "productBundleUpdate"
    | "combinedListingUpdate"
    | "productVariantRelationshipBulkUpdate"
    | "productCreateMedia"
    | "productUpdateMedia"
    | "productDeleteMedia"
    | "productReorderMedia"
    | "productVariantAppendMedia"
    | "productVariantDetachMedia"
    | "bulkProductResourceFeedbackCreate"
    | "inventoryShipmentCreate"
    | "inventoryShipmentCreateInTransit"
    | "inventoryShipmentAddItems"
    | "inventoryShipmentRemoveItems"
    | "inventoryShipmentReceive"
    | "inventoryShipmentUpdateItemQuantities"
    | "inventoryShipmentSetTracking"
    | "inventoryShipmentMarkInTransit"
    | "inventoryShipmentDelete"
    | "inventoryTransferCreate"
    | "inventoryTransferCreateAsReadyToShip"
    | "inventoryTransferEdit"
    | "inventoryTransferSetItems"
    | "inventoryTransferRemoveItems"
    | "inventoryTransferMarkAsReadyToShip"
    | "inventoryTransferDuplicate"
    | "inventoryTransferCancel"
    | "inventoryTransferDelete"
    | "shopResourceFeedbackCreate"
    | "sellingPlanGroupCreate"
    | "sellingPlanGroupUpdate"
    | "sellingPlanGroupDelete"
    | "sellingPlanGroupAddProducts"
    | "sellingPlanGroupRemoveProducts"
    | "sellingPlanGroupAddProductVariants"
    | "sellingPlanGroupRemoveProductVariants"
    | "productJoinSellingPlanGroups"
    | "productLeaveSellingPlanGroups"
    | "productVariantJoinSellingPlanGroups"
    | "productVariantLeaveSellingPlanGroups"
    | "tagsAdd"
    | "tagsRemove" -> True
    _ -> False
  }
}

/// True iff any string in the request's resolved root-field arguments
/// or in the variables dict points at a product already present in
/// local state, or at a proxy-synthetic gid. This gates LiveHybrid
/// passthrough for cold upstream `product` / `productByIdentifier`
/// reads while keeping staged read-after-write flows fully local.
///
/// We must scan resolved arguments — not just variable values —
/// because callers frequently embed proxy-synthetic gids as inline
/// string literals (`product(id: "gid://shopify/Product/N?shopify-
/// draft-proxy=synthetic")`). Inline literals never appear in the
/// variables dict, so a variables-only check sends synthetic gids
/// upstream where they 404.
@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let operation_path = get_operation_path_label(document)
      let hydrated_store =
        hydrate_products_for_live_hybrid_mutation(store, variables, upstream)
      handle_mutation_fields(
        hydrated_store,
        identity,
        upstream.origin,
        document,
        operation_path,
        request_path,
        fields,
        fragments,
        variables,
      )
    }
  }
}

@internal
pub fn get_operation_path_label(document: String) -> String {
  case parse_operation.parse_operation(document) {
    Ok(parsed) -> {
      let kind = case parsed.type_ {
        parse_operation.QueryOperation -> "query"
        parse_operation.MutationOperation -> "mutation"
      }
      case parsed.name {
        Some(name) -> kind <> " " <> name
        None -> kind
      }
    }
    Error(_) -> "mutation"
  }
}

@internal
pub fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  shopify_admin_origin: String,
  document: String,
  operation_path: String,
  request_path: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationOutcome {
  let uses_inventory_quantity_202604_contract =
    admin_api_version_at_least(request_path, "2026-04")
  let initial = #([], [], store, identity, [], [])
  let #(
    data_entries,
    all_errors,
    final_store,
    final_identity,
    all_staged,
    all_drafts,
  ) =
    list.fold(fields, initial, fn(acc, field) {
      let #(
        entries,
        errors,
        current_store,
        current_identity,
        staged_ids,
        drafts,
      ) = acc
      case field {
        Field(name: name, ..) ->
          case name.value {
            "productOptionsCreate" -> {
              let result =
                handle_product_options_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged productOptionsCreate locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected productOptionsCreate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productCreate" -> {
              let result =
                handle_product_create(
                  current_store,
                  current_identity,
                  shopify_admin_origin,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(store_types.Staged, "Staged productCreate locally.")
                True -> #(
                  store_types.Failed,
                  "Rejected productCreate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productOptionUpdate" -> {
              let result =
                handle_product_option_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged productOptionUpdate locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected productOptionUpdate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productOptionsDelete" -> {
              let result =
                handle_product_options_delete(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged productOptionsDelete locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected productOptionsDelete locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productOptionsReorder" -> {
              let result =
                handle_product_options_reorder(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged productOptionsReorder locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productChangeStatus" -> {
              let result =
                handle_product_change_status(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged productChangeStatus locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productDelete" -> {
              let result =
                handle_product_delete(
                  current_store,
                  current_identity,
                  document,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged productDelete locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productUpdate" -> {
              let result =
                handle_product_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(store_types.Staged, "Staged productUpdate locally.")
                True -> #(
                  store_types.Failed,
                  "Rejected productUpdate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productDuplicate" -> {
              let result =
                handle_product_duplicate(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged productDuplicate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productSet" -> {
              let result =
                handle_product_set(
                  current_store,
                  current_identity,
                  shopify_admin_origin,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(store_types.Staged, "Staged productSet locally.")
                True -> #(
                  store_types.Failed,
                  "Rejected productSet locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productVariantCreate" -> {
              let result =
                handle_product_variant_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged productVariantCreate locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected productVariantCreate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantUpdate" -> {
              let result =
                handle_product_variant_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged productVariantUpdate locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected productVariantUpdate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantDelete" -> {
              let result =
                handle_product_variant_delete(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged productVariantDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantsBulkCreate" -> {
              let result =
                handle_product_variants_bulk_create(
                  current_store,
                  current_identity,
                  document,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged productVariantsBulkCreate locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected productVariantsBulkCreate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                case result.top_level_errors {
                  [] -> list.append(entries, [#(result.key, result.payload)])
                  _ -> list.append(entries, result.top_level_error_data_entries)
                },
                list.append(errors, result.top_level_errors),
                result.store,
                result.identity,
                case result.top_level_errors {
                  [] -> list.append(staged_ids, result.staged_resource_ids)
                  _ -> staged_ids
                },
                case result.top_level_errors {
                  [] -> list.append(drafts, [draft])
                  _ -> drafts
                },
              )
            }
            "productVariantsBulkUpdate" -> {
              let result =
                handle_product_variants_bulk_update(
                  current_store,
                  current_identity,
                  document,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged productVariantsBulkUpdate locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected productVariantsBulkUpdate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantsBulkDelete" -> {
              let result =
                handle_product_variants_bulk_delete(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged productVariantsBulkDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantsBulkReorder" -> {
              let result =
                handle_product_variants_bulk_reorder(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged productVariantsBulkReorder locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryAdjustQuantities" -> {
              let result =
                handle_inventory_adjust_quantities(
                  current_store,
                  current_identity,
                  uses_inventory_quantity_202604_contract,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryAdjustQuantities locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "inventoryActivate" -> {
              let result =
                handle_inventory_activate(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryActivate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryDeactivate" -> {
              let result =
                handle_inventory_deactivate(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryDeactivate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryBulkToggleActivation" -> {
              let result =
                handle_inventory_bulk_toggle_activation(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryBulkToggleActivation locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryItemUpdate" -> {
              let result =
                handle_inventory_item_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryItemUpdate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventorySetQuantities" -> {
              let result =
                handle_inventory_set_quantities(
                  current_store,
                  current_identity,
                  uses_inventory_quantity_202604_contract,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventorySetQuantities locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "inventoryMoveQuantities" -> {
              let result =
                handle_inventory_move_quantities(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryMoveQuantities locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionAddProducts" -> {
              let result =
                handle_collection_add_products(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged collectionAddProducts locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionAddProductsV2" -> {
              let result =
                handle_collection_add_products_v2(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged collectionAddProductsV2 locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionCreate" -> {
              let result =
                handle_collection_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged collectionCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionRemoveProducts" -> {
              let result =
                handle_collection_remove_products(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged collectionRemoveProducts locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionReorderProducts" -> {
              let result =
                handle_collection_reorder_products(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged collectionReorderProducts locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionUpdate" -> {
              let result =
                handle_collection_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged collectionUpdate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "collectionDelete" -> {
              let result =
                handle_collection_delete(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged collectionDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productPublish" -> {
              let result =
                handle_product_publication_mutation(
                  current_store,
                  current_identity,
                  "ProductPublishPayload",
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged productPublish locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productUnpublish" -> {
              let result =
                handle_product_publication_mutation(
                  current_store,
                  current_identity,
                  "ProductUnpublishPayload",
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged productUnpublish locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "publicationCreate" | "publicationUpdate" | "publicationDelete" -> {
              let result =
                handle_publication_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged " <> name.value <> " locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "publishablePublish" | "publishableUnpublish" -> {
              let result =
                handle_publishable_publication_mutation(
                  current_store,
                  current_identity,
                  name.value == "publishablePublish",
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged " <> name.value <> " locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productFeedCreate" -> {
              let result =
                handle_product_feed_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged productFeedCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productFeedDelete" -> {
              let result =
                handle_product_feed_delete(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged productFeedDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productFullSync" -> {
              let result =
                handle_product_full_sync(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged productFullSync locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected productFullSync locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productBundleCreate" | "productBundleUpdate" -> {
              let result =
                handle_product_bundle_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged captured Product bundle operation locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected Product bundle mutation locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "combinedListingUpdate" -> {
              let result =
                handle_combined_listing_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged captured combinedListingUpdate guardrails locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected combinedListingUpdate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productVariantRelationshipBulkUpdate" -> {
              let result =
                handle_product_variant_relationship_bulk_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged captured ProductVariant relationship guardrails locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected productVariantRelationshipBulkUpdate locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "productCreateMedia"
            | "productUpdateMedia"
            | "productDeleteMedia"
            | "productReorderMedia" -> {
              let result =
                handle_product_media_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  document,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged " <> name.value <> " locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productVariantAppendMedia" | "productVariantDetachMedia" -> {
              let result =
                handle_product_variant_media_mutation(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged ProductVariant media membership locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "bulkProductResourceFeedbackCreate" -> {
              let result =
                handle_bulk_product_resource_feedback_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged bulkProductResourceFeedbackCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentCreate" | "inventoryShipmentCreateInTransit" -> {
              let result =
                handle_inventory_shipment_create(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged " <> name.value <> " locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentSetTracking" -> {
              let result =
                handle_inventory_shipment_set_tracking(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryShipmentSetTracking locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentMarkInTransit" -> {
              let result =
                handle_inventory_shipment_mark_in_transit(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryShipmentMarkInTransit locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentAddItems" -> {
              let result =
                handle_inventory_shipment_add_items(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryShipmentAddItems locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentRemoveItems" -> {
              let result =
                handle_inventory_shipment_remove_items(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryShipmentRemoveItems locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentReceive" -> {
              let result =
                handle_inventory_shipment_receive(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryShipmentReceive locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentUpdateItemQuantities" -> {
              let result =
                handle_inventory_shipment_update_item_quantities(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryShipmentUpdateItemQuantities locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryShipmentDelete" -> {
              let result =
                handle_inventory_shipment_delete(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged inventoryShipmentDelete locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "inventoryTransferCreate"
            | "inventoryTransferCreateAsReadyToShip"
            | "inventoryTransferEdit"
            | "inventoryTransferSetItems"
            | "inventoryTransferRemoveItems"
            | "inventoryTransferMarkAsReadyToShip"
            | "inventoryTransferDuplicate"
            | "inventoryTransferCancel"
            | "inventoryTransferDelete" -> {
              let result =
                handle_inventory_transfer_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged " <> name.value <> " locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "shopResourceFeedbackCreate" -> {
              let result =
                handle_shop_resource_feedback_create(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged shopResourceFeedbackCreate locally."),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "sellingPlanGroupCreate"
            | "sellingPlanGroupUpdate"
            | "sellingPlanGroupDelete"
            | "sellingPlanGroupAddProducts"
            | "sellingPlanGroupRemoveProducts"
            | "sellingPlanGroupAddProductVariants"
            | "sellingPlanGroupRemoveProductVariants" -> {
              let result =
                handle_selling_plan_group_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged " <> name.value <> " locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected "
                    <> name.value
                    <> " locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "productJoinSellingPlanGroups"
            | "productLeaveSellingPlanGroups"
            | "productVariantJoinSellingPlanGroups"
            | "productVariantLeaveSellingPlanGroups" -> {
              let result =
                handle_product_selling_plan_group_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              let #(entry_status, note) = case result.staging_failed {
                False -> #(
                  store_types.Staged,
                  "Staged " <> name.value <> " locally.",
                )
                True -> #(
                  store_types.Failed,
                  "Rejected "
                    <> name.value
                    <> " locally with userErrors before staging.",
                )
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  entry_status,
                  "products",
                  "stage-locally",
                  Some(note),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                errors,
                result.store,
                result.identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
            "tagsAdd" -> {
              let result =
                handle_tags_update(
                  current_store,
                  current_identity,
                  True,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged tagsAdd locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            "tagsRemove" -> {
              let result =
                handle_tags_update(
                  current_store,
                  current_identity,
                  False,
                  field,
                  fragments,
                  variables,
                )
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  store_types.Staged,
                  "products",
                  "stage-locally",
                  Some("Staged tagsRemove locally."),
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> list.append(entries, result.top_level_error_data_entries)
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = case result.top_level_errors {
                [] -> list.append(drafts, [draft])
                _ -> drafts
              }
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                next_drafts,
              )
            }
            _ -> acc
          }
        _ -> acc
      }
    })
  let envelope = case all_errors, data_entries {
    [], _ -> json.object([#("data", json.object(data_entries))])
    _, [] -> json.object([#("errors", json.preprocessed_array(all_errors))])
    _, _ ->
      json.object([
        #("errors", json.preprocessed_array(all_errors)),
        #("data", json.object(data_entries)),
      ])
  }
  let final_staged_ids = case all_errors {
    [] -> all_staged
    _ -> []
  }
  MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: final_staged_ids,
    log_drafts: all_drafts,
  )
}
