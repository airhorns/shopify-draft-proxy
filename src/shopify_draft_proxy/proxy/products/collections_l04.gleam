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
  collection_rule_set_has_rules, product_already_in_collection,
}
import shopify_draft_proxy/proxy/products/collections_l01.{
  apply_collection_product_moves, collection_handle_validation_errors,
  collection_is_smart, collection_title_validation_errors,
  read_collection_rule_set,
}
import shopify_draft_proxy/proxy/products/collections_l02.{
  collection_handle_should_dedupe, slugify_collection_handle,
}
import shopify_draft_proxy/proxy/products/collections_l03.{
  collection_type_update_errors, ensure_unique_collection_handle,
  read_collection_product_moves, stage_collection_product_memberships,
}
import shopify_draft_proxy/proxy/products/products_l01.{
  enumerate_items, normalize_product_handle, updated_product_seo,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  dedupe_preserving_order, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  read_non_empty_string_field,
}
import shopify_draft_proxy/proxy/products/types.{
  type CollectionProductMove, type CollectionProductPlacement,
  type ProductUserError, CollectionProductMove, ProductUserError,
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
pub fn collection_update_validation_errors(
  collection: CollectionRecord,
  input: Dict(String, ResolvedValue),
) -> List(ProductUserError) {
  let title_errors = case read_string_field(input, "title") {
    Some(title) -> collection_title_validation_errors(title)
    _ -> []
  }
  list.append(
    title_errors,
    list.append(
      collection_handle_validation_errors(input),
      collection_type_update_errors(collection, input),
    ),
  )
}

@internal
pub fn add_products_to_collection(
  store: Store,
  collection: CollectionRecord,
  product_ids: List(String),
  placement: CollectionProductPlacement,
) -> #(Store, Option(CollectionRecord), List(ProductUserError)) {
  let normalized_product_ids = dedupe_preserving_order(product_ids)
  case normalized_product_ids {
    [] -> #(store, None, [
      ProductUserError(
        ["productIds"],
        "At least one product id is required",
        None,
      ),
    ])
    _ ->
      case collection_is_smart(collection) {
        True -> #(store, None, [
          ProductUserError(
            ["id"],
            "Can't manually add products to a smart collection",
            None,
          ),
        ])
        False ->
          case
            list.find(normalized_product_ids, fn(product_id) {
              product_already_in_collection(store, collection.id, product_id)
            })
          {
            Ok(_) -> #(store, None, [
              ProductUserError(
                ["productIds"],
                "Product is already in the collection",
                None,
              ),
            ])
            Error(_) ->
              stage_collection_product_memberships(
                store,
                collection,
                normalized_product_ids,
                placement,
              )
          }
      }
  }
}

@internal
pub fn reorder_collection_products(
  store: Store,
  collection: CollectionRecord,
  raw_moves: List(Dict(String, ResolvedValue)),
) -> #(Store, List(ProductUserError)) {
  case
    collection.is_smart
    || case collection.sort_order {
      Some(sort_order) -> sort_order != "MANUAL"
      None -> False
    }
  {
    True -> #(store, [
      ProductUserError(
        ["id"],
        "Can't reorder products unless collection is manually sorted",
        Some("MANUALLY_SORTED_COLLECTION"),
      ),
    ])
    False -> {
      let #(moves, user_errors) = read_collection_product_moves(raw_moves)
      let ordered_entries =
        store.list_effective_products_for_collection(store, collection.id)
      let product_ids_in_collection =
        ordered_entries
        |> list.map(fn(entry) {
          let #(product, _) = entry
          product.id
        })
      let user_errors =
        list.fold(enumerate_items(moves), user_errors, fn(errors, entry) {
          let #(move, index) = entry
          let CollectionProductMove(id: product_id, new_position: _) = move
          case store.get_effective_product_by_id(store, product_id) {
            None ->
              list.append(errors, [
                ProductUserError(
                  ["moves", int.to_string(index), "id"],
                  "Product does not exist",
                  Some("INVALID_MOVE"),
                ),
              ])
            Some(_) ->
              case list.contains(product_ids_in_collection, product_id) {
                True -> errors
                False ->
                  list.append(errors, [
                    ProductUserError(
                      ["moves", int.to_string(index), "id"],
                      "Product is not in the collection",
                      Some("INVALID_MOVE"),
                    ),
                  ])
              }
          }
        })
      case user_errors {
        [] -> {
          let reordered_entries =
            apply_collection_product_moves(ordered_entries, moves)
          let next_store =
            reordered_entries
            |> enumerate_items()
            |> list.fold(store, fn(current_store, entry) {
              let #(#(product, _), position) = entry
              let next_memberships =
                store.list_effective_collections_for_product(
                  current_store,
                  product.id,
                )
                |> list.map(fn(collection_entry) {
                  let #(existing_collection, membership) = collection_entry
                  case existing_collection.id == collection.id {
                    True ->
                      ProductCollectionRecord(..membership, position: position)
                    False -> membership
                  }
                })
              store.replace_staged_collections_for_product(
                current_store,
                product.id,
                next_memberships,
              )
            })
          #(next_store, [])
        }
        _ -> #(store, user_errors)
      }
    }
  }
}

@internal
pub fn created_collection_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, ResolvedValue),
) -> #(CollectionRecord, SyntheticIdentityRegistry) {
  let title =
    read_non_empty_string_field(input, "title")
    |> option.unwrap("Untitled collection")
  let #(updated_at, identity_after_timestamp) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_timestamp,
      "Collection",
    )
  let handle = case read_non_empty_string_field(input, "handle") {
    Some(handle) -> normalize_product_handle(handle)
    None -> slugify_collection_handle(title)
  }
  let handle = case collection_handle_should_dedupe(input) {
    True -> ensure_unique_collection_handle(store, handle)
    False -> handle
  }
  let rule_set = read_collection_rule_set(input)
  #(
    CollectionRecord(
      id: id,
      legacy_resource_id: None,
      title: title,
      handle: handle,
      publication_ids: [],
      updated_at: Some(updated_at),
      description: read_string_field(input, "description"),
      description_html: read_string_field(input, "descriptionHtml")
        |> option.or(Some("")),
      image: None,
      sort_order: read_string_field(input, "sortOrder")
        |> option.or(Some("BEST_SELLING")),
      template_suffix: read_string_field(input, "templateSuffix"),
      seo: updated_product_seo(
        ProductSeoRecord(title: None, description: None),
        input,
      ),
      rule_set: rule_set,
      products_count: Some(0),
      is_smart: rule_set
        |> option.map(collection_rule_set_has_rules)
        |> option.unwrap(False),
      cursor: None,
      title_cursor: None,
      updated_at_cursor: None,
    ),
    next_identity,
  )
}
