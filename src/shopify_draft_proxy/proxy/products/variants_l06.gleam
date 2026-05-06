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
import shopify_draft_proxy/proxy/products/inventory_l04.{
  apply_product_set_inventory_quantities,
}
import shopify_draft_proxy/proxy/products/inventory_l05.{
  sync_product_inventory_summary,
}
import shopify_draft_proxy/proxy/products/products_l01.{enumerate_items}
import shopify_draft_proxy/proxy/products/products_l05.{product_source}
import shopify_draft_proxy/proxy/products/shared_l00.{
  read_object_list_field, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{mutation_result}
import shopify_draft_proxy/proxy/products/types.{
  type BulkVariantUserError, type MutationFieldResult, type ProductUserError,
  type VariantValidationProblem, BulkVariantUserError, MutationFieldResult,
  ProductUserError, RecomputeProductTotalInventory, VariantValidationProblem,
  product_set_option_limit,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  find_variant_update, variant_staged_ids,
}
import shopify_draft_proxy/proxy/products/variants_l01.{
  bulk_variant_error_from_problem, product_set_variant_defaults,
}
import shopify_draft_proxy/proxy/products/variants_l02.{
  product_variant_delete_payload,
}
import shopify_draft_proxy/proxy/products/variants_l03.{
  create_variant_strategy_errors,
}
import shopify_draft_proxy/proxy/products/variants_l05.{
  create_option_input_errors, make_created_variant_record, update_variant_record,
  variant_validation_problems,
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
pub fn variant_product_source(store: Store, product_id: String) -> SourceValue {
  case store.get_effective_product_by_id(store, product_id) {
    Some(product) -> product_source(product)
    None -> SrcNull
  }
}

@internal
pub fn product_set_scalar_variant_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  read_object_list_field(input, "variants")
  |> enumerate_items()
  |> list.flat_map(fn(pair) {
    let #(variant_input, index) = pair
    variant_validation_problems(variant_input)
    |> list.map(fn(problem) {
      let VariantValidationProblem(suffix: suffix, message: message, ..) =
        problem
      ProductOperationUserErrorRecord(
        field: Some(["input", "variants", int.to_string(index), ..suffix]),
        message: message,
        code: Some("INVALID_VARIANT"),
      )
    })
  })
}

/// Detect input variants whose option-value tuples collide with an
/// earlier variant in the same `productSet` input. Shopify rejects these
/// at the API layer with one userError per offending later occurrence;
/// without local detection the proxy stages the duplicates and the
/// failure only surfaces at __meta/commit replay (see QA evidence in
/// `config/parity-specs/products/productSet-duplicate-variants.json`).
@internal
pub fn handle_product_variant_delete(
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
        product_variant_delete_payload(
          None,
          [ProductUserError(["id"], "Variant id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(variant_id) ->
      case store.get_effective_variant_by_id(store, variant_id) {
        None ->
          mutation_result(
            key,
            product_variant_delete_payload(
              None,
              [ProductUserError(["id"], "Variant not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(existing_variant) -> {
          let next_variants =
            store.get_effective_variants_by_product_id(
              store,
              existing_variant.product_id,
            )
            |> list.filter(fn(variant) { variant.id != variant_id })
          let next_store =
            store.replace_staged_variants_for_product(
              store,
              existing_variant.product_id,
              next_variants,
            )
          let #(_, next_store, final_identity) =
            sync_product_inventory_summary(
              next_store,
              identity,
              existing_variant.product_id,
              RecomputeProductTotalInventory,
            )
          mutation_result(
            key,
            product_variant_delete_payload(
              Some(variant_id),
              [],
              field,
              fragments,
            ),
            next_store,
            final_identity,
            [variant_id],
          )
        }
      }
  }
}

@internal
pub fn product_set_variant_records(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry, List(String)) {
  let existing_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  let #(reversed, final_identity, ids) =
    list.fold(inputs, #([], identity, []), fn(acc, input) {
      let #(records, current_identity, collected_ids) = acc
      let existing = case read_string_field(input, "id") {
        Some(id) ->
          existing_variants
          |> list.find(fn(variant) { variant.id == id })
          |> option.from_result
        None -> None
      }
      let #(variant, identity_after_variant) = case existing {
        Some(variant) -> update_variant_record(current_identity, variant, input)
        None ->
          make_created_variant_record(current_identity, product_id, input, None)
      }
      let variant = product_set_variant_defaults(variant)
      let #(variant, next_identity) =
        apply_product_set_inventory_quantities(
          store,
          identity_after_variant,
          variant,
          input,
        )
      #(
        [variant, ..records],
        next_identity,
        list.append(collected_ids, variant_staged_ids(variant)),
      )
    })
  #(list.reverse(reversed), final_identity, ids)
}

@internal
pub fn make_created_variant_records(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
  defaults: Option(ProductVariantRecord),
) -> #(List(ProductVariantRecord), SyntheticIdentityRegistry) {
  let #(reversed, final_identity) =
    list.fold(inputs, #([], identity), fn(acc, input) {
      let #(variants, current_identity) = acc
      let #(variant, next_identity) =
        make_created_variant_record(
          current_identity,
          product_id,
          input,
          defaults,
        )
      #([variant, ..variants], next_identity)
    })
  #(list.reverse(reversed), final_identity)
}

@internal
pub fn validate_bulk_variant_scalar_input(
  input: Dict(String, ResolvedValue),
  variant_index: Int,
) -> List(BulkVariantUserError) {
  variant_validation_problems(input)
  |> list.flat_map(fn(problem) {
    bulk_variant_error_from_problem(problem, variant_index)
  })
}

@internal
pub fn validate_product_variant_scalar_input(
  input: Dict(String, ResolvedValue),
  prefix: List(String),
) -> List(ProductUserError) {
  variant_validation_problems(input)
  |> list.map(fn(problem) {
    let VariantValidationProblem(
      suffix: suffix,
      message: message,
      product_code: code,
      ..,
    ) = problem
    ProductUserError(list.append(prefix, suffix), message, code)
  })
}

@internal
pub fn update_variant_records(
  identity: SyntheticIdentityRegistry,
  variants: List(ProductVariantRecord),
  updates: List(Dict(String, ResolvedValue)),
) -> #(
  List(ProductVariantRecord),
  List(ProductVariantRecord),
  SyntheticIdentityRegistry,
) {
  let #(reversed_variants, reversed_updated, final_identity) =
    list.fold(variants, #([], [], identity), fn(acc, variant) {
      let #(next_variants, updated_variants, current_identity) = acc
      case find_variant_update(updates, variant.id) {
        Some(input) -> {
          let #(updated, next_identity) =
            update_variant_record(current_identity, variant, input)
          #(
            [updated, ..next_variants],
            [updated, ..updated_variants],
            next_identity,
          )
        }
        None -> #(
          [variant, ..next_variants],
          updated_variants,
          current_identity,
        )
      }
    })
  #(
    list.reverse(reversed_variants),
    list.reverse(reversed_updated),
    final_identity,
  )
}

@internal
pub fn validate_product_options_create_inputs(
  existing_options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
  inputs: List(Dict(String, ResolvedValue)),
  should_create_option_variants: Bool,
  replacing_default: Bool,
) -> List(ProductUserError) {
  let total_option_errors = case
    list.length(existing_options) + list.length(inputs)
    > product_set_option_limit
  {
    True -> [
      ProductUserError(
        ["options"],
        "Can only specify a maximum of 3 options",
        Some("OPTIONS_OVER_LIMIT"),
      ),
    ]
    False -> []
  }
  list.append(
    total_option_errors,
    list.append(
      create_option_input_errors(
        existing_options,
        existing_variants,
        inputs,
        replacing_default,
      ),
      create_variant_strategy_errors(
        existing_options,
        existing_variants,
        inputs,
        should_create_option_variants,
      ),
    ),
  )
}
