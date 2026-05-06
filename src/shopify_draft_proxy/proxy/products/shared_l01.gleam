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
  captured_object_field, connection_end_cursor, connection_start_cursor,
  is_decimal_digit, non_empty_string, normalize_string_catalog,
  parse_admin_api_version, read_bool_argument, read_string_field,
  resolved_value_to_captured, resolved_value_to_json, resource_tail,
  segment_after_store, string_cursor, trimmed_non_empty,
}
import shopify_draft_proxy/proxy/products/types.{
  type MutationFieldResult, type NullableFieldUserError, type NumericRead,
  type ProductUserError, MutationFieldResult, NullableFieldUserError,
  NumericMissing, NumericNotANumber, NumericNull, NumericValue, ProductUserError,
  max_product_variants,
} as product_types
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
pub fn resource_id_matches(
  resource_id: String,
  legacy_resource_id: Option(String),
  raw_value: String,
) -> Bool {
  let normalized =
    search_query_parser.strip_search_query_value_quotes(raw_value)
    |> string.trim
  case normalized {
    "" -> True
    _ -> {
      resource_id == normalized
      || option.unwrap(legacy_resource_id, "") == normalized
      || resource_tail(resource_id) == normalized
      || resource_tail(normalized) == resource_tail(resource_id)
    }
  }
}

@internal
pub fn serialize_string_connection(
  values: List(String),
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Json {
  let sorted_values = normalize_string_catalog(values)
  let ordered_values = case read_bool_argument(field, variables, "reverse") {
    Some(True) -> list.reverse(sorted_values)
    _ -> sorted_values
  }
  let window =
    paginate_connection_items(
      ordered_values,
      field,
      variables,
      string_cursor,
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: string_cursor,
      serialize_node: fn(value, _node_field, _index) { json.string(value) },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

@internal
pub fn read_include_inactive_argument(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Bool {
  case read_bool_argument(field, variables, "includeInactive") {
    Some(True) -> True
    _ -> False
  }
}

@internal
pub fn connection_page_info_source(
  items: List(a),
  get_cursor: fn(a, Int) -> String,
) -> SourceValue {
  src_object([
    #("hasNextPage", SrcBool(False)),
    #("hasPreviousPage", SrcBool(False)),
    #("startCursor", connection_start_cursor(items, get_cursor)),
    #("endCursor", connection_end_cursor(items, get_cursor)),
  ])
}

@internal
pub fn admin_api_version_from_path(path: String) -> Option(#(Int, Int)) {
  case string.split(path, "/") {
    ["", "admin", "api", version, "graphql.json"] ->
      parse_admin_api_version(version)
    _ -> None
  }
}

@internal
pub fn max_input_size_exceeded_error(
  root_name: String,
  argument_name: String,
  actual_size: Int,
  field: Selection,
  document: String,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "The input array size of "
        <> int.to_string(actual_size)
        <> " is greater than the maximum allowed of "
        <> int.to_string(max_product_variants)
        <> ".",
      ),
    ),
    #("locations", graphql_helpers.field_locations_json(field, document)),
    #("path", json.array([root_name, argument_name], json.string)),
    #(
      "extensions",
      json.object([#("code", json.string("MAX_INPUT_SIZE_EXCEEDED"))]),
    ),
  ])
}

@internal
pub fn read_idempotency_key(
  field: Selection,
  variables: Dict(String, ResolvedValue),
) -> Option(String) {
  let directive_arguments = case field {
    Field(directives: directives, ..) ->
      directives
      |> list.filter_map(fn(directive) {
        case directive {
          Directive(name: name, arguments: arguments, ..)
            if name.value == "idempotent"
          -> Ok(arguments)
          _ -> Error(Nil)
        }
      })
      |> list.first
      |> option.from_result
    _ -> None
  }
  case directive_arguments {
    None -> None
    Some(arguments) -> {
      let argument = case find_argument(arguments, "key") {
        Some(argument) -> Some(argument)
        None -> find_argument(arguments, "idempotencyKey")
      }
      case argument {
        Some(Argument(value: StringValue(value: value, ..), ..)) ->
          non_empty_string(value)
        Some(Argument(value: VariableValue(variable: variable), ..)) ->
          case dict.get(variables, variable.name.value) {
            Ok(StringVal(value)) -> non_empty_string(value)
            _ -> None
          }
        _ -> None
      }
    }
  }
}

@internal
pub fn parse_unsigned_int_string(value: String) -> Option(Int) {
  let trimmed = string.trim(value)
  case
    string.length(trimmed) > 0
    && list.all(string.to_graphemes(trimmed), is_decimal_digit)
  {
    False -> None
    True ->
      case int.parse(trimmed) {
        Ok(parsed) -> Some(parsed)
        Error(_) -> None
      }
  }
}

@internal
pub fn resolved_input_to_json(
  input: Option(Dict(String, ResolvedValue)),
) -> Json {
  case input {
    Some(fields) ->
      json.object(
        list.map(dict.to_list(fields), fn(entry) {
          let #(key, value) = entry
          #(key, resolved_value_to_json(value))
        }),
      )
    None -> json.null()
  }
}

@internal
pub fn captured_object_or_null(
  value: Option(Dict(String, ResolvedValue)),
) -> CapturedJsonValue {
  case value {
    Some(fields) ->
      CapturedObject(
        dict.to_list(fields)
        |> list.map(fn(pair) {
          let #(key, value) = pair
          #(key, resolved_value_to_captured(value))
        }),
      )
    None -> CapturedNull
  }
}

@internal
pub fn captured_string_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(String) {
  case captured_object_field(value, name) {
    Some(CapturedString(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn captured_int_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(Int) {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn captured_string_array_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(List(String)) {
  case captured_object_field(value, name) {
    Some(CapturedArray(items)) ->
      Some(
        list.filter_map(items, fn(item) {
          case item {
            CapturedString(value) -> Ok(value)
            _ -> Error(Nil)
          }
        }),
      )
    _ -> None
  }
}

@internal
pub fn captured_number_string_field(
  value: CapturedJsonValue,
  name: String,
) -> Option(String) {
  case captured_object_field(value, name) {
    Some(CapturedInt(value)) -> Some(int.to_string(value))
    Some(CapturedFloat(value)) -> Some(float.to_string(value))
    _ -> None
  }
}

@internal
pub fn mutation_result(
  key: String,
  payload: Json,
  store: Store,
  identity: SyntheticIdentityRegistry,
  staged_resource_ids: List(String),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: payload,
    store: store,
    identity: identity,
    staged_resource_ids: staged_resource_ids,
    top_level_errors: [],
    top_level_error_data_entries: [],
    staging_failed: False,
  )
}

@internal
pub fn mutation_rejected_result(
  key: String,
  payload: Json,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: payload,
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: [],
    top_level_error_data_entries: [],
    staging_failed: True,
  )
}

@internal
pub fn mutation_error_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  errors: List(Json),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: json.null(),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: errors,
    top_level_error_data_entries: [],
    staging_failed: False,
  )
}

@internal
pub fn mutation_error_with_null_data_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  errors: List(Json),
) -> MutationFieldResult {
  MutationFieldResult(
    key: key,
    payload: json.null(),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: errors,
    top_level_error_data_entries: [#(key, json.null())],
    staging_failed: False,
  )
}

@internal
pub fn user_errors_source(errors: List(ProductUserError)) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let ProductUserError(field: field, message: message, code: code) = error
      src_object([
        #("field", SrcList(list.map(field, SrcString))),
        #("message", SrcString(message)),
        #("code", graphql_helpers.option_string_source(code)),
      ])
    }),
  )
}

