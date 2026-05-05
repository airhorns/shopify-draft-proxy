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
import shopify_draft_proxy/proxy/products/inventory_l00.{
  ensure_inventory_quantity, is_on_hand_component_quantity_name,
  locations_payload,
}
import shopify_draft_proxy/proxy/products/products_l00.{
  enumerate_items_loop, finish_handle_parts, is_handle_digit, is_handle_grapheme,
  is_product_tag_identity_whitespace, product_metafield_to_core,
  product_numeric_id_from_gid, product_set_product_suspended_error,
  two_digit_cents,
}
import shopify_draft_proxy/proxy/products/publications_l00.{
  products_published_to_publication,
}
import shopify_draft_proxy/proxy/products/shared_l00.{
  dedupe_preserving_order, max_input_size_error, read_list_field_length,
  read_object_list_field, read_string_argument, read_string_field,
  trimmed_non_empty,
}
import shopify_draft_proxy/proxy/products/types.{
  type ProductUserError, type RenamedOptionValue, ProductUserError,
  product_description_html_limit_bytes, product_set_file_limit,
  product_string_character_limit, product_tag_character_limit, product_tag_limit,
} as product_types
import shopify_draft_proxy/proxy/products/variants_l00.{
  compare_optional_strings_as_empty, optional_string, optional_string_json,
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
pub fn product_by_identifier(
  store: Store,
  identifier: Dict(String, ResolvedValue),
) -> Option(ProductRecord) {
  case read_string_field(identifier, "id") {
    Some(id) -> store.get_effective_product_by_id(store, id)
    None ->
      case read_string_field(identifier, "handle") {
        Some(handle) -> store.get_effective_product_by_handle(store, handle)
        None -> None
      }
  }
}

@internal
pub fn serialize_product_metafield(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let namespace = read_string_argument(field, variables, "namespace")
  let key = read_string_argument(field, variables, "key")
  let found =
    store.get_effective_metafields_by_owner_id(store, owner_id)
    |> list.find(fn(metafield) {
      metafield.namespace == option.unwrap(namespace, "")
      && metafield.key == option.unwrap(key, "")
    })
    |> option.from_result
  case found {
    Some(metafield) ->
      metafields.serialize_metafield_selection(
        product_metafield_to_core(metafield),
        field,
        default_selected_field_options(),
      )
    None -> json.null()
  }
}

@internal
pub fn serialize_product_metafields_connection(
  store: Store,
  owner_id: String,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let namespace = read_string_argument(field, variables, "namespace")
  let records =
    store.get_effective_metafields_by_owner_id(store, owner_id)
    |> list.filter(fn(metafield) {
      case namespace {
        Some(ns) -> metafield.namespace == ns
        None -> True
      }
    })
    |> list.map(product_metafield_to_core)
  metafields.serialize_metafields_connection(
    records,
    field,
    variables,
    default_selected_field_options(),
  )
}

@internal
pub fn published_products_count_for_field(
  store: Store,
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Int {
  case read_string_argument(field, variables, "publicationId") {
    Some(publication_id) ->
      products_published_to_publication(store, publication_id) |> list.length
    None ->
      store.list_effective_products(store)
      |> list.filter(fn(product) {
        product.status == "ACTIVE" && !list.is_empty(product.publication_ids)
      })
      |> list.length
  }
}

@internal
pub fn compare_products_by_sort_key(
  left: ProductRecord,
  right: ProductRecord,
  sort_key: String,
) -> order.Order {
  case sort_key {
    "TITLE" ->
      case string.compare(left.title, right.title) {
        order.Eq -> string.compare(left.id, right.id)
        other -> other
      }
    "VENDOR" ->
      case compare_optional_strings_as_empty(left.vendor, right.vendor) {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    "PRODUCT_TYPE" ->
      case
        compare_optional_strings_as_empty(left.product_type, right.product_type)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    "PUBLISHED_AT" ->
      case
        compare_optional_strings_as_empty(left.published_at, right.published_at)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    "UPDATED_AT" ->
      case
        compare_optional_strings_as_empty(left.updated_at, right.updated_at)
      {
        order.Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        other -> other
      }
    "ID" -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
    _ -> order.Eq
  }
}

@internal
pub fn product_matches_search_text(
  product: ProductRecord,
  raw_value: String,
) -> Bool {
  let searchable_values = [
    product.title,
    product.handle,
    option.unwrap(product.vendor, ""),
    option.unwrap(product.product_type, ""),
  ]
  list.any(list.append(searchable_values, product.tags), fn(candidate) {
    search_query_parser.matches_search_query_string(
      Some(candidate),
      raw_value,
      search_query_parser.IncludesMatch,
      product_string_match_options(),
    )
  })
}

@internal
pub fn product_numeric_id(product: ProductRecord) -> Int {
  case product.legacy_resource_id {
    Some(value) ->
      case int.parse(value) {
        Ok(parsed) -> parsed
        Error(_) -> product_numeric_id_from_gid(product.id)
      }
    None -> product_numeric_id_from_gid(product.id)
  }
}

@internal
pub fn serialize_product_duplicate_job(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let id = read_string_argument(field, variables, "id")
  let entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "id" -> #(key, optional_string(id))
              "done" -> #(key, json.bool(True))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(entries)
}

@internal
pub fn enumerate_items(items: List(a)) -> List(#(a, Int)) {
  enumerate_items_loop(items, 0, [])
}

@internal
pub fn product_tags_max_input_size_errors(
  root: String,
  input_name: String,
  input: Dict(String, ResolvedValue),
) -> List(Json) {
  let path = case input_name {
    "" -> [root, "tags"]
    _ -> [root, input_name, "tags"]
  }
  case read_list_field_length(input, "tags") {
    Some(length) if length > product_tag_limit -> [
      max_input_size_error(length, product_tag_limit, path),
    ]
    _ -> []
  }
}

@internal
pub fn product_set_file_limit_errors(
  input: Dict(String, ResolvedValue),
) -> List(ProductOperationUserErrorRecord) {
  let files = read_object_list_field(input, "files")
  case list.length(files) > product_set_file_limit {
    True -> [
      ProductOperationUserErrorRecord(
        field: Some(["input", "files"]),
        message: "Files count is over the allowed limit.",
        code: Some("INVALID_INPUT"),
      ),
    ]
    False -> []
  }
}

@internal
pub fn product_set_input_product_reference(
  input: Dict(String, ResolvedValue),
) -> Option(#(String, List(String))) {
  case read_string_field(input, "id") {
    Some(id) -> Some(#(id, ["input", "id"]))
    None ->
      case read_string_field(input, "productId") {
        Some(product_id) -> Some(#(product_id, ["input", "productId"]))
        None -> None
      }
  }
}

@internal
pub fn product_set_identifier_reference_field(
  identifier: Dict(String, ResolvedValue),
) -> List(String) {
  case read_string_field(identifier, "id") {
    Some(_) -> ["identifier", "id"]
    None ->
      case read_string_field(identifier, "handle") {
        Some(_) -> ["identifier", "handle"]
        None -> ["identifier"]
      }
  }
}

@internal
pub fn validate_product_set_resolved_product(
  product: ProductRecord,
) -> Result(Option(ProductRecord), ProductOperationUserErrorRecord) {
  case product.status == "SUSPENDED" {
    True -> Error(product_set_product_suspended_error())
    False -> Ok(Some(product))
  }
}

@internal
pub fn mirrored_custom_product_type_length_errors(
  input: Dict(String, ResolvedValue),
  field_prefix: List(String),
) -> List(ProductUserError) {
  case read_string_field(input, "customProductType") {
    Some(_) -> []
    None ->
      case read_string_field(input, "productType") {
        Some(value) ->
          case string.length(value) > product_string_character_limit {
            True -> [
              ProductUserError(
                list.append(field_prefix, ["customProductType"]),
                "Custom product type is too long (maximum is 255 characters)",
                None,
              ),
            ]
            False -> []
          }
        None -> []
      }
  }
}

@internal
pub fn product_string_length_validation_error(
  input: Dict(String, ResolvedValue),
  field_prefix: List(String),
  field_name: String,
  label: String,
) -> List(ProductUserError) {
  case read_string_field(input, field_name) {
    Some(value) ->
      case string.length(value) > product_string_character_limit {
        True -> [
          ProductUserError(
            list.append(field_prefix, [field_name]),
            label <> " is too long (maximum is 255 characters)",
            None,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn product_description_html_validation_errors(
  input: Dict(String, ResolvedValue),
  field_prefix: List(String),
) -> List(ProductUserError) {
  case read_string_field(input, "descriptionHtml") {
    Some(value) ->
      case string.byte_size(value) > product_description_html_limit_bytes {
        True -> [
          ProductUserError(
            list.append(field_prefix, ["bodyHtml"]),
            "Body (HTML) is too big (maximum is 512 KB)",
            None,
          ),
        ]
        False -> []
      }
    None -> []
  }
}

@internal
pub fn product_tag_values_validation_error(
  tags: List(String),
) -> Option(ProductUserError) {
  case
    list.any(tags, fn(tag) {
      case trimmed_non_empty(tag) {
        Ok(trimmed) -> string.length(trimmed) > product_tag_character_limit
        Error(_) -> False
      }
    })
  {
    True -> Some(ProductUserError(["tags"], "Product tags is invalid", None))
    False -> None
  }
}

@internal
pub fn product_delete_input_object_error(
  message: String,
  loc: Option(Location),
  document: String,
  extensions: List(#(String, Json)),
) -> Json {
  let base = [#("message", json.string(message))]
  let with_locations = case locations_payload(loc, document) {
    Some(locations) -> list.append(base, [#("locations", locations)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array(["mutation", "productDelete", "input", "id"], json.string),
      ),
      #("extensions", json.object(extensions)),
    ]),
  )
}

@internal
pub fn build_product_delete_invalid_variable_error(
  variable_name: String,
  value: Json,
  loc: Option(Location),
  document: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Variable $"
        <> variable_name
        <> " of type ProductDeleteInput! was provided invalid value for id (Expected value to not be null)",
      ),
    ),
  ]
  let with_locations = case locations_payload(loc, document) {
    Some(locations) -> list.append(base, [#("locations", locations)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "extensions",
        json.object([
          #("code", json.string("INVALID_VARIABLE")),
          #("value", value),
          #(
            "problems",
            json.preprocessed_array([
              json.object([
                #("path", json.array(["id"], json.string)),
                #("explanation", json.string("Expected value to not be null")),
              ]),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

@internal
pub fn serialize_product_user_errors_json(
  errors: List(ProductUserError),
  field: Selection,
) -> Json {
  let selections =
    get_selected_child_fields(field, default_selected_field_options())
  json.array(errors, fn(error) {
    let ProductUserError(field: path, message: message, code: code) = error
    json.object(
      list.map(selections, fn(selection) {
        let key = get_field_response_key(selection)
        let value = case selection {
          Field(name: name, ..) ->
            case name.value {
              "field" -> json.array(path, json.string)
              "message" -> json.string(message)
              "code" -> optional_string_json(code)
              _ -> json.null()
            }
          _ -> json.null()
        }
        #(key, value)
      }),
    )
  })
}

@internal
pub fn read_tag_inputs(
  args: Dict(String, ResolvedValue),
  allow_comma_separated_string: Bool,
) -> List(String) {
  let values = case dict.get(args, "tags") {
    Ok(ListVal(items)) ->
      list.filter_map(items, fn(value) {
        case value {
          StringVal(tag) -> trimmed_non_empty(tag)
          _ -> Error(Nil)
        }
      })
    Ok(StringVal(raw)) ->
      case allow_comma_separated_string {
        True ->
          string.split(raw, on: ",")
          |> list.filter_map(trimmed_non_empty)
        False -> []
      }
    _ -> []
  }
  dedupe_preserving_order(values)
}

@internal
pub fn vendor_from_shop_domain(domain: String) -> Option(String) {
  case string.split(domain, ".") {
    [subdomain, "myshopify", "com"] ->
      trimmed_non_empty(subdomain) |> option.from_result
    _ -> None
  }
}

@internal
pub fn collapse_product_tag_identity_whitespace(value: String) -> String {
  let #(reversed, _) =
    value
    |> string.to_graphemes
    |> list.fold(#([], False), fn(acc, grapheme) {
      let #(items, in_whitespace) = acc
      case is_product_tag_identity_whitespace(grapheme), in_whitespace {
        True, True -> #(items, True)
        True, False -> #([" ", ..items], True)
        False, _ -> #([grapheme, ..items], False)
      }
    })

  reversed
  |> list.reverse
  |> string.join("")
}

@internal
pub fn ensure_product_set_default_quantities(
  quantities: List(InventoryQuantityRecord),
) -> List(InventoryQuantityRecord) {
  quantities
  |> ensure_inventory_quantity("available", 0)
  |> ensure_inventory_quantity("on_hand", 0)
  |> ensure_inventory_quantity("incoming", 0)
}

@internal
pub fn product_set_metafield_records(
  store: Store,
  identity: SyntheticIdentityRegistry,
  product_id: String,
  inputs: List(Dict(String, ResolvedValue)),
) -> #(List(ProductMetafieldRecord), SyntheticIdentityRegistry, List(String)) {
  let existing_metafields =
    store.get_effective_metafields_by_owner_id(store, product_id)
  let #(reversed, final_identity, ids) =
    list.fold(inputs, #([], identity, []), fn(acc, input) {
      let #(records, current_identity, collected_ids) = acc
      let existing = case read_string_field(input, "id") {
        Some(id) ->
          existing_metafields
          |> list.find(fn(metafield) { metafield.id == id })
          |> option.from_result
        None -> None
      }
      let #(metafield_id, next_identity, ids) = case existing {
        Some(metafield) -> #(metafield.id, current_identity, [metafield.id])
        None -> {
          let #(id, next_identity) =
            synthetic_identity.make_synthetic_gid(current_identity, "Metafield")
          #(id, next_identity, [id])
        }
      }
      let type_ =
        read_string_field(input, "type")
        |> option.or(option.then(existing, fn(metafield) { metafield.type_ }))
      let value =
        read_string_field(input, "value")
        |> option.or(option.then(existing, fn(metafield) { metafield.value }))
      let metafield =
        ProductMetafieldRecord(
          id: metafield_id,
          owner_id: product_id,
          namespace: read_string_field(input, "namespace")
            |> option.unwrap(
              option.map(existing, fn(metafield) { metafield.namespace })
              |> option.unwrap(""),
            ),
          key: read_string_field(input, "key")
            |> option.unwrap(
              option.map(existing, fn(metafield) { metafield.key })
              |> option.unwrap(""),
            ),
          type_: type_,
          value: value,
          compare_digest: None,
          json_value: None,
          created_at: option.then(existing, fn(metafield) {
            metafield.created_at
          }),
          updated_at: option.then(existing, fn(metafield) {
            metafield.updated_at
          }),
          owner_type: Some("PRODUCT"),
          market_localizable_content: option.map(existing, fn(metafield) {
            metafield.market_localizable_content
          })
            |> option.unwrap([]),
        )
      #([metafield, ..records], next_identity, list.append(collected_ids, ids))
    })
  #(list.reverse(reversed), final_identity, ids)
}

@internal
pub fn normalize_product_handle(value: String) -> String {
  value
  |> string.trim
  |> string.lowercase
  |> string.to_graphemes
  |> list.fold(#([], ""), fn(acc, grapheme) {
    let #(parts, current) = acc
    case is_handle_grapheme(grapheme) {
      True -> #(parts, current <> grapheme)
      False ->
        case current {
          "" -> #(parts, "")
          _ -> #([current, ..parts], "")
        }
    }
  })
  |> finish_handle_parts
}

@internal
pub fn split_reversed_trailing_digits(
  graphemes: List(String),
  digits: List(String),
) -> #(List(String), List(String)) {
  case graphemes {
    [] -> #(digits, [])
    [first, ..rest] ->
      case is_handle_digit(first) {
        True -> split_reversed_trailing_digits(rest, [first, ..digits])
        False -> #(digits, graphemes)
      }
  }
}

@internal
pub fn on_hand_component_delta(name: String, delta: Int) -> Int {
  case is_on_hand_component_quantity_name(name) {
    True -> delta
    False -> 0
  }
}

@internal
pub fn format_price_amount(amount: String) -> String {
  let trimmed = string.trim(amount)
  case string.split(trimmed, on: ".") {
    [whole] -> whole <> ".00"
    [whole, cents, ..] -> whole <> "." <> two_digit_cents(cents)
    _ -> trimmed
  }
}

@internal
pub fn read_product_status_field(
  input: Dict(String, ResolvedValue),
) -> Option(String) {
  case read_string_field(input, "status") {
    Some("ACTIVE") -> Some("ACTIVE")
    Some("ARCHIVED") -> Some("ARCHIVED")
    Some("DRAFT") -> Some("DRAFT")
    _ -> None
  }
}

@internal
pub fn updated_product_seo(
  current: ProductSeoRecord,
  input: Dict(String, ResolvedValue),
) -> ProductSeoRecord {
  case dict.get(input, "seo") {
    Ok(ObjectVal(seo)) ->
      ProductSeoRecord(
        title: read_string_field(seo, "title") |> option.or(current.title),
        description: read_string_field(seo, "description")
          |> option.or(current.description),
      )
    _ -> current
  }
}

@internal
pub fn renamed_value_name(
  renamed_values: List(RenamedOptionValue),
  current_name: String,
) -> String {
  case renamed_values {
    [] -> current_name
    [#(from, to), ..rest] ->
      case from == current_name {
        True -> to
        False -> renamed_value_name(rest, current_name)
      }
  }
}
