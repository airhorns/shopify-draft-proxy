import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/functions/types as function_types
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  ConnectionWindow, SelectedFieldOptions, SerializeConnectionConfig, SrcBool,
  SrcList, SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, get_field_response_key,
  paginate_connection_items, project_graphql_value, serialize_connection,
  src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type CartTransformRecord, type ShopifyFunctionAppRecord,
  type ShopifyFunctionRecord, type TaxAppConfigurationRecord,
  type ValidationMetafieldRecord, type ValidationRecord,
}

@internal
pub fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = root_payload_for_field(store, field, fragments, variables)
      #(key, value)
    })
  json.object(entries)
}

fn root_payload_for_field(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "validation" ->
          serialize_validation_by_id(store, field, fragments, variables)
        "validations" ->
          serialize_validations_connection(store, field, fragments, variables)
        "cartTransforms" ->
          serialize_cart_transforms_connection(
            store,
            field,
            fragments,
            variables,
          )
        "shopifyFunction" ->
          serialize_shopify_function_by_id(store, field, fragments, variables)
        "shopifyFunctions" ->
          serialize_shopify_functions_connection(
            store,
            field,
            fragments,
            variables,
          )
        _ -> json.null()
      }
    _ -> json.null()
  }
}

// ---------------------------------------------------------------------------
// Per-root serializers
// ---------------------------------------------------------------------------

fn serialize_validation_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    Some(id) ->
      case store.get_effective_validation_by_id(store, id) {
        Some(record) -> project_validation(store, record, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_shopify_function_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string(args, "id") {
    Some(id) ->
      case store.get_effective_shopify_function_by_id(store, id) {
        Some(record) -> project_shopify_function(record, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_validations_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  _variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let items = store.list_effective_validations(store)
  serialize_record_connection(
    items,
    field,
    fragments,
    validation_cursor,
    fn(item, node_field, _index) {
      project_validation(store, item, node_field, fragments)
    },
  )
}

fn serialize_cart_transforms_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  _variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let items = store.list_effective_cart_transforms(store)
  serialize_record_connection(
    items,
    field,
    fragments,
    cart_transform_cursor,
    fn(item, node_field, _index) {
      project_cart_transform(item, node_field, fragments)
    },
  )
}

fn serialize_shopify_functions_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let api_type = graphql_helpers.read_arg_string(args, "apiType")
  let all = store.list_effective_shopify_functions(store)
  let items = case api_type {
    Some(filter) ->
      list.filter(all, fn(record) { record.api_type == Some(filter) })
    None -> all
  }
  serialize_record_connection(
    items,
    field,
    fragments,
    shopify_function_cursor,
    fn(item, node_field, _index) {
      project_shopify_function(item, node_field, fragments)
    },
  )
}

fn validation_cursor(record: ValidationRecord, _index: Int) -> String {
  record.id
}

fn cart_transform_cursor(record: CartTransformRecord, _index: Int) -> String {
  record.id
}

fn shopify_function_cursor(
  record: ShopifyFunctionRecord,
  _index: Int,
) -> String {
  record.id
}

fn serialize_record_connection(
  items: List(a),
  field: Selection,
  _fragments: FragmentMap,
  cursor_value: fn(a, Int) -> String,
  serialize_node: fn(a, Selection, Int) -> Json,
) -> Json {
  let window =
    paginate_connection_items(
      items,
      field,
      dict.new(),
      cursor_value,
      default_connection_window_options(),
    )
  let ConnectionWindow(
    items: items,
    has_next_page: has_next,
    has_previous_page: has_prev,
  ) = window
  let selected_field_options =
    SelectedFieldOptions(include_inline_fragments: True)
  let page_info_options =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      include_inline_fragments: True,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: items,
      has_next_page: has_next,
      has_previous_page: has_prev,
      get_cursor_value: cursor_value,
      serialize_node: serialize_node,
      selected_field_options: selected_field_options,
      page_info_options: page_info_options,
    ),
  )
}

// ---------------------------------------------------------------------------
// Source projections
// ---------------------------------------------------------------------------

fn project_validation(
  store: Store,
  record: ValidationRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source = validation_to_source(store, record, fragments)
  project_payload(source, field, fragments)
}

fn project_cart_transform(
  record: CartTransformRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_payload(cart_transform_to_source(record), field, fragments)
}

fn project_shopify_function(
  record: ShopifyFunctionRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_payload(shopify_function_to_source(record), field, fragments)
}

