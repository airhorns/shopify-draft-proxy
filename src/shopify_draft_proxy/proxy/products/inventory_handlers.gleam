//// Products-domain submodule: inventory_handlers.
//// Combines layered files: inventory_l07, inventory_l08, inventory_l09, inventory_l10, inventory_l11, inventory_l12, inventory_l13, inventory_l14, inventory_l15.

import gleam/dict.{type Dict}

import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field.{type ResolvedValue}

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SerializeConnectionConfig, SrcInt, SrcList,
  SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_field_response_key, get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, serialize_connection,
  src_object,
}

import shopify_draft_proxy/proxy/products/inventory_apply.{
  stage_inventory_quantity_adjust, stage_inventory_quantity_move,
  stage_inventory_quantity_set, sync_product_inventory_summary,
  sync_product_summaries_for_inventory_group,
}
import shopify_draft_proxy/proxy/products/inventory_core.{
  active_inventory_levels, find_inventory_level, find_inventory_level_target,
  invalid_inventory_set_quantity_name_error, inventory_activate_staged_ids,
  inventory_adjust_product_total_inventory_sync, inventory_adjustment_app_source,
  inventory_change_location, inventory_item_variant_cursor,
  inventory_level_is_active, inventory_set_quantity_changes,
  product_set_inventory_level_id, product_set_inventory_location,
  quantity_compare_quantity, quantity_source, reactivate_inventory_level,
  read_inventory_adjustment_change_inputs, read_inventory_set_quantity_inputs,
  replace_inventory_level, reverse_inventory_item_variants,
  valid_inventory_set_quantity_name, variant_inventory_levels,
}
import shopify_draft_proxy/proxy/products/inventory_validation.{
  filtered_inventory_item_variants, inventory_adjust_202604_contract_error,
  inventory_adjustment_staged_ids, inventory_item_source_with_variant,
  inventory_set_202604_contract_error, make_inventory_adjustment_group,
  read_inventory_move_quantity_inputs, read_variant_inventory_item,
  serialize_inventory_item_level_field, serialize_inventory_item_levels_field,
  stage_variant_inventory_levels, validate_inventory_adjust_inputs,
  validate_inventory_move_inputs, validate_inventory_set_quantity_inputs,
  variant_with_inventory_levels,
}
import shopify_draft_proxy/proxy/products/media_core.{
  variant_media_connection_source,
}
import shopify_draft_proxy/proxy/products/product_types.{
  type InventoryAdjustmentChange, type InventoryAdjustmentChangeInput,
  type InventoryAdjustmentGroup, type InventoryMoveQuantityInput,
  type InventorySetQuantityInput, type MutationFieldResult,
  type ProductUserError, InventoryAdjustmentChange, ProductUserError,
  RecomputeProductTotalInventory,
}
import shopify_draft_proxy/proxy/products/products_core.{enumerate_items}
import shopify_draft_proxy/proxy/products/selling_plans_core.{
  selling_plan_group_connection_source,
}
import shopify_draft_proxy/proxy/products/shared.{
  count_source, mutation_error_with_null_data_result, mutation_result,
  read_arg_object_list, read_bool_field, read_int_field,
  read_non_empty_string_field, read_string_argument, read_string_field,
  user_errors_source,
}
import shopify_draft_proxy/proxy/products/variants_helpers.{
  selected_option_source, variant_staged_ids,
}
import shopify_draft_proxy/proxy/products/variants_options_core.{
  optional_captured_json_source,
}
import shopify_draft_proxy/proxy/products/variants_validation.{
  variant_product_source,
}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type InventoryLevelRecord, type InventoryLocationRecord,
  type ProductVariantRecord, InventoryLevelRecord, InventoryLocationRecord,
  InventoryQuantityRecord, ProductVariantRecord,
}

// ===== from inventory_l07 =====
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

// ===== from inventory_l08 =====
@internal
pub fn product_variant_source_without_inventory(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  product_variant_source_with_inventory(store, variant, SrcNull)
}

