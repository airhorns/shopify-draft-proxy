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
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_arg_object_list, read_arg_string_list,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  max_input_size_exceeded_error, mutation_error_result, mutation_rejected_result,
  mutation_result,
}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type ProductUserError, MutationFieldResult,
  ProductUserError, max_product_variants, product_does_not_exist_user_error,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l01.{
  product_uses_only_default_option_state,
}
import shopify_draft_proxy/proxy/products/variants_l06.{
  validate_product_options_create_inputs,
}
import shopify_draft_proxy/proxy/products/variants_l12.{
  product_option_update_payload, product_options_create_payload,
  product_options_delete_payload, product_options_reorder_payload,
}
import shopify_draft_proxy/proxy/products/variants_l13.{
  handle_product_variants_bulk_create_valid_size,
  handle_product_variants_bulk_update_valid_size, stage_product_option_update,
  stage_product_options_delete, stage_product_options_reorder,
  stage_valid_product_options_create,
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
pub fn handle_product_options_delete(
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
        product_options_delete_payload(
          store,
          [],
          None,
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
            product_options_delete_payload(
              store,
              [],
              None,
              [product_does_not_exist_user_error(["productId"])],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) ->
          stage_product_options_delete(
            store,
            identity,
            key,
            product,
            read_arg_string_list(args, "options"),
            field,
            fragments,
          )
      }
  }
}

@internal
pub fn handle_product_options_reorder(
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
        product_options_reorder_payload(
          store,
          None,
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
            product_options_reorder_payload(
              store,
              None,
              [product_does_not_exist_user_error(["productId"])],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) ->
          stage_product_options_reorder(
            store,
            identity,
            key,
            product,
            read_arg_object_list(args, "options"),
            field,
            fragments,
          )
      }
  }
}

@internal
pub fn handle_product_variants_bulk_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let variant_inputs = read_arg_object_list(args, "variants")
  case list.length(variant_inputs) > max_product_variants {
    True ->
      mutation_error_result(key, store, identity, [
        max_input_size_exceeded_error(
          "productVariantsBulkCreate",
          "variants",
          list.length(variant_inputs),
          field,
          document,
        ),
      ])
    False ->
      handle_product_variants_bulk_create_valid_size(
        store,
        identity,
        key,
        args,
        variant_inputs,
        field,
        fragments,
      )
  }
}

@internal
pub fn handle_product_variants_bulk_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let updates = read_arg_object_list(args, "variants")
  case list.length(updates) > max_product_variants {
    True ->
      mutation_error_result(key, store, identity, [
        max_input_size_exceeded_error(
          "productVariantsBulkUpdate",
          "variants",
          list.length(updates),
          field,
          document,
        ),
      ])
    False ->
      handle_product_variants_bulk_update_valid_size(
        store,
        identity,
        key,
        args,
        updates,
        field,
        fragments,
      )
  }
}

@internal
pub fn handle_product_option_update(
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
        product_option_update_payload(
          store,
          None,
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
            product_option_update_payload(
              store,
              None,
              [ProductUserError(["productId"], "Product not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(product) ->
          case graphql_helpers.read_arg_object(args, "option") {
            None ->
              mutation_result(
                key,
                product_option_update_payload(
                  store,
                  None,
                  [
                    ProductUserError(
                      ["option", "id"],
                      "Option id is required",
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
            Some(option_input) ->
              stage_product_option_update(
                store,
                identity,
                key,
                product,
                option_input,
                args,
                field,
                fragments,
              )
          }
      }
  }
}

@internal
pub fn stage_product_options_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  option_inputs: List(Dict(String, ResolvedValue)),
  should_create_option_variants: Bool,
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let existing_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  let replacing_default =
    product_uses_only_default_option_state(existing_options, existing_variants)
  let starting_options = case replacing_default {
    True -> []
    False -> existing_options
  }
  let validation_errors =
    validate_product_options_create_inputs(
      starting_options,
      existing_variants,
      option_inputs,
      should_create_option_variants,
      replacing_default,
    )
  case validation_errors {
    [_, ..] ->
      mutation_rejected_result(
        key,
        product_options_create_payload(
          store,
          store.get_effective_product_by_id(store, product_id),
          validation_errors,
          field,
          fragments,
        ),
        store,
        identity,
      )
    [] ->
      stage_valid_product_options_create(
        store,
        identity,
        key,
        product_id,
        option_inputs,
        should_create_option_variants,
        replacing_default,
        starting_options,
        existing_variants,
        field,
        fragments,
      )
  }
}
