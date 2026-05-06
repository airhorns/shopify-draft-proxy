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
pub fn location_cursor(location: LocationRecord, _index: Int) -> String {
  location.cursor |> option.unwrap(location.id)
}

@internal
pub fn location_source(location: LocationRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Location")),
    #("id", SrcString(location.id)),
    #("name", SrcString(location.name)),
  ])
}

@internal
pub fn inventory_item_variant_cursor(
  variant: ProductVariantRecord,
  _index: Int,
) -> String {
  case variant.inventory_item {
    Some(item) -> item.id
    None -> variant.id
  }
}

@internal
pub fn inventory_quantity_name_definitions() -> List(
  #(String, String, Bool, List(String), List(String)),
) {
  [
    #("available", "Available", True, ["on_hand"], []),
    #("committed", "Committed", True, ["on_hand"], []),
    #("damaged", "Damaged", False, ["on_hand"], []),
    #("incoming", "Incoming", False, [], []),
    #("on_hand", "On hand", True, [], [
      "available",
      "committed",
      "damaged",
      "quality_control",
      "reserved",
      "safety_stock",
    ]),
    #("quality_control", "Quality control", False, ["on_hand"], []),
    #("reserved", "Reserved", True, ["on_hand"], []),
    #("safety_stock", "Safety stock", False, ["on_hand"], []),
  ]
}

@internal
pub fn inventory_quantity_name_source(
  definition: #(String, String, Bool, List(String), List(String)),
) -> SourceValue {
  let #(name, display_name, is_in_use, belongs_to, comprises) = definition
  src_object([
    #("name", SrcString(name)),
    #("displayName", SrcString(display_name)),
    #("isInUse", SrcBool(is_in_use)),
    #("belongsTo", SrcList(list.map(belongs_to, SrcString))),
    #("comprises", SrcList(list.map(comprises, SrcString))),
  ])
}

@internal
pub fn quantity_source(quantity: InventoryQuantityRecord) -> SourceValue {
  src_object([
    #("name", SrcString(quantity.name)),
    #("quantity", SrcInt(quantity.quantity)),
    #("updatedAt", graphql_helpers.option_string_source(quantity.updated_at)),
  ])
}

@internal
pub fn inventory_weight_value_source(value) -> SourceValue {
  case value {
    InventoryWeightInt(value) -> SrcInt(value)
    InventoryWeightFloat(value) -> SrcFloat(value)
  }
}

@internal
pub fn inventory_level_cursor(
  level: InventoryLevelRecord,
  _index: Int,
) -> String {
  case level.cursor {
    Some(cursor) -> cursor
    None -> level.id
  }
}

@internal
pub fn inventory_missing_change_from_error(
  field: Selection,
  input_type: String,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        input_type
        <> " must include the following argument: changeFromQuantity.",
      ),
    ),
    #(
      "extensions",
      json.object([#("code", json.string("INVALID_FIELD_ARGUMENTS"))]),
    ),
    #("path", json.array([get_field_response_key(field)], json.string)),
  ])
}

@internal
pub fn inventory_item_legacy_id(inventory_item_id: String) -> String {
  let tail = case list.last(string.split(inventory_item_id, "/")) {
    Ok(value) -> value
    Error(_) -> inventory_item_id
  }
  case string.split(tail, "?") {
    [id, ..] -> id
    [] -> tail
  }
}

@internal
pub fn write_inventory_quantity_with_timestamp(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
  updated_at: Option(String),
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True ->
      list.map(quantities, fn(quantity) {
        case quantity.name == name {
          True ->
            InventoryQuantityRecord(
              ..quantity,
              quantity: amount,
              updated_at: updated_at,
            )
          False -> quantity
        }
      })
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(
          name: name,
          quantity: amount,
          updated_at: updated_at,
        ),
      ])
  }
}

@internal
pub fn replace_first_inventory_level(
  levels: List(InventoryLevelRecord),
  next_level: InventoryLevelRecord,
) -> List(InventoryLevelRecord) {
  case levels {
    [] -> [next_level]
    [_first, ..rest] -> [next_level, ..rest]
  }
}

@internal
pub fn locations_payload(
  loc: Option(Location),
  document: String,
) -> Option(Json) {
  option.map(loc, graphql_helpers.locations_json(_, document))
}

@internal
pub fn inventory_adjustment_app_source() -> SourceValue {
  src_object([
    #("__typename", SrcString("App")),
    #("id", SrcNull),
    #("title", SrcString("hermes-conformance-products")),
    #("apiKey", SrcNull),
    #("handle", SrcString("hermes-conformance-products")),
  ])
}

