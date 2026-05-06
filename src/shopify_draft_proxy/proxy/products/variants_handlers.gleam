//// Products-domain submodule: variants_handlers.
//// Combines layered files: variants_l12, variants_l13, variants_l14, variants_l15.

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
import shopify_draft_proxy/proxy/products/inventory_apply.{
  sync_product_inventory_summary,
}
import shopify_draft_proxy/proxy/products/products_handlers.{
  product_source_with_store,
}
import shopify_draft_proxy/proxy/products/shared.{
  max_input_size_exceeded_error, mutation_error_result, mutation_rejected_result,
  mutation_result, read_arg_object_list, read_arg_string_list, read_int_field,
  read_string_field, user_errors_source,
}
import shopify_draft_proxy/proxy/products/types.{
  type BulkVariantUserError, type MutationFieldResult, type ProductUserError,
  BulkVariantUserError, MutationFieldResult, ProductUserError,
  RecomputeProductTotalInventory, max_product_variants,
  product_does_not_exist_user_error,
} as product_types
import shopify_draft_proxy/proxy/products/variants_helpers.{
  find_product_option, position_options, read_option_create_inputs,
  restore_default_option_state, variant_staged_ids,
}
import shopify_draft_proxy/proxy/products/variants_options.{
  apply_sequential_variant_reorder,
  create_variants_for_option_value_combinations,
  first_bulk_delete_missing_variant, make_created_option_records,
  make_created_variant_record, make_options_from_variant_selections,
  product_has_standalone_default_variant, read_product_variant_positions,
  remap_variant_selections_for_option_update, reorder_product_options,
  update_product_option_record, update_variant_record,
  upsert_variant_selections_into_options, validate_product_option_update_inputs,
  validate_product_variant_positions,
}
import shopify_draft_proxy/proxy/products/variants_options_core.{
  bulk_variant_user_errors_source, insert_option_at_position,
  map_variants_to_first_new_option_values,
  product_uses_only_default_option_state, remap_variant_to_first_option_values,
  reorder_variant_selections_for_options, sort_and_position_options,
  sync_product_options_with_variants, unknown_option_errors,
}
import shopify_draft_proxy/proxy/products/variants_sources.{
  product_variant_source,
}
import shopify_draft_proxy/proxy/products/variants_validation.{
  make_created_variant_records, update_variant_records,
  validate_bulk_create_variant_batch, validate_bulk_update_variant_batch,
  validate_product_options_create_inputs, validate_product_variant_scalar_input,
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

// ===== from variants_l12 =====
@internal
pub fn product_variant_payload(
  store: Store,
  product: Option(ProductRecord),
  variant: Option(ProductVariantRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  let variant_value = case variant {
    Some(record) -> product_variant_source(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductVariantPayload")),
      #("product", product_value),
      #("productVariant", variant_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_variants_bulk_payload(
  typename: String,
  store: Store,
  product: Option(ProductRecord),
  variants: Option(List(ProductVariantRecord)),
  user_errors: List(BulkVariantUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  let variants_value = case variants {
    Some(records) ->
      SrcList(
        list.map(records, fn(variant) { product_variant_source(store, variant) }),
      )
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString(typename)),
      #("product", product_value),
      #("productVariants", variants_value),
      #("userErrors", bulk_variant_user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_variants_bulk_delete_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(BulkVariantUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductVariantsBulkDeletePayload")),
      #("product", product_value),
      #("userErrors", bulk_variant_user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_variants_bulk_reorder_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductVariantsBulkReorderPayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_options_delete_payload(
  store: Store,
  deleted_option_ids: List(String),
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductOptionsDeletePayload")),
      #("deletedOptionsIds", SrcList(list.map(deleted_option_ids, SrcString))),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_options_reorder_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductOptionsReorderPayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_option_update_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductOptionUpdatePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn product_options_create_payload(
  store: Store,
  product: Option(ProductRecord),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let product_value = case product {
    Some(record) -> product_source_with_store(store, record)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("ProductOptionsCreatePayload")),
      #("product", product_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

// ===== from variants_l13 =====
@internal
pub fn handle_product_variant_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "input")
  let product_id = case input {
    Some(input) -> graphql_helpers.read_arg_string(input, "productId")
    None -> None
  }
  case product_id, input {
    None, _ ->
      mutation_result(
        key,
        product_variant_payload(
          store,
          None,
          None,
          [
            ProductUserError(
              ["input", "productId"],
              "Product id is required",
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
    Some(product_id), Some(input) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_variant_payload(
              store,
              None,
              None,
              [
                ProductUserError(
                  ["input", "productId"],
                  "Product not found",
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
        Some(_) -> {
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          let defaults = list.first(effective_variants) |> option.from_result
          let user_errors =
            validate_product_variant_scalar_input(input, ["input"])
          case user_errors {
            [] -> {
              let #(created_variant, identity_after_variant) =
                make_created_variant_record(
                  identity,
                  product_id,
                  input,
                  defaults,
                )
              let next_variants =
                list.append(effective_variants, [created_variant])
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  product_id,
                  next_variants,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity_after_variant,
                  product_id,
                  RecomputeProductTotalInventory,
                )
              mutation_result(
                key,
                product_variant_payload(
                  next_store,
                  product,
                  Some(created_variant),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                variant_staged_ids(created_variant),
              )
            }
            _ ->
              mutation_rejected_result(
                key,
                product_variant_payload(
                  store,
                  None,
                  None,
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
              )
          }
        }
      }
    Some(_), None ->
      mutation_result(
        key,
        product_variant_payload(
          store,
          None,
          None,
          [
            ProductUserError(
              ["input", "productId"],
              "Product id is required",
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
  }
}

@internal
pub fn handle_product_variant_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = graphql_helpers.read_arg_object(args, "input")
  let variant_id = case input {
    Some(input) -> graphql_helpers.read_arg_string(input, "id")
    None -> None
  }
  case variant_id, input {
    None, _ ->
      mutation_result(
        key,
        product_variant_payload(
          store,
          None,
          None,
          [ProductUserError(["input", "id"], "Variant id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(variant_id), Some(input) ->
      case store.get_effective_variant_by_id(store, variant_id) {
        None ->
          mutation_result(
            key,
            product_variant_payload(
              store,
              None,
              None,
              [ProductUserError(["input", "id"], "Variant not found", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(existing_variant) -> {
          let user_errors =
            validate_product_variant_scalar_input(input, ["input"])
          case user_errors {
            [] -> {
              let #(updated_variant, identity_after_variant) =
                update_variant_record(identity, existing_variant, input)
              let next_variants =
                store.get_effective_variants_by_product_id(
                  store,
                  existing_variant.product_id,
                )
                |> list.map(fn(variant) {
                  case variant.id == variant_id {
                    True -> updated_variant
                    False -> variant
                  }
                })
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  existing_variant.product_id,
                  next_variants,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity_after_variant,
                  existing_variant.product_id,
                  RecomputeProductTotalInventory,
                )
              mutation_result(
                key,
                product_variant_payload(
                  next_store,
                  product,
                  Some(updated_variant),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                variant_staged_ids(updated_variant),
              )
            }
            _ ->
              mutation_rejected_result(
                key,
                product_variant_payload(
                  store,
                  None,
                  None,
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
              )
          }
        }
      }
    Some(_), None ->
      mutation_result(
        key,
        product_variant_payload(
          store,
          None,
          None,
          [ProductUserError(["input", "id"], "Variant id is required", None)],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
  }
}

@internal
pub fn handle_product_variants_bulk_create_valid_size(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, ResolvedValue),
  variant_inputs: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_variants_bulk_payload(
          "ProductVariantsBulkCreatePayload",
          store,
          None,
          Some([]),
          [
            BulkVariantUserError(
              Some(["productId"]),
              "Product id is required",
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
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_variants_bulk_payload(
              "ProductVariantsBulkCreatePayload",
              store,
              None,
              Some([]),
              [
                BulkVariantUserError(
                  Some(["productId"]),
                  "Product does not exist",
                  Some("PRODUCT_DOES_NOT_EXIST"),
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          let effective_options =
            store.get_effective_options_by_product_id(store, product_id)
          let defaults = list.first(effective_variants) |> option.from_result
          let should_remove_standalone_variant =
            !list.is_empty(variant_inputs)
            && list.length(effective_variants) == 1
            && {
              graphql_helpers.read_arg_string(args, "strategy")
              == Some("REMOVE_STANDALONE_VARIANT")
              || product_has_standalone_default_variant(
                effective_options,
                effective_variants,
              )
            }
          let retained_count = case should_remove_standalone_variant {
            True -> 0
            False -> list.length(effective_variants)
          }
          let user_errors =
            validate_bulk_create_variant_batch(
              store,
              product_id,
              variant_inputs,
              retained_count,
            )
          case user_errors {
            [] -> {
              let #(created_variants, identity_after_variants) =
                make_created_variant_records(
                  identity,
                  product_id,
                  variant_inputs,
                  defaults,
                )
              let retained_variants = case should_remove_standalone_variant {
                True -> []
                False -> effective_variants
              }
              let next_variants =
                list.append(retained_variants, created_variants)
              let #(synced_options, identity_after_options) = case
                should_remove_standalone_variant
              {
                True ->
                  make_options_from_variant_selections(
                    identity_after_variants,
                    product_id,
                    next_variants,
                  )
                False -> {
                  let #(next_options, identity_after_options) =
                    upsert_variant_selections_into_options(
                      identity_after_variants,
                      product_id,
                      effective_options,
                      next_variants,
                    )
                  #(
                    sync_product_options_with_variants(
                      next_options,
                      next_variants,
                    ),
                    identity_after_options,
                  )
                }
              }
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  product_id,
                  next_variants,
                )
                |> store.replace_staged_options_for_product(
                  product_id,
                  synced_options,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity_after_options,
                  product_id,
                  RecomputeProductTotalInventory,
                )
              mutation_result(
                key,
                product_variants_bulk_payload(
                  "ProductVariantsBulkCreatePayload",
                  next_store,
                  product,
                  Some(created_variants),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                list.flat_map(created_variants, variant_staged_ids),
              )
            }
            _ ->
              mutation_rejected_result(
                key,
                product_variants_bulk_payload(
                  "ProductVariantsBulkCreatePayload",
                  store,
                  None,
                  Some([]),
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
              )
          }
        }
      }
  }
}

@internal
pub fn handle_product_variants_bulk_update_valid_size(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, ResolvedValue),
  updates: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  case graphql_helpers.read_arg_string(args, "productId") {
    None ->
      mutation_result(
        key,
        product_variants_bulk_payload(
          "ProductVariantsBulkUpdatePayload",
          store,
          None,
          Some([]),
          [
            BulkVariantUserError(
              Some(["productId"]),
              "Product id is required",
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
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_variants_bulk_payload(
              "ProductVariantsBulkUpdatePayload",
              store,
              None,
              None,
              [
                BulkVariantUserError(
                  Some(["productId"]),
                  "Product does not exist",
                  Some("PRODUCT_DOES_NOT_EXIST"),
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          let user_errors =
            validate_bulk_update_variant_batch(
              store,
              product_id,
              updates,
              effective_variants,
            )
          case user_errors {
            [] -> {
              let #(next_variants, updated_variants, identity_after_variants) =
                update_variant_records(identity, effective_variants, updates)
              let synced_options =
                sync_product_options_with_variants(
                  store.get_effective_options_by_product_id(store, product_id),
                  next_variants,
                )
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  product_id,
                  next_variants,
                )
                |> store.replace_staged_options_for_product(
                  product_id,
                  synced_options,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity_after_variants,
                  product_id,
                  RecomputeProductTotalInventory,
                )
              mutation_result(
                key,
                product_variants_bulk_payload(
                  "ProductVariantsBulkUpdatePayload",
                  next_store,
                  product,
                  Some(updated_variants),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                list.flat_map(updated_variants, variant_staged_ids),
              )
            }
            _ -> {
              let response_product = case user_errors {
                [BulkVariantUserError(field: None, ..), ..] -> None
                _ -> store.get_effective_product_by_id(store, product_id)
              }
              mutation_rejected_result(
                key,
                product_variants_bulk_payload(
                  "ProductVariantsBulkUpdatePayload",
                  store,
                  response_product,
                  None,
                  user_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
              )
            }
          }
        }
      }
  }
}

@internal
pub fn handle_product_variants_bulk_delete(
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
        product_variants_bulk_delete_payload(
          store,
          None,
          [
            BulkVariantUserError(
              Some(["productId"]),
              "Product id is required",
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
    Some(product_id) ->
      case store.get_effective_product_by_id(store, product_id) {
        None ->
          mutation_result(
            key,
            product_variants_bulk_delete_payload(
              store,
              None,
              [
                BulkVariantUserError(
                  Some(["productId"]),
                  "Product does not exist",
                  Some("PRODUCT_DOES_NOT_EXIST"),
                ),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) -> {
          let variant_ids = read_arg_string_list(args, "variantsIds")
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          case
            first_bulk_delete_missing_variant(variant_ids, effective_variants)
          {
            Some(index) ->
              mutation_result(
                key,
                product_variants_bulk_delete_payload(
                  store,
                  None,
                  [
                    BulkVariantUserError(
                      Some(["variantsIds", int.to_string(index)]),
                      "At least one variant does not belong to the product",
                      Some(
                        "AT_LEAST_ONE_VARIANT_DOES_NOT_BELONG_TO_THE_PRODUCT",
                      ),
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
              let next_variants =
                effective_variants
                |> list.filter(fn(variant) {
                  !list.contains(variant_ids, variant.id)
                })
              let synced_options =
                sync_product_options_with_variants(
                  store.get_effective_options_by_product_id(store, product_id),
                  next_variants,
                )
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  product_id,
                  next_variants,
                )
                |> store.replace_staged_options_for_product(
                  product_id,
                  synced_options,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity,
                  product_id,
                  RecomputeProductTotalInventory,
                )
              mutation_result(
                key,
                product_variants_bulk_delete_payload(
                  next_store,
                  product,
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                variant_ids,
              )
            }
          }
        }
      }
  }
}

@internal
pub fn handle_product_variants_bulk_reorder(
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
        product_variants_bulk_reorder_payload(
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
            product_variants_bulk_reorder_payload(
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
        Some(_) -> {
          let #(positions, user_errors) =
            read_product_variant_positions(read_arg_object_list(
              args,
              "positions",
            ))
          let effective_variants =
            store.get_effective_variants_by_product_id(store, product_id)
          let missing_errors =
            validate_product_variant_positions(effective_variants, positions)
          let all_errors = list.append(user_errors, missing_errors)
          case all_errors {
            [_, ..] ->
              mutation_result(
                key,
                product_variants_bulk_reorder_payload(
                  store,
                  None,
                  all_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            [] -> {
              let next_variants =
                apply_sequential_variant_reorder(effective_variants, positions)
              let next_store =
                store.replace_staged_variants_for_product(
                  store,
                  product_id,
                  next_variants,
                )
              let #(product, next_store, final_identity) =
                sync_product_inventory_summary(
                  next_store,
                  identity,
                  product_id,
                  RecomputeProductTotalInventory,
                )
              mutation_result(
                key,
                product_variants_bulk_reorder_payload(
                  next_store,
                  product,
                  [],
                  field,
                  fragments,
                ),
                next_store,
                final_identity,
                list.map(next_variants, fn(variant) { variant.id }),
              )
            }
          }
        }
      }
  }
}

@internal
pub fn stage_product_options_reorder(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
  option_inputs: List(Dict(String, ResolvedValue)),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let product_id = product.id
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let #(next_options, user_errors) =
    reorder_product_options(existing_options, option_inputs)
  case user_errors {
    [_, ..] ->
      mutation_result(
        key,
        product_options_reorder_payload(
          store,
          Some(product),
          user_errors,
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    [] -> {
      let next_variants =
        store.get_effective_variants_by_product_id(store, product_id)
        |> reorder_variant_selections_for_options(next_options)
      let synced_options =
        sync_product_options_with_variants(next_options, next_variants)
      let next_store =
        store
        |> store.replace_staged_options_for_product(product_id, synced_options)
        |> store.replace_staged_variants_for_product(product_id, next_variants)
      mutation_result(
        key,
        product_options_reorder_payload(
          next_store,
          store.get_effective_product_by_id(next_store, product_id),
          [],
          field,
          fragments,
        ),
        next_store,
        identity,
        list.map(next_options, fn(option) { option.id }),
      )
    }
  }
}

@internal
pub fn stage_product_options_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
  option_ids: List(String),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let product_id = product.id
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let existing_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  let existing_ids = list.map(existing_options, fn(option) { option.id })
  let unknown_errors = unknown_option_errors(option_ids, existing_ids)
  case unknown_errors {
    [_, ..] ->
      mutation_rejected_result(
        key,
        product_options_delete_payload(
          store,
          [],
          Some(product),
          unknown_errors,
          field,
          fragments,
        ),
        store,
        identity,
      )
    [] -> {
      let deleted_ids =
        existing_options
        |> list.filter(fn(option) { list.contains(option_ids, option.id) })
        |> list.map(fn(option) { option.id })
      let remaining_options =
        existing_options
        |> list.filter(fn(option) { !list.contains(option_ids, option.id) })
        |> position_options(1, [])
      let #(next_options, next_variants, final_identity, restored_ids) = case
        remaining_options
      {
        [] -> restore_default_option_state(identity, product, existing_variants)
        [_, ..] -> #(remaining_options, existing_variants, identity, [])
      }
      let synced_options =
        sync_product_options_with_variants(next_options, next_variants)
      let next_store =
        store
        |> store.replace_staged_variants_for_product(product_id, next_variants)
        |> store.replace_staged_options_for_product(product_id, synced_options)
      mutation_result(
        key,
        product_options_delete_payload(
          next_store,
          deleted_ids,
          store.get_effective_product_by_id(next_store, product_id),
          [],
          field,
          fragments,
        ),
        next_store,
        final_identity,
        list.append(deleted_ids, restored_ids),
      )
    }
  }
}

@internal
pub fn stage_product_option_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product: ProductRecord,
  option_input: Dict(String, ResolvedValue),
  args: Dict(String, ResolvedValue),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let product_id = product.id
  let existing_options =
    store.get_effective_options_by_product_id(store, product_id)
  let existing_variants =
    store.get_effective_variants_by_product_id(store, product_id)
  case read_string_field(option_input, "id") {
    None ->
      mutation_result(
        key,
        product_option_update_payload(
          store,
          None,
          [
            ProductUserError(["option", "id"], "Option id is required", None),
          ],
          field,
          fragments,
        ),
        store,
        identity,
        [],
      )
    Some(option_id) ->
      case find_product_option(existing_options, product_id, option_id) {
        None ->
          mutation_result(
            key,
            product_option_update_payload(
              store,
              Some(product),
              [ProductUserError(["option"], "Option does not exist", None)],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(target_option) -> {
          let values_to_add = read_arg_object_list(args, "optionValuesToAdd")
          let values_to_update =
            read_arg_object_list(args, "optionValuesToUpdate")
          let value_ids_to_delete =
            read_arg_string_list(args, "optionValuesToDelete")
          let validation_errors =
            validate_product_option_update_inputs(
              existing_options,
              target_option,
              option_input,
              values_to_add,
              values_to_update,
              value_ids_to_delete,
            )
          case validation_errors {
            [_, ..] ->
              mutation_rejected_result(
                key,
                product_option_update_payload(
                  store,
                  Some(product),
                  validation_errors,
                  field,
                  fragments,
                ),
                store,
                identity,
              )
            [] -> {
              let #(
                updated_option,
                renamed_values,
                identity_after_values,
                new_ids,
              ) =
                update_product_option_record(
                  identity,
                  target_option,
                  option_input,
                  values_to_add,
                  values_to_update,
                  value_ids_to_delete,
                )
              let next_options =
                existing_options
                |> list.filter(fn(option) { option.id != option_id })
                |> insert_option_at_position(
                  updated_option,
                  read_int_field(option_input, "position"),
                )
              let next_variants =
                existing_variants
                |> remap_variant_selections_for_option_update(
                  target_option.name,
                  updated_option.name,
                  renamed_values,
                )
                |> reorder_variant_selections_for_options(next_options)
              let synced_options =
                sync_product_options_with_variants(next_options, next_variants)
              let next_store =
                store
                |> store.replace_staged_variants_for_product(
                  product_id,
                  next_variants,
                )
                |> store.replace_staged_options_for_product(
                  product_id,
                  synced_options,
                )
              mutation_result(
                key,
                product_option_update_payload(
                  next_store,
                  store.get_effective_product_by_id(next_store, product_id),
                  [],
                  field,
                  fragments,
                ),
                next_store,
                identity_after_values,
                list.append([option_id], new_ids),
              )
            }
          }
        }
      }
  }
}

@internal
pub fn stage_valid_product_options_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  product_id: String,
  option_inputs: List(Dict(String, ResolvedValue)),
  should_create_option_variants: Bool,
  replacing_default: Bool,
  starting_options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  let #(created_options, identity_after_options) =
    make_created_option_records(identity, product_id, option_inputs)
  let next_options =
    list.append(starting_options, created_options)
    |> sort_and_position_options
  let next_variants = case replacing_default, existing_variants {
    True, [first_variant, ..] -> [
      remap_variant_to_first_option_values(first_variant, next_options),
    ]
    _, _ ->
      map_variants_to_first_new_option_values(
        existing_variants,
        created_options,
      )
  }
  let #(next_variants, final_identity) = case should_create_option_variants {
    True ->
      create_variants_for_option_value_combinations(
        identity_after_options,
        product_id,
        next_options,
        created_options,
        next_variants,
      )
    False -> #(next_variants, identity_after_options)
  }
  let synced_options =
    sync_product_options_with_variants(next_options, next_variants)
  let next_store =
    store
    |> store.replace_staged_variants_for_product(product_id, next_variants)
    |> store.replace_staged_options_for_product(product_id, synced_options)
  let staged_ids =
    list.append(
      list.map(created_options, fn(option) { option.id }),
      list.flat_map(created_options, fn(option) {
        list.map(option.option_values, fn(value) { value.id })
      }),
    )
  mutation_result(
    key,
    product_options_create_payload(
      next_store,
      store.get_effective_product_by_id(next_store, product_id),
      [],
      field,
      fragments,
    ),
    next_store,
    final_identity,
    staged_ids,
  )
}

// ===== from variants_l14 =====
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

// ===== from variants_l15 =====
@internal
pub fn handle_product_options_create(
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
        product_options_create_payload(
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
            product_options_create_payload(
              store,
              None,
              [
                product_does_not_exist_user_error(["productId"]),
              ],
              field,
              fragments,
            ),
            store,
            identity,
            [],
          )
        Some(_) ->
          stage_product_options_create(
            store,
            identity,
            key,
            product_id,
            read_option_create_inputs(args),
            graphql_helpers.read_arg_string(args, "variantStrategy")
              == Some("CREATE"),
            field,
            fragments,
          )
      }
  }
}
