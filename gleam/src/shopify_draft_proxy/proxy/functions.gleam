//// Mirrors `src/proxy/functions.ts`.
////
//// Pass 18 ships the five query roots (`validation`, `validations`,
//// `cartTransforms`, `shopifyFunction`, `shopifyFunctions`) plus the six
//// mutation roots (`validationCreate`/`Update`/`Delete`,
//// `cartTransformCreate`/`Delete`, `taxAppConfigure`).
////
//// The TS handler implicitly hydrates a `ShopifyFunctionRecord` whenever
//// a validation or cart-transform mutation references one — either by
//// id, by handle, or by minting a fresh synthetic gid. Mirrored here as
//// `ensure_shopify_function`. The mutation pipeline returns a
//// `MutationOutcome` carrying the updated store + identity registry +
//// staged GIDs, matching the apps/webhooks/saved-search shape.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  ConnectionWindow, SelectedFieldOptions, SerializeConnectionConfig, SrcBool,
  SrcList, SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, get_document_fragments,
  get_field_response_key, paginate_connection_items, project_graphql_value,
  serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{type LogDraft, LogDraft}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, fetch_sync,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CartTransformRecord, type ShopifyFunctionAppRecord,
  type ShopifyFunctionRecord, type TaxAppConfigurationRecord,
  type ValidationRecord, CartTransformRecord, ShopifyFunctionAppRecord,
  ShopifyFunctionRecord, TaxAppConfigurationRecord, ValidationRecord,
}

// ---------------------------------------------------------------------------
// Public surface
// ---------------------------------------------------------------------------

/// Errors specific to the functions handler. Mirrors `AppsError`.
pub type FunctionsError {
  ParseFailed(root_field.RootFieldError)
}

/// Predicate matching the TS `FUNCTION_QUERY_ROOTS` set.
pub fn is_function_query_root(name: String) -> Bool {
  case name {
    "validation" -> True
    "validations" -> True
    "cartTransforms" -> True
    "shopifyFunction" -> True
    "shopifyFunctions" -> True
    _ -> False
  }
}

/// Predicate matching the TS `FUNCTION_MUTATION_ROOTS` set.
pub fn is_function_mutation_root(name: String) -> Bool {
  case name {
    "validationCreate" -> True
    "validationUpdate" -> True
    "validationDelete" -> True
    "cartTransformCreate" -> True
    "cartTransformDelete" -> True
    "taxAppConfigure" -> True
    _ -> False
  }
}

/// Process a functions query document and return a JSON `data`
/// envelope. Mirrors `handleFunctionQuery`.
pub fn handle_function_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, FunctionsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

/// Wrap a successful functions response in the standard GraphQL
/// envelope.
pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, FunctionsError) {
  use data <- result.try(handle_function_query(store, document, variables))
  Ok(wrap_data(data))
}

/// True when functions-domain reads need local handling because the
/// proxy already knows about function metadata or staged lifecycle
/// effects. In LiveHybrid, cold reads can be forwarded upstream
/// verbatim; once any local function metadata exists, reads must stay
/// local so staged Validation / CartTransform state remains visible.
pub fn local_has_function_metadata(proxy: DraftProxy) -> Bool {
  store_has_function_metadata(proxy.store)
}

fn store_has_function_metadata(store_in: Store) -> Bool {
  dict.size(store_in.base_state.shopify_functions) > 0
  || dict.size(store_in.staged_state.shopify_functions) > 0
  || dict.size(store_in.base_state.validations) > 0
  || dict.size(store_in.staged_state.validations) > 0
  || dict.size(store_in.staged_state.deleted_validation_ids) > 0
  || dict.size(store_in.base_state.cart_transforms) > 0
  || dict.size(store_in.staged_state.cart_transforms) > 0
  || dict.size(store_in.staged_state.deleted_cart_transform_ids) > 0
  || option.is_some(store_in.base_state.tax_app_configuration)
  || option.is_some(store_in.staged_state.tax_app_configuration)
}

fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "validation" ->
      !local_has_function_metadata(proxy)
    parse_operation.QueryOperation, "validations" ->
      !local_has_function_metadata(proxy)
    parse_operation.QueryOperation, "cartTransforms" ->
      !local_has_function_metadata(proxy)
    parse_operation.QueryOperation, "shopifyFunction" ->
      !local_has_function_metadata(proxy)
    parse_operation.QueryOperation, "shopifyFunctions" ->
      !local_has_function_metadata(proxy)
    _, _ -> False
  }
}

pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let want_passthrough = case proxy.config.read_mode {
    LiveHybrid ->
      should_passthrough_in_live_hybrid(proxy, parsed.type_, primary_root_field)
    _ -> False
  }
  case want_passthrough {
    True -> passthrough.passthrough_sync(proxy, request)
    False ->
      case process(proxy.store, document, variables) {
        Ok(envelope) -> #(
          Response(status: 200, body: envelope, headers: []),
          proxy,
        )
        Error(_) -> #(
          Response(
            status: 400,
            body: json.object([
              #(
                "errors",
                json.array(
                  [
                    json.object([
                      #(
                        "message",
                        json.string("Failed to handle functions query"),
                      ),
                    ]),
                  ],
                  fn(x) { x },
                ),
              ),
            ]),
            headers: [],
          ),
          proxy,
        )
      }
  }
}

// ---------------------------------------------------------------------------
// Query dispatch
// ---------------------------------------------------------------------------

fn serialize_root_fields(
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
    #("title", optional_string_to_source(record.title)),
    #("enable", optional_bool_to_source(record.enable)),
    #("enabled", optional_bool_to_source(record.enable)),
    #("blockOnFailure", optional_bool_to_source(record.block_on_failure)),
    #("functionId", function_id_source),
    #("functionHandle", optional_string_to_source(record.function_handle)),
    #("shopifyFunction", shopify_function_source),
    #("createdAt", optional_string_to_source(record.created_at)),
    #("updatedAt", optional_string_to_source(record.updated_at)),
    #("metafield", SrcNull),
    #("metafields", empty_metafield_connection_source()),
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
    #("title", optional_string_to_source(record.title)),
    #("blockOnFailure", optional_bool_to_source(record.block_on_failure)),
    #("functionId", function_id_source),
    #("functionHandle", optional_string_to_source(record.function_handle)),
    #("createdAt", optional_string_to_source(record.created_at)),
    #("updatedAt", optional_string_to_source(record.updated_at)),
    #("metafield", SrcNull),
    #("metafields", empty_metafield_connection_source()),
  ])
}

fn shopify_function_to_source(record: ShopifyFunctionRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopifyFunction")),
    #("id", SrcString(record.id)),
    #("title", optional_string_to_source(record.title)),
    #("handle", optional_string_to_source(record.handle)),
    #("apiType", optional_string_to_source(record.api_type)),
    #("description", optional_string_to_source(record.description)),
    #("appKey", optional_string_to_source(record.app_key)),
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
        #("__typename", optional_string_to_source(record.typename)),
        #("id", optional_string_to_source(record.id)),
        #("title", optional_string_to_source(record.title)),
        #("handle", optional_string_to_source(record.handle)),
        #("apiKey", optional_string_to_source(record.api_key)),
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
    #("updatedAt", optional_string_to_source(record.updated_at)),
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

fn optional_string_to_source(value: Option(String)) -> SourceValue {
  case value {
    Some(s) -> SrcString(s)
    None -> SrcNull
  }
}

fn optional_bool_to_source(value: Option(Bool)) -> SourceValue {
  case value {
    Some(b) -> SrcBool(b)
    None -> SrcNull
  }
}

// ===========================================================================
// Mutation path
// ===========================================================================

/// Outcome of a functions mutation. Same shape as the apps / webhooks /
/// saved-search outcome record.
pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

/// User-error payload. Mirrors the TS `FunctionUserError` shape (path,
/// message, optional code).
pub type UserError {
  UserError(field: List(String), message: String, code: Option(String))
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
  )
}

/// Process a functions mutation document. Mirrors
/// `handleFunctionMutation`.
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, FunctionsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_mutation_fields(
        store,
        identity,
        request_path,
        document,
        fields,
        fragments,
        variables,
      ))
    }
  }
}

/// Pattern 2: dispatched LiveHybrid function metadata mutations first
/// try to hydrate referenced ShopifyFunction owner/app metadata from
/// upstream, then stage the mutation locally. Snapshot/no-transport
/// paths fall back to the existing local synthetic Function record.
pub fn process_mutation_with_upstream(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Result(MutationOutcome, FunctionsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let identity_for_handlers =
        reserve_multiroot_log_identity(identity, fields)
      let hydrated_store =
        hydrate_referenced_shopify_functions(store, fields, variables, upstream)
      Ok(handle_mutation_fields(
        hydrated_store,
        identity_for_handlers,
        request_path,
        document,
        fields,
        fragments,
        variables,
      ))
    }
  }
}