@internal
pub fn default_inventory_item_measurement() -> InventoryMeasurementRecord {
  InventoryMeasurementRecord(
    weight: Some(InventoryWeightRecord(
      unit: "KILOGRAMS",
      value: InventoryWeightInt(0),
    )),
  )
}

@internal
pub fn ensure_product_set_inventory_item(
  identity: SyntheticIdentityRegistry,
  inventory_item: Option(InventoryItemRecord),
) -> #(InventoryItemRecord, SyntheticIdentityRegistry) {
  case inventory_item {
    Some(item) -> #(item, identity)
    None -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        InventoryItemRecord(
          id: id,
          tracked: None,
          requires_shipping: None,
          measurement: None,
          country_code_of_origin: None,
          province_code_of_origin: None,
          harmonized_system_code: None,
          inventory_levels: [],
        ),
        next_identity,
      )
    }
  }
}

@internal
pub fn write_inventory_quantity(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
  updated_at: Option(String),
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True ->
      list.map(quantities, fn(quantity) {
        case quantity.name == name {
          True ->
            InventoryQuantityRecord(
              ..quantity,
              quantity: amount,
              updated_at: updated_at,
            )
          False -> quantity
        }
      })
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(
          name: name,
          quantity: amount,
          updated_at: updated_at,
        ),
      ])
  }
}

@internal
pub fn ensure_inventory_quantity(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True -> quantities
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(name: name, quantity: amount, updated_at: None),
      ])
  }
}

@internal
pub fn product_set_inventory_location(
  store: Store,
  existing: Option(InventoryLevelRecord),
  location_id: String,
) -> InventoryLocationRecord {
  case store.get_effective_location_by_id(store, location_id) {
    Some(location) ->
      InventoryLocationRecord(id: location.id, name: location.name)
    None ->
      case existing {
        Some(level) -> level.location
        None -> InventoryLocationRecord(id: location_id, name: "")
      }
  }
}

@internal
pub fn product_set_inventory_level_id(
  inventory_item_id: String,
  location_id: String,
) -> String {
  let inventory_tail =
    inventory_item_id |> string.split("/") |> list.last |> result.unwrap("0")
  let location_tail =
    location_id |> string.split("/") |> list.last |> result.unwrap("0")
  "gid://shopify/InventoryLevel/"
  <> inventory_tail
  <> "-"
  <> location_tail
  <> "?inventory_item_id="
  <> inventory_item_id
}

@internal
pub fn duplicate_inventory_item(
  identity: SyntheticIdentityRegistry,
  inventory_item: Option(InventoryItemRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry, List(String)) {
  case inventory_item {
    None -> #(None, identity, [])
    Some(record) -> {
      let #(inventory_item_id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        Some(InventoryItemRecord(..record, id: inventory_item_id)),
        next_identity,
        [inventory_item_id],
      )
    }
  }
}

@internal
pub fn valid_weight_unit(unit: String) -> Bool {
  case unit {
    "KILOGRAMS" | "GRAMS" | "POUNDS" | "OUNCES" -> True
    _ -> False
  }
}

@internal
pub fn read_inventory_weight_value_input(
  input: Dict(String, ResolvedValue),
) -> Option(InventoryWeightValue) {
  case dict.get(input, "value") {
    Ok(IntVal(value)) -> Some(InventoryWeightInt(value))
    Ok(FloatVal(value)) -> Some(InventoryWeightFloat(value))
    _ -> None
  }
}

@internal
pub fn clone_default_inventory_item(
  identity: SyntheticIdentityRegistry,
  item: Option(InventoryItemRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry) {
  case item {
    None -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        Some(
          InventoryItemRecord(
            id: id,
            tracked: Some(False),
            requires_shipping: Some(True),
            measurement: None,
            country_code_of_origin: None,
            province_code_of_origin: None,
            harmonized_system_code: None,
            inventory_levels: [],
          ),
        ),
        next_identity,
      )
    }
    Some(item) -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        Some(InventoryItemRecord(..item, id: id, tracked: Some(True))),
        next_identity,
      )
    }
  }
}

@internal
pub fn inventory_compare_field_name(use_change_from_quantity: Bool) -> String {
  case use_change_from_quantity {
    True -> "changeFromQuantity"
    False -> "compareQuantity"
  }
}

