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
pub fn serialize_collection_rule(
  rule: CollectionRuleRecord,
  selections: List(Selection),
) -> Json {
  json.object(
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      let value = case selection {
        Field(name: name, ..) ->
          case name.value {
            "column" -> json.string(rule.column)
            "relation" -> json.string(rule.relation)
            "condition" -> json.string(rule.condition)
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

@internal
pub fn collections_published_to_publication(
  store: Store,
  publication_id: String,
) -> List(CollectionRecord) {
  store.list_effective_collections(store)
  |> list.filter(fn(collection) {
    list.contains(collection.publication_ids, publication_id)
  })
}

@internal
pub fn compare_collections_by_sort_key(
  left: CollectionRecord,
  right: CollectionRecord,
  sort_key: Option(String),
) -> order.Order {
  case sort_key {
    Some("TITLE") ->
      case string.compare(left.title, right.title) {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    Some("UPDATED_AT") ->
      case
        resource_ids.compare_nullable_strings(left.updated_at, right.updated_at)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    _ -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
  }
}

@internal
pub fn collection_product_cursor(
  entry: #(ProductRecord, ProductCollectionRecord),
  _index: Int,
) -> String {
  let #(_, membership) = entry
  membership.cursor |> option.unwrap(membership.product_id)
}

@internal
pub fn product_collection_cursor(
  entry: #(CollectionRecord, ProductCollectionRecord),
  _index: Int,
) -> String {
  let #(collection, membership) = entry
  membership.cursor |> option.unwrap(collection.id)
}

@internal
pub fn collection_products_count(
  store: Store,
  collection: CollectionRecord,
) -> Int {
  collection.products_count
  |> option.unwrap(
    list.length(store.list_effective_products_for_collection(
      store,
      collection.id,
    )),
  )
}

@internal
pub fn collection_rule_set_source(
  rule_set: Option(CollectionRuleSetRecord),
) -> SourceValue {
  case rule_set {
    None -> SrcNull
    Some(rule_set) ->
      src_object([
        #("appliedDisjunctively", SrcBool(rule_set.applied_disjunctively)),
        #(
          "rules",
          SrcList(
            list.map(rule_set.rules, fn(rule) {
              src_object([
                #("column", SrcString(rule.column)),
                #("relation", SrcString(rule.relation)),
                #("condition", SrcString(rule.condition)),
              ])
            }),
          ),
        ),
      ])
  }
}

@internal
pub fn collection_rule_set_has_rules(
  rule_set: CollectionRuleSetRecord,
) -> Bool {
  case rule_set.rules {
    [] -> False
    _ -> True
  }
}

@internal
pub fn insert_collection_entry(
  entries: List(#(ProductRecord, ProductCollectionRecord)),
  entry: #(ProductRecord, ProductCollectionRecord),
  position: Int,
) -> List(#(ProductRecord, ProductCollectionRecord)) {
  let insertion_index = int.min(position, list.length(entries))
  let before = list.take(entries, insertion_index)
  let after = list.drop(entries, insertion_index)
  list.append(before, [entry, ..after])
}

@internal
pub fn product_already_in_collection(
  store: Store,
  collection_id: String,
  product_id: String,
) -> Bool {
  store.list_effective_collections_for_product(store, product_id)
  |> list.any(fn(entry) {
    let #(collection, _) = entry
    collection.id == collection_id
  })
}
