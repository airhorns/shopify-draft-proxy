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
import shopify_draft_proxy/proxy/products/collections_l01.{
  collection_handle_validation_errors, collection_is_smart,
  collection_title_validation_errors, remove_products_from_collection,
}
import shopify_draft_proxy/proxy/products/collections_l02.{
  collection_delete_payload, collection_matches_positive_query_term,
  collection_remove_products_payload, collection_rule_set_presence,
  read_collection_reorder_position,
}
import shopify_draft_proxy/proxy/products/products_l00.{ensure_unique_handle}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/products_l02.{
  dedup_base_and_next_suffix, enumerate_strings,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_arg_string_list, read_string_argument, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  mutation_result, read_non_empty_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type CollectionProductMove, type CollectionProductPlacement,
  type MutationFieldResult, type ProductUserError, AppendProducts,
  CollectionProductMove, MutationFieldResult, PrependReverseProducts,
  ProductUserError, RuleSetCustom, RuleSetSmart, blank_product_user_error,
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
pub fn filtered_collections(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(CollectionRecord) {
  search_query_parser.apply_search_query(
    store.list_effective_collections(store),
    read_string_argument(field, variables, "query"),
    product_search_parse_options(),
    fn(collection, term) {
      collection_matches_positive_query_term(store, collection, term)
    },
  )
}

@internal
pub fn collection_create_validation_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  let title_errors = case read_non_empty_string_field(input, "title") {
    None -> [blank_product_user_error(["title"], "Title can't be blank")]
    Some(title) -> collection_title_validation_errors(title)
  }
  list.append(title_errors, collection_handle_validation_errors(input))
}

@internal
pub fn collection_type_update_errors(
  collection: CollectionRecord,
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  case collection_is_smart(collection), collection_rule_set_presence(input) {
    False, RuleSetSmart -> [
      ProductUserError(
        ["id"],
        "Cannot update rule set of a custom collection",
        None,
      ),
    ]
    True, RuleSetCustom -> [
      ProductUserError(
        ["id"],
        "Cannot update rule set of a smart collection",
        None,
      ),
    ]
    _, _ -> []
  }
}

@internal
pub fn stage_collection_product_memberships(
  store: Store,
  collection: CollectionRecord,
  product_ids: List(String),
  placement: CollectionProductPlacement,
) -> #(Store, Option(CollectionRecord), List(ProductUserError)) {
  let existing_product_ids =
    product_ids
    |> list.filter(fn(product_id) {
      case store.get_effective_product_by_id(store, product_id) {
        Some(_) -> True
        None -> False
      }
    })
  case existing_product_ids {
    [] -> #(store, Some(collection), [])
    _ -> {
      let existing_positions =
        store.list_effective_products_for_collection(store, collection.id)
        |> list.map(fn(entry) {
          let #(_, membership) = entry
          membership.position
        })
      let first_position = case placement {
        AppendProducts ->
          case existing_positions {
            [] -> 0
            _ -> {
              list.fold(existing_positions, -1, int.max) + 1
            }
          }
        PrependReverseProducts ->
          case existing_positions {
            [] -> 0
            [first, ..rest] ->
              list.fold(rest, first, int.min)
              - list.length(existing_product_ids)
          }
      }
      let positioned_product_ids = case placement {
        AppendProducts -> existing_product_ids
        PrependReverseProducts -> list.reverse(existing_product_ids)
      }
      let existing_memberships =
        store.list_effective_products_for_collection(store, collection.id)
      let memberships =
        positioned_product_ids
        |> enumerate_strings()
        |> list.map(fn(entry) {
          let #(product_id, index) = entry
          ProductCollectionRecord(
            collection_id: collection.id,
            product_id: product_id,
            position: first_position + index,
            cursor: None,
          )
        })
      let next_count =
        list.length(existing_memberships) + list.length(memberships)
      let next_collection =
        CollectionRecord(..collection, products_count: Some(next_count))
      let next_store =
        store
        |> store.upsert_staged_collections([next_collection])
        |> store.upsert_staged_product_collections(memberships)
      #(next_store, Some(next_collection), [])
    }
  }
}

