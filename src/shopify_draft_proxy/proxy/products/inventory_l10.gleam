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
  inventory_item_variant_cursor, quantity_source,
}
import shopify_draft_proxy/proxy/products/inventory_l01.{
  inventory_change_location, reverse_inventory_item_variants,
}
import shopify_draft_proxy/proxy/products/inventory_l02.{
  serialize_inventory_item_level_field,
}
import shopify_draft_proxy/proxy/products/inventory_l03.{
  filtered_inventory_item_variants, serialize_inventory_item_levels_field,
}
import shopify_draft_proxy/proxy/products/inventory_l09.{inventory_item_source}
import shopify_draft_proxy/proxy/products/shared_l01.{user_errors_source}
import shopify_draft_proxy/proxy/products/types.{
  type InventoryAdjustmentChange, type ProductUserError,
  InventoryAdjustmentChange, ProductUserError,
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