fn reserve_multiroot_log_identity(
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
) -> SyntheticIdentityRegistry {
  case list.length(mutation_root_names(fields)) > 1 {
    True -> {
      let #(_, identity_after_reserved_id) =
        synthetic_identity.make_synthetic_gid(identity, "MutationLogEntry")
      let #(_, identity_after_reserved_log) =
        synthetic_identity.make_synthetic_timestamp(identity_after_reserved_id)
      identity_after_reserved_log
    }
    False -> identity
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial = #([], store, identity, [])
  let #(data_entries, final_store, final_identity, all_staged) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids) = acc
      case field {
        Field(name: name, ..) -> {
          let dispatch = case name.value {
            "validationCreate" ->
              Some(handle_validation_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "validationUpdate" ->
              Some(handle_validation_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "validationDelete" ->
              Some(handle_validation_delete(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "cartTransformCreate" ->
              Some(handle_cart_transform_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "cartTransformDelete" ->
              Some(handle_cart_transform_delete(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "taxAppConfigure" ->
              Some(handle_tax_app_configure(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            _ -> None
          }
          case dispatch {
            None -> acc
            Some(#(result, next_store, next_identity)) -> #(
              list.append(entries, [#(result.key, result.payload)]),
              next_store,
              next_identity,
              list.append(staged_ids, result.staged_resource_ids),
            )
          }
        }
        _ -> acc
      }
    })
  let root_names = mutation_root_names(fields)
  let primary_root = case list.first(root_names) {
    Ok(name) -> Some(name)
    Error(_) -> None
  }
  let notes = case primary_root {
    Some("taxAppConfigure") ->
      "Staged locally in the in-memory tax app configuration metadata store; no tax calculation app callbacks are invoked."
    _ ->
      "Staged locally in the in-memory Shopify Functions metadata store; external Shopify Function code is not executed."
  }
  let draft =
    LogDraft(
      operation_name: primary_root,
      root_fields: root_names,
      primary_root_field: primary_root,
      domain: "functions",
      execution: "stage-locally",
      query: None,
      variables: None,
      staged_resource_ids: all_staged,
      status: store.Staged,
      notes: Some(notes),
    )
  MutationOutcome(
    data: json.object([#("data", json.object(data_entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: all_staged,
    log_drafts: [draft],
  )
}

fn mutation_root_names(fields: List(Selection)) -> List(String) {
  list.filter_map(fields, fn(field) {
    case field {
      Field(name: name, ..) -> Ok(name.value)
      _ -> Error(Nil)
    }
  })
}

// ---------------------------------------------------------------------------
// Mutation handlers
// ---------------------------------------------------------------------------

type FunctionReference {
  FunctionReference(
    function_id: Option(String),
    function_handle: Option(String),
  )
}

fn read_function_reference(
  input: Dict(String, root_field.ResolvedValue),
) -> FunctionReference {
  FunctionReference(
    function_id: graphql_helpers.read_arg_string(input, "functionId"),
    function_handle: graphql_helpers.read_arg_string(input, "functionHandle"),
  )
}

fn missing_function_error(field: List(String)) -> UserError {
  UserError(
    field: field,
    message: "Function handle or function ID must be provided",
    code: Some("MISSING_FUNCTION"),
  )
}

fn not_found_error(field_name: String, id: String) -> UserError {
  UserError(
    field: [field_name],
    message: "No function-backed resource exists with id " <> id,
    code: Some("NOT_FOUND"),
  )
}

fn handle_validation_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = case graphql_helpers.read_arg_object(args, "validation") {
    Some(d) -> d
    None -> dict.new()
  }
  let reference = read_function_reference(input)
  case reference.function_id, reference.function_handle {
    None, None -> {
      let payload =
        validation_mutation_payload(store, field, fragments, None, [
          missing_function_error(["validation", "functionHandle"]),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    _, _ -> {
      let title = graphql_helpers.read_arg_string(input, "title")
      let fallback = case title {
        Some(t) -> t
        None -> "Local validation function"
      }
      let #(shopify_fn, store_after_fn, identity_after_fn) =
        ensure_shopify_function(
          store,
          identity,
          reference,
          "VALIDATION",
          fallback,
        )
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity_after_fn)
      let #(validation_id, identity_final) =
        synthetic_identity.make_synthetic_gid(identity_after_ts, "Validation")
      let enable = case graphql_helpers.read_arg_bool(input, "enable") {
        Some(b) -> Some(b)
        None ->
          case graphql_helpers.read_arg_bool(input, "enabled") {
            Some(b) -> Some(b)
            None -> Some(True)
          }
      }
      let block_on_failure = case
        graphql_helpers.read_arg_bool(input, "blockOnFailure")
      {
        Some(b) -> Some(b)
        None -> Some(False)
      }
      let function_handle = case reference.function_handle {
        Some(_) -> reference.function_handle
        None -> shopify_fn.handle
      }
      let validation =
        ValidationRecord(
          id: validation_id,
          title: title,
          enable: enable,
          block_on_failure: block_on_failure,
          function_id: reference.function_id,
          function_handle: function_handle,
          shopify_function_id: Some(shopify_fn.id),
          created_at: Some(timestamp),
          updated_at: Some(timestamp),
        )
      let #(_, store_final) =
        store.upsert_staged_validation(store_after_fn, validation)
      let payload =
        validation_mutation_payload(
          store_final,
          field,
          fragments,
          Some(validation),
          [],
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: [
          validation.id,
        ]),
        store_final,
        identity_final,
      )
    }
  }
}

fn handle_validation_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = case graphql_helpers.read_arg_string(args, "id") {
    Some(s) -> s
    None -> ""
  }
  case store.get_effective_validation_by_id(store, id) {
    None -> {
      let payload =
        validation_mutation_payload(store, field, fragments, None, [
          not_found_error("id", id),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    Some(current) -> {
      let input = case graphql_helpers.read_arg_object(args, "validation") {
        Some(d) -> d
        None -> dict.new()
      }
      let reference = read_function_reference(input)
      let has_function_input = case
        reference.function_id,
        reference.function_handle
      {
        None, None -> False
        _, _ -> True
      }
      let #(maybe_shopify_fn, store_after_fn, identity_after_fn) = case
        has_function_input
      {
        True -> {
          let fallback = case current.title {
            Some(t) -> t
            None -> "Local validation function"
          }
          let #(record, next_store, next_identity) =
            ensure_shopify_function(
              store,
              identity,
              reference,
              "VALIDATION",
              fallback,
            )
          #(Some(record), next_store, next_identity)
        }
        False ->
          case current.shopify_function_id {
            Some(fn_id) -> #(
              store.get_effective_shopify_function_by_id(store, fn_id),
              store,
              identity,
            )
            None -> #(None, store, identity)
          }
      }
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity_after_fn)
      let new_title = case graphql_helpers.read_arg_string(input, "title") {
        Some(s) -> Some(s)
        None -> current.title
      }
      let new_enable = case graphql_helpers.read_arg_bool(input, "enable") {
        Some(b) -> Some(b)
        None ->
          case graphql_helpers.read_arg_bool(input, "enabled") {
            Some(b) -> Some(b)
            None -> current.enable
          }
      }
      let new_block_on_failure = case
        graphql_helpers.read_arg_bool(input, "blockOnFailure")
      {
        Some(b) -> Some(b)
        None -> current.block_on_failure
      }
      let new_function_id = case reference.function_id {
        Some(_) -> reference.function_id
        None ->
          case reference.function_handle {
            Some(_) -> None
            None -> current.function_id
          }
      }
      let new_function_handle = case reference.function_handle {
        Some(_) -> reference.function_handle
        None ->
          case reference.function_id {
            Some(_) ->
              case maybe_shopify_fn {
                Some(fn_record) -> fn_record.handle
                None -> None
              }
            None -> current.function_handle
          }
      }
      let new_shopify_function_id = case maybe_shopify_fn {
        Some(fn_record) -> Some(fn_record.id)
        None -> current.shopify_function_id
      }
      let updated =
        ValidationRecord(
          id: current.id,
          title: new_title,
          enable: new_enable,
          block_on_failure: new_block_on_failure,
          function_id: new_function_id,
          function_handle: new_function_handle,
          shopify_function_id: new_shopify_function_id,
          created_at: current.created_at,
          updated_at: Some(timestamp),
        )
      let #(_, store_final) =
        store.upsert_staged_validation(store_after_fn, updated)
      let payload =
        validation_mutation_payload(
          store_final,
          field,
          fragments,
          Some(updated),
          [],
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: [
          updated.id,
        ]),
        store_final,
        identity_after_ts,
      )
    }
  }
}

fn handle_validation_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = case graphql_helpers.read_arg_string(args, "id") {
    Some(s) -> s
    None -> ""
  }
  case store.get_effective_validation_by_id(store, id) {
    None -> {
      let payload =
        delete_payload(field, fragments, None, [not_found_error("id", id)])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    Some(_) -> {
      let next_store = store.delete_staged_validation(store, id)
      let payload = delete_payload(field, fragments, Some(id), [])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: [
          id,
        ]),
        next_store,
        identity,
      )
    }
  }
}

fn handle_cart_transform_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = case graphql_helpers.read_arg_object(args, "cartTransform") {
    Some(d) -> d
    None -> args
  }
  let reference = read_function_reference(input)
  case reference.function_id, reference.function_handle {
    None, None -> {
      let payload =
        cart_transform_mutation_payload(field, fragments, None, [
          missing_function_error(["functionHandle"]),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    _, _ -> {
      let title = graphql_helpers.read_arg_string(input, "title")
      let fallback = case title {
        Some(t) -> t
        None -> "Local cart transform function"
      }
      let #(shopify_fn, store_after_fn, identity_after_fn) =
        ensure_shopify_function(
          store,
          identity,
          reference,
          "CART_TRANSFORM",
          fallback,
        )
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity_after_fn)
      let #(cart_transform_id, identity_final) =
        synthetic_identity.make_synthetic_gid(
          identity_after_ts,
          "CartTransform",
        )
      let final_title = case title {
        Some(t) -> Some(t)
        None -> shopify_fn.title
      }
      let function_handle = case reference.function_handle {
        Some(_) -> reference.function_handle
        None -> shopify_fn.handle
      }
      let block_on_failure = case
        graphql_helpers.read_arg_bool(input, "blockOnFailure")
      {
        Some(b) -> Some(b)
        None -> Some(False)
      }
      let cart_transform =
        CartTransformRecord(
          id: cart_transform_id,
          title: final_title,
          block_on_failure: block_on_failure,
          function_id: reference.function_id,
          function_handle: function_handle,
          shopify_function_id: Some(shopify_fn.id),
          created_at: Some(timestamp),
          updated_at: Some(timestamp),
        )
      let #(_, store_final) =
        store.upsert_staged_cart_transform(store_after_fn, cart_transform)
      let payload =
        cart_transform_mutation_payload(
          field,
          fragments,
          Some(cart_transform),
          [],
        )
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: [
          cart_transform.id,
        ]),
        store_final,
        identity_final,
      )
    }
  }
}

fn handle_cart_transform_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = case graphql_helpers.read_arg_string(args, "id") {
    Some(s) -> s
    None -> ""
  }
  case store.get_effective_cart_transform_by_id(store, id) {
    None -> {
      let payload =
        delete_payload(field, fragments, None, [not_found_error("id", id)])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    Some(_) -> {
      let next_store = store.delete_staged_cart_transform(store, id)
      let payload = delete_payload(field, fragments, Some(id), [])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: [
          id,
        ]),
        next_store,
        identity,
      )
    }
  }
}

fn handle_tax_app_configure(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let ready = graphql_helpers.read_arg_bool(args, "ready")
  let user_errors = case ready {
    None -> [
      UserError(
        field: ["ready"],
        message: "Ready must be true or false",
        code: Some("INVALID"),
      ),
    ]
    Some(_) -> []
  }
  let #(configuration_after, next_store, next_identity, staged_id) = case
    ready
  {
    Some(value) -> {
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let state = case value {
        True -> "READY"
        False -> "NOT_READY"
      }
      let configuration =
        TaxAppConfigurationRecord(
          id: "gid://shopify/TaxAppConfiguration/local",
          ready: value,
          state: state,
          updated_at: Some(timestamp),
        )
      let updated_store =
        store.set_staged_tax_app_configuration(store, configuration)
      #(Some(configuration), updated_store, identity_after_ts, [
        configuration.id,
      ])
    }
    None -> #(
      store.get_effective_tax_app_configuration(store),
      store,
      identity,
      [],
    )
  }
  let payload =
    tax_app_payload(field, fragments, configuration_after, user_errors)
  #(
    MutationFieldResult(
      key: key,
      payload: payload,
      staged_resource_ids: staged_id,
    ),
    next_store,
    next_identity,
  )
}

// ---------------------------------------------------------------------------
// Upstream ShopifyFunction hydration
// ---------------------------------------------------------------------------

fn hydrate_referenced_shopify_functions(
  store: Store,
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  list.fold(fields, store, fn(acc, field) {
    case function_reference_for_mutation(field, variables) {
      Some(#(reference, api_type)) ->
        hydrate_shopify_function_reference(acc, reference, api_type, upstream)
      None -> acc
    }
  })
}

fn function_reference_for_mutation(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(#(FunctionReference, String)) {
  case field {
    Field(name: name, ..) -> {
      let args = graphql_helpers.field_args(field, variables)
      case name.value {
        "validationCreate" -> {
          let input = case graphql_helpers.read_arg_object(args, "validation") {
            Some(d) -> d
            None -> dict.new()
          }
          Some(#(read_function_reference(input), "VALIDATION"))
        }
        "validationUpdate" -> {
          let input = case graphql_helpers.read_arg_object(args, "validation") {
            Some(d) -> d
            None -> dict.new()
          }
          let reference = read_function_reference(input)
          case reference.function_id, reference.function_handle {
            None, None -> None
            _, _ -> Some(#(reference, "VALIDATION"))
          }
        }
        "cartTransformCreate" -> {
          let input = case
            graphql_helpers.read_arg_object(args, "cartTransform")
          {
            Some(d) -> d
            None -> args
          }
          Some(#(read_function_reference(input), "CART_TRANSFORM"))
        }
        _ -> None
      }
    }
    _ -> None
  }
}

fn hydrate_shopify_function_reference(
  store: Store,
  reference: FunctionReference,
  api_type: String,
  upstream: UpstreamContext,
) -> Store {
  case reference.function_id, reference.function_handle {
    None, None -> store
    _, _ ->
      case find_existing_shopify_function(store, reference) {
        Some(_) -> store
        None ->
          case fetch_shopify_function(upstream, reference, api_type) {
            Some(record) -> store.upsert_base_shopify_functions(store, [record])
            None -> store
          }
      }
  }
}

fn fetch_shopify_function(
  upstream: UpstreamContext,
  reference: FunctionReference,
  api_type: String,
) -> Option(ShopifyFunctionRecord) {
  case reference.function_id {
    Some(id) -> fetch_shopify_function_by_id(upstream, id)
    None ->
      case reference.function_handle {
        Some(handle) ->
          fetch_shopify_function_by_handle(upstream, handle, api_type)
        None -> None
      }
  }
}

const function_hydrate_selection: String = " id title handle apiType description appKey app { __typename id title handle apiKey } "

fn fetch_shopify_function_by_id(
  upstream: UpstreamContext,
  id: String,
) -> Option(ShopifyFunctionRecord) {
  let query =
    "query FunctionHydrateById($id: String!) { shopifyFunction(id: $id) {"
    <> function_hydrate_selection
    <> " } }"
  let variables = json.object([#("id", json.string(id))])
  case
    fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "FunctionHydrateById",
      query,
      variables,
    )
  {
    Ok(response) -> shopify_function_from_id_response(response)
    Error(_) -> None
  }
}

fn fetch_shopify_function_by_handle(
  upstream: UpstreamContext,
  handle: String,
  api_type: String,
) -> Option(ShopifyFunctionRecord) {
  let query =
    "query FunctionHydrateByHandle { shopifyFunctions(first: 50, apiType: "
    <> api_type
    <> ") { nodes {"
    <> function_hydrate_selection
    <> " } } }"
  let variables =
    json.object([
      #("handle", json.string(handle)),
      #("apiType", json.string(api_type)),
    ])
  case
    fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "FunctionHydrateByHandle",
      query,
      variables,
    )
  {
    Ok(response) -> shopify_function_from_handle_response(response, handle)
    Error(_) -> None
  }
}

fn shopify_function_from_id_response(
  value: commit.JsonValue,
) -> Option(ShopifyFunctionRecord) {
  use data <- option.then(json_get(value, "data"))
  use node <- option.then(non_null_json(json_get(data, "shopifyFunction")))
  shopify_function_from_json(node)
}

fn shopify_function_from_handle_response(
  value: commit.JsonValue,
  handle: String,
) -> Option(ShopifyFunctionRecord) {
  use data <- option.then(json_get(value, "data"))
  use connection <- option.then(
    non_null_json(json_get(data, "shopifyFunctions")),
  )
  use nodes <- option.then(json_get_array(connection, "nodes"))
  list.find_map(nodes, fn(node) {
    case shopify_function_from_json(node) {
      Some(record) ->
        case shopify_function_matches_handle(record, handle) {
          True -> Ok(record)
          False -> Error(Nil)
        }
      None -> Error(Nil)
    }
  })
  |> result_to_option
}

fn shopify_function_matches_handle(
  record: ShopifyFunctionRecord,
  handle: String,
) -> Bool {
  let normalized = normalize_function_handle(handle)
  let handle_id = shopify_function_id_from_handle(handle)
  record.handle == Some(handle)
  || record.handle == Some(normalized)
  || record.id == handle_id
}

fn shopify_function_from_json(
  node: commit.JsonValue,
) -> Option(ShopifyFunctionRecord) {
  use id <- option.then(json_get_string(node, "id"))
  Some(ShopifyFunctionRecord(
    id: id,
    title: json_get_string(node, "title"),
    handle: json_get_string(node, "handle"),
    api_type: json_get_string(node, "apiType"),
    description: json_get_string(node, "description"),
    app_key: json_get_string(node, "appKey"),
    app: non_null_json(json_get(node, "app"))
      |> option.then(shopify_function_app_from_json),
  ))
}

fn shopify_function_app_from_json(
  node: commit.JsonValue,
) -> Option(ShopifyFunctionAppRecord) {
  Some(ShopifyFunctionAppRecord(
    typename: json_get_string(node, "__typename"),
    id: json_get_string(node, "id"),
    title: json_get_string(node, "title"),
    handle: json_get_string(node, "handle"),
    api_key: json_get_string(node, "apiKey"),
  ))
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(name, child) if name == key -> Ok(child)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn non_null_json(value: Option(commit.JsonValue)) -> Option(commit.JsonValue) {
  case value {
    Some(commit.JsonNull) -> None
    Some(v) -> Some(v)
    None -> None
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

fn json_get_array(
  value: commit.JsonValue,
  key: String,
) -> Option(List(commit.JsonValue)) {
  case json_get(value, key) {
    Some(commit.JsonArray(items)) -> Some(items)
    _ -> None
  }
}

// ---------------------------------------------------------------------------
// Payload builders
// ---------------------------------------------------------------------------

fn validation_mutation_payload(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  validation: Option(ValidationRecord),
  user_errors: List(UserError),
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

fn cart_transform_mutation_payload(
  field: Selection,
  fragments: FragmentMap,
  cart_transform: Option(CartTransformRecord),
  user_errors: List(UserError),
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

fn delete_payload(
  field: Selection,
  fragments: FragmentMap,
  deleted_id: Option(String),
  user_errors: List(UserError),
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

fn tax_app_payload(
  field: Selection,
  fragments: FragmentMap,
  configuration: Option(TaxAppConfigurationRecord),
  user_errors: List(UserError),
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

fn user_errors_source(errors: List(UserError)) -> SourceValue {
  SrcList(list.map(errors, user_error_to_source))
}

fn user_error_to_source(error: UserError) -> SourceValue {
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

// ---------------------------------------------------------------------------
// Shopify function helpers
// ---------------------------------------------------------------------------

/// Look up an existing `ShopifyFunctionRecord` matching the supplied
/// reference. Mirrors `findExistingShopifyFunction`. Match order:
///   1. exact-id match (when functionId provided)
///   2. exact-handle match
///   3. normalized-handle match
///   4. handle-derived id match
fn find_existing_shopify_function(
  store: Store,
  reference: FunctionReference,
) -> Option(ShopifyFunctionRecord) {
  case reference.function_id {
    Some(id) -> store.get_effective_shopify_function_by_id(store, id)
    None ->
      case reference.function_handle {
        None -> None
        Some(handle) -> {
          let normalized = normalize_function_handle(handle)
          let handle_based_id = shopify_function_id_from_handle(handle)
          let candidates = store.list_effective_shopify_functions(store)
          list.find(candidates, fn(record) {
            record.handle == Some(handle)
            || record.handle == Some(normalized)
            || record.id == handle_based_id
          })
          |> result_to_option
        }
      }
  }
}

/// Hydrate a `ShopifyFunctionRecord` given a reference + api type.
/// Mirrors `ensureShopifyFunction` — it reuses an existing record when
/// one matches, otherwise mints a fresh one (using a handle-derived id
/// if a handle is supplied, or a synthetic gid otherwise).
fn ensure_shopify_function(
  store: Store,
  identity: SyntheticIdentityRegistry,
  reference: FunctionReference,
  api_type: String,
  fallback_title: String,
) -> #(ShopifyFunctionRecord, Store, SyntheticIdentityRegistry) {
  let existing = find_existing_shopify_function(store, reference)
  let #(id, identity_after_id) = case existing {
    Some(record) -> #(record.id, identity)
    None ->
      case reference.function_id {
        Some(id) -> #(id, identity)
        None ->
          case reference.function_handle {
            Some(handle) -> #(shopify_function_id_from_handle(handle), identity)
            None -> {
              let #(synthetic, next_identity) =
                synthetic_identity.make_synthetic_gid(
                  identity,
                  "ShopifyFunction",
                )
              #(synthetic, next_identity)
            }
          }
      }
  }
  let handle = case reference.function_handle {
    Some(_) -> reference.function_handle
    None ->
      case existing {
        Some(record) -> record.handle
        None -> None
      }
  }
  let title = case existing {
    Some(record) -> record.title
    None ->
      case handle {
        Some(h) -> Some(title_from_handle(h))
        None -> Some(fallback_title)
      }
  }
  let description = case existing {
    Some(record) -> record.description
    None -> None
  }
  let app_key = case existing {
    Some(record) -> record.app_key
    None -> None
  }
  let app = case existing {
    Some(record) -> record.app
    None -> None
  }
  let record =
    ShopifyFunctionRecord(
      id: id,
      title: title,
      handle: handle,
      api_type: Some(api_type),
      description: description,
      app_key: app_key,
      app: app,
    )
  let #(_, next_store) = store.upsert_staged_shopify_function(store, record)
  #(record, next_store, identity_after_id)
}

/// Mirror `normalizeFunctionHandle`. Lowercases, trims, replaces runs of
/// disallowed characters with `-`, strips leading/trailing `-`, and
/// returns `local-function` if the result is empty.
pub fn normalize_function_handle(handle: String) -> String {
  let lowered = string.lowercase(string.trim(handle))
  let mapped =
    string.to_graphemes(lowered)
    |> list.fold(#([], False), fn(acc, char) {
      let #(out, in_bad_run) = acc
      case is_handle_char(char) {
        True -> #(list.append(out, [char]), False)
        False ->
          case in_bad_run {
            True -> #(out, True)
            False -> #(list.append(out, ["-"]), True)
          }
      }
    })
  let #(chars, _) = mapped
  let joined = string.join(chars, "")
  let trimmed = trim_dashes(joined)
  case trimmed {
    "" -> "local-function"
    _ -> trimmed
  }
}

fn is_handle_char(char: String) -> Bool {
  case char {
    "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" -> True
    "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" -> True
    "u" | "v" | "w" | "x" | "y" | "z" -> True
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    "_" | "-" -> True
    _ -> False
  }
}

fn trim_dashes(s: String) -> String {
  let chars = string.to_graphemes(s)
  let dropped_left = list.drop_while(chars, fn(c) { c == "-" })
  list.reverse(dropped_left)
  |> list.drop_while(fn(c) { c == "-" })
  |> list.reverse()
  |> string.join("")
}

/// Build a deterministic ShopifyFunction gid from a handle. Mirrors
/// `shopifyFunctionIdFromHandle`.
pub fn shopify_function_id_from_handle(handle: String) -> String {
  "gid://shopify/ShopifyFunction/" <> normalize_function_handle(handle)
}

/// Convert a handle to a human-readable title. Mirrors `titleFromHandle`
/// — splits on `-`, `_`, and whitespace; drops empty segments;
/// title-cases each segment; joins with a single space.
pub fn title_from_handle(handle: String) -> String {
  string.to_graphemes(handle)
  |> split_on_handle_separators([], [])
  |> list.filter(fn(seg) { seg != "" })
  |> list.map(capitalize_segment)
  |> string.join(" ")
}

fn split_on_handle_separators(
  remaining: List(String),
  current: List(String),
  acc: List(List(String)),
) -> List(String) {
  case remaining {
    [] ->
      list.append(acc, [list.reverse(current)])
      |> list.map(string.join(_, ""))
    [char, ..rest] ->
      case is_handle_separator(char) {
        True ->
          split_on_handle_separators(
            rest,
            [],
            list.append(acc, [list.reverse(current)]),
          )
        False -> split_on_handle_separators(rest, [char, ..current], acc)
      }
  }
}

fn is_handle_separator(char: String) -> Bool {
  case char {
    "-" | "_" | " " | "\t" | "\n" | "\r" -> True
    _ -> False
  }
}

fn capitalize_segment(segment: String) -> String {
  case string.to_graphemes(segment) {
    [] -> ""
    [first, ..rest] -> string.uppercase(first) <> string.join(rest, "")
  }
}

fn result_to_option(result: Result(a, b)) -> Option(a) {
  case result {
    Ok(value) -> Some(value)
    Error(_) -> None
  }
}