fn project_payload(
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(source, selections, fragments)
    _ -> json.object([])
  }
}

fn validation_to_source(
  store: Store,
  record: ValidationRecord,
  _fragments: FragmentMap,
) -> SourceValue {
  let function_id_source = case record.function_id {
    Some(id) -> SrcString(id)
    None ->
      case record.shopify_function_id {
        Some(id) -> SrcString(id)
        None -> SrcNull
      }
  }
  let shopify_function_source = case record.shopify_function_id {
    Some(id) ->
      case store.get_effective_shopify_function_by_id(store, id) {
        Some(fn_record) -> shopify_function_to_source(fn_record)
        None -> SrcNull
      }
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("Validation")),
    #("id", SrcString(record.id)),
    #("title", graphql_helpers.option_string_source(record.title)),
    #("enable", graphql_helpers.option_bool_source(record.enable)),
    #("enabled", graphql_helpers.option_bool_source(record.enable)),
    #(
      "blockOnFailure",
      graphql_helpers.option_bool_source(record.block_on_failure),
    ),
    #("functionId", function_id_source),
    #(
      "functionHandle",
      graphql_helpers.option_string_source(record.function_handle),
    ),
    #("shopifyFunction", shopify_function_source),
    #("createdAt", graphql_helpers.option_string_source(record.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(record.updated_at)),
    #("metafield", SrcNull),
    #("metafields", validation_metafields_connection_source(record.metafields)),
  ])
}

fn validation_metafields_connection_source(
  rows: List(ValidationMetafieldRecord),
) -> SourceValue {
  let nodes = list.map(rows, validation_metafield_to_source)
  let edges =
    list.map(rows, fn(row) {
      src_object([
        #("cursor", SrcString("cursor:" <> row.id)),
        #("node", validation_metafield_to_source(row)),
      ])
    })
  let page_info = case rows {
    [] -> empty_page_info_source()
    [first, ..] -> {
      let last = list.last(rows) |> result.unwrap(first)
      src_object([
        #("__typename", SrcString("PageInfo")),
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", SrcString("cursor:" <> first.id)),
        #("endCursor", SrcString("cursor:" <> last.id)),
      ])
    }
  }
  src_object([
    #("__typename", SrcString("MetafieldConnection")),
    #("edges", SrcList(edges)),
    #("nodes", SrcList(nodes)),
    #("pageInfo", page_info),
  ])
}

fn validation_metafield_to_source(
  row: ValidationMetafieldRecord,
) -> SourceValue {
  let core =
    metafields.MetafieldRecordCore(
      id: row.id,
      namespace: row.namespace,
      key: row.key,
      type_: row.type_,
      value: row.value,
      compare_digest: row.compare_digest,
      json_value: None,
      created_at: row.created_at,
      updated_at: row.updated_at,
      owner_type: row.owner_type,
    )
  src_object([
    #("__typename", SrcString("Metafield")),
    #("id", SrcString(core.id)),
    #("namespace", SrcString(core.namespace)),
    #("key", SrcString(core.key)),
    #("type", graphql_helpers.option_string_source(core.type_)),
    #("value", graphql_helpers.option_string_source(core.value)),
    #("compareDigest", case core.compare_digest {
      Some(digest) -> SrcString(digest)
      None -> SrcString(metafields.make_metafield_compare_digest(core))
    }),
    #("createdAt", graphql_helpers.option_string_source(core.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(core.updated_at)),
    #("ownerType", graphql_helpers.option_string_source(core.owner_type)),
  ])
}

fn cart_transform_to_source(record: CartTransformRecord) -> SourceValue {
  let function_id_source = case record.function_id {
    Some(id) -> SrcString(id)
    None ->
      case record.shopify_function_id {
        Some(id) -> SrcString(id)
        None -> SrcNull
      }
  }
  src_object([
    #("__typename", SrcString("CartTransform")),
    #("id", SrcString(record.id)),
    #("title", graphql_helpers.option_string_source(record.title)),
    #(
      "blockOnFailure",
      graphql_helpers.option_bool_source(record.block_on_failure),
    ),
    #("functionId", function_id_source),
    #(
      "functionHandle",
      graphql_helpers.option_string_source(record.function_handle),
    ),
    #("createdAt", graphql_helpers.option_string_source(record.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(record.updated_at)),
    #("metafield", SrcNull),
    #("metafields", empty_metafield_connection_source()),
  ])
}

