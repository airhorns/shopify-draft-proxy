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
  find_inventory_level, inventory_missing_change_from_error,
  product_set_inventory_level_id, product_set_inventory_location,
}
import shopify_draft_proxy/proxy/products/inventory_l01.{
  active_inventory_levels, inventory_item_variants, quantity_problem,
  read_quantity_field,
}
import shopify_draft_proxy/proxy/products/inventory_l02.{
  filter_inventory_levels_by_include_inactive,
  group_product_set_quantities_by_location,
  has_duplicate_inventory_item_location_pair,
  inventory_item_variant_matches_positive_query_term,
  inventory_levels_connection_source, optional_measurement_source,
  read_inventory_measurement_input, sum_inventory_level_available,
  validate_inventory_move_input, variant_quantity_range_problems,
}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/products_l02.{
  apply_product_set_level_quantities,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  input_list_has_object_missing_field, missing_idempotency_key_error,
  read_bool_field, read_object_list_field, read_string_argument,
  read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  read_include_inactive_argument,
}
import shopify_draft_proxy/proxy/products/shared_l02.{has_idempotency_key}
import shopify_draft_proxy/proxy/products/types.{
  type InventoryMoveQuantityInput, type InventorySetQuantityInput,
  type ProductSetInventoryQuantityInput, type ProductUserError,
  type VariantValidationProblem, InventoryMoveQuantityInput,
  InventorySetQuantityInput, ProductSetInventoryQuantityInput, ProductUserError,
  QuantityFloat, QuantityInt, QuantityMissing, QuantityNotANumber, QuantityNull,
  VariantValidationProblem,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  product_search_parse_options,
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
pub fn filtered_inventory_item_variants(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductVariantRecord) {
  search_query_parser.apply_search_query(
    inventory_item_variants(store),
    read_string_argument(field, variables, "query"),
    product_search_parse_options(),
    inventory_item_variant_matches_positive_query_term,
  )
}

@internal
pub fn inventory_item_source_with_variant(
  item: InventoryItemRecord,
  variant: SourceValue,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("InventoryItem")),
    #("id", SrcString(item.id)),
    #("tracked", graphql_helpers.option_bool_source(item.tracked)),
    #(
      "requiresShipping",
      graphql_helpers.option_bool_source(item.requires_shipping),
    ),
    #("measurement", optional_measurement_source(item.measurement)),
    #(
      "countryCodeOfOrigin",
      graphql_helpers.option_string_source(item.country_code_of_origin),
    ),
    #(
      "provinceCodeOfOrigin",
      graphql_helpers.option_string_source(item.province_code_of_origin),
    ),
    #(
      "harmonizedSystemCode",
      graphql_helpers.option_string_source(item.harmonized_system_code),
    ),
    #(
      "inventoryLevels",
      inventory_levels_connection_source(active_inventory_levels(
        item.inventory_levels,
      )),
    ),
    #("variant", variant),
  ])
}

@internal
pub fn serialize_inventory_item_levels_field(
  item: InventoryItemRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let levels =
    filter_inventory_levels_by_include_inactive(
      item.inventory_levels,
      read_include_inactive_argument(field, variables),
    )
  project_graphql_field_value(
    src_object([
      #("inventoryLevels", inventory_levels_connection_source(levels)),
    ]),
    field,
    fragments,
  )
}

@internal
pub fn inventory_adjust_202604_contract_error(
  enabled: Bool,
  input: Dict(String, ResolvedValue),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(Json) {
  case enabled {
    False -> None
    True ->
      case
        input_list_has_object_missing_field(
          input,
          "changes",
          "changeFromQuantity",
        )
      {
        True ->
          Some(inventory_missing_change_from_error(
            field,
            "InventoryChangeInput",
          ))
        False ->
          case has_idempotency_key(field, variables) {
            True -> None
            False -> Some(missing_idempotency_key_error(field))
          }
      }
  }
}

@internal
pub fn inventory_set_202604_contract_error(
  enabled: Bool,
  input: Dict(String, ResolvedValue),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(Json) {
  case enabled {
    False -> None
    True ->
      case
        input_list_has_object_missing_field(
          input,
          "quantities",
          "changeFromQuantity",
        )
      {
        True ->
          Some(inventory_missing_change_from_error(
            field,
            "InventoryQuantityInput",
          ))
        False ->
          case has_idempotency_key(field, variables) {
            True -> None
            False -> Some(missing_idempotency_key_error(field))
          }
      }
  }
}

@internal
pub fn product_set_inventory_levels(
  store: Store,
  identity: SyntheticIdentityRegistry,
  inventory_item: InventoryItemRecord,
  inputs: List(ProductSetInventoryQuantityInput),
) -> #(List(InventoryLevelRecord), SyntheticIdentityRegistry) {
  inputs
  |> group_product_set_quantities_by_location
  |> list.fold(#([], identity), fn(acc, entry) {
    let #(levels, current_identity) = acc
    let #(location_id, location_inputs) = entry
    let existing =
      find_inventory_level(inventory_item.inventory_levels, location_id)
    let base_quantities = case existing {
      Some(level) -> level.quantities
      None -> []
    }
    let #(quantities, next_identity) =
      apply_product_set_level_quantities(
        current_identity,
        base_quantities,
        location_inputs,
      )
    let level =
      InventoryLevelRecord(
        id: existing
          |> option.map(fn(level) { level.id })
          |> option.unwrap(product_set_inventory_level_id(
            inventory_item.id,
            location_id,
          )),
        location: product_set_inventory_location(store, existing, location_id),
        quantities: quantities,
        is_active: Some(True),
        cursor: option.then(existing, fn(level) { level.cursor }),
      )
    #([level, ..levels], next_identity)
  })
  |> fn(result) {
    let #(levels, final_identity) = result
    #(list.reverse(levels), final_identity)
  }
}

