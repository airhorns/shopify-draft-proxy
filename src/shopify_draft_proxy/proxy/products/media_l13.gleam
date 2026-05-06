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
import shopify_draft_proxy/proxy/products/media_l00.{
  is_valid_media_source, media_record_id_result,
  product_media_product_image_id_result, transition_created_media_to_processing,
}
import shopify_draft_proxy/proxy/products/media_l01.{
  make_created_media_record, read_variant_media_inputs,
}
import shopify_draft_proxy/proxy/products/media_l02.{
  first_unknown_media_id, product_update_media_payload_with_media_value,
}
import shopify_draft_proxy/proxy/products/media_l04.{
  stage_variant_media_memberships,
}
import shopify_draft_proxy/proxy/products/media_l12.{
  product_create_media_payload, product_delete_media_payload,
  product_variant_media_payload,
}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/shared_l00.{
  dedupe_preserving_order, read_arg_object_list, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  mutation_result, user_errors_source,
}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type ProductUserError, MutationFieldResult,
  ProductUserError,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{option_to_result}
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
pub fn stage_product_create_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let existing_media =
    store.get_effective_media_by_product_id(store, product_id)
  let initial = #(identity, [], [])
  let #(next_identity, created_reversed, user_errors_reversed) =
    inputs
    |> enumerate_items()
    |> list.fold(initial, fn(acc, entry) {
      let #(current_identity, created, errors) = acc
      let #(input, index) = entry
      let media_content_type =
        read_string_field(input, "mediaContentType") |> option.unwrap("IMAGE")
      case
        media_content_type == "IMAGE"
        && !is_valid_media_source(read_string_field(input, "originalSource"))
      {
        True -> #(current_identity, created, [
          ProductUserError(
            ["media", int.to_string(index), "originalSource"],
            "Image URL is invalid",
            None,
          ),
          ..errors
        ])
        False -> {
          let position = list.length(existing_media) + list.length(created)
          let #(record, identity_after_record) =
            make_created_media_record(
              current_identity,
              product_id,
              input,
              position,
            )
          #(identity_after_record, [record, ..created], errors)
        }
      }
    })
  let created_media = list.reverse(created_reversed)
  let user_errors = list.reverse(user_errors_reversed)
  let response_store = case created_media {
    [] -> store
    _ ->
      store.replace_staged_media_for_product(
        store,
        product_id,
        list.append(existing_media, created_media),
      )
  }
  let final_store = case created_media {
    [] -> store
    _ ->
      store.replace_staged_media_for_product(
        store,
        product_id,
        list.append(
          existing_media,
          list.map(created_media, transition_created_media_to_processing),
        ),
      )
  }
  let product = store.get_effective_product_by_id(response_store, product_id)
  let staged_ids = created_media |> list.filter_map(media_record_id_result)
  mutation_result(
    key,
    product_create_media_payload(
      response_store,
      created_media,
      user_errors,
      product,
      field,
      fragments,
    ),
    final_store,
    next_identity,
    staged_ids,
  )
}

@internal
pub fn stage_product_delete_media(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  media_ids: List(String),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let effective_media =
    store.get_effective_media_by_product_id(store, product_id)
  case first_unknown_media_id(media_ids, effective_media) {
    Some(media_id) ->
      mutation_result(
        key,
        product_delete_media_payload(
          store,
          None,
          SrcNull,
          SrcNull,
          [
            ProductUserError(
              ["mediaIds"],
              "Media id " <> media_id <> " does not exist",
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
      let deleted_media =
        effective_media
        |> list.filter(fn(media) {
          case media.id {
            Some(id) -> list.contains(media_ids, id)
            None -> False
          }
        })
      let next_media =
        effective_media
        |> list.filter(fn(media) {
          case media.id {
            Some(id) -> !list.contains(media_ids, id)
            None -> True
          }
        })
      let next_store =
        store.replace_staged_media_for_product(store, product_id, next_media)
      let deleted_media_ids =
        deleted_media |> list.filter_map(media_record_id_result)
      let deleted_product_image_ids =
        deleted_media |> list.filter_map(product_media_product_image_id_result)
      let product = store.get_effective_product_by_id(next_store, product_id)
      mutation_result(
        key,
        product_delete_media_payload(
          next_store,
          product,
          SrcList(list.map(deleted_media_ids, SrcString)),
          SrcList(list.map(deleted_product_image_ids, SrcString)),
          [],
          field,
          fragments,
        ),
        next_store,
        identity,
        deleted_media_ids,
      )
    }
  }
}

@internal
pub fn handle_product_variant_media_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_variant_media_payload(
          store,
          None,
          [],
          [ProductUserError(["productId"], "Product id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_variant_media_payload(
              store,
              None,
              [],
              [ProductUserError(["productId"], "Product does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) -> {
          let inputs =
            read_variant_media_inputs(read_arg_object_list(args, "variantMedia"))
          let is_append = case field {
            Field(name: name, ..) -> name.value == "productVariantAppendMedia"
            _ -> False
          }
          let #(next_store, updated_variant_ids, user_errors) =
            stage_variant_media_memberships(
              store,
              product_id,
              inputs,
              is_append,
            )
          let response_store = case user_errors {
            [] -> next_store
            _ -> store
          }
          let variants =
            updated_variant_ids
            |> dedupe_preserving_order
            |> list.filter_map(fn(variant_id) {
              store.get_effective_variant_by_id(response_store, variant_id)
              |> option_to_result
            })
          let staged_ids = case user_errors {
            [] -> [product_id, ..dedupe_preserving_order(updated_variant_ids)]
            _ -> []
          }
          mutation_result(
            key,
            product_variant_media_payload(
              response_store,
              Some(product),
              variants,
              user_errors,
              field,
              fragments,
            ),
            response_store,
            identity,
            staged_ids,
          )
        }
      }
  }
}

@internal
pub fn product_media_not_found_payload(
  store: Store,
  shape: String,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case shape {
    "delete" ->
      product_delete_media_payload(
        store,
        None,
        SrcNull,
        SrcNull,
        [ProductUserError(["productId"], "Product does not exist", None)],
        field,
        fragments,
      )
    "create" ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString("ProductCreateMediaPayload")),
          #("media", SrcNull),
          #(
            "mediaUserErrors",
            user_errors_source([
              ProductUserError(["productId"], "Product does not exist", None),
            ]),
          ),
          #("product", SrcNull),
        ]),
        get_selected_child_fields(field, default_selected_field_options()),
        fragments,
      )
    _ ->
      product_update_media_payload_with_media_value(
        SrcNull,
        [ProductUserError(["productId"], "Product does not exist", None)],
        field,
        fragments,
      )
  }
}
