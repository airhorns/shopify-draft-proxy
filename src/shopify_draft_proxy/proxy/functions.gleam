//// Mirrors `src/proxy/functions.ts`.
////
//// Pass 18 ships the five query roots (`validation`, `validations`,
//// `cartTransforms`, `shopifyFunction`, `shopifyFunctions`) plus the six
//// mutation roots (`validationCreate`/`Update`/`Delete`,
//// `cartTransformCreate`/`Delete`, `taxAppConfigure`).
////
//// Validation mutations still preserve the legacy local helper that mints
//// synthetic function metadata for local fixtures. Cart-transform creates
//// follow Shopify's Function-resolution guardrails: ambiguous, missing,
//// unknown, duplicate, or wrong-API Function references return userErrors
//// before staging any local CartTransform. The mutation pipeline returns a
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
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationFieldResult, type MutationOutcome, LogDraft, MutationFieldResult,
  MutationOutcome,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, fetch_sync,
}
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CartTransformRecord, type ShopifyFunctionAppRecord,
  type ShopifyFunctionRecord, type TaxAppConfigurationRecord,
  type ValidationMetafieldRecord, type ValidationRecord, CartTransformRecord,
  ShopifyFunctionAppRecord, ShopifyFunctionRecord, TaxAppConfigurationRecord,
  ValidationMetafieldRecord, ValidationRecord,
}

const max_active_validations: Int = 25