@internal
pub fn inventory_quantity_list_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  read_object_list_field(input, "inventoryQuantities")
  |> list.flat_map(fn(quantity_input) {
    let path = ["inventoryQuantities"]
    case read_quantity_field(quantity_input, "availableQuantity") {
      QuantityInt(quantity) -> variant_quantity_range_problems(quantity, path)
      QuantityFloat(_) -> [
        quantity_problem(path, "Inventory quantity must be an integer"),
      ]
      QuantityNotANumber -> [
        quantity_problem(path, "Inventory quantity must be an integer"),
      ]
      QuantityMissing | QuantityNull ->
        case read_quantity_field(quantity_input, "quantity") {
          QuantityInt(quantity) ->
            variant_quantity_range_problems(quantity, path)
          QuantityFloat(_) -> [
            quantity_problem(path, "Inventory quantity must be an integer"),
          ]
          QuantityNotANumber -> [
            quantity_problem(path, "Inventory quantity must be an integer"),
          ]
          QuantityMissing | QuantityNull -> []
        }
    }
  })
}

@internal
pub fn read_variant_inventory_item(
  identity: SyntheticIdentityRegistry,
  input: Option(Dict(String, ResolvedValue)),
  existing: Option(InventoryItemRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry) {
  case input {
    None -> #(existing, identity)
    Some(input) -> {
      let #(id, next_identity) = case existing {
        Some(item) -> #(item.id, identity)
        None -> synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      }
      let current_levels = case existing {
        Some(item) -> item.inventory_levels
        None -> []
      }
      #(
        Some(InventoryItemRecord(
          id: id,
          tracked: read_bool_field(input, "tracked")
            |> option.or(option.then(existing, fn(item) { item.tracked })),
          requires_shipping: read_bool_field(input, "requiresShipping")
            |> option.or(
              option.then(existing, fn(item) { item.requires_shipping }),
            ),
          measurement: read_inventory_measurement_input(
            input,
            option.then(existing, fn(item) { item.measurement }),
          ),
          country_code_of_origin: read_string_field(
            input,
            "countryCodeOfOrigin",
          )
            |> option.or(
              option.then(existing, fn(item) { item.country_code_of_origin }),
            ),
          province_code_of_origin: read_string_field(
            input,
            "provinceCodeOfOrigin",
          )
            |> option.or(
              option.then(existing, fn(item) { item.province_code_of_origin }),
            ),
          harmonized_system_code: read_string_field(
            input,
            "harmonizedSystemCode",
          )
            |> option.or(
              option.then(existing, fn(item) { item.harmonized_system_code }),
            ),
          inventory_levels: current_levels,
        )),
        next_identity,
      )
    }
  }
}

@internal
pub fn duplicate_inventory_set_quantity_errors(
  quantities: List(InventorySetQuantityInput),
) -> List(ProductUserError) {
  quantities
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(quantity, index) = pair
    case quantity.inventory_item_id, quantity.location_id {
      Some(inventory_item_id), Some(location_id) -> {
        case
          has_duplicate_inventory_item_location_pair(
            quantities,
            index,
            inventory_item_id,
            location_id,
          )
        {
          True -> [
            ProductUserError(
              ["input", "quantities", int.to_string(index), "locationId"],
              "The combination of inventoryItemId and locationId must be unique.",
              Some("NO_DUPLICATE_INVENTORY_ITEM_ID_GROUP_ID_PAIR"),
            ),
          ]
          False -> []
        }
      }
      _, _ -> []
    }
  })
}

@internal
pub fn validate_inventory_move_inputs(
  changes: List(InventoryMoveQuantityInput),
) -> List(ProductUserError) {
  changes
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(change, index) = pair
    validate_inventory_move_input(change, index)
  })
}

@internal
pub fn variant_with_inventory_levels(
  variant: ProductVariantRecord,
  next_levels: List(InventoryLevelRecord),
) -> ProductVariantRecord {
  ProductVariantRecord(
    ..variant,
    inventory_quantity: sum_inventory_level_available(next_levels),
    inventory_item: option.map(variant.inventory_item, fn(item) {
      InventoryItemRecord(..item, inventory_levels: next_levels)
    }),
  )
}