fn shopify_function_to_source(record: ShopifyFunctionRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopifyFunction")),
    #("id", SrcString(record.id)),
    #("title", graphql_helpers.option_string_source(record.title)),
    #("handle", graphql_helpers.option_string_source(record.handle)),
    #("apiType", graphql_helpers.option_string_source(record.api_type)),
    #("description", graphql_helpers.option_string_source(record.description)),
    #("appKey", graphql_helpers.option_string_source(record.app_key)),
    #("app", shopify_function_app_to_source(record.app)),
  ])
}

fn shopify_function_app_to_source(
  app: Option(ShopifyFunctionAppRecord),
) -> SourceValue {
  case app {
    None -> SrcNull
    Some(record) ->
      src_object([
        #("__typename", graphql_helpers.option_string_source(record.typename)),
        #("id", graphql_helpers.option_string_source(record.id)),
        #("title", graphql_helpers.option_string_source(record.title)),
        #("handle", graphql_helpers.option_string_source(record.handle)),
        #("apiKey", graphql_helpers.option_string_source(record.api_key)),
      ])
  }
}

fn tax_app_configuration_to_source(
  record: TaxAppConfigurationRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("TaxAppConfiguration")),
    #("id", SrcString(record.id)),
    #("ready", SrcBool(record.ready)),
    #("state", SrcString(record.state)),
    #("updatedAt", graphql_helpers.option_string_source(record.updated_at)),
  ])
}

fn empty_metafield_connection_source() -> SourceValue {
  src_object([
    #("__typename", SrcString("MetafieldConnection")),
    #("edges", SrcList([])),
    #("nodes", SrcList([])),
    #("pageInfo", empty_page_info_source()),
  ])
}

fn empty_page_info_source() -> SourceValue {
  src_object([
    #("__typename", SrcString("PageInfo")),
    #("hasNextPage", SrcBool(False)),
    #("hasPreviousPage", SrcBool(False)),
    #("startCursor", SrcNull),
    #("endCursor", SrcNull),
  ])
}

// ---------------------------------------------------------------------------
// Payload builders
// ---------------------------------------------------------------------------

@internal
pub fn validation_mutation_payload(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  validation: Option(ValidationRecord),
  user_errors: List(function_types.UserError),
) -> Json {
  let validation_source = case validation {
    Some(record) -> validation_to_source(store, record, fragments)
    None -> SrcNull
  }
  let payload =
    src_object([
      #("validation", validation_source),
      #("userErrors", user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

@internal
pub fn cart_transform_mutation_payload(
  field: Selection,
  fragments: FragmentMap,
  cart_transform: Option(CartTransformRecord),
  user_errors: List(function_types.UserError),
) -> Json {
  let cart_transform_source = case cart_transform {
    Some(record) -> cart_transform_to_source(record)
    None -> SrcNull
  }
  let payload =
    src_object([
      #("cartTransform", cart_transform_source),
      #("userErrors", user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

@internal
pub fn delete_payload(
  field: Selection,
  fragments: FragmentMap,
  deleted_id: Option(String),
  user_errors: List(function_types.UserError),
) -> Json {
  let deleted_id_source = case deleted_id {
    Some(id) -> SrcString(id)
    None -> SrcNull
  }
  let payload =
    src_object([
      #("deletedId", deleted_id_source),
      #("userErrors", user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

@internal
pub fn tax_app_payload(
  field: Selection,
  fragments: FragmentMap,
  configuration: Option(TaxAppConfigurationRecord),
  user_errors: List(function_types.UserError),
) -> Json {
  let configuration_source = case configuration {
    Some(record) -> tax_app_configuration_to_source(record)
    None -> SrcNull
  }
  let payload =
    src_object([
      #("taxAppConfiguration", configuration_source),
      #("userErrors", user_errors_source(user_errors)),
    ])
  project_payload(payload, field, fragments)
}

fn user_errors_source(errors: List(function_types.UserError)) -> SourceValue {
  SrcList(list.map(errors, user_error_to_source))
}

fn user_error_to_source(error: function_types.UserError) -> SourceValue {
  let code_source = case error.code {
    Some(c) -> SrcString(c)
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", SrcList(list.map(error.field, fn(part) { SrcString(part) }))),
    #("message", SrcString(error.message)),
    #("code", code_source),
  ])
}
