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
import shopify_draft_proxy/proxy/products/shared_l00.{read_string_field}
import shopify_draft_proxy/proxy/products/types.{
  type ProductUserError, ProductUserError, product_option_name_limit,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  make_default_option_record, make_default_variant_record,
}
import shopify_draft_proxy/proxy/products/variants_l01.{
  make_default_variant_for_options, option_name_exists,
  sort_and_position_options, sync_product_options_with_variants,
}
import shopify_draft_proxy/proxy/products/variants_l02.{
  option_value_already_exists_errors, read_variant_selected_option,
  update_option_name_errors, update_option_result_value_errors,
}
import shopify_draft_proxy/proxy/products/variants_l03.{
  create_option_value_errors, make_created_option_records,
  read_variant_option_values, update_option_value_input_errors,
  upsert_variant_selections_into_options,
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
pub fn make_product_create_option_graph(
  identity: SyntheticIdentityRegistry,
  product: ProductRecord,
  option_inputs: List(Dict(String, ResolvedValue)),
) -> #(
  List(ProductOptionRecord),
  ProductVariantRecord,
  SyntheticIdentityRegistry,
  List(String),
) {
  case option_inputs {
    [] -> {
      let #(default_option, identity_after_option, option_ids) =
        make_default_option_record(identity, product)
      let #(default_variant, final_identity, variant_ids) =
        make_default_variant_record(identity_after_option, product)
      #(
        [default_option],
        default_variant,
        final_identity,
        list.append(option_ids, variant_ids),
      )
    }
    _ -> {
      let #(options, identity_after_options) =
        make_created_option_records(identity, product.id, option_inputs)
      let positioned_options = sort_and_position_options(options)
      let #(default_variant, final_identity, variant_ids) =
        make_default_variant_for_options(
          identity_after_options,
          product,
          positioned_options,
        )
      let synced_options =
        sync_product_options_with_variants(positioned_options, [default_variant])
      let option_ids =
        list.append(
          list.map(synced_options, fn(option) { option.id }),
          list.flat_map(synced_options, fn(option) {
            list.map(option.option_values, fn(value) { value.id })
          }),
        )
      #(
        synced_options,
        default_variant,
        final_identity,
        list.append(option_ids, variant_ids),
      )
    }
  }
}

@internal
pub fn read_variant_selected_options(
  input: Dict(String, ResolvedValue),
  fallback: List(ProductVariantSelectedOptionRecord),
) -> List(ProductVariantSelectedOptionRecord) {
  case dict.get(input, "selectedOptions") {
    Ok(ListVal(values)) -> {
      let selected =
        list.filter_map(values, fn(value) {
          case value {
            ObjectVal(fields) -> read_variant_selected_option(fields)
            _ -> Error(Nil)
          }
        })
      case selected {
        [] -> fallback
        _ -> selected
      }
    }
    _ -> read_variant_option_values(input, fallback)
  }
}

@internal
pub fn create_single_option_input_errors(
  input: Dict(String, ResolvedValue),
  index: Int,
  existing_options: List(ProductOptionRecord),
  existing_variants: List(ProductVariantRecord),
  replacing_default: Bool,
) -> List(ProductUserError) {
  let name_errors = case read_string_field(input, "name") {
    None -> [
      ProductUserError(
        ["options", int.to_string(index), "name"],
        "Each option must have a name specified.",
        Some("OPTION_NAME_MISSING"),
      ),
    ]
    Some(name) -> {
      let trimmed = string.trim(name)
      case
        string.length(trimmed) == 0,
        string.length(name) > product_option_name_limit,
        option_name_exists(existing_options, name)
      {
        True, _, _ -> [
          ProductUserError(
            ["options", int.to_string(index), "name"],
            "Each option must have a name specified.",
            Some("OPTION_NAME_MISSING"),
          ),
        ]
        _, True, _ -> [
          ProductUserError(
            ["options", int.to_string(index)],
            "Option name is too long.",
            Some("OPTION_NAME_TOO_LONG"),
          ),
        ]
        _, _, True -> [
          ProductUserError(
            ["options", int.to_string(index)],
            "Option '" <> name <> "' already exists.",
            Some("OPTION_ALREADY_EXISTS"),
          ),
        ]
        _, _, False -> []
      }
    }
  }
  let value_errors =
    create_option_value_errors(
      input,
      index,
      existing_variants,
      replacing_default,
    )
  list.append(name_errors, value_errors)
}

@internal
pub fn validate_product_option_update_inputs(
  existing_options: List(ProductOptionRecord),
  target_option: ProductOptionRecord,
  option_input: Dict(String, ResolvedValue),
  values_to_add: List(Dict(String, ResolvedValue)),
  values_to_update: List(Dict(String, ResolvedValue)),
  value_ids_to_delete: List(String),
) -> List(ProductUserError) {
  list.append(
    update_option_name_errors(existing_options, target_option, option_input),
    list.append(
      update_option_value_input_errors(values_to_add, "optionValuesToAdd"),
      list.append(
        option_value_already_exists_errors(
          target_option.option_values,
          values_to_add,
        ),
        list.append(
          update_option_value_input_errors(
            values_to_update,
            "optionValuesToUpdate",
          ),
          update_option_result_value_errors(
            target_option,
            values_to_add,
            values_to_update,
            value_ids_to_delete,
          ),
        ),
      ),
    ),
  )
}

@internal
pub fn make_options_from_variant_selections(
  identity: SyntheticIdentityRegistry,
  product_id: String,
  variants: List(ProductVariantRecord),
) -> #(List(ProductOptionRecord), SyntheticIdentityRegistry) {
  let #(options, next_identity) =
    upsert_variant_selections_into_options(identity, product_id, [], variants)
  #(sync_product_options_with_variants(options, variants), next_identity)
}