const function_app_id: String = "347082227713"

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

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, FunctionsError) {
  use data <- result.try(handle_function_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
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

// ===========================================================================
// Mutation path
// ===========================================================================

/// Outcome of a functions mutation. Same shape as the apps / webhooks /
/// saved-search outcome record.
/// User-error payload. Mirrors the TS `FunctionUserError` shape (path,
/// message, optional code).
pub type UserError {
  UserError(field: List(String), message: String, code: Option(String))
}

/// Process a functions mutation document. Mirrors
/// `handleFunctionMutation`.
/// Pattern 2: dispatched LiveHybrid function metadata mutations first
/// try to hydrate referenced ShopifyFunction owner/app metadata from
/// upstream, then stage the mutation locally. Cart-transform creation
/// requires the referenced Function to resolve locally or from that
/// upstream lookup before it stages any local write.
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let identity_for_handlers =
        reserve_multiroot_log_identity(identity, fields)
      let hydrated_store =
        hydrate_referenced_shopify_functions(store, fields, variables, upstream)
      handle_mutation_fields(
        hydrated_store,
        identity_for_handlers,
        request_path,
        document,
        fields,
        fragments,
        variables,
      )
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
  let log_drafts = case list.is_empty(all_staged) {
    True -> []
    False -> {
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
          status: store_types.Staged,
          notes: Some(notes),
        )
      [draft]
    }
  }
  MutationOutcome(
    data: json.object([#("data", json.object(data_entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: all_staged,
    log_drafts: log_drafts,
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

fn validation_enable_would_exceed_cap(
  store: Store,
  exclude_id: String,
  enable: Option(Bool),
) -> Bool {
  case enable {
    Some(True) ->
      active_validation_count_excluding(store, exclude_id)
      >= max_active_validations
    _ -> False
  }
}

fn active_validation_count_excluding(store: Store, exclude_id: String) -> Int {
  store.list_effective_validations(store)
  |> list.filter(fn(record) {
    record.id != exclude_id && record.enable == Some(True)
  })
  |> list.length
}

fn read_validation_metafields(
  input: Dict(String, root_field.ResolvedValue),
  validation_id: String,
  timestamp: String,
  identity: SyntheticIdentityRegistry,
) -> #(List(ValidationMetafieldRecord), SyntheticIdentityRegistry) {
  case dict.get(input, "metafields") {
    Ok(root_field.ListVal(items)) ->
      list.fold(items, #([], identity), fn(acc, item) {
        let #(rows, current_identity) = acc
        case item {
          root_field.ObjectVal(fields) ->
            case
              graphql_helpers.read_arg_string(fields, "namespace"),
              graphql_helpers.read_arg_string(fields, "key")
            {
              Some(namespace), Some(key) -> {
                let #(id, next_identity) =
                  synthetic_identity.make_synthetic_gid(
                    current_identity,
                    "Metafield",
                  )
                #(
                  list.append(rows, [
                    ValidationMetafieldRecord(
                      id: id,
                      validation_id: validation_id,
                      namespace: namespace,
                      key: key,
                      type_: graphql_helpers.read_arg_string(fields, "type"),
                      value: graphql_helpers.read_arg_string(fields, "value"),
                      compare_digest: None,
                      created_at: Some(timestamp),
                      updated_at: Some(timestamp),
                      owner_type: Some("VALIDATION"),
                    ),
                  ]),
                  next_identity,
                )
              }
              _, _ -> acc
            }
          _ -> acc
        }
      })
    _ -> #([], identity)
  }
}

fn missing_cart_transform_function_error() -> UserError {
  UserError(
    field: ["functionHandle"],
    message: "Either function_id or function_handle must be provided.",
    code: Some("MISSING_FUNCTION_IDENTIFIER"),
  )
}

fn multiple_function_identifiers_error() -> UserError {
  UserError(
    field: ["functionHandle"],
    message: "Only one of function_id or function_handle can be provided, not both.",
    code: Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
  )
}

fn validation_missing_function_identifier_error() -> UserError {
  UserError(
    field: ["validation", "functionHandle"],
    message: "Either function_id or function_handle must be provided.",
    code: Some("MISSING_FUNCTION_IDENTIFIER"),
  )
}

fn validation_multiple_function_identifiers_error() -> UserError {
  UserError(
    field: ["validation"],
    message: "Only one of function_id or function_handle can be provided, not both.",
    code: Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
  )
}

fn validation_function_not_found_error(field_name: String) -> UserError {
  UserError(
    field: ["validation", field_name],
    message: "Extension not found.",
    code: Some("NOT_FOUND"),
  )
}

fn function_not_found_error(field_name: String, value: String) -> UserError {
  UserError(
    field: [field_name],
    message: function_not_found_message(field_name, value),
    code: Some("FUNCTION_NOT_FOUND"),
  )
}

fn function_not_found_message(field_name: String, value: String) -> String {
  case field_name {
    "functionId" ->
      "Function "
      <> value
      <> " not found. Ensure that it is released in the current app ("
      <> function_app_id
      <> "), and that the app is installed."
    "functionHandle" -> "Could not find function with handle: " <> value <> "."
    _ -> "Could not find function with " <> field_name <> ": " <> value <> "."
  }
}

fn function_does_not_implement_error(field_name: String) -> UserError {
  UserError(
    field: [field_name],
    message: "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].",
    code: Some("FUNCTION_DOES_NOT_IMPLEMENT"),
  )
}

fn validation_function_does_not_implement_error(
  field_name: String,
) -> UserError {
  UserError(
    field: ["validation", field_name],
    message: "Unexpected Function API. The provided function must implement one of the following extension targets: [%{targets}].",
    code: Some("FUNCTION_DOES_NOT_IMPLEMENT"),
  )
}

fn function_already_registered_error(field_name: String) -> UserError {
  UserError(
    field: [field_name],
    message: "Could not enable cart transform because it is already registered",
    code: Some("FUNCTION_ALREADY_REGISTERED"),
  )
}

fn max_validations_activated_error() -> UserError {
  UserError(
    field: [],
    message: "Cannot have more than 25 active validation functions.",
    code: Some("MAX_VALIDATIONS_ACTIVATED"),
  )
}

fn validation_not_found_error() -> UserError {
  UserError(
    field: ["id"],
    message: "Extension not found.",
    code: Some("NOT_FOUND"),
  )
}

fn cart_transform_delete_not_found_error(id: String) -> UserError {
  let canonical_id = canonical_cart_transform_id(id)
  UserError(
    field: ["id"],
    message: "Could not find cart transform with id: " <> canonical_id,
    code: Some("NOT_FOUND"),
  )
}

fn unauthorized_app_scope_error() -> UserError {
  UserError(
    field: ["base"],
    message: "The app is not authorized to access this Function resource.",
    code: Some("UNAUTHORIZED_APP_SCOPE"),
  )
}

fn canonical_validation_id(id: String) -> String {
  resource_ids.canonical_shopify_resource_gid("Validation", id)
}

fn canonical_cart_transform_id(id: String) -> String {
  resource_ids.canonical_shopify_resource_gid("CartTransform", id)
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
          validation_missing_function_identifier_error(),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    Some(_), Some(_) -> {
      let payload =
        validation_mutation_payload(store, field, fragments, None, [
          validation_multiple_function_identifiers_error(),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    _, _ -> {
      let enable =
        graphql_helpers.read_arg_bool(input, "enable")
        |> option.or(Some(False))
      case validation_enable_would_exceed_cap(store, "", enable) {
        True -> {
          let payload =
            validation_mutation_payload(store, field, fragments, None, [
              max_validations_activated_error(),
            ])
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
            ),
            store,
            identity,
          )
        }
        False -> {
          case resolve_validation_function(store, reference) {
            Error(user_error) -> {
              let payload =
                validation_mutation_payload(store, field, fragments, None, [
                  user_error,
                ])
              #(
                MutationFieldResult(
                  key: key,
                  payload: payload,
                  staged_resource_ids: [],
                ),
                store,
                identity,
              )
            }
            Ok(shopify_fn) -> {
              let title = graphql_helpers.read_arg_string(input, "title")
              let #(timestamp, identity_after_ts) =
                synthetic_identity.make_synthetic_timestamp(identity)
              let #(validation_id, identity_final) =
                synthetic_identity.make_synthetic_gid(
                  identity_after_ts,
                  "Validation",
                )
              let #(metafields, identity_after_metafields) =
                read_validation_metafields(
                  input,
                  validation_id,
                  timestamp,
                  identity_final,
                )
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
                  metafields: metafields,
                  created_at: Some(timestamp),
                  updated_at: Some(timestamp),
                )
              let #(_, store_final) =
                store.upsert_staged_validation(store, validation)
              let payload =
                validation_mutation_payload(
                  store_final,
                  field,
                  fragments,
                  Some(validation),
                  [],
                )
              #(
                MutationFieldResult(
                  key: key,
                  payload: payload,
                  staged_resource_ids: [
                    validation.id,
                  ],
                ),
                store_final,
                identity_after_metafields,
              )
            }
          }
        }
      }
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
          validation_not_found_error(),
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
      let #(maybe_shopify_fn, store_after_fn, identity_after_fn) = case
        current.shopify_function_id
      {
        Some(fn_id) -> #(
          store.get_effective_shopify_function_by_id(store, fn_id),
          store,
          identity,
        )
        None -> #(None, store, identity)
      }
      let #(timestamp, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity_after_fn)
      let new_title = case graphql_helpers.read_arg_string(input, "title") {
        Some(s) -> Some(s)
        None -> current.title
      }
      let new_enable =
        graphql_helpers.read_arg_bool(input, "enable")
        |> option.or(Some(False))
      case validation_enable_would_exceed_cap(store, current.id, new_enable) {
        True -> {
          let payload =
            validation_mutation_payload(store, field, fragments, None, [
              max_validations_activated_error(),
            ])
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
            ),
            store,
            identity,
          )
        }
        False -> {
          let new_block_on_failure = case
            graphql_helpers.read_arg_bool(input, "blockOnFailure")
          {
            Some(b) -> Some(b)
            None -> Some(False)
          }
          let #(new_metafields, identity_after_metafields) = case
            dict.has_key(input, "metafields")
          {
            True ->
              read_validation_metafields(
                input,
                current.id,
                timestamp,
                identity_after_ts,
              )
            False -> #(current.metafields, identity_after_ts)
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
              function_id: current.function_id,
              function_handle: current.function_handle,
              shopify_function_id: new_shopify_function_id,
              metafields: new_metafields,
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
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [
                updated.id,
              ],
            ),
            store_final,
            identity_after_metafields,
          )
        }
      }
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
    Some(s) -> canonical_validation_id(s)
    None -> canonical_validation_id("")
  }
  case store.get_effective_validation_by_id(store, id) {
    None -> {
      let payload =
        delete_payload(field, fragments, None, [
          validation_not_found_error(),
        ])
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
          missing_cart_transform_function_error(),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    Some(_), Some(_) -> {
      let payload =
        cart_transform_mutation_payload(field, fragments, None, [
          multiple_function_identifiers_error(),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    _, _ -> {
      let title = graphql_helpers.read_arg_string(input, "title")
      let #(resolution, store_after_fn, identity_after_fn) =
        resolve_cart_transform_function(store, identity, reference)
      case resolution {
        Error(user_error) -> {
          let payload =
            cart_transform_mutation_payload(field, fragments, None, [
              user_error,
            ])
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
            ),
            store_after_fn,
            identity_after_fn,
          )
        }
        Ok(shopify_fn) -> {
          let field_name = cart_transform_reference_field(reference)
          case cart_transform_function_in_use(store_after_fn, shopify_fn) {
            True -> {
              let payload =
                cart_transform_mutation_payload(field, fragments, None, [
                  function_already_registered_error(field_name),
                ])
              #(
                MutationFieldResult(
                  key: key,
                  payload: payload,
                  staged_resource_ids: [],
                ),
                store_after_fn,
                identity_after_fn,
              )
            }
            False -> {
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
                  function_id: Some(shopify_fn.id),
                  function_handle: function_handle,
                  shopify_function_id: Some(shopify_fn.id),
                  created_at: Some(timestamp),
                  updated_at: Some(timestamp),
                )
              let #(_, store_final) =
                store.upsert_staged_cart_transform(
                  store_after_fn,
                  cart_transform,
                )
              let payload =
                cart_transform_mutation_payload(
                  field,
                  fragments,
                  Some(cart_transform),
                  [],
                )
              #(
                MutationFieldResult(
                  key: key,
                  payload: payload,
                  staged_resource_ids: [cart_transform.id],
                ),
                store_final,
                identity_final,
              )
            }
          }
        }
      }
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
    Some(s) -> canonical_cart_transform_id(s)
    None -> canonical_cart_transform_id("")
  }
  case store.get_effective_cart_transform_by_id(store, id) {
    None -> {
      let payload =
        delete_payload(field, fragments, None, [
          cart_transform_delete_not_found_error(id),
        ])
      #(
        MutationFieldResult(key: key, payload: payload, staged_resource_ids: []),
        store,
        identity,
      )
    }
    Some(record) -> {
      case cart_transform_delete_authorization_error(store, record) {
        Some(error) -> {
          let payload = delete_payload(field, fragments, None, [error])
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [],
            ),
            store,
            identity,
          )
        }
        None -> {
          let next_store = store.delete_staged_cart_transform(store, id)
          let payload = delete_payload(field, fragments, Some(id), [])
          #(
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: [id],
            ),
            next_store,
            identity,
          )
        }
      }
    }
  }
}

