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
  collection_rule_set_has_rules,
}
import shopify_draft_proxy/proxy/products/collections_l01.{
  collection_has_product_id, read_collection_rule_set,
}
import shopify_draft_proxy/proxy/products/products_l01.{
  normalize_product_handle, updated_product_seo,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  job_source, read_string_field,
}
import shopify_draft_proxy/proxy/products/shared_l01.{
  parse_unsigned_int_string, read_non_empty_string_field, resource_id_matches,
  user_errors_source,
}
import shopify_draft_proxy/proxy/products/types.{
  type CollectionRuleSetPresence, type ProductUserError, ProductUserError,
  RuleSetAbsent, RuleSetCustom, RuleSetSmart,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  product_string_match_options,
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
pub fn collection_matches_positive_query_term(
  store: Store,
  collection: CollectionRecord,
  term: search_query_parser.SearchQueryTerm,
) -> Bool {
  let value = search_query_parser.search_query_term_value(term)
  case option.map(term.field, string.lowercase) {
    None ->
      search_query_parser.matches_search_query_string(
        Some(collection.title),
        value,
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
      || search_query_parser.matches_search_query_string(
        Some(collection.handle),
        value,
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
    Some("title") ->
      search_query_parser.matches_search_query_string(
        Some(collection.title),
        value,
        search_query_parser.IncludesMatch,
        product_string_match_options(),
      )
    Some("handle") ->
      search_query_parser.matches_search_query_string(
        Some(collection.handle),
        value,
        search_query_parser.ExactMatch,
        product_string_match_options(),
      )
    Some("collection_type") -> {
      let normalized =
        search_query_parser.strip_search_query_value_quotes(value)
        |> string.trim
        |> string.lowercase
      case normalized {
        "smart" -> collection.is_smart
        "custom" -> !collection.is_smart
        _ -> True
      }
    }
    Some("id") ->
      resource_id_matches(collection.id, collection.legacy_resource_id, value)
    Some("product_id") -> collection_has_product_id(store, collection.id, value)
    Some("updated_at") -> True
    Some("product_publication_status")
    | Some("publishable_status")
    | Some("published_at")
    | Some("published_status") -> True
    _ -> True
  }
}

@internal
pub fn collection_rule_set_presence(
  input: Dict(String, ResolvedValue),
) -> CollectionRuleSetPresence {
  case dict.get(input, "ruleSet") {
    Error(_) -> RuleSetAbsent
    Ok(ObjectVal(_)) ->
      case read_collection_rule_set(input) {
        Some(rule_set) ->
          case collection_rule_set_has_rules(rule_set) {
            True -> RuleSetSmart
            False -> RuleSetCustom
          }
        None -> RuleSetCustom
      }
    _ -> RuleSetCustom
  }
}

@internal
pub fn read_collection_reorder_position(
  fields: Dict(String, ResolvedValue),
) -> Option(Int) {
  case dict.get(fields, "newPosition") {
    Ok(IntVal(value)) -> Some(int.max(0, value))
    Ok(StringVal(value)) -> parse_unsigned_int_string(value)
    _ -> None
  }
}

@internal
pub fn collection_add_products_v2_payload(
  job_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let job_value = case job_id {
    Some(id) -> job_source(id, False)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionAddProductsV2Payload")),
      #("job", job_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn collection_delete_payload(
  deleted_collection_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let deleted_value = case deleted_collection_id {
    Some(id) -> SrcString(id)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionDeletePayload")),
      #("deletedCollectionId", deleted_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn collection_remove_products_payload(
  job_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let job_value = case job_id {
    Some(id) -> job_source(id, False)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionRemoveProductsPayload")),
      #("job", job_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn collection_reorder_products_payload(
  job_id: Option(String),
  user_errors: List(ProductUserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let job_value = case job_id {
    Some(id) -> job_source(id, False)
    None -> SrcNull
  }
  project_graphql_value(
    src_object([
      #("__typename", SrcString("CollectionReorderProductsPayload")),
      #("job", job_value),
      #("userErrors", user_errors_source(user_errors)),
    ]),
    get_selected_child_fields(field, default_selected_field_options()),
    fragments,
  )
}

@internal
pub fn updated_collection_record(
  identity: SyntheticIdentityRegistry,
  collection: CollectionRecord,
  input: Dict(String, ResolvedValue),
) -> #(CollectionRecord, SyntheticIdentityRegistry) {
  let #(updated_at, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let title =
    read_non_empty_string_field(input, "title")
    |> option.unwrap(collection.title)
  let handle =
    read_non_empty_string_field(input, "handle")
    |> option.unwrap(collection.handle)
  let rule_set =
    read_collection_rule_set(input)
    |> option.or(collection.rule_set)
  #(
    CollectionRecord(
      ..collection,
      title: title,
      handle: handle,
      updated_at: Some(updated_at),
      description: read_string_field(input, "description")
        |> option.or(collection.description),
      description_html: read_string_field(input, "descriptionHtml")
        |> option.or(collection.description_html),
      sort_order: read_string_field(input, "sortOrder")
        |> option.or(collection.sort_order),
      template_suffix: read_string_field(input, "templateSuffix")
        |> option.or(collection.template_suffix),
      seo: updated_product_seo(collection.seo, input),
      rule_set: rule_set,
      is_smart: rule_set
        |> option.map(collection_rule_set_has_rules)
        |> option.unwrap(collection.is_smart),
    ),
    next_identity,
  )
}

@internal
pub fn slugify_collection_handle(title: String) -> String {
  let normalized = normalize_product_handle(title)
  let handle = case normalized {
    "" -> "untitled-collection"
    _ -> normalized
  }
  case string.ends_with(handle, "product") {
    True -> string.drop_end(handle, 7) <> "collection"
    False -> handle
  }
}

@internal
pub fn collection_handle_should_dedupe(
  input: Dict(String, ResolvedValue),
) -> Bool {
  case read_non_empty_string_field(input, "handle") {
    Some(_) -> False
    None -> True
  }
}
