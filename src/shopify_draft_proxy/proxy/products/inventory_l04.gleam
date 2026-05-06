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
  ensure_product_set_inventory_item,
}
import shopify_draft_proxy/proxy/products/inventory_l01.{
  product_set_available_quantity, quantity_problem, read_quantity_field,
}
import shopify_draft_proxy/proxy/products/inventory_l02.{
  read_product_set_inventory_quantity_inputs, validate_inventory_set_quantity,
  variant_quantity_range_problems,
}
import shopify_draft_proxy/proxy/products/inventory_l03.{
  duplicate_inventory_set_quantity_errors, inventory_item_source_with_variant,
  inventory_quantity_list_problems, product_set_inventory_levels,
  variant_with_inventory_levels,
}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/types.{
  type InventorySetQuantityInput, type ProductUserError,
  type VariantValidationProblem, InventorySetQuantityInput, ProductUserError,
  QuantityFloat, QuantityInt, QuantityMissing, QuantityNotANumber, QuantityNull,
  VariantValidationProblem,
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
pub fn inventory_item_source_without_variant(
  item: InventoryItemRecord,
) -> SourceValue {
  inventory_item_source_with_variant(item, SrcNull)
}

@internal
pub fn apply_product_set_inventory_quantities(
  store: Store,
  identity: SyntheticIdentityRegistry,
  variant: ProductVariantRecord,
  input: Dict(String, ResolvedValue),
) -> #(ProductVariantRecord, SyntheticIdentityRegistry) {
  let quantity_inputs = read_product_set_inventory_quantity_inputs(input)
  case quantity_inputs {
    [] -> #(variant, identity)
    _ -> {
      let #(inventory_item, identity_after_item) =
        ensure_product_set_inventory_item(identity, variant.inventory_item)
      let #(levels, next_identity) =
        product_set_inventory_levels(
          store,
          identity_after_item,
          inventory_item,
          quantity_inputs,
        )
      let available = product_set_available_quantity(quantity_inputs)
      #(
        ProductVariantRecord(
          ..variant,
          inventory_quantity: available |> option.or(variant.inventory_quantity),
          inventory_item: Some(
            InventoryItemRecord(..inventory_item, inventory_levels: levels),
          ),
        ),
        next_identity,
      )
    }
  }
}

@internal
pub fn variant_quantity_problems(
  input: Dict(String, ResolvedValue),
) -> List(VariantValidationProblem) {
  let direct_errors = case read_quantity_field(input, "inventoryQuantity") {
    QuantityInt(quantity) ->
      variant_quantity_range_problems(quantity, ["inventoryQuantity"])
    QuantityFloat(_) -> [
      quantity_problem(
        ["inventoryQuantity"],
        "Inventory quantity must be an integer",
      ),
    ]
    QuantityNotANumber -> [
      quantity_problem(
        ["inventoryQuantity"],
        "Inventory quantity must be an integer",
      ),
    ]
    QuantityMissing | QuantityNull -> []
  }
  list.append(direct_errors, inventory_quantity_list_problems(input))
}

@internal
pub fn validate_inventory_set_quantity_inputs(
  quantities: List(InventorySetQuantityInput),
) -> List(ProductUserError) {
  let input_errors =
    quantities
    |> enumerate_items()
    |> list.flat_map(fn(pair) {
      let #(quantity, index) = pair
      validate_inventory_set_quantity(quantity, index)
    })
  list.append(input_errors, duplicate_inventory_set_quantity_errors(quantities))
}

@internal
pub fn stage_variant_inventory_levels(
  store: Store,
  variant: ProductVariantRecord,
  next_levels: List(InventoryLevelRecord),
) -> Store {
  let next_variant = variant_with_inventory_levels(variant, next_levels)
  let next_variants =
    store.get_effective_variants_by_product_id(store, variant.product_id)
    |> list.map(fn(candidate) {
      case candidate.id == variant.id {
        True -> next_variant
        False -> candidate
      }
    })
  store.replace_staged_variants_for_product(
    store,
    variant.product_id,
    next_variants,
  )
}