@internal
pub fn handle_collection_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "input")
  let collection_id = case input {
    Some(input) -> graphql_helpers.read_arg_string(input, "id")
    None -> None
  }
  case collection_id {
    None ->
      mutation_result(
        key,
        collection_delete_payload(
          None,
          [
            ProductUserError(["input", "id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_delete_payload(
              None,
              [
                ProductUserError(["input", "id"], "Collection not found", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let next_store = store.delete_staged_collection(store, collection_id)
          mutation_result(
            key,
            collection_delete_payload(Some(collection_id), [], field, fragments),
            next_store,
            identity,
            [collection_id],
          )
        }
      }
  }
}

@internal
pub fn handle_collection_remove_products(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    None ->
      mutation_result(
        key,
        collection_remove_products_payload(
          None,
          [
            ProductUserError(["id"], "Collection id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(collection_id) ->
      case store.get_effective_collection_by_id(store, collection_id) {
        None ->
          mutation_result(
            key,
            collection_remove_products_payload(
              None,
              [
                ProductUserError(["id"], "Collection not found", None),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(collection) ->
          case collection_is_smart(collection) {
            True ->
              mutation_result(
                key,
                collection_remove_products_payload(
                  None,
                  [
                    ProductUserError(
                      ["id"],
                      "Can't manually remove products from a smart collection",
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
            False -> {
              let next_store =
                remove_products_from_collection(
                  store,
                  collection,
                  read_arg_string_list(args, "productIds"),
                )
              let next_count =
                store.list_effective_products_for_collection(
                  next_store,
                  collection.id,
                )
                |> list.length
              let next_collection =
                CollectionRecord(..collection, products_count: Some(next_count))
              let next_store =
                store.upsert_staged_collections(next_store, [next_collection])
              let #(job_id, next_identity) =
                synthetic_identity.make_synthetic_gid(identity, "Job")
              mutation_result(
                key,
                collection_remove_products_payload(
                  Some(job_id),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                next_identity,
                [collection.id],
              )
            }
          }
      }
  }
}

@internal
pub fn read_collection_product_moves(
  raw_moves: List(Dict(String, ResolvedValue)),
) -> #(List(CollectionProductMove), List(ProductUserError)) {
  case raw_moves {
    [] -> #([], [
      ProductUserError(
        ["moves"],
        "At least one move is required",
        Some("INVALID_MOVE"),
      ),
    ])
    _ -> {
      let too_many_errors = case list.length(raw_moves) > 250 {
        True -> [
          ProductUserError(
            ["moves"],
            "Too many moves were provided",
            Some("INVALID_MOVE"),
          ),
        ]
        False -> []
      }
      let result =
        raw_moves
        |> enumerate_items()
        |> list.fold(#([], too_many_errors), fn(acc, entry) {
          let #(moves, errors) = acc
          let #(raw_move, index) = entry
          let product_id = read_string_field(raw_move, "id")
          let new_position = read_collection_reorder_position(raw_move)
          let errors = case product_id {
            Some(_) -> errors
            None ->
              list.append(errors, [
                ProductUserError(
                  ["moves", int.to_string(index), "id"],
                  "Product id is required",
                  Some("INVALID_MOVE"),
                ),
              ])
          }
          let errors = case new_position {
            Some(_) -> errors
            None ->
              list.append(errors, [
                ProductUserError(
                  ["moves", int.to_string(index), "newPosition"],
                  "Position is invalid",
                  Some("INVALID_MOVE"),
                ),
              ])
          }
          case product_id, new_position {
            Some(id), Some(position) -> #(
              list.append(moves, [
                CollectionProductMove(id: id, new_position: position),
              ]),
              errors,
            )
            _, _ -> #(moves, errors)
          }
        })
      result
    }
  }
}

@internal
pub fn ensure_unique_collection_handle(store: Store, handle: String) -> String {
  let in_use = fn(candidate) {
    store.get_effective_collection_by_handle(store, candidate) != None
  }
  case in_use(handle) {
    True -> {
      let #(base_handle, suffix) = dedup_base_and_next_suffix(handle)
      ensure_unique_handle(base_handle, suffix, in_use)
    }
    False -> handle
  }
}