@internal
pub fn inventory_activate_staged_ids(
  resolved: Option(#(ProductVariantRecord, InventoryLevelRecord)),
) -> List(String) {
  case resolved {
    Some(#(variant, level)) ->
      case variant.inventory_item {
        Some(item) -> [level.id, item.id]
        None -> [level.id]
      }
    None -> []
  }
}

@internal
pub fn variant_inventory_levels(
  variant: ProductVariantRecord,
) -> List(InventoryLevelRecord) {
  case variant.inventory_item {
    Some(item) -> item.inventory_levels
    None -> []
  }
}

@internal
pub fn inventory_level_is_active(level: InventoryLevelRecord) -> Bool {
  case level.is_active {
    Some(False) -> False
    _ -> True
  }
}

@internal
pub fn normalize_inventory_item_id(id: String) -> String {
  case string.starts_with(id, "gid://shopify/InventoryItem/") {
    True -> id
    False -> "gid://shopify/InventoryItem/" <> id
  }
}

@internal
pub fn inventory_level_item_id(id: String) -> Option(String) {
  case string.split(id, "?inventory_item_id=") {
    [_, item_id] -> Some(item_id)
    _ -> None
  }
}

@internal
pub fn find_inventory_level(
  levels: List(InventoryLevelRecord),
  location_id: String,
) -> Option(InventoryLevelRecord) {
  levels
  |> list.find(fn(level) { level.location.id == location_id })
  |> option.from_result
}

@internal
pub fn replace_inventory_level(
  levels: List(InventoryLevelRecord),
  location_id: String,
  next_level: InventoryLevelRecord,
) -> List(InventoryLevelRecord) {
  list.map(levels, fn(level) {
    case level.location.id == location_id {
      True -> next_level
      False -> level
    }
  })
}

@internal
pub fn inventory_quantity_amount(
  quantities: List(InventoryQuantityRecord),
  name: String,
) -> Int {
  case list.find(quantities, fn(quantity) { quantity.name == name }) {
    Ok(quantity) -> quantity.quantity
    Error(_) -> 0
  }
}

@internal
pub fn write_inventory_quantity_amount(
  quantities: List(InventoryQuantityRecord),
  name: String,
  amount: Int,
) -> List(InventoryQuantityRecord) {
  case list.any(quantities, fn(quantity) { quantity.name == name }) {
    True ->
      list.map(quantities, fn(quantity) {
        case quantity.name == name {
          True -> InventoryQuantityRecord(..quantity, quantity: amount)
          False -> quantity
        }
      })
    False ->
      list.append(quantities, [
        InventoryQuantityRecord(name: name, quantity: amount, updated_at: None),
      ])
  }
}

@internal
pub fn is_on_hand_component_quantity_name(name: String) -> Bool {
  case name {
    "available"
    | "committed"
    | "damaged"
    | "quality_control"
    | "reserved"
    | "safety_stock" -> True
    _ -> False
  }
}

@internal
pub fn valid_inventory_set_quantity_name(name: String) -> Bool {
  case name {
    "available" | "on_hand" -> True
    _ -> False
  }
}

@internal
pub fn valid_staged_inventory_quantity_name(name: String) -> Bool {
  case name {
    "available"
    | "committed"
    | "damaged"
    | "incoming"
    | "quality_control"
    | "reserved"
    | "safety_stock" -> True
    _ -> False
  }
}

@internal
pub fn valid_inventory_adjust_quantity_name(name: String) -> Bool {
  case name {
    "available"
    | "on_hand"
    | "committed"
    | "damaged"
    | "incoming"
    | "quality_control"
    | "reserved"
    | "safety_stock" -> True
    _ -> False
  }
}

@internal
pub fn variant_tracks_inventory(variant: ProductVariantRecord) -> Bool {
  case variant.inventory_item {
    Some(item) ->
      case item.tracked {
        Some(True) -> True
        Some(False) -> False
        None -> variant.inventory_quantity != None
      }
    None -> False
  }
}

@internal
pub fn make_inventory_item_for_variant(
  identity: SyntheticIdentityRegistry,
  template: Option(ProductVariantRecord),
) -> #(Option(InventoryItemRecord), SyntheticIdentityRegistry) {
  let template_item =
    option.then(template, fn(variant) { variant.inventory_item })
  case template_item {
    Some(item) -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "InventoryItem")
      #(
        Some(InventoryItemRecord(..item, id: id, inventory_levels: [])),
        next_identity,
      )
    }
    None -> #(None, identity)
  }
}
