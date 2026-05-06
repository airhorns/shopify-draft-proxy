//// Products-domain submodule: products_validation.
//// Combines layered files: products_l03, products_l04.

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
import shopify_draft_proxy/proxy/products/inventory_core.{
  variant_tracks_inventory,
}
import shopify_draft_proxy/proxy/products/inventory_validation.{
  find_variable_definition_location,
  product_set_inventory_quantities_limit_errors, variant_available_quantity,
}
import shopify_draft_proxy/proxy/products/products_core.{
  build_product_delete_invalid_variable_error,
  build_product_delete_missing_input_id_error,
  build_product_delete_null_input_id_error, dedup_base_and_next_suffix,
  ensure_unique_handle, matches_nullable_product_timestamp,
  product_by_identifier, product_delete_payload,
  product_description_html_validation_errors, product_handle_in_use,
  product_handle_in_use_by_other, product_id_matches,
  product_matches_search_text, product_searchable_status,
  product_searchable_tags, product_set_file_limit_errors,
  product_set_identifier_has_reference, product_set_identifier_reference_field,
  product_set_product_does_not_exist_error, product_sort_cursor_payload,
  product_string_length_validation_errors, product_tag_identity_key,
  read_collision_checked_explicit_product_handle,
  resolve_product_set_input_product, slugify_product_handle,
  validate_product_set_resolved_product, vendor_from_shop_domain,
  vendor_from_shopify_admin_origin,
}
import shopify_draft_proxy/proxy/products/shared.{
  find_object_field, mutation_error_result, mutation_result,
  read_non_empty_string_field, read_string_argument, read_string_field,
  resolved_input_to_json, trimmed_non_empty,
}
import shopify_draft_proxy/proxy/products/shared_money.{money_v2_source}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type ProductDerivedSummary, type ProductUserError,
  MutationFieldResult, ProductDerivedSummary, ProductUserError,
  blank_product_user_error,
} as product_types
import shopify_draft_proxy/proxy/products/variants_helpers.{
  has_only_default_variant, product_search_parse_options,
  product_string_match_options,
}
import shopify_draft_proxy/proxy/products/variants_options.{
  max_variant_price_amount, min_variant_price_amount,
  product_set_option_limit_errors,
}
import shopify_draft_proxy/proxy/products/variants_options_core.{
  product_searchable_variants, product_set_variant_limit_errors,
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

// ===== from products_l03 =====
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

// ===== from products_l04 =====
@internal
pub fn filtered_products(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> List(ProductRecord) {
  search_query_parser.apply_search_query(
    store.list_effective_products(store),
    read_string_argument(field, variables, "query"),
    product_search_parse_options(),
    fn(product, term) {
      product_matches_positive_query_term(store, product, term)
    },
  )
}

@internal
pub fn product_sort_cursor_timestamp(
  product: ProductRecord,
  value: Option(String),
) -> String {
  let timestamp = case value {
    Some(raw) -> iso_timestamp.parse_iso(raw) |> result.unwrap(0)
    None -> 0
  }
  product_sort_cursor_int(product, timestamp)
}

@internal
pub fn handle_product_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, ResolvedValue),
) -> MutationFieldResult {
  let key = get_field_response_key(field)
  case product_delete_input_error(document, field, variables) {
    Some(error) -> mutation_error_result(key, store, identity, [error])
    None -> {
      let args = graphql_helpers.field_args(field, variables)
      let input = graphql_helpers.read_arg_object(args, "input")
      let id = case input {
        Some(input) -> graphql_helpers.read_arg_string(input, "id")
        None -> graphql_helpers.read_arg_string(args, "id")
      }
      case id {
        None ->
          mutation_error_result(key, store, identity, [
            build_product_delete_invalid_variable_error(
              "input",
              json.object([]),
              None,
              document,
            ),
          ])
        Some(product_id) ->
          case store.get_effective_product_by_id(store, product_id) {
            None ->
              mutation_result(
                key,
                product_delete_payload(
                  None,
                  [ProductUserError(["id"], "Product does not exist", None)],
                  field,
                  fragments,
                ),
                store,
                identity,
                [],
              )
            Some(_) -> {
              let next_store = store.delete_staged_product(store, product_id)
              mutation_result(
                key,
                product_delete_payload(Some(product_id), [], field, fragments),
                next_store,
                identity,
                [product_id],
              )
            }
          }
      }
    }
  }
}

@internal
pub fn product_scalar_validation_errors(
  input: Dict(String, ResolvedValue),
  field_prefix: List(String),
  require_title require_title: Bool,
) -> List(ProductUserError) {
  list.append(
    product_title_validation_errors(
      input,
      field_prefix,
      require_missing: require_title,
    ),
    list.append(
      product_string_length_validation_errors(input, field_prefix),
      product_description_html_validation_errors(input, field_prefix),
    ),
  )
}

@internal
pub fn product_vendor_for_create(
  store: Store,
  shopify_admin_origin: String,
  input: Dict(String, ResolvedValue),
) -> Option(String) {
  read_non_empty_string_field(input, "vendor")
  |> option.or(default_product_vendor(store, shopify_admin_origin))
}

@internal
pub fn normalize_product_tags(tags: List(String)) -> List(String) {
  let #(reversed, _) =
    tags
    |> list.filter_map(trimmed_non_empty)
    |> list.fold(#([], dict.new()), fn(acc, tag) {
      let #(items, seen) = acc
      let key = product_tag_identity_key(tag)
      case dict.has_key(seen, key) {
        True -> #(items, seen)
        False -> #([tag, ..items], dict.insert(seen, key, True))
      }
    })

  reversed
  |> list.reverse
  |> list.sort(compare_product_tags)
}

@internal
pub fn duplicated_product_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  source_product: ProductRecord,
  new_title: Option(String),
) -> #(ProductRecord, SyntheticIdentityRegistry) {
  let title = new_title |> option.unwrap(source_product.title <> " Copy")
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "Product")
  let base_handle = slugify_product_handle(title)
  let handle = ensure_unique_product_handle(store, base_handle)
  #(
    ProductRecord(
      ..source_product,
      id: id,
      legacy_resource_id: None,
      title: title,
      handle: handle,
      status: "DRAFT",
      cursor: None,
    ),
    next_identity,
  )
}

@internal
pub fn product_derived_summary(
  variants: List(ProductVariantRecord),
) -> ProductDerivedSummary {
  let tracked_variants = list.filter(variants, variant_tracks_inventory)
  ProductDerivedSummary(
    price_range_min: min_variant_price_amount(variants),
    price_range_max: max_variant_price_amount(variants),
    total_variants: Some(list.length(variants)),
    has_only_default_variant: Some(has_only_default_variant(variants)),
    has_out_of_stock_variants: Some(
      list.any(tracked_variants, fn(variant) {
        variant_available_quantity(variant) <= 0
      }),
    ),
    total_inventory: Some(
      list.fold(tracked_variants, 0, fn(total, variant) {
        total + variant_available_quantity(variant)
      }),
    ),
    tracks_inventory: Some(!list.is_empty(tracked_variants)),
  )
}