// ===== from inventory_l09 =====
@internal
pub fn inventory_item_source(
  store: Store,
  variant: ProductVariantRecord,
) -> SourceValue {
  case variant.inventory_item {
    Some(item) ->
      inventory_item_source_with_variant(
        item,
        product_variant_source_without_inventory(store, variant),
      )
    None -> SrcNull
  }
}

// ===== from inventory_l10 =====
@internal
pub fn serialize_inventory_items_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let variants =
    filtered_inventory_item_variants(store, field, variables)
    |> reverse_inventory_item_variants(field, variables)
  let window =
    paginate_connection_items(
      variants,
      field,
      variables,
      inventory_item_variant_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: inventory_item_variant_cursor,
      serialize_node: fn(variant, node_field, _index) {
        project_graphql_value(
          inventory_item_source(store, variant),
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
pub fn serialize_inventory_item_object(
  store: Store,
  variant: ProductVariantRecord,
  selections: List(Selection),
  owner_field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case variant.inventory_item {
    Some(item) -> {
      let source = inventory_item_source(store, variant)
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          let value = case selection {
            Field(name: name, ..) ->
              case name.value {
                "inventoryLevels" ->
                  serialize_inventory_item_levels_field(
                    item,
                    selection,
                    variables,
                    fragments,
                  )
                "inventoryLevel" ->
                  serialize_inventory_item_level_field(
                    item,
                    selection,
                    variables,
                    fragments,
                  )
                _ -> project_graphql_field_value(source, selection, fragments)
              }
            _ -> project_graphql_field_value(source, owner_field, fragments)
          }
          #(key, value)
        }),
      )
    }
    None -> json.null()
  }
}

@internal
pub fn inventory_level_source_with_item(
  store: Store,
  variant: ProductVariantRecord,
  level: InventoryLevelRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("InventoryLevel")),
    #("id", SrcString(level.id)),
    #("isActive", graphql_helpers.option_bool_source(level.is_active)),
    #(
      "location",
      src_object([
        #("__typename", SrcString("Location")),
        #("id", SrcString(level.location.id)),
        #("name", SrcString(level.location.name)),
      ]),
    ),
    #("quantities", SrcList(list.map(level.quantities, quantity_source))),
    #("item", inventory_item_source(store, variant)),
  ])
}

@internal
pub fn inventory_item_update_payload(
  store: Store,
  variant: Option(ProductVariantRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let inventory_item = case variant {
    Some(variant) -> inventory_item_source(store, variant)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryItemUpdatePayload")),
      #("inventoryItem", inventory_item),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn inventory_adjustment_change_source(
  store: Store,
  change: InventoryAdjustmentChange,
) -> SourceValue {
  let location = inventory_change_location(store, change)
  let item = case
    store.find_effective_variant_by_inventory_item_id(
      store,
      change.inventory_item_id,
    )
  {
    Some(variant) -> inventory_item_source(store, variant)
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("InventoryChange")),
    #("name", SrcString(change.name)),
    #("delta", SrcInt(change.delta)),
    #(
      "quantityAfterChange",
      graphql_helpers.option_int_source(change.quantity_after_change),
    ),
    #(
      "ledgerDocumentUri",
      graphql_helpers.option_string_source(change.ledger_document_uri),
    ),
    #("item", item),
    #(
      "location",
      src_object([
        #("__typename", SrcString("Location")),
        #("id", SrcString(location.id)),
        #("name", SrcString(location.name)),
      ]),
    ),
  ])
}

