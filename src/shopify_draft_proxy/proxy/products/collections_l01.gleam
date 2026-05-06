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
import shopify_draft_proxy/proxy/products/collections_l00.{
  collection_rule_set_has_rules, compare_collections_by_sort_key,
  insert_collection_entry, serialize_collection_rule,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  dedupe_preserving_order, read_bool_argument, read_bool_field,
  read_object_field, read_object_list_field, read_string_argument,
  read_string_field, read_string_list_field, resource_tail,
}
import shopify_draft_proxy/proxy/products/types.{
  type CollectionProductMove, type ProductUserError, CollectionProductMove,
  ProductUserError, collection_handle_character_limit,
  collection_title_character_limit,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  optional_int_json, optional_string_json,
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
pub fn collection_by_identifier(
  store: Store,
  identifier: Dict(String, ResolvedValue),
) -> Option(CollectionRecord) {
  case read_string_field(identifier, "id") {
    Some(id) -> store.get_effective_collection_by_id(store, id)
    None ->
      case read_string_field(identifier, "handle") {
        Some(handle) -> store.get_effective_collection_by_handle(store, handle)
        None -> None
      }
  }
}

@internal
pub fn serialize_collection_image(
  image: Option(CollectionImageRecord),
  selections: List(Selection),
) -> Json {
  case image {
    None -> json.null()
    Some(image) ->
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          let value = case selection {
            Field(name: name, ..) ->
              case name.value {
                "id" -> optional_string_json(image.id)
                "altText" -> optional_string_json(image.alt_text)
                "url" | "src" | "originalSrc" | "transformedSrc" ->
                  optional_string_json(image.url)
                "width" -> optional_int_json(image.width)
                "height" -> optional_int_json(image.height)
                _ -> json.null()
              }
            _ -> json.null()
          }
          #(key, value)
        }),
      )
  }
}

@internal
pub fn serialize_collection_rule_set(
  rule_set: Option(CollectionRuleSetRecord),
  selections: List(Selection),
) -> Json {
  case rule_set {
    None -> json.null()
    Some(rule_set) ->
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          let value = case selection {
            Field(name: name, ..) ->
              case name.value {
                "appliedDisjunctively" ->
                  json.bool(rule_set.applied_disjunctively)
                "rules" ->
                  json.array(rule_set.rules, fn(rule) {
                    serialize_collection_rule(
                      rule,
                      get_selected_child_fields(
                        selection,
                        default_selected_field_options(),
                      ),
                    )
                  })
                _ -> json.null()
              }
            _ -> json.null()
          }
          #(key, value)
        }),
      )
  }
}

@internal
pub fn collection_has_product_id(
  store: Store,
  collection_id: String,
  raw_value: String,
) -> Bool {
  let normalized =
    search_query_parser.strip_search_query_value_quotes(raw_value)
    |> string.trim
  store.list_effective_products_for_collection(store, collection_id)
  |> list.any(fn(entry) {
    let #(product, _) = entry
    product.id == normalized
    || product.legacy_resource_id == Some(normalized)
    || resource_tail(product.id) == normalized
    || resource_tail(normalized) == resource_tail(product.id)
  })
}

@internal
pub fn sort_collections(
  collections: List(CollectionRecord),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(CollectionRecord) {
  let sort_key = read_string_argument(field, variables, "sortKey")
  let sorted =
    list.sort(collections, fn(left, right) {
      compare_collections_by_sort_key(left, right, sort_key)
    })
  case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(sorted)
    _ -> sorted
  }
}