@internal
pub fn nullable_field_user_errors_source(
  errors: List(NullableFieldUserError),
) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      let NullableFieldUserError(field: field, message: message) = error
      let field_value = case field {
        Some(field) -> SrcList(list.map(field, SrcString))
        None -> SrcNull
      }
      src_object([
        #("field", field_value),
        #("message", SrcString(message)),
      ])
    }),
  )
}

@internal
pub fn store_slug_from_admin_origin(origin: String) -> Option(String) {
  origin
  |> string.split("/")
  |> segment_after_store
  |> option.then(fn(slug) { trimmed_non_empty(slug) |> option.from_result })
}

@internal
pub fn read_numeric_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> NumericRead {
  case dict.get(input, name) {
    Error(_) -> NumericMissing
    Ok(NullVal) -> NumericNull
    Ok(IntVal(value)) -> NumericValue(int.to_float(value))
    Ok(FloatVal(value)) -> NumericValue(value)
    Ok(StringVal(value)) ->
      case int.parse(value) {
        Ok(parsed) -> NumericValue(int.to_float(parsed))
        Error(_) ->
          case float.parse(value) {
            Ok(parsed) -> NumericValue(parsed)
            Error(_) -> NumericNotANumber
          }
      }
    _ -> NumericNotANumber
  }
}

@internal
pub fn read_non_empty_string_field(
  input: Dict(String, ResolvedValue),
  name: String,
) -> Option(String) {
  case read_string_field(input, name) {
    Some(value) ->
      case string.length(string.trim(value)) > 0 {
        True -> Some(value)
        False -> None
      }
    None -> None
  }
}