// ===== from inventory_l11 =====
@internal
pub fn serialize_inventory_item_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case store.find_effective_variant_by_inventory_item_id(store, id) {
        Some(variant) ->
          serialize_inventory_item_object(
            store,
            variant,
            get_selected_child_fields(field, default_selected_field_options()),
            field,
            variables,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn serialize_inventory_level_root(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  case read_string_argument(field, variables, "id") {
    Some(id) ->
      case find_inventory_level_target(store, id) {
        Some(#(variant, level)) ->
          project_graphql_value(
            inventory_level_source_with_item(store, variant, level),
            get_selected_child_fields(field, default_selected_field_options()),
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn inventory_activate_payload(
  store: Store,
  resolved: Option(#(ProductVariantRecord, InventoryLevelRecord)),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let inventory_level = case resolved {
    Some(#(variant, level)) ->
      inventory_level_source_with_item(store, variant, level)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryActivatePayload")),
      #("inventoryLevel", inventory_level),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn inventory_bulk_toggle_activation_payload(
  store: Store,
  variant: Option(ProductVariantRecord),
  levels: Option(List(#(ProductVariantRecord, InventoryLevelRecord))),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let inventory_item = case variant {
    Some(variant) -> inventory_item_source(store, variant)
    None -> SrcNull
  }
  let inventory_levels = case levels {
    Some(levels) ->
      SrcList(
        list.map(levels, fn(level) {
          let #(variant, level) = level
          inventory_level_source_with_item(store, variant, level)
        }),
      )
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("InventoryBulkToggleActivationPayload")),
      #("inventoryItem", inventory_item),
      #("inventoryLevels", inventory_levels),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn inventory_item_update_missing_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  mutation_result(
    key,
    inventory_item_update_payload(
      store,
      None,
      [
        ProductUserError(
          ["id"],
          "The product couldn't be updated because it does not exist.",
          None,
        ),
      ],
      field,
      fragments,
    ),
    store,
    identity,
    [],
  )
}

@internal
pub fn inventory_adjustment_group_source(
  store: Store,
  group: Option(InventoryAdjustmentGroup),
) -> SourceValue {
  case group {
    None -> SrcNull
    Some(group) ->
      src_object([
        #("__typename", SrcString("InventoryAdjustmentGroup")),
        #("id", SrcString(group.id)),
        #("createdAt", SrcString(group.created_at)),
        #("reason", SrcString(group.reason)),
        #(
          "referenceDocumentUri",
          graphql_helpers.option_string_source(group.reference_document_uri),
        ),
        #("app", inventory_adjustment_app_source()),
        #(
          "changes",
          SrcList(
            list.map(group.changes, fn(change) {
              inventory_adjustment_change_source(store, change)
            }),
          ),
        ),
      ])
  }
}

// ===== from inventory_l12 =====
fn known_inventory_location(
  store: Store,
  location_id: String,
) -> Option(InventoryLocationRecord) {
  case store.get_effective_location_by_id(store, location_id) {
    Some(location) ->
      Some(InventoryLocationRecord(id: location.id, name: location.name))
    None -> {
      store.list_effective_product_variants(store)
      |> list.filter_map(fn(variant) {
        case
          find_inventory_level(variant_inventory_levels(variant), location_id)
        {
          Some(level) -> Ok(level.location)
          None -> Error(Nil)
        }
      })
      |> list.first
      |> option.from_result
    }
  }
}

fn negative_inventory_activate_quantity_errors(
  available: Option(Int),
  on_hand: Option(Int),
) -> List(ProductUserError) {
  let available_errors = case available {
    Some(quantity) if quantity < 0 -> [
      ProductUserError(
        ["available"],
        "Available must be greater than or equal to 0",
        Some("NEGATIVE"),
      ),
    ]
    _ -> []
  }
  let on_hand_errors = case on_hand {
    Some(quantity) if quantity < 0 -> [
      ProductUserError(
        ["onHand"],
        "On hand must be greater than or equal to 0",
        Some("NEGATIVE"),
      ),
    ]
    _ -> []
  }
  list.append(available_errors, on_hand_errors)
}

fn inventory_activate_not_found_error(field: List(String)) -> ProductUserError {
  ProductUserError(
    field,
    "The product couldn't be stocked because the location wasn't found.",
    Some("NOT_FOUND"),
  )
}

fn inventory_activate_item_not_found_error() -> ProductUserError {
  ProductUserError(
    ["inventoryItemId"],
    "Inventory item does not exist",
    Some("NOT_FOUND"),
  )
}

fn inventory_activate_taken_error() -> ProductUserError {
  ProductUserError(
    ["locationId"],
    "Inventory level has already been taken",
    Some("TAKEN"),
  )
}

fn new_inventory_activation_level(
  store: Store,
  inventory_item_id: String,
  location_id: String,
) -> InventoryLevelRecord {
  InventoryLevelRecord(
    id: product_set_inventory_level_id(inventory_item_id, location_id),
    cursor: None,
    is_active: Some(True),
    location: product_set_inventory_location(store, None, location_id),
    quantities: [
      InventoryQuantityRecord(name: "available", quantity: 0, updated_at: None),
      InventoryQuantityRecord(name: "on_hand", quantity: 0, updated_at: None),
      InventoryQuantityRecord(name: "incoming", quantity: 0, updated_at: None),
      InventoryQuantityRecord(name: "reserved", quantity: 0, updated_at: None),
    ],
  )
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
  let available = read_int_field(args, "available")
  let on_hand = read_int_field(args, "onHand")
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
  let location = case location_id {
    Some(location_id) -> known_inventory_location(store, location_id)
    None -> None
  }
  let user_errors = case inventory_item_id, location_id, variant {
    None, _, _ -> [inventory_activate_item_not_found_error()]
    Some(_), _, None -> [inventory_activate_item_not_found_error()]
    _, None, _ -> [inventory_activate_not_found_error(["locationId"])]
    _, _, _ ->
      case location {
        None -> [inventory_activate_not_found_error(["locationId"])]
        Some(_) -> {
          let quantity_errors =
            negative_inventory_activate_quantity_errors(available, on_hand)
          case resolved, inventory_item_id, location_id, available_supplied {
            Some(#(_, level)), Some(item_id), Some(location_id), _ -> {
              case
                inventory_level_is_active(level)
                && level.id
                == product_set_inventory_level_id(item_id, location_id)
              {
                True -> [inventory_activate_taken_error()]
                False ->
                  case available_supplied {
                    True ->
                      case inventory_level_is_active(level) {
                        True -> [
                          ProductUserError(
                            ["available"],
                            "Not allowed to set available quantity when the item is already active at the location.",
                            None,
                          ),
                        ]
                        False -> quantity_errors
                      }
                    False -> quantity_errors
                  }
              }
            }
            Some(#(_, level)), _, _, True ->
              case inventory_level_is_active(level) {
                True -> [
                  ProductUserError(
                    ["available"],
                    "Not allowed to set available quantity when the item is already active at the location.",
                    None,
                  ),
                ]
                False -> quantity_errors
              }
            _, _, _, _ -> {
              quantity_errors
            }
          }
        }
      }
  }
  let activation_result = case
    resolved,
    user_errors,
    inventory_item_id,
    location_id
  {
    Some(#(variant, level)), [], _, _ -> {
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
    None, [], Some(item_id), Some(location_id) -> {
      case variant, location {
        Some(variant), Some(_) -> {
          let next_level =
            new_inventory_activation_level(store, item_id, location_id)
          let next_levels =
            list.append(variant_inventory_levels(variant), [next_level])
          let next_variant = variant_with_inventory_levels(variant, next_levels)
          #(
            stage_variant_inventory_levels(store, variant, next_levels),
            identity,
            Some(#(next_variant, next_level)),
          )
        }
        _, _ -> #(store, identity, resolved)
      }
    }
    _, _, _, _ -> #(store, identity, resolved)
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
  let location = case location_id {
    Some(location_id) -> known_inventory_location(store, location_id)
    None -> None
  }
  let user_errors = case
    inventory_item_id,
    variant,
    location_id,
    location,
    target,
    activate
  {
    None, _, _, _, _, _ -> [
      ProductUserError(
        ["inventoryItemId"],
        "Inventory item does not exist",
        Some("ITEM_NOT_FOUND"),
      ),
    ]
    Some(_), None, _, _, _, _ -> [
      ProductUserError(
        ["inventoryItemId"],
        "Inventory item does not exist",
        Some("ITEM_NOT_FOUND"),
      ),
    ]
    Some(_), Some(_variant), Some(_location_id), None, None, _ -> [
      ProductUserError(
        ["inventoryItemUpdates", "0", "locationId"],
        "The quantity couldn't be updated because the location was not found.",
        Some("LOCATION_NOT_FOUND"),
      ),
    ]
    Some(_),
      Some(_variant),
      Some(_location_id),
      Some(_location),
      None,
      Some(False)
    -> [
      ProductUserError(
        ["inventoryItemUpdates", "0", "locationId"],
        "The quantity couldn't be updated because the location was not found.",
        Some("LOCATION_NOT_FOUND"),
      ),
    ]
    Some(_),
      Some(variant),
      Some(_location_id),
      _,
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
        False ->
          case
            list.length(
              active_inventory_levels(variant_inventory_levels(variant)),
            )
            <= 1
          {
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
    }
    _, _, _, _, _, _ -> []
  }
  let outcome = case
    target,
    activate,
    user_errors,
    inventory_item_id,
    location_id
  {
    Some(#(variant, level)), Some(False), [], _, _ -> {
      let next_level = InventoryLevelRecord(..level, is_active: Some(False))
      let next_levels =
        variant_inventory_levels(variant)
        |> replace_inventory_level(level.location.id, next_level)
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
    None, Some(True), [], Some(item_id), Some(location_id) -> {
      case variant, location {
        Some(variant), Some(_) -> {
          let next_level =
            new_inventory_activation_level(store, item_id, location_id)
          let next_levels =
            list.append(variant_inventory_levels(variant), [next_level])
          let next_store =
            stage_variant_inventory_levels(store, variant, next_levels)
          let next_variant =
            store.find_effective_variant_by_inventory_item_id(
              next_store,
              item_id,
            )
          #(
            next_store,
            next_variant,
            option.map(next_variant, fn(variant) { [#(variant, next_level)] }),
            case variant.inventory_item {
              Some(item) -> [next_level.id, item.id]
              None -> [next_level.id]
            },
          )
        }
        _, _ -> #(store, None, None, [])
      }
    }
    Some(#(variant, level)), _, [], _, _ -> #(
      store,
      Some(variant),
      Some([#(variant, level)]),
      case variant.inventory_item {
        Some(item) -> [level.id, item.id]
        None -> [level.id]
      },
    )
    _, _, [], _, _ -> #(store, variant, None, [])
    _, _, _, _, _ -> #(store, None, None, [])
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

// ===== from inventory_l13 =====
@internal
pub fn inventory_quantity_mutation_result(
  key: String,
  typename: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  group: Option(InventoryAdjustmentGroup),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
  staged_ids: List(String),
) -> MutationFieldResult {
  mutation_result(
    key,
    inventory_quantity_payload(
      typename,
      store,
      group,
      user_errors,
      field,
      fragments,
    ),
    store,
    identity,
    staged_ids,
  )
}

// ===== from inventory_l14 =====
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

// ===== from inventory_l15 =====
@internal
pub fn handle_inventory_set_quantities(
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
  let quantity_name = read_non_empty_string_field(input, "name")
  let reason = read_non_empty_string_field(input, "reason")
  let quantities = read_inventory_set_quantity_inputs(input)
  let ignore_compare_quantity =
    read_bool_field(input, "ignoreCompareQuantity") == Some(True)
  case
    inventory_set_202604_contract_error(
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
    None ->
      case quantity_name, reason, quantities {
        None, _, _ ->
          inventory_quantity_mutation_result(
            key,
            "InventorySetQuantitiesPayload",
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
        Some(name), _, _ -> {
          case valid_inventory_set_quantity_name(name) {
            False ->
              inventory_quantity_mutation_result(
                key,
                "InventorySetQuantitiesPayload",
                store,
                identity,
                None,
                [invalid_inventory_set_quantity_name_error()],
                field,
                fragments,
                [],
              )
            True ->
              handle_valid_inventory_set_quantities(
                store,
                identity,
                key,
                input,
                name,
                reason,
                quantities,
                ignore_compare_quantity,
                uses_202604_contract,
                field,
                fragments,
              )
          }
        }
      }
  }
}