fn cart_transform_delete_authorization_error(
  store: Store,
  record: CartTransformRecord,
) -> Option(UserError) {
  use function_id <- option.then(record.shopify_function_id)
  use function_record <- option.then(store.get_effective_shopify_function_by_id(
    store,
    function_id,
  ))
  use function_app_key <- option.then(shopify_function_app_key(function_record))
  use current_installation <- option.then(store.get_current_app_installation(
    store,
  ))
  use current_app <- option.then(store.get_effective_app_by_id(
    store,
    current_installation.app_id,
  ))
  use current_app_key <- option.then(current_app.api_key)
  case function_app_key == current_app_key {
    True -> None
    False -> Some(unauthorized_app_scope_error())
  }
}

fn shopify_function_app_key(record: ShopifyFunctionRecord) -> Option(String) {
  case record.app_key {
    Some(key) -> Some(key)
    None ->
      case record.app {
        Some(app) -> app.api_key
        None -> None
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
          let reference = read_function_reference(input)
          case reference.function_id, reference.function_handle {
            Some(_), Some(_) -> None
            None, None -> None
            _, _ -> Some(#(reference, "VALIDATION"))
          }
        }
        "validationUpdate" -> None
        "cartTransformCreate" -> {
          let input = case
            graphql_helpers.read_arg_object(args, "cartTransform")
          {
            Some(d) -> d
            None -> args
          }
          let reference = read_function_reference(input)
          case reference.function_id, reference.function_handle {
            Some(_), Some(_) -> None
            None, None -> None
            _, _ -> Some(#(reference, "CART_TRANSFORM"))
          }
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

fn resolve_validation_function(
  store: Store,
  reference: FunctionReference,
) -> Result(ShopifyFunctionRecord, UserError) {
  let field_name = cart_transform_reference_field(reference)
  case find_existing_shopify_function(store, reference) {
    None -> Error(validation_function_not_found_error(field_name))
    Some(record) ->
      case validation_function_api_supported(record) {
        True -> Ok(record)
        False -> Error(validation_function_does_not_implement_error(field_name))
      }
  }
}

fn resolve_cart_transform_function(
  store: Store,
  identity: SyntheticIdentityRegistry,
  reference: FunctionReference,
) -> #(
  Result(ShopifyFunctionRecord, UserError),
  Store,
  SyntheticIdentityRegistry,
) {
  let field_name = cart_transform_reference_field(reference)
  let value = cart_transform_reference_value(reference)
  case find_existing_shopify_function(store, reference) {
    None -> #(
      Error(function_not_found_error(field_name, value)),
      store,
      identity,
    )
    Some(record) ->
      case cart_transform_function_api_supported(record) {
        True -> #(Ok(record), store, identity)
        False -> #(
          Error(function_does_not_implement_error(field_name)),
          store,
          identity,
        )
      }
  }
}

fn cart_transform_reference_field(reference: FunctionReference) -> String {
  case reference.function_id {
    Some(_) -> "functionId"
    None -> "functionHandle"
  }
}

fn cart_transform_reference_value(reference: FunctionReference) -> String {
  case reference.function_id {
    Some(id) -> id
    None ->
      case reference.function_handle {
        Some(handle) -> handle
        None -> ""
      }
  }
}

fn cart_transform_function_api_supported(
  record: ShopifyFunctionRecord,
) -> Bool {
  case record.api_type {
    None -> True
    Some(api_type) -> normalize_function_api_type(api_type) == "CART_TRANSFORM"
  }
}

fn validation_function_api_supported(record: ShopifyFunctionRecord) -> Bool {
  case record.api_type {
    None -> True
    Some(api_type) -> {
      let normalized = normalize_function_api_type(api_type)
      normalized == "VALIDATION" || normalized == "CART_CHECKOUT_VALIDATION"
    }
  }
}

fn normalize_function_api_type(api_type: String) -> String {
  api_type
  |> string.uppercase
  |> string.replace("-", "_")
}

fn cart_transform_function_in_use(
  store: Store,
  shopify_fn: ShopifyFunctionRecord,
) -> Bool {
  store.list_effective_cart_transforms(store)
  |> list.any(fn(record) {
    record.shopify_function_id == Some(shopify_fn.id)
    || record.function_id == Some(shopify_fn.id)
  })
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
