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
import shopify_draft_proxy/proxy/products/inventory_l02.{
  find_variable_definition_location,
  product_set_inventory_quantities_limit_errors,
}
import shopify_draft_proxy/proxy/products/products_l00.{
  ensure_unique_handle, matches_nullable_product_timestamp,
  product_handle_in_use, product_handle_in_use_by_other,
  product_searchable_status, product_searchable_tags,
  product_set_identifier_has_reference, product_set_product_does_not_exist_error,
}
import shopify_draft_proxy/proxy/products/products_l01.{
  build_product_delete_invalid_variable_error, product_by_identifier,
  product_matches_search_text, product_set_file_limit_errors,
  product_set_identifier_reference_field, validate_product_set_resolved_product,
  vendor_from_shop_domain,
}
import shopify_draft_proxy/proxy/products/products_l02.{
  build_product_delete_missing_input_id_error,
  build_product_delete_null_input_id_error, dedup_base_and_next_suffix,
  product_id_matches, product_sort_cursor_payload, product_tag_identity_key,
  read_collision_checked_explicit_product_handle,
  resolve_product_set_input_product, vendor_from_shopify_admin_origin,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  find_object_field, read_string_field, trimmed_non_empty,
}
import shopify_draft_proxy/proxy/products/shared_l01.{resolved_input_to_json}
import shopify_draft_proxy/proxy/products/shared_l02.{money_v2_source}
import shopify_draft_proxy/proxy/products/types.{
  type ProductUserError, ProductUserError, blank_product_user_error,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  product_string_match_options,
}
import shopify_draft_proxy/proxy/products/variants_l01.{
  product_searchable_variants, product_set_variant_limit_errors,
}
import shopify_draft_proxy/proxy/products/variants_l02.{
  product_set_option_limit_errors,
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
pub fn product_matches_positive_query_term(
  store: Store,
  product: ProductRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  case option.map(term.field, string.lowercase) {
    None -> product_matches_search_text(product, term.value)
    Some("id") -> product_id_matches(product, term.value)
    Some("title") ->
      search_query_parser.matches_search_query_string(
        Some(product.title),
        search_query_parser.search_query_term_value(term),
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
    Some("handle") ->
      search_query_parser.matches_search_query_string(
        Some(product.handle),
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("tag") ->
      list.any(product_searchable_tags(store, product), fn(tag) {
        search_query_parser.matches_search_query_string(
          Some(tag),
          search_query_parser.search_query_term_value(term),
          search_query_parser.ExactMatch,
          product_string_match_options(),
        )
      })
    Some("product_type") ->
      search_query_parser.matches_search_query_string(
        product.product_type,
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("vendor") ->
      search_query_parser.matches_search_query_string(
        product.vendor,
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("status") ->
      search_query_parser.matches_search_query_string(
        Some(product_searchable_status(store, product)),
        search_query_parser.search_query_term_value(term),
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("sku") ->
      product_searchable_variants(store, product.id)
      |> list.any(fn(variant) {
        search_query_parser.matches_search_query_string(
          variant.sku,
          search_query_parser.search_query_term_value(term),
          search_query_parser.ExactMatch,
          product_string_match_options(),
        )
      })
    Some("inventory_total") ->
      search_query_parser.matches_search_query_number(
        option.map(product.total_inventory, int.to_float),
        term,
      )
    Some("tag_not") ->
      !list.any(product_searchable_tags(store, product), fn(tag) {
        search_query_parser.matches_search_query_string(
          Some(tag),
          search_query_parser.search_query_term_value(term),
          search_query_parser.ExactMatch,
          product_string_match_options(),
        )
      })
    Some("published_at") ->
      matches_nullable_product_timestamp(product.published_at, term)
    Some("updated_at") ->
      matches_nullable_product_timestamp(product.updated_at, term)
    Some("created_at") ->
      matches_nullable_product_timestamp(product.created_at, term)
    _ -> True
  }
}

@internal
pub fn product_sort_cursor_string(
  product: ProductRecord,
  value: String,
) -> String {
  product_sort_cursor_payload(product, json.to_string(json.string(value)))
}

@internal
pub fn product_sort_cursor_int(product: ProductRecord, value: Int) -> String {
  product_sort_cursor_payload(product, int.to_string(value))
}

@internal
pub fn product_price_range_source(
  product: ProductRecord,
  currency_code: String,
) -> SourceValue {
  case product.price_range_min, product.price_range_max {
    Some(min_amount), Some(max_amount) ->
      src_object([
        #("minVariantPrice", money_v2_source(min_amount, currency_code)),
        #("maxVariantPrice", money_v2_source(max_amount, currency_code)),
      ])
    _, _ -> SrcNull
  }
}

@internal
pub fn product_set_shape_validation_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  list.append(
    product_set_variant_limit_errors(input),
    list.append(
      product_set_option_limit_errors(input),
      list.append(
        product_set_file_limit_errors(input),
        product_set_inventory_quantities_limit_errors(input),
      ),
    ),
  )
}

@internal
pub fn resolve_product_set_existing_product(
  store: Store,
  identifier: Option(Dict(String, ResolvedValue)),
  input: Dict(String, ResolvedValue),
) -> Result(Option(ProductRecord), ProductOperationUserErrorRecord) {
  case identifier {
    Some(identifier) ->
      case product_set_identifier_has_reference(identifier) {
        True ->
          case product_by_identifier(store, identifier) {
            Some(product) -> validate_product_set_resolved_product(product)
            None ->
              Error(
                product_set_product_does_not_exist_error(
                  product_set_identifier_reference_field(identifier),
                ),
              )
          }
        False -> resolve_product_set_input_product(store, input)
      }
    None -> resolve_product_set_input_product(store, input)
  }
}

@internal
pub fn product_title_validation_errors(
  input: Dict(String, ResolvedValue),
  field_prefix: List(String),
  require_missing require_missing: Bool,
) -> List(ProductUserError) {
  case read_string_field(input, "title") {
    Some(value) ->
      case string.length(string.trim(value)) == 0 {
        True -> [
          blank_product_user_error(
            list.append(field_prefix, ["title"]),
            "Title can't be blank",
          ),
        ]
        False -> []
      }
    None ->
      case require_missing {
        True -> [
          blank_product_user_error(
            list.append(field_prefix, ["title"]),
            "Title can't be blank",
          ),
        ]
        False -> []
      }
  }
}

@internal
pub fn explicit_product_handle_collision_errors(
  store: Store,
  input: Dict(String, ResolvedValue),
  allowed_product_id: Option(String),
) -> List(ProductUserError) {
  case read_collision_checked_explicit_product_handle(input) {
    Some(handle) ->
      case product_handle_in_use_by_other(store, handle, allowed_product_id) {
        True -> [
          ProductUserError(
            ["handle"],
            "Handle '"
              <> handle
              <> "' already in use. Please provide a new handle.",
            None,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn remove_product_tags_by_identity(
  current_tags: List(String),
  tags_to_remove: List(String),
) -> List(String) {
  let removal_keys = list.map(tags_to_remove, product_tag_identity_key)
  list.filter(current_tags, fn(tag) {
    !list.contains(removal_keys, product_tag_identity_key(tag))
  })
}

@internal
pub fn product_delete_input_error(
  document: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(Json) {
  let arguments = case field {
    Field(arguments: args, ..) -> args
    _ -> []
  }
  case find_argument(arguments, "input") {
    Some(argument) ->
      case argument.value {
        ObjectValue(fields: fields, loc: loc) ->
          case find_object_field(fields, "id") {
            None ->
              Some(build_product_delete_missing_input_id_error(loc, document))
            Some(ObjectField(value: NullValue(..), ..)) ->
              Some(build_product_delete_null_input_id_error(loc, document))
            Some(_) -> None
          }
        VariableValue(variable: variable) -> {
          let args = graphql_helpers.field_args(field, variables)
          let input = graphql_helpers.read_arg_object(args, "input")
          let invalid = case input {
            Some(input) ->
              case dict.get(input, "id") {
                Ok(StringVal(_)) -> False
                _ -> True
              }
            None -> True
          }
          case invalid {
            False -> None
            True ->
              Some(build_product_delete_invalid_variable_error(
                variable.name.value,
                resolved_input_to_json(input),
                find_variable_definition_location(document, variable.name.value),
                document,
              ))
          }
        }
        _ -> None
      }
    None -> None
  }
}

@internal
pub fn default_product_vendor(
  store: Store,
  shopify_admin_origin: String,
) -> Option(String) {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case trimmed_non_empty(shop.name) {
        Ok(name) -> Some(name)
        Error(_) -> vendor_from_shop_domain(shop.myshopify_domain)
      }
    None -> vendor_from_shopify_admin_origin(shopify_admin_origin)
  }
}

@internal
pub fn compare_product_tags(a: String, b: String) -> order.Order {
  let a_key = product_tag_identity_key(a)
  let b_key = product_tag_identity_key(b)
  case string.compare(a_key, b_key) {
    order.Eq -> string.compare(a, b)
    other -> other
  }
}

@internal
pub fn ensure_unique_product_handle(store: Store, handle: String) -> String {
  let in_use = fn(candidate) { product_handle_in_use(store, candidate) }
  case in_use(handle) {
    True -> {
      let #(base_handle, suffix) = dedup_base_and_next_suffix(handle)
      ensure_unique_handle(base_handle, suffix, in_use)
    }
    False -> handle
  }
}
