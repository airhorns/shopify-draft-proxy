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
import shopify_draft_proxy/proxy/products/collections_l03.{
  read_collection_product_moves,
}
import shopify_draft_proxy/proxy/products/media_l00.{
  is_create_media_content_type, media_record_id_result,
}
import shopify_draft_proxy/proxy/products/media_l01.{
  apply_product_media_moves, find_media_update, settle_media_to_ready,
  update_media_record,
}
import shopify_draft_proxy/proxy/products/media_l02.{
  first_missing_media_update, first_non_ready_media_update,
  product_update_media_payload_with_media_value,
}
import shopify_draft_proxy/proxy/products/media_l03.{
  first_unknown_media_index, invalid_product_media_content_type_variable_error,
  product_update_media_payload,
}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/shared_l00.{
  dedupe_preserving_order, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{mutation_result}
import shopify_draft_proxy/proxy/products/types.{
  type CollectionProductMove, type MutationFieldResult, type ProductUserError,
  type VariantMediaInput, CollectionProductMove, MutationFieldResult,
  ProductUserError, VariantMediaInput,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{find_variant_by_id}
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
pub fn stage_product_update_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  updates: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let effective_media =
    store.get_effective_media_by_product_id(store, product_id)
  case first_missing_media_update(updates, effective_media) {
    Some(update) -> {
      let media_id = read_string_field(update, "id")
      let media_value = case media_id {
        Some(_) -> SrcNull
        None -> SrcList([])
      }
      let error = case media_id {
        Some(id) ->
          ProductUserError(
            ["media"],
            "Media id " <> id <> " does not exist",
            None,
          )
        None -> ProductUserError(["media", "id"], "Media id is required", None)
      }
      mutation_result(
        key,
        product_update_media_payload_with_media_value(
          media_value,
          [error],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    }
    None ->
      case first_non_ready_media_update(updates, effective_media) {
        Some(index) ->
          mutation_result(
            key,
            product_update_media_payload(
              [],
              [
                ProductUserError(
                  ["media", int.to_string(index), "id"],
                  "Non-ready media cannot be updated.",
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
        None -> {
          let updated_media =
            effective_media
            |> list.map(fn(media) {
              case find_media_update(updates, media.id) {
                Some(update) -> update_media_record(media, update)
                None -> settle_media_to_ready(media)
              }
            })
          let changed_media =
            updated_media
            |> list.filter(fn(media) {
              case find_media_update(updates, media.id) {
                Some(_) -> True
                None -> False
              }
            })
          let next_store =
            store.replace_staged_media_for_product(
              store,
              product_id,
              updated_media,
            )
          mutation_result(
            key,
            product_update_media_payload(changed_media, [], field, fragments),
            next_store,
            identity,
            changed_media |> list.filter_map(media_record_id_result),
          )
        }
      }
  }
}

@internal
pub fn reorder_product_media(
  store: Store,
  product_id: String,
  raw_moves: List(Dict(String, ResolvedValue)),
) -> #(Store, List(ProductUserError)) {
  let #(moves, user_errors) = read_collection_product_moves(raw_moves)
  let effective_media =
    store.get_effective_media_by_product_id(store, product_id)
  let media_ids =
    effective_media
    |> list.filter_map(media_record_id_result)
  let user_errors =
    list.fold(enumerate_items(moves), user_errors, fn(errors, entry) {
      let #(move, index) = entry
      let CollectionProductMove(id: media_id, new_position: _) = move
      case list.contains(media_ids, media_id) {
        True -> errors
        False ->
          list.append(errors, [
            ProductUserError(
              ["moves", int.to_string(index), "id"],
              "Media does not exist",
              None,
            ),
          ])
      }
    })
  case user_errors {
    [] -> {
      let reordered_media =
        apply_product_media_moves(effective_media, moves)
        |> enumerate_items()
        |> list.map(fn(entry) {
          let #(media, position) = entry
          ProductMediaRecord(..media, position: position)
        })
      #(
        store.replace_staged_media_for_product(
          store,
          product_id,
          reordered_media,
        ),
        [],
      )
    }
    _ -> #(store, user_errors)
  }
}

@internal
pub fn stage_variant_media_memberships(
  store: Store,
  product_id: String,
  inputs: List(VariantMediaInput),
  is_append: Bool,
) -> #(Store, List(String), List(ProductUserError)) {
  let effective_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  let product_media_ids =
    store.get_effective_media_by_product_id(store, product_id)
    |> list.filter_map(media_record_id_result)
  let #(next_variants, updated_variant_ids, user_errors) =
    list.fold(enumerate_items(inputs), #([], [], []), fn(acc, item) {
      let #(updated_variants, updated_ids, errors) = acc
      let #(entry, index) = item
      let VariantMediaInput(variant_id: variant_id, media_ids: media_ids) =
        entry
      case find_variant_by_id(effective_variants, variant_id) {
        None -> #(
          updated_variants,
          updated_ids,
          list.append(errors, [
            ProductUserError(
              ["variantMedia", int.to_string(index), "variantId"],
              "Variant does not exist",
              None,
            ),
          ]),
        )
        Some(variant) ->
          case first_unknown_media_index(media_ids, product_media_ids) {
            Some(media_index) -> #(
              updated_variants,
              updated_ids,
              list.append(errors, [
                ProductUserError(
                  [
                    "variantMedia",
                    int.to_string(index),
                    "mediaIds",
                    int.to_string(media_index),
                  ],
                  "Media does not exist",
                  None,
                ),
              ]),
            )
            None -> {
              let next_media_ids = case is_append {
                True ->
                  dedupe_preserving_order(list.append(
                    variant.media_ids,
                    media_ids,
                  ))
                False ->
                  list.filter(variant.media_ids, fn(media_id) {
                    !list.contains(media_ids, media_id)
                  })
              }
              #(
                [
                  ProductVariantRecord(..variant, media_ids: next_media_ids),
                  ..updated_variants
                ],
                [variant.id, ..updated_ids],
                errors,
              )
            }
          }
      }
    })
  case user_errors {
    [] -> {
      let staged_variants =
        effective_variants
        |> list.map(fn(variant) {
          find_variant_by_id(next_variants, variant.id)
          |> option.unwrap(variant)
        })
      #(
        store.replace_staged_variants_for_product(
          store,
          product_id,
          staged_variants,
        ),
        list.reverse(updated_variant_ids),
        [],
      )
    }
    _ -> #(store, list.reverse(updated_variant_ids), user_errors)
  }
}

@internal
pub fn invalid_create_media_content_type(
  args: Dict(String, ResolvedValue),
  document: String,
) -> Option(Json) {
  case dict.get(args, "media") {
    Ok(ListVal(values)) ->
      values
      |> enumerate_items()
      |> list.find_map(fn(entry) {
        let #(value, index) = entry
        case value {
          ObjectVal(fields) ->
            case read_string_field(fields, "mediaContentType") {
              Some(media_content_type) ->
                case is_create_media_content_type(media_content_type) {
                  True -> Error(Nil)
                  False ->
                    Ok(invalid_product_media_content_type_variable_error(
                      values,
                      index,
                      media_content_type,
                      document,
                    ))
                }
              None -> Error(Nil)
            }
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}