@internal
pub fn collection_cursor_for_field(
  collection: CollectionRecord,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> String {
  case read_string_argument(field, variables, "sortKey") {
    Some("TITLE") ->
      collection.title_cursor
      |> option.unwrap(option.unwrap(collection.cursor, collection.id))
    Some("UPDATED_AT") ->
      collection.updated_at_cursor
      |> option.unwrap(option.unwrap(collection.cursor, collection.id))
    _ -> option.unwrap(collection.cursor, collection.id)
  }
}

@internal
pub fn collection_has_product(
  store: Store,
  collection_id: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Bool {
  case read_string_argument(field, variables, "id") {
    Some(product_id) ->
      store.list_effective_products_for_collection(store, collection_id)
      |> list.any(fn(entry) {
        let #(product, _) = entry
        product.id == product_id
      })
    None -> False
  }
}

@internal
pub fn collection_title_validation_errors(
  title: String,
) -> List(ProductUserError) {
  case string.length(title) > collection_title_character_limit {
    True -> [
      ProductUserError(
        ["title"],
        "Title is too long (maximum is 255 characters)",
        Some("INVALID"),
      ),
    ]
    False -> []
  }
}

@internal
pub fn collection_handle_validation_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  case read_string_field(input, "handle") {
    Some(handle) ->
      case string.length(handle) > collection_handle_character_limit {
        True -> [
          ProductUserError(
            ["handle"],
            "Handle is too long (maximum is 255 characters)",
            Some("INVALID"),
          ),
        ]
        False -> []
      }
    _ -> []
  }
}

@internal
pub fn collection_is_smart(collection: CollectionRecord) -> Bool {
  case collection.rule_set {
    Some(rule_set) -> collection_rule_set_has_rules(rule_set)
    None -> collection.is_smart
  }
}

@internal
pub fn remove_products_from_collection(
  store: Store,
  collection: CollectionRecord,
  product_ids: List(String),
) -> Store {
  let normalized_product_ids = dedupe_preserving_order(product_ids)
  list.fold(normalized_product_ids, store, fn(current_store, product_id) {
    case store.get_effective_product_by_id(current_store, product_id) {
      None -> current_store
      Some(_) -> {
        let next_memberships =
          store.list_effective_collections_for_product(
            current_store,
            product_id,
          )
          |> list.filter(fn(entry) {
            let #(existing_collection, _) = entry
            existing_collection.id != collection.id
          })
          |> list.map(fn(entry) {
            let #(_, membership) = entry
            membership
          })
        store.replace_staged_collections_for_product(
          current_store,
          product_id,
          next_memberships,
        )
      }
    }
  })
}

@internal
pub fn apply_collection_product_moves(
  entries: List(#(ProductRecord, ProductCollectionRecord)),
  moves: List(CollectionProductMove),
) -> List(#(ProductRecord, ProductCollectionRecord)) {
  list.fold(moves, entries, fn(current_entries, move) {
    let CollectionProductMove(id: product_id, new_position: new_position) = move
    case
      list.find(current_entries, fn(entry) {
        let #(product, _) = entry
        product.id == product_id
      })
    {
      Error(_) -> current_entries
      Ok(entry) -> {
        let without_entry =
          current_entries
          |> list.filter(fn(candidate) {
            let #(product, _) = candidate
            product.id != product_id
          })
        insert_collection_entry(without_entry, entry, new_position)
      }
    }
  })
}

@internal
pub fn read_collection_product_ids(
  input: Dict(String, ResolvedValue),
) -> List(String) {
  read_string_list_field(input, "products")
  |> option.unwrap([])
  |> dedupe_preserving_order
}

@internal
pub fn read_collection_rule_set(
  input: Dict(String, ResolvedValue),
) -> Option(CollectionRuleSetRecord) {
  use rule_set <- option.then(read_object_field(input, "ruleSet"))
  Some(CollectionRuleSetRecord(
    applied_disjunctively: read_bool_field(rule_set, "appliedDisjunctively")
      |> option.unwrap(False),
    rules: read_object_list_field(rule_set, "rules")
      |> list.filter_map(fn(rule) {
        case
          read_string_field(rule, "column"),
          read_string_field(rule, "relation"),
          read_string_field(rule, "condition")
        {
          Some(column), Some(relation), Some(condition) ->
            Ok(CollectionRuleRecord(
              column: column,
              relation: relation,
              condition: condition,
            ))
          _, _, _ -> Error(Nil)
        }
      }),
  ))
}
