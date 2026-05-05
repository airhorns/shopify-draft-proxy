//// Stateful Gleam port of the metaobject definition + entry runtime.
////
//// This module mirrors the TypeScript `metaobject-definitions.ts` slice:
//// definitions and entries are staged locally, downstream reads resolve from
//// effective base+staged state, and successful supported mutations record log
//// drafts for commit replay.

import gleam/dict.{type Dict}
import gleam/dynamic.{type Dynamic}
import gleam/dynamic/decode
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order.{Eq}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Location, type ObjectField, type Selection, Argument, Field,
  FragmentDefinition, FragmentSpread, InlineFragment, NamedType, ObjectField,
  ObjectValue, SelectionSet,
}
import shopify_draft_proxy/graphql/location as graphql_location
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/graphql/source as graphql_source
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionWindow, SelectedFieldOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_type_condition_applies,
  get_document_fragments, get_field_response_key, paginate_connection_items,
  project_graphql_value, serialize_connection, source_to_json, src_object,
}
import shopify_draft_proxy/proxy/metafield_values
import shopify_draft_proxy/proxy/metaobject_standard_templates_data as standard_templates
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type MutationOutcome, MutationOutcome, read_optional_string,
  single_root_log_draft,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{
  type Store, delete_staged_metaobject, delete_staged_metaobject_definition,
  find_effective_metaobject_by_handle,
  find_effective_metaobject_definition_by_type, get_effective_metaobject_by_id,
  get_effective_metaobject_definition_by_id,
  list_effective_metaobject_definitions, list_effective_metaobjects,
  list_effective_metaobjects_by_type, upsert_base_metaobject_definitions,
  upsert_base_metaobjects, upsert_staged_metaobject,
  upsert_staged_metaobject_definition,
}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type MetaobjectCapabilitiesRecord, type MetaobjectDefinitionCapabilitiesRecord,
  type MetaobjectDefinitionCapabilityRecord, type MetaobjectDefinitionRecord,
  type MetaobjectDefinitionTypeRecord, type MetaobjectFieldDefinitionRecord,
  type MetaobjectFieldDefinitionReferenceRecord,
  type MetaobjectFieldDefinitionValidationRecord, type MetaobjectFieldRecord,
  type MetaobjectJsonValue, type MetaobjectOnlineStoreCapabilityRecord,
  type MetaobjectPublishableCapabilityRecord, type MetaobjectRecord,
  type MetaobjectStandardTemplateRecord, MetaobjectBool,
  MetaobjectCapabilitiesRecord, MetaobjectDefinitionCapabilitiesRecord,
  MetaobjectDefinitionCapabilityRecord, MetaobjectDefinitionRecord,
  MetaobjectDefinitionTypeRecord, MetaobjectFieldDefinitionRecord,
  MetaobjectFieldDefinitionReferenceRecord,
  MetaobjectFieldDefinitionValidationRecord, MetaobjectFieldRecord,
  MetaobjectFloat, MetaobjectInt, MetaobjectList, MetaobjectNull,
  MetaobjectObject, MetaobjectOnlineStoreCapabilityRecord,
  MetaobjectPublishableCapabilityRecord, MetaobjectRecord,
  MetaobjectStandardTemplateRecord, MetaobjectString,
}

const domain_name = "metaobjects"

const execution_name = "stage-locally"

pub type MetaobjectDefinitionsError {
  ParseFailed(root_field.RootFieldError)
}

type UserError {
  UserError(
    field: Option(List(String)),
    message: String,
    code: String,
    element_key: Option(String),
    element_index: Option(Int),
  )
}

type BulkDeleteJob {
  BulkDeleteJob(id: String, done: Bool)
}

type BulkDeleteWhere {
  BulkDeleteByIds(List(String))
  BulkDeleteByType(String)
  BulkDeleteNoSelector
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
    log_drafts: List(LogDraft),
  )
}

type FieldOperation {
  FieldCreate(Dict(String, root_field.ResolvedValue))
  FieldUpdate(Dict(String, root_field.ResolvedValue))
  FieldDelete(String)
  FieldUpsert(Dict(String, root_field.ResolvedValue))
}

pub fn is_metaobject_definitions_query_root(name: String) -> Bool {
  case name {
    "metaobject" -> True
    "metaobjectByHandle" -> True
    "metaobjects" -> True
    "metaobjectDefinition" -> True
    "metaobjectDefinitionByType" -> True
    "metaobjectDefinitions" -> True
    _ -> False
  }
}

pub fn is_metaobject_definitions_mutation_root(name: String) -> Bool {
  case name {
    "metaobjectDefinitionCreate" -> True
    "metaobjectDefinitionUpdate" -> True
    "metaobjectDefinitionDelete" -> True
    "standardMetaobjectDefinitionEnable" -> True
    "metaobjectCreate" -> True
    "metaobjectUpdate" -> True
    "metaobjectUpsert" -> True
    "metaobjectDelete" -> True
    "metaobjectBulkDelete" -> True
    _ -> False
  }
}

pub fn handle_metaobject_definitions_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MetaobjectDefinitionsError) {
  handle_metaobject_definitions_query_with_app_id(
    store,
    document,
    variables,
    None,
  )
}

pub fn handle_metaobject_definitions_query_with_app_id(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Result(Json, MetaobjectDefinitionsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(
        store,
        fields,
        fragments,
        variables,
        requesting_api_client_id,
      ))
    }
  }
}

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Json {
  json.object(
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = case field {
        Field(name: name, ..) ->
          case name.value {
            "metaobjectDefinition" ->
              case read_id_arg(field, variables) {
                Some(id) ->
                  case get_effective_metaobject_definition_by_id(store, id) {
                    Some(definition) ->
                      serialize_definition_selection(
                        definition,
                        field,
                        fragments,
                      )
                    None -> json.null()
                  }
                None -> json.null()
              }
            "metaobjectDefinitionByType" ->
              case read_string_arg(field, variables, "type") {
                Some(type_) ->
                  case
                    find_effective_metaobject_definition_by_normalized_type(
                      store,
                      normalize_definition_type(type_, requesting_api_client_id),
                    )
                  {
                    Some(definition) ->
                      serialize_definition_selection(
                        definition,
                        field,
                        fragments,
                      )
                    None -> json.null()
                  }
                None -> json.null()
              }
            "metaobjectDefinitions" ->
              serialize_definitions_connection(
                store,
                field,
                fragments,
                variables,
                requesting_api_client_id,
              )
            "metaobject" ->
              case read_id_arg(field, variables) {
                Some(id) ->
                  case get_effective_metaobject_by_id(store, id) {
                    Some(metaobject) ->
                      serialize_metaobject_selection(
                        store,
                        metaobject,
                        field,
                        fragments,
                      )
                    None -> json.null()
                  }
                None -> json.null()
              }
            "metaobjectByHandle" ->
              case read_handle_arg(field, variables) {
                #(Some(type_), Some(handle)) ->
                  case
                    find_effective_metaobject_by_handle(store, type_, handle)
                  {
                    Some(metaobject) ->
                      serialize_metaobject_selection(
                        store,
                        metaobject,
                        field,
                        fragments,
                      )
                    None -> json.null()
                  }
                _ -> json.null()
              }
            "metaobjects" ->
              serialize_metaobjects_connection(
                store,
                field,
                fragments,
                variables,
              )
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    }),
  )
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, MetaobjectDefinitionsError) {
  process_with_requesting_api_client_id(store, document, variables, None)
}

pub fn process_with_requesting_api_client_id(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Result(Json, MetaobjectDefinitionsError) {
  case
    handle_metaobject_definitions_query_with_app_id(
      store,
      document,
      variables,
      requesting_api_client_id,
    )
  {
    Ok(data) -> Ok(graphql_helpers.wrap_data(data))
    Error(e) -> Error(e)
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
      should_passthrough_in_live_hybrid(
        proxy,
        parsed.type_,
        primary_root_field,
        variables,
      )
    _ -> False
  }
  case want_passthrough {
    // Pattern 1: cold LiveHybrid metaobject reads are upstream-verbatim.
    // Once local definitions/entries are staged or deleted, reads stay local
    // so supported mutations preserve read-after-write behavior.
    True -> passthrough.passthrough_sync(proxy, request)
    False ->
      case
        process_with_requesting_api_client_id(
          proxy.store,
          document,
          variables,
          app_identity.read_requesting_api_client_id(request.headers),
        )
      {
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
                        json.string(
                          "Failed to handle metaobject definitions query",
                        ),
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

fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "metaobject" ->
      !local_has_metaobject_id(proxy, variables)
    parse_operation.QueryOperation, "metaobjectByHandle" ->
      !local_has_metaobjects(proxy)
    parse_operation.QueryOperation, "metaobjects" ->
      !local_has_metaobjects(proxy)
    parse_operation.QueryOperation, "metaobjectDefinition" ->
      !local_has_metaobject_definition_id(proxy, variables)
    parse_operation.QueryOperation, "metaobjectDefinitionByType" ->
      !local_has_metaobject_definitions(proxy)
    parse_operation.QueryOperation, "metaobjectDefinitions" ->
      !local_has_metaobject_definitions(proxy)
    _, _ -> False
  }
}

fn normalize_definition_type(
  type_: String,
  requesting_api_client_id: Option(String),
) -> String {
  let resolved = case string.starts_with(type_, "$app:") {
    True ->
      case requesting_api_client_id {
        Some(api_client_id) ->
          "app--"
          <> api_client_id
          <> "--"
          <> string.drop_start(type_, string.length("$app:"))
        None -> type_
      }
    False -> type_
  }
  string.lowercase(resolved)
}

fn normalized_definition_type_from_input(
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Option(String) {
  read_string(input, "type")
  |> option.map(fn(type_) {
    normalize_definition_type(type_, requesting_api_client_id)
  })
}

fn is_app_reserved_definition_type_input(type_: String) -> Bool {
  string.starts_with(type_, "$app:")
}

fn local_has_metaobject_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.flat_map(resolved_value_strings)
  |> list.any(fn(id) {
    is_proxy_synthetic_gid(id) || local_metaobject_id_known(proxy.store, id)
  })
}

fn local_metaobject_id_known(store: Store, id: String) -> Bool {
  case get_effective_metaobject_by_id(store, id) {
    Some(_) -> True
    None ->
      case dict.get(store.staged_state.deleted_metaobject_ids, id) {
        Ok(True) -> True
        _ -> False
      }
  }
}

fn local_has_metaobject_definition_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.flat_map(resolved_value_strings)
  |> list.any(fn(id) {
    is_proxy_synthetic_gid(id)
    || local_metaobject_definition_id_known(proxy.store, id)
  })
}

fn local_metaobject_definition_id_known(store: Store, id: String) -> Bool {
  case get_effective_metaobject_definition_by_id(store, id) {
    Some(_) -> True
    None ->
      case dict.get(store.staged_state.deleted_metaobject_definition_ids, id) {
        Ok(True) -> True
        _ -> False
      }
  }
}

fn local_has_metaobjects(proxy: DraftProxy) -> Bool {
  !list.is_empty(list_effective_metaobjects(proxy.store))
  || !list.is_empty(dict.keys(proxy.store.staged_state.deleted_metaobject_ids))
}

fn local_has_metaobject_definitions(proxy: DraftProxy) -> Bool {
  !list.is_empty(list_effective_metaobject_definitions(proxy.store))
  || !list.is_empty(dict.keys(
    proxy.store.staged_state.deleted_metaobject_definition_ids,
  ))
}

fn find_effective_metaobject_definition_by_normalized_type(
  store: Store,
  normalized_type: String,
) -> Option(MetaobjectDefinitionRecord) {
  list_effective_metaobject_definitions(store)
  |> list.find(fn(definition) {
    string.lowercase(definition.type_) == normalized_type
  })
  |> option.from_result
}

fn find_effective_metaobject_definition_by_input_type(
  store: Store,
  type_: String,
  requesting_api_client_id: Option(String),
) -> Option(MetaobjectDefinitionRecord) {
  find_effective_metaobject_definition_by_normalized_type(
    store,
    normalize_definition_type(type_, requesting_api_client_id),
  )
}

fn resolved_value_strings(value: root_field.ResolvedValue) -> List(String) {
  case value {
    root_field.StringVal(value) -> [value]
    root_field.ListVal(values) -> list.flat_map(values, resolved_value_strings)
    root_field.ObjectVal(fields) ->
      dict.values(fields) |> list.flat_map(resolved_value_strings)
    _ -> []
  }
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  process_mutation_with_requesting_api_client_id(
    store,
    identity,
    document,
    variables,
    upstream,
    app_identity.read_requesting_api_client_id(upstream.headers),
  )
}

pub fn process_mutation_with_headers(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
  request_headers: Dict(String, String),
) -> MutationOutcome {
  process_mutation_with_requesting_api_client_id(
    store,
    identity,
    document,
    variables,
    upstream,
    app_identity.read_requesting_api_client_id(request_headers),
  )
}

fn process_mutation_with_requesting_api_client_id(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
  requesting_api_client_id: Option(String),
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      case bulk_delete_where_exactly_one_errors(fields, variables, document) {
        [] ->
          handle_mutation_fields(
            store,
            identity,
            fields,
            fragments,
            variables,
            upstream,
            requesting_api_client_id,
          )
        errors ->
          MutationOutcome(
            data: json.object([
              #("errors", json.preprocessed_array(errors)),
              #("data", json.object([#("metaobjectBulkDelete", json.null())])),
            ]),
            store: store,
            identity: identity,
            staged_resource_ids: [],
            log_drafts: [],
          )
      }
    }
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
  requesting_api_client_id: Option(String),
) -> MutationOutcome {
  let initial = #([], [], store, identity, [], [])
  let #(entries, errors, final_store, final_identity, staged_ids, drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(
        data_entries,
        all_errors,
        current_store,
        current_identity,
        ids,
        all_drafts,
      ) = acc
      case field {
        Field(name: name, ..) -> {
          let result =
            dispatch_mutation_field(
              current_store,
              current_identity,
              field,
              fragments,
              variables,
              name.value,
              upstream,
              requesting_api_client_id,
            )
          let #(field_result, next_store, next_identity) = result
          let next_errors =
            list.append(all_errors, field_result.top_level_errors)
          let next_entries = case field_result.top_level_errors {
            [] ->
              list.append(data_entries, [
                #(field_result.key, field_result.payload),
              ])
            _ -> data_entries
          }
          let next_ids = case field_result.top_level_errors {
            [] -> list.append(ids, field_result.staged_resource_ids)
            _ -> ids
          }
          #(
            next_entries,
            next_errors,
            next_store,
            next_identity,
            next_ids,
            list.append(all_drafts, field_result.log_drafts),
          )
        }
        _ -> acc
      }
    })
  let envelope = case errors {
    [] -> json.object([#("data", json.object(entries))])
    _ -> json.object([#("errors", json.preprocessed_array(errors))])
  }
  MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: case errors {
      [] -> staged_ids
      _ -> []
    },
    log_drafts: case errors {
      [] -> drafts
      _ -> []
    },
  )
}

fn bulk_delete_where_exactly_one_errors(
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
  source_body: String,
) -> List(Json) {
  list.filter_map(fields, fn(field) {
    case field {
      Field(name: name, ..) ->
        case name.value == "metaobjectBulkDelete" {
          False -> Error(Nil)
          True -> {
            let args = graphql_helpers.field_args(field, variables)
            case read_object(args, "where") {
              None -> Error(Nil)
              Some(where) -> {
                let provided =
                  bool_to_int(dict.has_key(where, "type"))
                  + bool_to_int(dict.has_key(where, "ids"))
                case provided == 1 {
                  True -> Error(Nil)
                  False -> Ok(bulk_delete_exactly_one_error(field, source_body))
                }
              }
            }
          }
        }
      _ -> Error(Nil)
    }
  })
}

fn bool_to_int(value: Bool) -> Int {
  case value {
    True -> 1
    False -> 0
  }
}

fn bulk_delete_exactly_one_error(
  field: Selection,
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "MetaobjectBulkDeleteWhereCondition requires exactly one of type, ids",
      ),
    ),
  ]
  let with_locations = case bulk_delete_where_locations(field, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #("path", json.array(["metaobjectBulkDelete"], json.string)),
      #(
        "extensions",
        json.object([#("code", json.string("INVALID_FIELD_ARGUMENTS"))]),
      ),
    ]),
  )
}

fn bulk_delete_where_locations(
  field: Selection,
  source_body: String,
) -> Option(Json) {
  case field {
    Field(arguments: arguments, loc: field_loc, ..) ->
      case mutation_helpers.find_argument(arguments, "where") {
        Some(Argument(value: ObjectValue(fields: fields, ..), ..)) ->
          json_locations(
            [field_loc, bulk_delete_conflicting_field_location(fields)],
            source_body,
          )
        _ -> json_locations([field_loc], source_body)
      }
    _ -> None
  }
}

fn bulk_delete_conflicting_field_location(
  fields: List(ObjectField),
) -> Option(Location) {
  case fields {
    [] -> None
    [ObjectField(name: name, loc: loc, ..), ..rest] ->
      case name.value == "ids" {
        True -> loc
        False -> bulk_delete_conflicting_field_location(rest)
      }
  }
}

fn json_locations(
  locations: List(Option(Location)),
  source_body: String,
) -> Option(Json) {
  let encoded =
    list.filter_map(locations, fn(location) {
      use loc <- result.try(option_to_result(location))
      let source = graphql_source.new(source_body)
      let computed = graphql_location.get_location(source, position: loc.start)
      Ok(
        json.object([
          #("line", json.int(computed.line)),
          #("column", json.int(computed.column)),
        ]),
      )
    })
  case encoded {
    [] -> None
    _ -> Some(json.preprocessed_array(encoded))
  }
}

fn dispatch_mutation_field(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
  upstream: UpstreamContext,
  requesting_api_client_id: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  case name {
    "metaobjectDefinitionCreate" ->
      handle_definition_create(
        store,
        identity,
        field,
        fragments,
        variables,
        upstream,
        requesting_api_client_id,
      )
    "metaobjectDefinitionUpdate" ->
      handle_definition_update(
        store,
        identity,
        field,
        fragments,
        variables,
        requesting_api_client_id,
      )
    "metaobjectDefinitionDelete" ->
      handle_definition_delete(store, identity, field, variables)
    "standardMetaobjectDefinitionEnable" ->
      handle_standard_definition_enable(
        store,
        identity,
        field,
        fragments,
        variables,
      )
    "metaobjectCreate" ->
      handle_metaobject_create(
        store,
        identity,
        field,
        fragments,
        variables,
        upstream,
        requesting_api_client_id,
      )
    "metaobjectUpdate" ->
      handle_metaobject_update(
        store,
        identity,
        field,
        fragments,
        variables,
        upstream,
      )
    "metaobjectUpsert" ->
      handle_metaobject_upsert(
        store,
        identity,
        field,
        fragments,
        variables,
        upstream,
        requesting_api_client_id,
      )
    "metaobjectDelete" ->
      handle_metaobject_delete(store, identity, field, variables, upstream)
    "metaobjectBulkDelete" ->
      handle_metaobject_bulk_delete(
        store,
        identity,
        field,
        fragments,
        variables,
        upstream,
      )
    _ -> #(
      MutationFieldResult(
        get_field_response_key(field),
        json.null(),
        [],
        [],
        [],
      ),
      store,
      identity,
    )
  }
}

// ---------------------------------------------------------------------------
// Mutation handlers
// ---------------------------------------------------------------------------

fn handle_definition_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
  requesting_api_client_id: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = read_object_arg(args, "definition")
  let validation_errors =
    build_create_definition_validation_errors(input, requesting_api_client_id)
  let store = case validation_errors {
    [] ->
      maybe_hydrate_definition_for_create(
        store,
        input,
        upstream,
        requesting_api_client_id,
      )
    [_, ..] -> store
  }
  let user_errors = case validation_errors {
    [] ->
      build_create_definition_uniqueness_errors(
        store,
        input,
        requesting_api_client_id,
      )
    [_, ..] -> validation_errors
  }
  case user_errors {
    [_, ..] -> {
      let payload = definition_payload(field, fragments, None, user_errors)
      #(MutationFieldResult(key, payload, [], [], []), store, identity)
    }
    [] -> {
      let #(definition, next_identity) =
        build_definition_from_create_input(
          identity,
          input,
          requesting_api_client_id,
        )
      let #(staged, next_store) =
        upsert_staged_metaobject_definition(store, definition)
      let payload = definition_payload(field, fragments, Some(staged), [])
      #(
        MutationFieldResult(key, payload, [staged.id], [], [
          log_draft("metaobjectDefinitionCreate", [staged.id]),
        ]),
        next_store,
        next_identity,
      )
    }
  }
}

fn handle_definition_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = read_string(args, "id")
  let input = read_object_arg(args, "definition")
  case id {
    None -> {
      let payload =
        definition_payload(field, fragments, None, [
          record_not_found_user_error(["id"]),
        ])
      #(MutationFieldResult(key, payload, [], [], []), store, identity)
    }
    Some(definition_id) ->
      case get_effective_metaobject_definition_by_id(store, definition_id) {
        None -> {
          let payload =
            definition_payload(field, fragments, None, [
              record_not_found_user_error(["id"]),
            ])
          #(MutationFieldResult(key, payload, [], [], []), store, identity)
        }
        Some(existing) -> {
          let reset_field_order =
            read_bool_arg(field, variables, "resetFieldOrder")
            || option.unwrap(read_bool(input, "resetFieldOrder"), False)
          let #(updated, next_identity, user_errors) =
            apply_definition_update(
              store,
              identity,
              existing,
              input,
              reset_field_order,
              requesting_api_client_id,
            )
          case user_errors {
            [_, ..] -> {
              let payload =
                definition_payload(field, fragments, None, user_errors)
              #(MutationFieldResult(key, payload, [], [], []), store, identity)
            }
            [] -> {
              let #(staged, next_store) =
                upsert_staged_metaobject_definition(store, updated)
              let payload =
                definition_payload(field, fragments, Some(staged), [])
              #(
                MutationFieldResult(key, payload, [staged.id], [], [
                  log_draft("metaobjectDefinitionUpdate", [staged.id]),
                ]),
                next_store,
                next_identity,
              )
            }
          }
        }
      }
  }
}

fn handle_definition_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let id = read_string(graphql_helpers.field_args(field, variables), "id")
  case id {
    None -> #(
      definition_delete_result(key, field, None, [
        record_not_found_user_error(["id"]),
      ]),
      store,
      identity,
    )
    Some(definition_id) ->
      case get_effective_metaobject_definition_by_id(store, definition_id) {
        None -> #(
          definition_delete_result(key, field, None, [
            record_not_found_user_error(["id"]),
          ]),
          store,
          identity,
        )
        Some(definition) -> {
          let count = option.unwrap(definition.metaobjects_count, 0)
          case count > 0 {
            True -> {
              let user_error =
                UserError(
                  field: Some(["id"]),
                  message: "Local proxy cannot delete a metaobject definition with associated metaobjects until entry cascade behavior is modeled.",
                  code: "UNSUPPORTED",
                  element_key: None,
                  element_index: None,
                )
              #(
                definition_delete_result(key, field, None, [user_error]),
                store,
                identity,
              )
            }
            False -> {
              let next_store =
                delete_staged_metaobject_definition(store, definition_id)
              #(
                definition_delete_result(key, field, Some(definition_id), []),
                next_store,
                identity,
              )
            }
          }
        }
      }
  }
}

fn handle_standard_definition_enable(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case read_string(args, "type") {
    None -> {
      let payload =
        definition_payload(field, fragments, None, [
          standard_template_record_not_found_error(),
        ])
      #(MutationFieldResult(key, payload, [], [], []), store, identity)
    }
    Some(type_) ->
      case standard_template(type_) {
        None -> {
          let payload =
            definition_payload(field, fragments, None, [
              standard_template_record_not_found_error(),
            ])
          #(MutationFieldResult(key, payload, [], [], []), store, identity)
        }
        Some(template) -> {
          case find_effective_metaobject_definition_by_type(store, type_) {
            Some(existing) -> {
              let payload =
                definition_payload(field, fragments, Some(existing), [])
              #(MutationFieldResult(key, payload, [], [], []), store, identity)
            }
            None -> {
              let #(definition, next_identity) =
                build_standard_definition(identity, template)
              let #(staged, next_store) =
                upsert_staged_metaobject_definition(store, definition)
              let payload =
                definition_payload(field, fragments, Some(staged), [])
              #(
                MutationFieldResult(key, payload, [staged.id], [], [
                  log_draft("standardMetaobjectDefinitionEnable", [staged.id]),
                ]),
                next_store,
                next_identity,
              )
            }
          }
        }
      }
  }
}

fn standard_template_record_not_found_error() -> UserError {
  UserError(Some(["type"]), "Record not found", "RECORD_NOT_FOUND", None, None)
}

fn handle_metaobject_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
  requesting_api_client_id: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let input =
    read_object_arg(graphql_helpers.field_args(field, variables), "metaobject")
  let type_ = read_string(input, "type")
  // Pattern 2: cold LiveHybrid creates still stage locally, but first
  // hydrate the upstream definition so valid types do not fail as unknown.
  let store =
    maybe_hydrate_definition_by_type(
      store,
      type_,
      upstream,
      requesting_api_client_id,
    )
  let definition = case type_ {
    Some(t) ->
      find_effective_metaobject_definition_by_input_type(
        store,
        t,
        requesting_api_client_id,
      )
    None -> None
  }
  let user_errors = build_create_metaobject_user_errors(type_, definition)
  case user_errors, definition {
    [_, ..], _ -> #(
      MutationFieldResult(
        key,
        metaobject_payload(store, field, fragments, None, user_errors),
        [],
        [],
        [],
      ),
      store,
      identity,
    )
    [], None -> #(
      MutationFieldResult(
        key,
        metaobject_payload(store, field, fragments, None, []),
        [],
        [],
        [],
      ),
      store,
      identity,
    )
    [], Some(defn) -> {
      let #(created, next_identity, field_errors) =
        build_metaobject_from_create_input(store, identity, input, defn)
      case field_errors, created {
        [_, ..], _ -> #(
          MutationFieldResult(
            key,
            metaobject_payload(store, field, fragments, None, field_errors),
            [],
            [],
            [],
          ),
          store,
          identity,
        )
        [], None -> #(
          MutationFieldResult(
            key,
            metaobject_payload(store, field, fragments, None, []),
            [],
            [],
            [],
          ),
          store,
          identity,
        )
        [], Some(metaobject) -> {
          let #(staged, staged_store) =
            upsert_staged_metaobject(store, metaobject)
          let #(next_store, final_identity) =
            adjust_definition_count(
              staged_store,
              next_identity,
              staged.type_,
              1,
            )
          #(
            MutationFieldResult(
              key,
              metaobject_payload(next_store, field, fragments, Some(staged), []),
              [staged.id],
              [],
              [log_draft("metaobjectCreate", [staged.id])],
            ),
            next_store,
            final_identity,
          )
        }
      }
    }
  }
}

fn handle_metaobject_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = read_object_arg(args, "metaobject")
  case read_string(args, "id") {
    None -> #(
      MutationFieldResult(
        key,
        metaobject_payload(store, field, fragments, None, [
          record_not_found_user_error(["id"]),
        ]),
        [],
        [],
        [],
      ),
      store,
      identity,
    )
    Some(id) -> {
      // Pattern 2: existing upstream entries are hydrated before local
      // update staging so supported mutations still avoid upstream writes.
      let store = maybe_hydrate_metaobject_by_id(store, Some(id), upstream)
      case get_effective_metaobject_by_id(store, id) {
        None -> #(
          MutationFieldResult(
            key,
            metaobject_payload(store, field, fragments, None, [
              record_not_found_user_error(["id"]),
            ]),
            [],
            [],
            [],
          ),
          store,
          identity,
        )
        Some(existing) ->
          case
            find_effective_metaobject_definition_by_type(store, existing.type_)
          {
            None -> {
              let err =
                UserError(
                  Some(["metaobject", "type"]),
                  "No metaobject definition exists for type \""
                    <> existing.type_
                    <> "\"",
                  "UNDEFINED_OBJECT_TYPE",
                  None,
                  None,
                )
              #(
                MutationFieldResult(
                  key,
                  metaobject_payload(store, field, fragments, None, [err]),
                  [],
                  [],
                  [],
                ),
                store,
                identity,
              )
            }
            Some(definition) -> {
              let #(updated, next_identity, user_errors) =
                apply_metaobject_update_input(
                  store,
                  identity,
                  existing,
                  input,
                  definition,
                )
              case user_errors, updated {
                [_, ..], _ -> #(
                  MutationFieldResult(
                    key,
                    metaobject_payload(
                      store,
                      field,
                      fragments,
                      None,
                      user_errors,
                    ),
                    [],
                    [],
                    [],
                  ),
                  store,
                  identity,
                )
                [], None -> #(
                  MutationFieldResult(
                    key,
                    metaobject_payload(store, field, fragments, None, []),
                    [],
                    [],
                    [],
                  ),
                  store,
                  identity,
                )
                [], Some(metaobject) -> {
                  let #(staged, next_store) =
                    upsert_staged_metaobject(store, metaobject)
                  #(
                    MutationFieldResult(
                      key,
                      metaobject_payload(
                        next_store,
                        field,
                        fragments,
                        Some(staged),
                        [],
                      ),
                      [staged.id],
                      [],
                      [log_draft("metaobjectUpdate", [staged.id])],
                    ),
                    next_store,
                    next_identity,
                  )
                }
              }
            }
          }
      }
    }
  }
}

fn handle_metaobject_upsert(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
  requesting_api_client_id: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let #(type_, handle) = read_handle_value(read_object_arg(args, "handle"))
  let input = read_upsert_payload_input(args)
  let handle_is_blank = case handle {
    Some(h) -> string.trim(h) == ""
    None -> False
  }
  case type_, handle, handle_is_blank {
    Some(_), Some(_), True -> {
      let err =
        UserError(
          Some(["handle", "handle"]),
          "Handle can't be blank",
          "BLANK",
          None,
          None,
        )
      #(
        MutationFieldResult(
          key,
          metaobject_payload(store, field, fragments, None, [err]),
          [],
          [],
          [],
        ),
        store,
        identity,
      )
    }
    Some(t), Some(h), False -> {
      // Pattern 2: hydrate the definition for cold LiveHybrid upserts; the
      // upsert write itself remains local and Snapshot mode stays local.
      let store =
        maybe_hydrate_definition_by_type(
          store,
          Some(t),
          upstream,
          requesting_api_client_id,
        )
      let store = maybe_hydrate_metaobject_by_handle(store, t, h, upstream)
      case
        find_effective_metaobject_definition_by_input_type(
          store,
          t,
          requesting_api_client_id,
        )
      {
        None -> {
          let err =
            UserError(
              Some(["handle", "type"]),
              "No metaobject definition exists for type \"" <> t <> "\"",
              "UNDEFINED_OBJECT_TYPE",
              None,
              None,
            )
          #(
            MutationFieldResult(
              key,
              metaobject_payload(store, field, fragments, None, [err]),
              [],
              [],
              [],
            ),
            store,
            identity,
          )
        }
        Some(definition) ->
          case find_effective_metaobject_by_handle(store, t, h) {
            Some(existing) -> {
              let store =
                maybe_hydrate_metaobject_by_handle(
                  store,
                  t,
                  read_string(input, "handle")
                    |> option.unwrap(existing.handle),
                  upstream,
                )
              let input_with_handle = case dict.get(input, "handle") {
                Ok(_) -> input
                Error(_) ->
                  dict.insert(
                    input,
                    "handle",
                    root_field.StringVal(existing.handle),
                  )
              }
              let #(updated, next_identity, user_errors) =
                apply_metaobject_update_input(
                  store,
                  identity,
                  existing,
                  input_with_handle,
                  definition,
                )
              let user_errors = partition_upsert_user_errors(user_errors)
              case user_errors, updated {
                [_, ..], _ -> #(
                  MutationFieldResult(
                    key,
                    metaobject_payload(
                      store,
                      field,
                      fragments,
                      None,
                      user_errors,
                    ),
                    [],
                    [],
                    [],
                  ),
                  store,
                  identity,
                )
                [], Some(metaobject) -> {
                  case metaobject_upsert_exact_match(existing, metaobject) {
                    True -> #(
                      MutationFieldResult(
                        key,
                        metaobject_payload(
                          store,
                          field,
                          fragments,
                          Some(existing),
                          [],
                        ),
                        [],
                        [],
                        [],
                      ),
                      store,
                      identity,
                    )
                    False -> {
                      let #(staged, next_store) =
                        upsert_staged_metaobject(store, metaobject)
                      #(
                        MutationFieldResult(
                          key,
                          metaobject_payload(
                            next_store,
                            field,
                            fragments,
                            Some(staged),
                            [],
                          ),
                          [staged.id],
                          [],
                          [log_draft("metaobjectUpsert", [staged.id])],
                        ),
                        next_store,
                        next_identity,
                      )
                    }
                  }
                }
                [], None -> #(
                  MutationFieldResult(
                    key,
                    metaobject_payload(store, field, fragments, None, []),
                    [],
                    [],
                    [],
                  ),
                  store,
                  identity,
                )
              }
            }
            None -> {
              let create_input =
                dict.insert(input, "type", root_field.StringVal(t))
              let create_input =
                dict.insert(create_input, "handle", root_field.StringVal(h))
              let #(created, next_identity, field_errors) =
                build_metaobject_from_create_input(
                  store,
                  identity,
                  create_input,
                  definition,
                )
              let field_errors = partition_upsert_user_errors(field_errors)
              case field_errors, created {
                [_, ..], _ -> #(
                  MutationFieldResult(
                    key,
                    metaobject_payload(
                      store,
                      field,
                      fragments,
                      None,
                      field_errors,
                    ),
                    [],
                    [],
                    [],
                  ),
                  store,
                  identity,
                )
                [], Some(metaobject) -> {
                  let #(staged, staged_store) =
                    upsert_staged_metaobject(store, metaobject)
                  let #(next_store, final_identity) =
                    adjust_definition_count(
                      staged_store,
                      next_identity,
                      staged.type_,
                      1,
                    )
                  #(
                    MutationFieldResult(
                      key,
                      metaobject_payload(
                        next_store,
                        field,
                        fragments,
                        Some(staged),
                        [],
                      ),
                      [staged.id],
                      [],
                      [log_draft("metaobjectUpsert", [staged.id])],
                    ),
                    next_store,
                    final_identity,
                  )
                }
                [], None -> #(
                  MutationFieldResult(
                    key,
                    metaobject_payload(store, field, fragments, None, []),
                    [],
                    [],
                    [],
                  ),
                  store,
                  identity,
                )
              }
            }
          }
      }
    }
    _, _, _ -> {
      let err =
        UserError(
          Some(["handle", "handle"]),
          "Handle can't be blank",
          "BLANK",
          None,
          None,
        )
      #(
        MutationFieldResult(
          key,
          metaobject_payload(store, field, fragments, None, [err]),
          [],
          [],
          [],
        ),
        store,
        identity,
      )
    }
  }
}

fn read_upsert_payload_input(
  args: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case read_object(args, "metaobject") {
    Some(input) -> input
    None ->
      case dict.get(args, "values") {
        Ok(root_field.ListVal(values)) ->
          dict.insert(dict.new(), "fields", root_field.ListVal(values))
        _ -> dict.new()
      }
  }
}

fn metaobject_upsert_exact_match(
  existing: MetaobjectRecord,
  updated: MetaobjectRecord,
) -> Bool {
  existing.handle == updated.handle
  && existing.fields == updated.fields
  && existing.capabilities == updated.capabilities
}

fn partition_upsert_user_errors(errors: List(UserError)) -> List(UserError) {
  list.map(errors, fn(error) {
    let UserError(field, message, code, element_key, element_index) = error
    UserError(
      partition_upsert_user_error_field(field),
      message,
      code,
      element_key,
      element_index,
    )
  })
}

fn partition_upsert_user_error_field(
  field: Option(List(String)),
) -> Option(List(String)) {
  case field {
    Some(["metaobject", "handle", ..rest]) ->
      Some(list.append(["handle", "handle"], rest))
    Some(["metaobject", "fields", ..rest]) ->
      Some(list.append(["fields"], rest))
    Some(["metaobject"]) -> Some([])
    _ -> field
  }
}

fn handle_metaobject_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let id = read_string(graphql_helpers.field_args(field, variables), "id")
  case id {
    None -> #(
      metaobject_delete_result(key, field, None, [
        record_not_found_user_error(["id"]),
      ]),
      store,
      identity,
    )
    Some(metaobject_id) -> {
      // Pattern 2: delete can target an upstream row; hydrate it first,
      // then stage a local delete and keep downstream reads local.
      let store =
        maybe_hydrate_metaobject_by_id(store, Some(metaobject_id), upstream)
      case get_effective_metaobject_by_id(store, metaobject_id) {
        None -> #(
          metaobject_delete_result(key, field, None, [
            record_not_found_user_error(["id"]),
          ]),
          store,
          identity,
        )
        Some(metaobject) -> {
          let staged_store = delete_staged_metaobject(store, metaobject_id)
          let #(next_store, next_identity) =
            adjust_definition_count(
              staged_store,
              identity,
              metaobject.type_,
              -1,
            )
          #(
            metaobject_delete_result(key, field, Some(metaobject_id), []),
            next_store,
            next_identity,
          )
        }
      }
    }
  }
}

fn handle_metaobject_bulk_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  // Pattern 2: type-scoped bulk delete needs the upstream selection set
  // before staging local deletes; downstream reads observe local markers.
  let store = maybe_hydrate_bulk_delete_selection(store, args, upstream)
  case read_bulk_delete_where(args) {
    BulkDeleteByType(type_) ->
      case find_effective_metaobject_definition_by_type(store, type_) {
        None -> {
          let err =
            UserError(
              Some(["where", "type"]),
              "No metaobject definition exists for type \"" <> type_ <> "\"",
              "RECORD_NOT_FOUND",
              None,
              None,
            )
          #(
            MutationFieldResult(
              key,
              bulk_delete_payload(field, fragments, None, [err]),
              [],
              [],
              [],
            ),
            store,
            identity,
          )
        }
        Some(_) -> {
          let ids = read_bulk_delete_ids(store, args)
          bulk_delete_selected_ids(store, identity, key, field, fragments, ids)
        }
      }
    BulkDeleteByIds(ids) ->
      bulk_delete_selected_ids(
        store,
        identity,
        key,
        field,
        fragments,
        list.take(ids, 250),
      )
    BulkDeleteNoSelector ->
      bulk_delete_selected_ids(store, identity, key, field, fragments, [])
  }
}

fn bulk_delete_selected_ids(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  field: Selection,
  fragments: FragmentMap,
  ids: List(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  case ids {
    [] -> {
      let #(job_id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "Job")
      let job = BulkDeleteJob(id: job_id, done: False)
      #(
        MutationFieldResult(
          key,
          bulk_delete_payload(field, fragments, Some(job), []),
          [job_id],
          [],
          [log_draft("metaobjectBulkDelete", [job_id])],
        ),
        store,
        next_identity,
      )
    }
    _ -> {
      let #(job_id, identity_after_job) =
        synthetic_identity.make_synthetic_gid(identity, "Job")
      let #(next_store, user_errors, deleted_ids, final_identity) =
        delete_metaobject_ids(store, identity_after_job, ids)
      let job = BulkDeleteJob(id: job_id, done: True)
      #(
        MutationFieldResult(
          key,
          bulk_delete_payload(field, fragments, Some(job), user_errors),
          list.append([job_id], deleted_ids),
          [],
          [
            log_draft(
              "metaobjectBulkDelete",
              list.append([job_id], deleted_ids),
            ),
          ],
        ),
        next_store,
        final_identity,
      )
    }
  }
}

fn delete_metaobject_ids(
  store: Store,
  identity: SyntheticIdentityRegistry,
  ids: List(String),
) -> #(Store, List(UserError), List(String), SyntheticIdentityRegistry) {
  list.fold(ids, #(store, [], [], identity), fn(acc, id) {
    let #(current_store, errors, deleted_ids, current_identity) = acc
    case get_effective_metaobject_by_id(current_store, id) {
      None -> {
        let err =
          UserError(
            Some(["where", "ids"]),
            "Record not found",
            "RECORD_NOT_FOUND",
            Some(id),
            None,
          )
        #(
          current_store,
          list.append(errors, [err]),
          deleted_ids,
          current_identity,
        )
      }
      Some(metaobject) -> {
        let staged_store = delete_staged_metaobject(current_store, id)
        let #(next_store, next_identity) =
          adjust_definition_count(
            staged_store,
            current_identity,
            metaobject.type_,
            -1,
          )
        #(next_store, errors, list.append(deleted_ids, [id]), next_identity)
      }
    }
  })
}

// ---------------------------------------------------------------------------
// LiveHybrid hydration
// ---------------------------------------------------------------------------

fn maybe_hydrate_definition_by_type(
  store: Store,
  type_: Option(String),
  upstream: UpstreamContext,
  requesting_api_client_id: Option(String),
) -> Store {
  case type_ {
    None -> store
    Some(metaobject_type) -> {
      let normalized_type =
        normalize_definition_type(metaobject_type, requesting_api_client_id)
      case
        find_effective_metaobject_definition_by_normalized_type(
          store,
          normalized_type,
        )
      {
        Some(_) -> store
        None ->
          fetch_and_hydrate(
            store,
            upstream,
            "MetaobjectDefinitionHydrateByType",
            metaobject_definition_hydrate_query(),
            json.object([#("type", json.string(normalized_type))]),
          )
      }
    }
  }
}

fn maybe_hydrate_definition_for_create(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
  requesting_api_client_id: Option(String),
) -> Store {
  case read_string(input, "type") {
    None -> store
    Some(raw_type) -> {
      let normalized_type =
        normalize_definition_type(raw_type, requesting_api_client_id)
      case
        raw_type == normalized_type
        || is_app_reserved_definition_type_input(raw_type)
      {
        True ->
          fetch_and_hydrate(
            store,
            upstream,
            "MetaobjectDefinitionHydrateByType",
            metaobject_definition_hydrate_query(),
            json.object([#("type", json.string(normalized_type))]),
          )
        False -> store
      }
    }
  }
}

fn maybe_hydrate_metaobject_by_id(
  store: Store,
  id: Option(String),
  upstream: UpstreamContext,
) -> Store {
  case id {
    None -> store
    Some(metaobject_id) ->
      case get_effective_metaobject_by_id(store, metaobject_id) {
        Some(_) -> store
        None ->
          fetch_and_hydrate(
            store,
            upstream,
            "MetaobjectHydrateById",
            metaobject_hydrate_query(),
            json.object([#("id", json.string(metaobject_id))]),
          )
      }
  }
}

fn maybe_hydrate_metaobject_by_handle(
  store: Store,
  type_: String,
  handle: String,
  upstream: UpstreamContext,
) -> Store {
  case find_effective_metaobject_by_handle(store, type_, handle) {
    Some(_) -> store
    None ->
      fetch_and_hydrate(
        store,
        upstream,
        "MetaobjectHydrateByHandle",
        metaobject_hydrate_by_handle_query(),
        json.object([
          #("type", json.string(type_)),
          #("handle", json.string(handle)),
        ]),
      )
  }
}

fn maybe_hydrate_bulk_delete_selection(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  let explicit_ids = case read_object(args, "where") {
    Some(where) -> read_string_list(where, "ids") |> list.take(250)
    None -> []
  }
  case explicit_ids {
    [_, ..] ->
      list.fold(explicit_ids, store, fn(current, id) {
        maybe_hydrate_metaobject_by_id(current, Some(id), upstream)
      })
    [] ->
      case read_object(args, "where") {
        Some(where) ->
          case read_string(where, "type") {
            Some(type_) ->
              case list_effective_metaobject_definitions(store) {
                [] ->
                  fetch_and_hydrate(
                    store,
                    upstream,
                    "MetaobjectBulkDeleteHydrateByType",
                    metaobject_bulk_delete_hydrate_query(),
                    json.object([#("type", json.string(type_))]),
                  )
                _ -> store
              }
            None -> store
          }
        None -> store
      }
  }
}

fn fetch_and_hydrate(
  store: Store,
  upstream: UpstreamContext,
  operation_name: String,
  query: String,
  variables: Json,
) -> Store {
  case upstream.transport, upstream.origin {
    None, "" -> store
    _, _ ->
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          operation_name,
          query,
          variables,
        )
      {
        Ok(value) -> hydrate_from_upstream_response(store, value)
        Error(_) -> store
      }
  }
}

fn hydrate_from_upstream_response(
  store: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) -> hydrate_data_roots(store, data)
    None -> store
  }
}

fn hydrate_data_roots(store: Store, data: commit.JsonValue) -> Store {
  let store = hydrate_definitions_from_value(store, data)
  hydrate_metaobjects_from_value(store, data)
}

fn hydrate_definitions_from_value(
  store: Store,
  value: commit.JsonValue,
) -> Store {
  let definitions =
    collect_definition_nodes(value)
    |> list.filter_map(definition_from_json)
  case definitions {
    [] -> store
    _ -> upsert_base_metaobject_definitions(store, definitions)
  }
}

fn hydrate_metaobjects_from_value(
  store: Store,
  value: commit.JsonValue,
) -> Store {
  let metaobjects =
    collect_metaobject_nodes(value)
    |> list.filter_map(metaobject_from_json)
  case metaobjects {
    [] -> store
    _ -> upsert_base_metaobjects(store, metaobjects)
  }
}

fn collect_definition_nodes(value: commit.JsonValue) -> List(commit.JsonValue) {
  let current = case json_get_string(value, "id") {
    Some(id) ->
      case string.starts_with(id, "gid://shopify/MetaobjectDefinition/") {
        True -> [value]
        False -> []
      }
    None -> []
  }
  case value {
    commit.JsonObject(fields) ->
      list.fold(fields, current, fn(acc, pair) {
        let #(_, child) = pair
        list.append(acc, collect_definition_nodes(child))
      })
    commit.JsonArray(items) ->
      list.fold(items, current, fn(acc, child) {
        list.append(acc, collect_definition_nodes(child))
      })
    _ -> current
  }
}

fn collect_metaobject_nodes(value: commit.JsonValue) -> List(commit.JsonValue) {
  let current = case json_get_string(value, "id") {
    Some(id) ->
      case string.starts_with(id, "gid://shopify/Metaobject/") {
        True -> [value]
        False -> []
      }
    None -> []
  }
  case value {
    commit.JsonObject(fields) ->
      list.fold(fields, current, fn(acc, pair) {
        let #(_, child) = pair
        list.append(acc, collect_metaobject_nodes(child))
      })
    commit.JsonArray(items) ->
      list.fold(items, current, fn(acc, child) {
        list.append(acc, collect_metaobject_nodes(child))
      })
    _ -> current
  }
}

fn definition_from_json(
  value: commit.JsonValue,
) -> Result(MetaobjectDefinitionRecord, Nil) {
  use id <- result.try(option_to_result(json_get_string(value, "id")))
  use type_ <- result.try(option_to_result(json_get_string(value, "type")))
  Ok(MetaobjectDefinitionRecord(
    id: id,
    type_: type_,
    name: json_get_string(value, "name"),
    description: json_get_nullable_string(value, "description"),
    access: definition_access_from_json(json_get(value, "access")),
    capabilities: definition_capabilities_from_json(json_get(
      value,
      "capabilities",
    )),
    field_definitions: json_array(json_get(value, "fieldDefinitions"))
      |> list.filter_map(field_definition_from_json),
    display_name_key: json_get_nullable_string(value, "displayNameKey"),
    has_thumbnail_field: json_get_bool(value, "hasThumbnailField"),
    metaobjects_count: json_get_int(value, "metaobjectsCount"),
    standard_template: standard_template_from_json(json_get(
      value,
      "standardTemplate",
    )),
    created_at: json_get_string(value, "createdAt"),
    updated_at: json_get_string(value, "updatedAt"),
  ))
}

fn field_definition_from_json(
  value: commit.JsonValue,
) -> Result(MetaobjectFieldDefinitionRecord, Nil) {
  use key <- result.try(option_to_result(json_get_string(value, "key")))
  let type_record = case json_get(value, "type") {
    Some(type_value) -> type_record_from_json(type_value)
    None ->
      MetaobjectDefinitionTypeRecord(
        name: "single_line_text_field",
        category: None,
      )
  }
  Ok(MetaobjectFieldDefinitionRecord(
    key: key,
    name: json_get_string(value, "name"),
    description: json_get_nullable_string(value, "description"),
    required: json_get_bool(value, "required"),
    type_: type_record,
    validations: json_array(json_get(value, "validations"))
      |> list.filter_map(validation_from_json),
  ))
}

fn validation_from_json(
  value: commit.JsonValue,
) -> Result(MetaobjectFieldDefinitionValidationRecord, Nil) {
  use name <- result.try(option_to_result(json_get_string(value, "name")))
  Ok(MetaobjectFieldDefinitionValidationRecord(
    name: name,
    value: json_get_nullable_string(value, "value"),
  ))
}

fn type_record_from_json(
  value: commit.JsonValue,
) -> MetaobjectDefinitionTypeRecord {
  MetaobjectDefinitionTypeRecord(
    name: option.unwrap(
      json_get_string(value, "name"),
      "single_line_text_field",
    ),
    category: json_get_nullable_string(value, "category"),
  )
}

fn definition_access_from_json(
  value: Option(commit.JsonValue),
) -> Dict(String, Option(String)) {
  case value {
    Some(commit.JsonObject(fields)) ->
      list.fold(fields, dict.new(), fn(acc, pair) {
        let #(key, child) = pair
        dict.insert(acc, key, json_scalar_nullable_string(child))
      })
    _ -> default_definition_access()
  }
}

fn definition_capabilities_from_json(
  value: Option(commit.JsonValue),
) -> MetaobjectDefinitionCapabilitiesRecord {
  MetaobjectDefinitionCapabilitiesRecord(
    publishable: definition_capability_from_json(value, "publishable"),
    translatable: definition_capability_from_json(value, "translatable"),
    renderable: definition_capability_from_json(value, "renderable"),
    online_store: definition_capability_from_json(value, "onlineStore"),
  )
}

fn definition_capability_from_json(
  value: Option(commit.JsonValue),
  key: String,
) -> Option(MetaobjectDefinitionCapabilityRecord) {
  case value {
    Some(object) ->
      case json_get(object, key) {
        Some(capability) ->
          case json_get_bool(capability, "enabled") {
            Some(enabled) ->
              Some(MetaobjectDefinitionCapabilityRecord(enabled: enabled))
            None -> None
          }
        None -> None
      }
    None -> None
  }
}

fn standard_template_from_json(
  value: Option(commit.JsonValue),
) -> Option(MetaobjectStandardTemplateRecord) {
  case value {
    Some(commit.JsonObject(_)) ->
      Some(MetaobjectStandardTemplateRecord(
        type_: json_get_string(option.unwrap(value, commit.JsonNull), "type"),
        name: json_get_string(option.unwrap(value, commit.JsonNull), "name"),
      ))
    _ -> None
  }
}

fn metaobject_from_json(
  value: commit.JsonValue,
) -> Result(MetaobjectRecord, Nil) {
  use id <- result.try(option_to_result(json_get_string(value, "id")))
  use type_ <- result.try(option_to_result(json_get_string(value, "type")))
  use handle <- result.try(option_to_result(json_get_string(value, "handle")))
  Ok(MetaobjectRecord(
    id: id,
    handle: handle,
    type_: type_,
    display_name: json_get_nullable_string(value, "displayName"),
    fields: json_array(json_get(value, "fields"))
      |> list.filter_map(metaobject_field_from_json),
    capabilities: metaobject_capabilities_from_json(json_get(
      value,
      "capabilities",
    )),
    created_at: json_get_string(value, "createdAt"),
    updated_at: json_get_string(value, "updatedAt"),
  ))
}

fn metaobject_field_from_json(
  value: commit.JsonValue,
) -> Result(MetaobjectFieldRecord, Nil) {
  use key <- result.try(option_to_result(json_get_string(value, "key")))
  let type_ = json_get_string(value, "type")
  let raw_value = json_get_nullable_string(value, "value")
  let json_value = case json_get(value, "jsonValue") {
    Some(value) -> metaobject_json_value_from_json(value)
    None ->
      case type_ {
        Some(type_name) -> read_metaobject_json_value(type_name, raw_value)
        None -> MetaobjectNull
      }
  }
  Ok(
    MetaobjectFieldRecord(
      key: key,
      type_: type_,
      value: raw_value,
      json_value: json_value,
      definition: case json_get(value, "definition") {
        Some(definition) -> field_definition_reference_from_json(definition)
        None -> None
      },
    ),
  )
}

fn field_definition_reference_from_json(
  value: commit.JsonValue,
) -> Option(MetaobjectFieldDefinitionReferenceRecord) {
  case json_get_string(value, "key") {
    Some(key) ->
      Some(
        MetaobjectFieldDefinitionReferenceRecord(
          key: key,
          name: json_get_string(value, "name"),
          required: json_get_bool(value, "required"),
          type_: case json_get(value, "type") {
            Some(type_value) -> type_record_from_json(type_value)
            None ->
              MetaobjectDefinitionTypeRecord("single_line_text_field", None)
          },
        ),
      )
    None -> None
  }
}

fn metaobject_capabilities_from_json(
  value: Option(commit.JsonValue),
) -> MetaobjectCapabilitiesRecord {
  MetaobjectCapabilitiesRecord(
    publishable: metaobject_publishable_from_json(value),
    online_store: metaobject_online_store_from_json(value),
  )
}

fn metaobject_publishable_from_json(
  value: Option(commit.JsonValue),
) -> Option(MetaobjectPublishableCapabilityRecord) {
  case value {
    Some(object) ->
      case json_get(object, "publishable") {
        Some(publishable) ->
          Some(
            MetaobjectPublishableCapabilityRecord(status: json_get_string(
              publishable,
              "status",
            )),
          )
        None -> None
      }
    None -> None
  }
}

fn metaobject_online_store_from_json(
  value: Option(commit.JsonValue),
) -> Option(MetaobjectOnlineStoreCapabilityRecord) {
  case value {
    Some(object) ->
      case json_get(object, "onlineStore") {
        Some(commit.JsonNull) -> None
        Some(online_store) ->
          Some(
            MetaobjectOnlineStoreCapabilityRecord(
              template_suffix: json_get_nullable_string(
                online_store,
                "templateSuffix",
              ),
            ),
          )
        None -> None
      }
    None -> None
  }
}

fn metaobject_json_value_from_json(
  value: commit.JsonValue,
) -> MetaobjectJsonValue {
  case value {
    commit.JsonNull -> MetaobjectNull
    commit.JsonBool(value) -> MetaobjectBool(value)
    commit.JsonInt(value) -> MetaobjectInt(value)
    commit.JsonFloat(value) -> whole_float_to_metaobject_number(value)
    commit.JsonString(value) -> MetaobjectString(value)
    commit.JsonArray(items) ->
      MetaobjectList(list.map(items, metaobject_json_value_from_json))
    commit.JsonObject(fields) ->
      MetaobjectObject(
        list.fold(fields, dict.new(), fn(acc, pair) {
          let #(key, child) = pair
          dict.insert(acc, key, metaobject_json_value_from_json(child))
        }),
      )
  }
}

fn metaobject_definition_hydrate_query() -> String {
  "query MetaobjectDefinitionHydrateByType($type: String!) { metaobjectDefinitionByType(type: $type) { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } }"
}

fn metaobject_hydrate_query() -> String {
  "query MetaobjectHydrateById($id: ID!) { metaobject(id: $id) { id handle type displayName createdAt updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } definition { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } } }"
}

fn metaobject_hydrate_by_handle_query() -> String {
  "query MetaobjectHydrateByHandle($type: String!, $handle: String!) { metaobjectByHandle(handle: { type: $type, handle: $handle }) { id handle type displayName createdAt updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } definition { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } } }"
}

fn metaobject_bulk_delete_hydrate_query() -> String {
  "query MetaobjectBulkDeleteHydrateByType($type: String!) { catalog: metaobjects(type: $type, first: 250) { nodes { id handle type displayName createdAt updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } } } definition: metaobjectDefinitionByType(type: $type) { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } }"
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      fields
      |> list.find(fn(pair) {
        let #(field_key, _) = pair
        field_key == key
      })
      |> result.map(fn(pair) {
        let #(_, child) = pair
        child
      })
      |> option.from_result
    _ -> None
  }
}

fn json_array(value: Option(commit.JsonValue)) -> List(commit.JsonValue) {
  case value {
    Some(commit.JsonArray(items)) -> items
    _ -> []
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(child) -> json_scalar_string(child)
    None -> None
  }
}

fn json_get_nullable_string(
  value: commit.JsonValue,
  key: String,
) -> Option(String) {
  case json_get(value, key) {
    Some(child) -> json_scalar_nullable_string(child)
    None -> None
  }
}

fn json_scalar_nullable_string(value: commit.JsonValue) -> Option(String) {
  case value {
    commit.JsonNull -> None
    _ -> json_scalar_string(value)
  }
}

fn json_scalar_string(value: commit.JsonValue) -> Option(String) {
  case value {
    commit.JsonString(value) -> Some(value)
    commit.JsonInt(value) -> Some(int.to_string(value))
    commit.JsonFloat(value) -> Some(float.to_string(value))
    commit.JsonBool(True) -> Some("true")
    commit.JsonBool(False) -> Some("false")
    _ -> None
  }
}

fn json_get_bool(value: commit.JsonValue, key: String) -> Option(Bool) {
  case json_get(value, key) {
    Some(commit.JsonBool(value)) -> Some(value)
    _ -> None
  }
}

fn json_get_int(value: commit.JsonValue, key: String) -> Option(Int) {
  case json_get(value, key) {
    Some(commit.JsonInt(value)) -> Some(value)
    _ -> None
  }
}

fn option_to_result(value: Option(a)) -> Result(a, Nil) {
  case value {
    Some(value) -> Ok(value)
    None -> Error(Nil)
  }
}

// ---------------------------------------------------------------------------
// Definition construction/update
// ---------------------------------------------------------------------------

fn default_definition_access() -> Dict(String, Option(String)) {
  dict.from_list([
    #("admin", Some("PUBLIC_READ_WRITE")),
    #("storefront", Some("NONE")),
  ])
}

fn default_definition_capabilities() -> MetaobjectDefinitionCapabilitiesRecord {
  MetaobjectDefinitionCapabilitiesRecord(
    publishable: Some(MetaobjectDefinitionCapabilityRecord(False)),
    translatable: Some(MetaobjectDefinitionCapabilityRecord(False)),
    renderable: Some(MetaobjectDefinitionCapabilityRecord(False)),
    online_store: Some(MetaobjectDefinitionCapabilityRecord(False)),
  )
}

fn build_create_definition_validation_errors(
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> List(UserError) {
  let type_ = read_string(input, "type")
  let name = read_string(input, "name")
  let access = read_object(input, "access")
  []
  |> append_if(
    is_missing_definition_type(type_),
    UserError(
      Some(["definition", "type"]),
      "Type can't be blank",
      "BLANK",
      None,
      None,
    ),
  )
  |> append_if(
    option.is_none(name),
    UserError(
      Some(["definition", "name"]),
      "Name can't be blank",
      "BLANK",
      None,
      None,
    ),
  )
  |> append_if(
    case type_, access {
      Some(t), Some(a) ->
        !is_app_reserved_definition_type_input(t) && dict.has_key(a, "admin")
      _, _ -> False
    },
    UserError(
      Some(["definition", "access", "admin"]),
      "Admin access can only be specified on metaobject definitions that have an app-reserved type.",
      "ADMIN_ACCESS_INPUT_NOT_ALLOWED",
      None,
      None,
    ),
  )
  |> append_definition_type_validation_errors(type_, requesting_api_client_id)
  |> append_create_field_definition_key_errors(read_list(
    input,
    "fieldDefinitions",
  ))
}

fn build_create_definition_uniqueness_errors(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> List(UserError) {
  []
  |> append_if(
    case
      normalized_definition_type_from_input(input, requesting_api_client_id)
    {
      Some(t) ->
        case find_effective_metaobject_definition_by_normalized_type(store, t) {
          Some(_) -> True
          None -> False
        }
      None -> False
    },
    UserError(
      Some(["definition", "type"]),
      "Type has already been taken",
      "TAKEN",
      None,
      None,
    ),
  )
}

fn is_missing_definition_type(type_: Option(String)) -> Bool {
  case type_ {
    None -> True
    Some(value) -> string.trim(value) == ""
  }
}

fn append_definition_type_validation_errors(
  errors: List(UserError),
  type_: Option(String),
  requesting_api_client_id: Option(String),
) -> List(UserError) {
  case type_ {
    None -> errors
    Some(raw) ->
      case string.trim(raw) {
        "" -> errors
        _ -> {
          let type_ = normalize_definition_type(raw, requesting_api_client_id)
          let length = string.length(type_)
          errors
          |> append_if(
            length < 3,
            UserError(
              Some(["definition", "type"]),
              "Type is too short (minimum is 3 characters)",
              "TOO_SHORT",
              None,
              None,
            ),
          )
          |> append_if(
            length > 255,
            UserError(
              Some(["definition", "type"]),
              "Type is too long (maximum is 255 characters)",
              "TOO_LONG",
              None,
              None,
            ),
          )
          |> append_if(
            !is_valid_definition_type(type_),
            UserError(
              Some(["definition", "type"]),
              "Type contains one or more invalid characters. Only alphanumeric characters, underscores, and dashes are allowed.",
              "INVALID",
              None,
              None,
            ),
          )
        }
      }
  }
}

fn is_valid_definition_type(type_: String) -> Bool {
  type_ != ""
  && list.all(string.to_utf_codepoints(type_), fn(char) {
    is_definition_type_codepoint(string.utf_codepoint_to_int(char))
  })
}

fn is_definition_type_codepoint(codepoint: Int) -> Bool {
  is_ascii_lowercase_letter(codepoint)
  || is_ascii_uppercase_letter(codepoint)
  || is_ascii_digit(codepoint)
  || codepoint == 45
  || codepoint == 95
}

fn is_valid_field_key(key: String) -> Bool {
  key != ""
  && list.all(string.to_utf_codepoints(key), fn(char) {
    let codepoint = string.utf_codepoint_to_int(char)
    is_ascii_lowercase_letter(codepoint)
    || is_ascii_digit(codepoint)
    || codepoint == 95
  })
}

fn is_ascii_lowercase_letter(codepoint: Int) -> Bool {
  codepoint >= 97 && codepoint <= 122
}

fn is_ascii_uppercase_letter(codepoint: Int) -> Bool {
  codepoint >= 65 && codepoint <= 90
}

fn is_ascii_digit(codepoint: Int) -> Bool {
  codepoint >= 48 && codepoint <= 57
}

fn append_create_field_definition_key_errors(
  errors: List(UserError),
  values: List(root_field.ResolvedValue),
) -> List(UserError) {
  list.fold(enumerate_values(values), errors, fn(acc, pair) {
    let #(index, value) = pair
    case value {
      root_field.ObjectVal(input) ->
        append_field_key_validation_error(acc, read_string(input, "key"), index)
      _ -> acc
    }
  })
}

fn append_field_key_validation_error(
  errors: List(UserError),
  key: Option(String),
  index: Int,
) -> List(UserError) {
  case key {
    Some(k) ->
      append_if(
        errors,
        !is_valid_field_key(k),
        invalid_field_key_user_error(index, k),
      )
    None -> errors
  }
}

fn invalid_field_key_user_error(index: Int, key: String) -> UserError {
  UserError(
    Some(["definition", "fieldDefinitions", int.to_string(index), "key"]),
    "is invalid",
    "INVALID",
    Some(key),
    Some(index),
  )
}

fn build_definition_from_create_input(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(MetaobjectDefinitionRecord, SyntheticIdentityRegistry) {
  let #(id, after_id) =
    synthetic_identity.make_proxy_synthetic_gid(
      identity,
      "MetaobjectDefinition",
    )
  let #(now, after_time) = synthetic_identity.make_synthetic_timestamp(after_id)
  let type_ =
    normalized_definition_type_from_input(input, requesting_api_client_id)
    |> option.unwrap("metaobject_definition")
  #(
    MetaobjectDefinitionRecord(
      id: id,
      type_: type_,
      name: read_string(input, "name"),
      description: read_string(input, "description"),
      display_name_key: read_string(input, "displayNameKey"),
      access: build_definition_access(
        read_object(input, "access"),
        default_definition_access(),
      ),
      capabilities: normalize_definition_capabilities(
        read_object(input, "capabilities"),
        default_definition_capabilities(),
      ),
      field_definitions: read_field_definitions(read_list(
        input,
        "fieldDefinitions",
      )),
      has_thumbnail_field: Some(False),
      metaobjects_count: Some(0),
      standard_template: None,
      created_at: Some(now),
      updated_at: Some(now),
    ),
    after_time,
  )
}

fn apply_definition_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  existing: MetaobjectDefinitionRecord,
  input: Dict(String, root_field.ResolvedValue),
  reset_field_order: Bool,
  requesting_api_client_id: Option(String),
) -> #(MetaobjectDefinitionRecord, SyntheticIdentityRegistry, List(UserError)) {
  let #(now, next_identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(fields, user_errors, ordered_keys) =
    apply_field_definition_operations(
      existing.field_definitions,
      read_list(input, "fieldDefinitions"),
    )
  let next_fields = case reset_field_order {
    True -> reorder_field_definitions(fields, ordered_keys)
    False -> fields
  }
  let type_ =
    normalized_definition_type_from_input(input, requesting_api_client_id)
    |> option.unwrap(existing.type_)
  let type_errors =
    build_update_definition_type_user_errors(
      store,
      existing.id,
      input,
      requesting_api_client_id,
    )
  let access_errors = build_update_definition_access_user_errors(input, type_)
  let updated =
    MetaobjectDefinitionRecord(
      ..existing,
      type_: type_,
      name: read_string_if_present(input, "name", existing.name),
      description: read_string_if_present(
        input,
        "description",
        existing.description,
      ),
      display_name_key: read_string_if_present(
        input,
        "displayNameKey",
        existing.display_name_key,
      ),
      access: case read_object(input, "access") {
        Some(access) -> build_definition_access(Some(access), existing.access)
        None -> existing.access
      },
      capabilities: case read_object(input, "capabilities") {
        Some(capabilities) ->
          normalize_definition_capabilities(
            Some(capabilities),
            existing.capabilities,
          )
        None -> existing.capabilities
      },
      field_definitions: next_fields,
      updated_at: Some(now),
    )
  #(
    updated,
    next_identity,
    list.flatten([
      type_errors,
      access_errors,
      user_errors,
    ]),
  )
}

fn build_update_definition_type_user_errors(
  store: Store,
  existing_id: String,
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> List(UserError) {
  let type_ = read_string(input, "type")
  let validation_errors =
    []
    |> append_definition_type_validation_errors(type_, requesting_api_client_id)
  case
    validation_errors,
    normalized_definition_type_from_input(input, requesting_api_client_id)
  {
    [], Some(normalized_type) ->
      []
      |> append_if(
        case
          find_effective_metaobject_definition_by_normalized_type(
            store,
            normalized_type,
          )
        {
          Some(definition) -> definition.id != existing_id
          None -> False
        },
        UserError(
          Some(["definition", "type"]),
          "Type has already been taken",
          "TAKEN",
          None,
          None,
        ),
      )
    [_, ..], _ -> validation_errors
    [], None -> []
  }
}

fn build_update_definition_access_user_errors(
  input: Dict(String, root_field.ResolvedValue),
  next_type: String,
) -> List(UserError) {
  []
  |> append_if(
    case read_object(input, "access") {
      Some(access) ->
        dict.has_key(access, "admin")
        && !{
          case read_string(input, "type") {
            Some(raw_type) -> is_app_reserved_definition_type_input(raw_type)
            None -> is_app_reserved_resolved_definition_type(next_type)
          }
        }
      None -> False
    },
    UserError(
      Some(["definition", "access", "admin"]),
      "Admin access can only be specified on metaobject definitions that have an app-reserved type.",
      "ADMIN_ACCESS_INPUT_NOT_ALLOWED",
      None,
      None,
    ),
  )
}

fn is_app_reserved_resolved_definition_type(type_: String) -> Bool {
  case string.split(type_, "--") {
    ["app", api_client_id, rest, ..] -> api_client_id != "" && rest != ""
    _ -> False
  }
}

fn standard_template(
  type_: String,
) -> Option(standard_templates.StandardMetaobjectTemplate) {
  case
    standard_templates.templates()
    |> list.find(fn(template) { template.type_ == type_ })
  {
    Ok(template) -> Some(template)
    Error(_) -> None
  }
}

fn build_standard_definition(
  identity: SyntheticIdentityRegistry,
  template: standard_templates.StandardMetaobjectTemplate,
) -> #(MetaobjectDefinitionRecord, SyntheticIdentityRegistry) {
  let #(id, after_id) =
    synthetic_identity.make_proxy_synthetic_gid(
      identity,
      "MetaobjectDefinition",
    )
  let #(now, after_time) = synthetic_identity.make_synthetic_timestamp(after_id)
  #(
    MetaobjectDefinitionRecord(
      id: id,
      type_: template.type_,
      name: Some(template.name),
      description: template.description,
      display_name_key: Some(template.display_name_key),
      access: template.access,
      capabilities: template.capabilities,
      field_definitions: template.field_definitions,
      has_thumbnail_field: template.has_thumbnail_field,
      metaobjects_count: Some(0),
      standard_template: Some(MetaobjectStandardTemplateRecord(
        Some(template.type_),
        Some(template.name),
      )),
      created_at: Some(now),
      updated_at: Some(now),
    ),
    after_time,
  )
}

fn read_field_definitions(
  values: List(root_field.ResolvedValue),
) -> List(MetaobjectFieldDefinitionRecord) {
  list.filter_map(values, fn(value) {
    case value {
      root_field.ObjectVal(obj) ->
        case read_field_definition_input(obj) {
          Some(field) -> Ok(field)
          None -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
}

fn read_field_definition_input(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(MetaobjectFieldDefinitionRecord) {
  case read_string(input, "key"), read_type_name(input) {
    Some(key), Some(type_name) ->
      Some(MetaobjectFieldDefinitionRecord(
        key: key,
        name: read_string(input, "name"),
        description: read_string(input, "description"),
        required: Some(read_bool(input, "required") |> option.unwrap(False)),
        type_: MetaobjectDefinitionTypeRecord(
          type_name,
          infer_field_type_category(type_name),
        ),
        validations: read_validation_inputs(read_list(input, "validations")),
      ))
    _, _ -> None
  }
}

fn apply_field_definition_operations(
  existing: List(MetaobjectFieldDefinitionRecord),
  operations: List(root_field.ResolvedValue),
) -> #(List(MetaobjectFieldDefinitionRecord), List(UserError), List(String)) {
  list.fold(enumerate_values(operations), #(existing, [], []), fn(acc, pair) {
    let #(fields, errors, ordered_keys) = acc
    let #(index, value) = pair
    case read_field_operation(value) {
      None -> #(fields, errors, ordered_keys)
      Some(operation) -> {
        let key = field_operation_key(operation)
        case key {
          None -> #(
            fields,
            list.append(errors, [
              UserError(
                Some([
                  "definition",
                  "fieldDefinitions",
                  int.to_string(index),
                  "key",
                ]),
                "Key can't be blank",
                "BLANK",
                None,
                Some(index),
              ),
            ]),
            ordered_keys,
          )
          Some(k) ->
            case is_valid_field_key(k) {
              False -> #(
                fields,
                list.append(errors, [invalid_field_key_user_error(index, k)]),
                ordered_keys,
              )
              True ->
                apply_field_operation(
                  fields,
                  errors,
                  list.append(ordered_keys, [k]),
                  operation,
                  k,
                  index,
                )
            }
        }
      }
    }
  })
}

fn apply_field_operation(
  fields: List(MetaobjectFieldDefinitionRecord),
  errors: List(UserError),
  ordered_keys: List(String),
  operation: FieldOperation,
  key: String,
  index: Int,
) -> #(List(MetaobjectFieldDefinitionRecord), List(UserError), List(String)) {
  let existing = find_field_definition(fields, key)
  case operation {
    FieldDelete(_) ->
      case existing {
        None -> #(
          fields,
          list.append(errors, [
            UserError(
              Some([
                "definition",
                "fieldDefinitions",
                int.to_string(index),
                "delete",
              ]),
              "Field definition not found.",
              "NOT_FOUND",
              Some(key),
              Some(index),
            ),
          ]),
          ordered_keys,
        )
        Some(_) -> #(
          list.filter(fields, fn(field) { field.key != key }),
          errors,
          ordered_keys,
        )
      }
    FieldCreate(input) ->
      case existing {
        Some(_) -> #(
          fields,
          list.append(errors, [
            UserError(
              Some([
                "definition",
                "fieldDefinitions",
                int.to_string(index),
                "create",
              ]),
              "Field definition already exists.",
              "TAKEN",
              Some(key),
              Some(index),
            ),
          ]),
          ordered_keys,
        )
        None ->
          case read_field_definition_input(input) {
            Some(field) -> #(list.append(fields, [field]), errors, ordered_keys)
            None -> #(fields, errors, ordered_keys)
          }
      }
    FieldUpdate(input) ->
      case existing {
        None -> #(
          fields,
          list.append(errors, [
            UserError(
              Some([
                "definition",
                "fieldDefinitions",
                int.to_string(index),
                "update",
              ]),
              "Field definition not found.",
              "NOT_FOUND",
              Some(key),
              Some(index),
            ),
          ]),
          ordered_keys,
        )
        Some(field) -> #(
          replace_field_definition(fields, merge_field_definition(field, input)),
          errors,
          ordered_keys,
        )
      }
    FieldUpsert(input) ->
      case existing {
        Some(field) -> #(
          replace_field_definition(fields, merge_field_definition(field, input)),
          errors,
          ordered_keys,
        )
        None ->
          case read_field_definition_input(input) {
            Some(field) -> #(list.append(fields, [field]), errors, ordered_keys)
            None -> #(fields, errors, ordered_keys)
          }
      }
  }
}

fn read_field_operation(
  value: root_field.ResolvedValue,
) -> Option(FieldOperation) {
  case value {
    root_field.ObjectVal(obj) ->
      case read_object(obj, "create") {
        Some(payload) -> Some(FieldCreate(payload))
        None ->
          case read_object(obj, "update") {
            Some(payload) -> Some(FieldUpdate(payload))
            None ->
              case dict.get(obj, "delete") {
                Ok(root_field.StringVal(key)) -> Some(FieldDelete(key))
                Ok(root_field.ObjectVal(payload)) ->
                  Some(FieldDelete(
                    read_string(payload, "key") |> option.unwrap(""),
                  ))
                _ -> Some(FieldUpsert(obj))
              }
          }
      }
    _ -> None
  }
}

fn field_operation_key(operation: FieldOperation) -> Option(String) {
  case operation {
    FieldDelete(key) ->
      case key {
        "" -> None
        _ -> Some(key)
      }
    FieldCreate(input) | FieldUpdate(input) | FieldUpsert(input) ->
      read_string(input, "key")
  }
}

fn merge_field_definition(
  existing: MetaobjectFieldDefinitionRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> MetaobjectFieldDefinitionRecord {
  let type_name = read_type_name(input)
  MetaobjectFieldDefinitionRecord(
    key: read_string(input, "key") |> option.unwrap(existing.key),
    name: read_string_if_present(input, "name", existing.name),
    description: read_string_if_present(
      input,
      "description",
      existing.description,
    ),
    required: case dict.get(input, "required") {
      Ok(root_field.BoolVal(value)) -> Some(value)
      Ok(root_field.NullVal) -> None
      _ -> existing.required
    },
    type_: case type_name {
      Some(name) ->
        MetaobjectDefinitionTypeRecord(
          name,
          infer_field_type_category(name) |> option.or(existing.type_.category),
        )
      None -> existing.type_
    },
    validations: case dict.get(input, "validations") {
      Ok(root_field.ListVal(values)) -> read_validation_inputs(values)
      _ -> existing.validations
    },
  )
}

fn reorder_field_definitions(
  fields: List(MetaobjectFieldDefinitionRecord),
  ordered_keys: List(String),
) -> List(MetaobjectFieldDefinitionRecord) {
  let ordered =
    list.filter_map(ordered_keys |> dedupe_strings(), fn(key) {
      case find_field_definition(fields, key) {
        Some(field) -> Ok(field)
        None -> Error(Nil)
      }
    })
  let ordered_set = list_to_set(ordered_keys)
  list.append(
    ordered,
    list.filter(fields, fn(field) { !dict.has_key(ordered_set, field.key) }),
  )
}

// ---------------------------------------------------------------------------
// Metaobject construction/update
// ---------------------------------------------------------------------------

fn build_create_metaobject_user_errors(
  type_: Option(String),
  definition: Option(MetaobjectDefinitionRecord),
) -> List(UserError) {
  []
  |> append_if(
    option.is_none(type_),
    UserError(
      Some(["metaobject", "type"]),
      "Type can't be blank",
      "BLANK",
      None,
      None,
    ),
  )
  |> append_if(
    case type_, definition {
      Some(_), None -> True
      _, _ -> False
    },
    UserError(
      Some(["metaobject", "type"]),
      "No metaobject definition exists for type \""
        <> option.unwrap(type_, "")
        <> "\"",
      "UNDEFINED_OBJECT_TYPE",
      None,
      None,
    ),
  )
}

fn build_metaobject_from_create_input(
  store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
  definition: MetaobjectDefinitionRecord,
) -> #(Option(MetaobjectRecord), SyntheticIdentityRegistry, List(UserError)) {
  let #(fields, errors) =
    build_metaobject_fields_from_input(
      store,
      input,
      definition,
      [],
      True,
      True,
      True,
    )
  case errors {
    [_, ..] -> #(None, identity, errors)
    [] -> {
      let display_name = metaobject_display_name(definition, fields, None)
      let preferred =
        read_non_blank_string(input, "handle")
        |> option.or(display_name)
        |> option.unwrap(definition.type_)
      let handle =
        make_unique_metaobject_handle(store, definition.type_, preferred)
      let #(id, after_id) =
        synthetic_identity.make_proxy_synthetic_gid(identity, "Metaobject")
      let #(now, after_time) =
        synthetic_identity.make_synthetic_timestamp(after_id)
      #(
        Some(MetaobjectRecord(
          id: id,
          handle: handle,
          type_: definition.type_,
          display_name: display_name,
          fields: fields,
          capabilities: build_metaobject_capabilities(input, definition, None),
          created_at: Some(now),
          updated_at: Some(now),
        )),
        after_time,
        [],
      )
    }
  }
}

fn apply_metaobject_update_input(
  store: Store,
  identity: SyntheticIdentityRegistry,
  existing: MetaobjectRecord,
  input: Dict(String, root_field.ResolvedValue),
  definition: MetaobjectDefinitionRecord,
) -> #(Option(MetaobjectRecord), SyntheticIdentityRegistry, List(UserError)) {
  let requested_handle = case dict.get(input, "handle") {
    Ok(root_field.StringVal(value)) -> Some(value)
    Ok(root_field.NullVal) -> None
    _ -> Some(existing.handle)
  }
  case requested_handle {
    None | Some("") -> #(None, identity, [
      UserError(
        Some(["metaobject", "handle"]),
        "Handle can't be blank",
        "BLANK",
        None,
        None,
      ),
    ])
    Some(handle) -> {
      case find_effective_metaobject_by_handle(store, existing.type_, handle) {
        Some(owner) if owner.id != existing.id -> #(None, identity, [
          UserError(
            Some(["metaobject", "handle"]),
            "Handle has already been taken",
            "TAKEN",
            None,
            None,
          ),
        ])
        _ -> {
          let #(fields_from_input, errors) =
            build_metaobject_fields_from_input(
              store,
              input,
              definition,
              existing.fields,
              False,
              True,
              False,
            )
          case errors {
            [_, ..] -> #(None, identity, errors)
            [] -> {
              let fields = case dict.get(input, "fields") {
                Ok(_) -> fields_from_input
                Error(_) -> existing.fields
              }
              let #(now, next_identity) =
                synthetic_identity.make_synthetic_timestamp(identity)
              #(
                Some(
                  MetaobjectRecord(
                    ..existing,
                    handle: handle,
                    display_name: metaobject_display_name(
                      definition,
                      fields,
                      Some(handle),
                    ),
                    fields: fields,
                    capabilities: build_metaobject_capabilities(
                      input,
                      definition,
                      Some(existing.capabilities),
                    ),
                    updated_at: Some(now),
                  ),
                ),
                next_identity,
                [],
              )
            }
          }
        }
      }
    }
  }
}

fn build_metaobject_fields_from_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  definition: MetaobjectDefinitionRecord,
  existing_fields: List(MetaobjectFieldRecord),
  include_missing: Bool,
  require_required: Bool,
  allow_scalar_boolean_coercion: Bool,
) -> #(List(MetaobjectFieldRecord), List(UserError)) {
  let existing_by_key =
    list.fold(existing_fields, dict.new(), fn(acc, field) {
      dict.insert(acc, field.key, field)
    })
  let definitions_by_key =
    list.fold(definition.field_definitions, dict.new(), fn(acc, field) {
      dict.insert(acc, field.key, field)
    })
  let #(fields_by_key, errors, provided_keys) =
    list.fold(
      enumerate_values(read_list(input, "fields")),
      #(existing_by_key, [], []),
      fn(acc, pair) {
        let #(by_key, errs, provided) = acc
        let #(index, value) = pair
        case value {
          root_field.ObjectVal(raw_field) ->
            case read_string(raw_field, "key") {
              None -> #(
                by_key,
                list.append(errs, [
                  UserError(
                    Some(["metaobject", "fields", int.to_string(index), "key"]),
                    "Key can't be blank",
                    "BLANK",
                    None,
                    Some(index),
                  ),
                ]),
                provided,
              )
              Some(key) ->
                case dict.get(definitions_by_key, key) {
                  Error(_) -> #(
                    by_key,
                    list.append(errs, [
                      UserError(
                        Some(["metaobject", "fields", int.to_string(index)]),
                        "Field definition \"" <> key <> "\" does not exist",
                        "UNDEFINED_OBJECT_FIELD",
                        Some(key),
                        None,
                      ),
                    ]),
                    provided,
                  )
                  Ok(field_definition) -> {
                    let value_errors =
                      validate_metaobject_field_input_value(
                        store,
                        raw_field,
                        field_definition,
                        index,
                        allow_scalar_boolean_coercion,
                      )
                    case value_errors {
                      [_, ..] -> #(
                        by_key,
                        list.append(errs, value_errors),
                        list.append(provided, [key]),
                      )
                      [] -> #(
                        dict.insert(
                          by_key,
                          key,
                          build_metaobject_field_from_input(
                            raw_field,
                            field_definition,
                          ),
                        ),
                        errs,
                        list.append(provided, [key]),
                      )
                    }
                  }
                }
            }
          _ -> #(by_key, errs, provided)
        }
      },
    )
  let required_errors = case require_required {
    False -> []
    True ->
      list.filter_map(definition.field_definitions, fn(field_definition) {
        let has_field = dict.has_key(fields_by_key, field_definition.key)
        let provided = list.contains(provided_keys, field_definition.key)
        case
          field_definition.required == Some(True) && !has_field && !provided
        {
          True ->
            Ok(UserError(
              Some(["metaobject"]),
              option.unwrap(field_definition.name, field_definition.key)
                <> " can't be blank",
              "OBJECT_FIELD_REQUIRED",
              Some(field_definition.key),
              None,
            ))
          False -> Error(Nil)
        }
      })
  }
  let all_errors = list.append(errors, required_errors)
  let fields =
    list.filter_map(definition.field_definitions, fn(field_definition) {
      case dict.get(fields_by_key, field_definition.key) {
        Ok(field) -> Ok(field)
        Error(_) ->
          case include_missing {
            True -> Ok(empty_metaobject_field(field_definition))
            False -> Error(Nil)
          }
      }
    })
  #(fields, all_errors)
}

fn validate_metaobject_field_input_value(
  store: Store,
  raw_field: Dict(String, root_field.ResolvedValue),
  field_definition: MetaobjectFieldDefinitionRecord,
  index: Int,
  allow_scalar_boolean_coercion: Bool,
) -> List(UserError) {
  let value = read_string(raw_field, "value")
  let json_error = case value, field_definition.type_.name {
    Some(v), "json" ->
      case json.parse(v, decode.dynamic) {
        Ok(_) -> []
        Error(_) -> [
          UserError(
            Some(["metaobject", "fields", int.to_string(index)]),
            build_invalid_json_message(v),
            "INVALID_VALUE",
            Some(field_definition.key),
            None,
          ),
        ]
      }
    _, _ -> []
  }
  let coercion_errors =
    metafield_values.validate_metaobject_value(
      store,
      field_definition.type_.name,
      value,
      field_definition.validations,
      allow_scalar_boolean_coercion,
    )
    |> list.map(fn(error) {
      let metafield_values.ValidationError(message:, element_index:) = error
      UserError(
        Some(["metaobject", "fields", int.to_string(index)]),
        message,
        "INVALID_VALUE",
        Some(field_definition.key),
        element_index,
      )
    })
  list.append(json_error, coercion_errors)
}

fn build_metaobject_field_from_input(
  input: Dict(String, root_field.ResolvedValue),
  definition: MetaobjectFieldDefinitionRecord,
) -> MetaobjectFieldRecord {
  let value =
    normalize_metaobject_value(
      definition.type_.name,
      read_string(input, "value"),
    )
  MetaobjectFieldRecord(
    key: definition.key,
    type_: Some(definition.type_.name),
    value: value,
    json_value: read_metaobject_json_value(definition.type_.name, value),
    definition: Some(field_definition_reference(definition)),
  )
}

fn empty_metaobject_field(
  definition: MetaobjectFieldDefinitionRecord,
) -> MetaobjectFieldRecord {
  MetaobjectFieldRecord(
    key: definition.key,
    type_: Some(definition.type_.name),
    value: None,
    json_value: MetaobjectNull,
    definition: Some(field_definition_reference(definition)),
  )
}

fn project_metaobject_fields_through_definition(
  metaobject: MetaobjectRecord,
  definition: Option(MetaobjectDefinitionRecord),
) -> List(MetaobjectFieldRecord) {
  case definition {
    None -> metaobject.fields
    Some(defn) ->
      case defn.field_definitions {
        [] -> metaobject.fields
        definitions -> {
          let fields_by_key =
            list.fold(metaobject.fields, dict.new(), fn(acc, field) {
              dict.insert(acc, field.key, field)
            })
          list.map(definitions, fn(field_definition) {
            case dict.get(fields_by_key, field_definition.key) {
              Ok(field) ->
                MetaobjectFieldRecord(
                  ..field,
                  type_: Some(field_definition.type_.name),
                  json_value: read_metaobject_json_value(
                    field_definition.type_.name,
                    field.value,
                  ),
                  definition: Some(field_definition_reference(field_definition)),
                )
              Error(_) -> empty_metaobject_field(field_definition)
            }
          })
        }
      }
  }
}

fn project_metaobject_through_definition(
  store: Store,
  metaobject: MetaobjectRecord,
) -> MetaobjectRecord {
  let definition =
    find_effective_metaobject_definition_by_type(store, metaobject.type_)
  let fields =
    project_metaobject_fields_through_definition(metaobject, definition)
  let display_name = case definition {
    Some(defn) ->
      case list.is_empty(defn.field_definitions) {
        True -> metaobject.display_name
        False -> metaobject_display_name(defn, fields, Some(metaobject.handle))
      }
    _ -> metaobject.display_name
  }
  MetaobjectRecord(..metaobject, display_name: display_name, fields: fields)
}

fn metaobject_display_name(
  definition: MetaobjectDefinitionRecord,
  fields: List(MetaobjectFieldRecord),
  handle: Option(String),
) -> Option(String) {
  case definition.display_name_key {
    None -> None
    Some(key) ->
      case list.find(fields, fn(field) { field.key == key }) {
        Ok(field) ->
          case field.type_ {
            Some(type_) ->
              case
                is_display_measurement_metaobject_type(type_),
                field.json_value
              {
                True, MetaobjectNull -> field_value_or_handle(field, handle)
                True, json_value ->
                  Some(measurement_display_json_value_to_string(json_value))
                _, _ -> field_value_or_handle(field, handle)
              }
            None -> field_value_or_handle(field, handle)
          }
        Error(_) -> option.map(handle, metaobject_handle_display_name)
      }
  }
}

fn is_display_measurement_metaobject_type(type_: String) -> Bool {
  case string.starts_with(type_, "list.") {
    True -> is_measurement_metaobject_type(string.drop_start(type_, 5))
    False -> is_measurement_metaobject_type(type_)
  }
}

fn field_value_or_handle(
  field: MetaobjectFieldRecord,
  handle: Option(String),
) -> Option(String) {
  case field.value {
    Some(value) -> Some(value)
    None -> option.map(handle, metaobject_handle_display_name)
  }
}

fn measurement_display_json_value_to_string(
  value: MetaobjectJsonValue,
) -> String {
  case value {
    MetaobjectList(items) ->
      "["
      <> string.join(
        list.map(items, measurement_display_json_value_to_string),
        ",",
      )
      <> "]"
    MetaobjectObject(fields) -> {
      let normalized_fields = case dict.get(fields, "unit") {
        Ok(MetaobjectString(unit)) ->
          dict.insert(fields, "unit", MetaobjectString(string.lowercase(unit)))
        _ -> fields
      }
      measurement_object_to_compact_string(normalized_fields)
    }
    _ -> metaobject_json_value_to_compact_string(value)
  }
}

fn measurement_object_to_compact_string(
  fields: Dict(String, MetaobjectJsonValue),
) -> String {
  let value = case dict.get(fields, "value") {
    Ok(value) -> measurement_display_scalar_to_string(value)
    Error(_) -> "null"
  }
  let unit = case dict.get(fields, "unit") {
    Ok(MetaobjectString(unit)) -> json_string_literal(unit)
    Ok(value) -> metaobject_json_value_to_compact_string(value)
    Error(_) -> "null"
  }
  "{\"value\":" <> value <> ",\"unit\":" <> unit <> "}"
}

fn measurement_display_scalar_to_string(value: MetaobjectJsonValue) -> String {
  case value {
    MetaobjectFloat(float_value) -> {
      let rendered = float.to_string(float_value)
      case string.ends_with(rendered, ".0") {
        True -> string.drop_end(rendered, 2)
        False -> rendered
      }
    }
    _ -> metaobject_json_value_to_compact_string(value)
  }
}

fn json_string_literal(value: String) -> String {
  json.string(value) |> json.to_string
}

fn metaobject_handle_display_name(handle: String) -> String {
  handle
  |> string.replace("-", " ")
  |> string.replace("_", " ")
  |> string.split(" ")
  |> list.filter(fn(part) { part != "" })
  |> list.map(capitalise_handle_part)
  |> string.join(" ")
}

fn capitalise_handle_part(part: String) -> String {
  case string.pop_grapheme(part) {
    Ok(#(first, rest)) -> string.uppercase(first) <> rest
    Error(_) -> part
  }
}

fn make_unique_metaobject_handle(
  store: Store,
  type_: String,
  preferred: String,
) -> String {
  let base = normalize_metaobject_handle(preferred)
  let base = case base {
    "" -> normalize_metaobject_handle(type_)
    other -> other
  }
  unique_handle_loop(
    store,
    type_,
    case base {
      "" -> "metaobject"
      other -> other
    },
    case base {
      "" -> "metaobject"
      other -> other
    },
    1,
  )
}

fn unique_handle_loop(
  store: Store,
  type_: String,
  base: String,
  handle: String,
  suffix: Int,
) -> String {
  case find_effective_metaobject_by_handle(store, type_, handle) {
    None -> handle
    Some(_) ->
      unique_handle_loop(
        store,
        type_,
        base,
        base <> "-" <> int.to_string(suffix + 1),
        suffix + 1,
      )
  }
}

fn normalize_metaobject_handle(value: String) -> String {
  value
  |> string.trim
  |> string.lowercase
  |> string.replace(" ", "-")
  |> string.replace("_", "-")
}

fn build_metaobject_capabilities(
  input: Dict(String, root_field.ResolvedValue),
  definition: MetaobjectDefinitionRecord,
  existing: Option(MetaobjectCapabilitiesRecord),
) -> MetaobjectCapabilitiesRecord {
  let raw = read_object(input, "capabilities")
  let existing_record =
    option.unwrap(existing, MetaobjectCapabilitiesRecord(None, None))
  let publishable = case raw {
    Some(capabilities) ->
      case read_object(capabilities, "publishable") {
        Some(publishable) ->
          case read_string(publishable, "status") {
            Some(status) ->
              Some(MetaobjectPublishableCapabilityRecord(Some(status)))
            None -> existing_record.publishable
          }
        None -> existing_record.publishable
      }
    None -> existing_record.publishable
  }
  let publishable = case publishable, existing {
    None, None ->
      case definition.capabilities.publishable {
        Some(MetaobjectDefinitionCapabilityRecord(enabled: True)) ->
          Some(MetaobjectPublishableCapabilityRecord(Some("DRAFT")))
        _ -> None
      }
    _, _ -> publishable
  }
  let online_store = case raw {
    Some(capabilities) ->
      case dict.get(capabilities, "onlineStore") {
        Ok(root_field.NullVal) -> None
        Ok(root_field.ObjectVal(obj)) ->
          Some(
            MetaobjectOnlineStoreCapabilityRecord(read_string(
              obj,
              "templateSuffix",
            )),
          )
        _ ->
          case existing {
            Some(_) -> existing_record.online_store
            None -> None
          }
      }
    None ->
      case existing {
        Some(_) -> existing_record.online_store
        None -> None
      }
  }
  MetaobjectCapabilitiesRecord(
    publishable: publishable,
    online_store: online_store,
  )
}

fn adjust_definition_count(
  store: Store,
  identity: SyntheticIdentityRegistry,
  type_: String,
  delta: Int,
) -> #(Store, SyntheticIdentityRegistry) {
  case find_effective_metaobject_definition_by_type(store, type_) {
    None -> #(store, identity)
    Some(definition) -> {
      let #(now, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let count = option.unwrap(definition.metaobjects_count, 0) + delta
      let next_count = case count < 0 {
        True -> 0
        False -> count
      }
      let updated =
        MetaobjectDefinitionRecord(
          ..definition,
          metaobjects_count: Some(next_count),
          updated_at: Some(now),
        )
      let #(_, next_store) = upsert_staged_metaobject_definition(store, updated)
      #(next_store, next_identity)
    }
  }
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

pub fn metaobject_definition_source(
  definition: MetaobjectDefinitionRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("MetaobjectDefinition")),
    #("id", SrcString(definition.id)),
    #("type", SrcString(definition.type_)),
    #("name", graphql_helpers.option_string_source(definition.name)),
    #(
      "description",
      graphql_helpers.option_string_source(definition.description),
    ),
    #(
      "displayNameKey",
      graphql_helpers.option_string_source(definition.display_name_key),
    ),
    #("access", access_source(definition.access)),
    #("capabilities", definition_capabilities_source(definition.capabilities)),
    #(
      "fieldDefinitions",
      SrcList(list.map(definition.field_definitions, field_definition_source)),
    ),
    #(
      "hasThumbnailField",
      graphql_helpers.option_bool_source(definition.has_thumbnail_field),
    ),
    #(
      "metaobjectsCount",
      graphql_helpers.option_int_source(definition.metaobjects_count),
    ),
    #(
      "standardTemplate",
      standard_template_source(definition.standard_template),
    ),
    #("createdAt", graphql_helpers.option_string_source(definition.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(definition.updated_at)),
  ])
}

fn serialize_definition_selection(
  definition: MetaobjectDefinitionRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_selection(metaobject_definition_source(definition), field, fragments)
}

fn serialize_definitions_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let items = case read_string(args, "type") {
    Some(type_) -> {
      let normalized_type =
        normalize_definition_type(type_, requesting_api_client_id)
      list.filter(list_effective_metaobject_definitions(store), fn(defn) {
        string.lowercase(defn.type_) == normalized_type
      })
    }
    None -> list_effective_metaobject_definitions(store)
  }
  let window =
    paginate_connection_items(
      items,
      field,
      variables,
      fn(item, _index) { item.id },
      default_connection_window_options(),
    )
  let ConnectionWindow(items: page_items, has_next_page:, has_previous_page:) =
    window
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: page_items,
      has_next_page: has_next_page,
      has_previous_page: has_previous_page,
      get_cursor_value: fn(item, _index) { item.id },
      serialize_node: fn(item, node_field, _index) {
        serialize_definition_selection(item, node_field, fragments)
      },
      selected_field_options: SelectedFieldOptions(
        include_inline_fragments: False,
      ),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

pub fn metaobject_source(
  store: Store,
  metaobject: MetaobjectRecord,
) -> SourceValue {
  let projected = project_metaobject_through_definition(store, metaobject)
  let definition =
    find_effective_metaobject_definition_by_type(store, projected.type_)
  src_object([
    #("__typename", SrcString("Metaobject")),
    #("id", SrcString(projected.id)),
    #("handle", SrcString(projected.handle)),
    #("type", SrcString(projected.type_)),
    #(
      "displayName",
      graphql_helpers.option_string_source(projected.display_name),
    ),
    #("createdAt", graphql_helpers.option_string_source(projected.created_at)),
    #("updatedAt", graphql_helpers.option_string_source(projected.updated_at)),
    #(
      "capabilities",
      metaobject_capabilities_source(projected.capabilities, definition),
    ),
    #("fields", SrcList(list.map(projected.fields, metaobject_field_source))),
    #("definition", case definition {
      Some(defn) -> metaobject_definition_source(defn)
      None -> SrcNull
    }),
  ])
}

fn serialize_metaobject_selection(
  store: Store,
  metaobject: MetaobjectRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let projected = project_metaobject_through_definition(store, metaobject)
  let source = metaobject_source(store, projected)
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      json.object(
        list.flat_map(selections, fn(selection) {
          project_metaobject_selection(
            store,
            projected,
            source,
            selection,
            fragments,
          )
        }),
      )
    _ -> source_to_json(source)
  }
}

fn project_metaobject_selection(
  store: Store,
  metaobject: MetaobjectRecord,
  source: SourceValue,
  selection: Selection,
  fragments: FragmentMap,
) -> List(#(String, Json)) {
  case selection {
    Field(name: name, ..) -> {
      let key = get_field_response_key(selection)
      case name.value {
        "field" -> {
          let args = graphql_helpers.field_args(selection, dict.new())
          let selected = case read_string(args, "key") {
            Some(field_key) ->
              list.find(metaobject.fields, fn(f) { f.key == field_key })
              |> option.from_result
            None -> None
          }
          [
            #(key, case selected {
              Some(meta_field) ->
                serialize_metaobject_field_selection(
                  store,
                  meta_field,
                  selection,
                  fragments,
                )
              None -> json.null()
            }),
          ]
        }
        "fields" -> [
          #(
            key,
            json.array(metaobject.fields, fn(meta_field) {
              serialize_metaobject_field_selection(
                store,
                meta_field,
                selection,
                fragments,
              )
            }),
          ),
        ]
        "referencedBy" -> [
          #(
            key,
            serialize_referenced_by_connection(
              store,
              metaobject.id,
              selection,
              fragments,
            ),
          ),
        ]
        _ ->
          case source {
            SrcObject(fields) -> [
              project_source_field(fields, selection, fragments),
            ]
            _ -> [#(key, json.null())]
          }
      }
    }
    InlineFragment(type_condition: tc, selection_set: ss, ..) ->
      case source {
        SrcObject(fields) -> {
          let cond = case tc {
            Some(NamedType(name: name, ..)) -> Some(name.value)
            _ -> None
          }
          case default_type_condition_applies(fields, cond) {
            True -> {
              let SelectionSet(selections: inner, ..) = ss
              list.flat_map(inner, fn(child) {
                project_metaobject_selection(
                  store,
                  metaobject,
                  source,
                  child,
                  fragments,
                )
              })
            }
            False -> []
          }
        }
        _ -> []
      }
    FragmentSpread(name: name, ..) ->
      case dict.get(fragments, name.value), source {
        Ok(FragmentDefinition(
          type_condition: NamedType(name: cond_name, ..),
          selection_set: SelectionSet(selections: inner, ..),
          ..,
        )),
          SrcObject(fields)
        ->
          case default_type_condition_applies(fields, Some(cond_name.value)) {
            True ->
              list.flat_map(inner, fn(child) {
                project_metaobject_selection(
                  store,
                  metaobject,
                  source,
                  child,
                  fragments,
                )
              })
            False -> []
          }
        _, _ -> []
      }
  }
}

fn serialize_metaobjects_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let items = case read_string(args, "type") {
    Some(type_) -> list_effective_metaobjects_by_type(store, type_)
    None -> list_effective_metaobjects(store)
  }
  let items =
    items
    |> list.filter(fn(item) { is_metaobject_visible_in_catalog(store, item) })
    |> sort_metaobjects_for_connection(
      read_string(args, "sortKey"),
      option.unwrap(read_bool(args, "reverse"), False),
    )
  let window =
    paginate_connection_items(
      items,
      field,
      variables,
      fn(item, _index) { item.id },
      default_connection_window_options(),
    )
  let ConnectionWindow(items: page_items, has_next_page:, has_previous_page:) =
    window
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: page_items,
      has_next_page: has_next_page,
      has_previous_page: has_previous_page,
      get_cursor_value: fn(item, _index) { item.id },
      serialize_node: fn(item, node_field, _index) {
        serialize_metaobject_selection(store, item, node_field, fragments)
      },
      selected_field_options: SelectedFieldOptions(
        include_inline_fragments: False,
      ),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn sort_metaobjects_for_connection(
  items: List(MetaobjectRecord),
  sort_key: Option(String),
  reverse: Bool,
) -> List(MetaobjectRecord) {
  let normalized = option.unwrap(sort_key, "id") |> string.lowercase
  let sorted =
    list.sort(items, fn(left, right) {
      case normalized {
        "display_name" -> {
          let primary =
            resource_ids.compare_nullable_strings(
              left.display_name,
              right.display_name,
            )
          case primary {
            Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
            _ -> primary
          }
        }
        "type" -> {
          let primary = string.compare(left.type_, right.type_)
          case primary {
            Eq -> {
              let secondary = string.compare(left.handle, right.handle)
              case secondary {
                Eq ->
                  resource_ids.compare_shopify_resource_ids(left.id, right.id)
                _ -> secondary
              }
            }
            _ -> primary
          }
        }
        "updated_at" -> {
          let primary =
            resource_ids.compare_nullable_strings(
              left.updated_at,
              right.updated_at,
            )
          case primary {
            Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
            _ -> primary
          }
        }
        _ -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
      }
    })
  case reverse {
    True -> list.reverse(sorted)
    False -> sorted
  }
}

fn is_metaobject_visible_in_catalog(
  store: Store,
  metaobject: MetaobjectRecord,
) -> Bool {
  case find_effective_metaobject_definition_by_type(store, metaobject.type_) {
    None -> True
    Some(definition) ->
      metaobject_has_required_field_values(metaobject, definition)
      && metaobject_publishable_visible(metaobject, definition)
  }
}

fn metaobject_has_required_field_values(
  metaobject: MetaobjectRecord,
  definition: MetaobjectDefinitionRecord,
) -> Bool {
  list.all(definition.field_definitions, fn(field_definition) {
    case field_definition.required {
      Some(True) ->
        case
          list.find(metaobject.fields, fn(field) {
            field.key == field_definition.key
          })
        {
          Ok(field) ->
            case field.value {
              Some(value) -> value != ""
              None -> False
            }
          Error(_) -> False
        }
      _ -> True
    }
  })
}

fn metaobject_publishable_visible(
  metaobject: MetaobjectRecord,
  definition: MetaobjectDefinitionRecord,
) -> Bool {
  case
    definition.capabilities.publishable,
    metaobject.capabilities.publishable
  {
    Some(MetaobjectDefinitionCapabilityRecord(enabled: False)), None -> False
    _, _ -> True
  }
}

fn metaobject_field_source(field: MetaobjectFieldRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("MetaobjectField")),
    #("key", SrcString(field.key)),
    #("type", graphql_helpers.option_string_source(field.type_)),
    #("value", graphql_helpers.option_string_source(field.value)),
    #("jsonValue", metaobject_field_json_value_source(field)),
    #("definition", case field.definition {
      Some(defn) -> field_definition_reference_source(defn)
      None -> SrcNull
    }),
  ])
}

fn metaobject_field_json_value_source(
  field: MetaobjectFieldRecord,
) -> SourceValue {
  case field.type_, field.value {
    Some(type_), Some(raw) ->
      case measurement_json_value_source(type_, raw) {
        Some(source) -> source
        None -> metaobject_field_stored_json_value_source(field)
      }
    _, _ -> metaobject_field_stored_json_value_source(field)
  }
}

fn metaobject_field_stored_json_value_source(
  field: MetaobjectFieldRecord,
) -> SourceValue {
  case field.json_value, field.type_ {
    MetaobjectString(raw), Some(type_) ->
      case should_parse_metaobject_json_value(type_) {
        True ->
          metaobject_json_value_to_source(read_metaobject_json_value(
            type_,
            Some(raw),
          ))
        False -> metaobject_json_value_to_source(field.json_value)
      }
    _, _ -> metaobject_json_value_to_source(field.json_value)
  }
}

fn measurement_json_value_source(
  type_: String,
  raw: String,
) -> Option(SourceValue) {
  case string.starts_with(type_, "list.") {
    True -> {
      let base_type = string.drop_start(type_, 5)
      case is_measurement_metaobject_type(base_type) {
        True -> parse_single_item_measurement_list_source(raw, base_type)
        False -> None
      }
    }
    False ->
      case is_measurement_metaobject_type(type_) {
        True -> parse_measurement_source(raw)
        False -> None
      }
  }
}

fn parse_single_item_measurement_list_source(
  raw: String,
  type_: String,
) -> Option(SourceValue) {
  case string.starts_with(raw, "[") && string.ends_with(raw, "]") {
    True ->
      parse_measurement_list_item_source(
        string.drop_start(raw, 1) |> string.drop_end(1),
        type_,
      )
      |> option.map(fn(item) { SrcList([item]) })
    False -> None
  }
}

fn parse_measurement_list_item_source(
  raw: String,
  type_: String,
) -> Option(SourceValue) {
  case parse_measurement_source(raw) {
    Some(SrcObject(fields)) ->
      case dict.get(fields, "unit") {
        Ok(SrcString(unit)) ->
          Some(
            SrcObject(dict.insert(
              fields,
              "unit",
              SrcString(normalize_measurement_list_json_unit(type_, unit)),
            )),
          )
        _ -> Some(SrcObject(fields))
      }
    other -> other
  }
}

fn parse_measurement_source(raw: String) -> Option(SourceValue) {
  case string.split(raw, on: ",\"unit\":\"") {
    [left, right] -> {
      let value_raw = string.drop_start(left, string.length("{\"value\":"))
      let unit = string.drop_end(right, 2)
      case measurement_number_source(value_raw) {
        Some(value) ->
          Some(src_object([#("value", value), #("unit", SrcString(unit))]))
        None -> None
      }
    }
    _ -> None
  }
}

fn measurement_number_source(raw: String) -> Option(SourceValue) {
  case string.contains(raw, ".") {
    True ->
      case float.parse(raw) {
        Ok(value) -> Some(whole_float_to_number_source(value))
        Error(_) -> None
      }
    False ->
      case int.parse(raw) {
        Ok(value) -> Some(SrcInt(value))
        Error(_) ->
          case float.parse(raw) {
            Ok(value) -> Some(whole_float_to_number_source(value))
            Error(_) -> None
          }
      }
  }
}

fn whole_float_to_number_source(value: Float) -> SourceValue {
  let truncated = float.truncate(value)
  case int.to_float(truncated) == value {
    True -> SrcInt(truncated)
    False -> SrcFloat(value)
  }
}

fn serialize_metaobject_field_selection(
  store: Store,
  meta_field: MetaobjectFieldRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source = metaobject_field_source(meta_field)
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      json.object(
        list.flat_map(selections, fn(selection) {
          project_metaobject_field_selection(
            store,
            meta_field,
            source,
            selection,
            fragments,
          )
        }),
      )
    _ -> source_to_json(source)
  }
}

fn project_metaobject_field_selection(
  store: Store,
  meta_field: MetaobjectFieldRecord,
  source: SourceValue,
  selection: Selection,
  fragments: FragmentMap,
) -> List(#(String, Json)) {
  case selection {
    Field(name: name, ..) -> {
      let key = get_field_response_key(selection)
      case name.value {
        "reference" -> [
          #(
            key,
            serialize_single_reference(store, meta_field, selection, fragments),
          ),
        ]
        "references" -> [
          #(
            key,
            serialize_field_references_connection(
              store,
              meta_field,
              selection,
              fragments,
            ),
          ),
        ]
        _ ->
          case source {
            SrcObject(fields) -> [
              project_source_field(fields, selection, fragments),
            ]
            _ -> [#(key, json.null())]
          }
      }
    }
    InlineFragment(type_condition: tc, selection_set: ss, ..) ->
      case source {
        SrcObject(fields) -> {
          let cond = case tc {
            Some(NamedType(name: name, ..)) -> Some(name.value)
            _ -> None
          }
          case default_type_condition_applies(fields, cond) {
            True -> {
              let SelectionSet(selections: inner, ..) = ss
              list.flat_map(inner, fn(child) {
                project_metaobject_field_selection(
                  store,
                  meta_field,
                  source,
                  child,
                  fragments,
                )
              })
            }
            False -> []
          }
        }
        _ -> []
      }
    FragmentSpread(name: name, ..) ->
      case dict.get(fragments, name.value), source {
        Ok(FragmentDefinition(
          type_condition: NamedType(name: cond_name, ..),
          selection_set: SelectionSet(selections: inner, ..),
          ..,
        )),
          SrcObject(fields)
        ->
          case default_type_condition_applies(fields, Some(cond_name.value)) {
            True ->
              list.flat_map(inner, fn(child) {
                project_metaobject_field_selection(
                  store,
                  meta_field,
                  source,
                  child,
                  fragments,
                )
              })
            False -> []
          }
        _, _ -> []
      }
  }
}

fn serialize_single_reference(
  store: Store,
  field: MetaobjectFieldRecord,
  selection: Selection,
  fragments: FragmentMap,
) -> Json {
  case field.type_, field.value {
    Some("metaobject_reference"), Some(id) ->
      case get_effective_metaobject_by_id(store, id) {
        Some(metaobject) ->
          serialize_metaobject_selection(
            store,
            metaobject,
            selection,
            fragments,
          )
        None -> json.null()
      }
    _, _ -> json.null()
  }
}

fn serialize_field_references_connection(
  store: Store,
  field_record: MetaobjectFieldRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field_record.type_ {
    Some("list.metaobject_reference") -> {
      let refs =
        read_metaobject_reference_ids_from_field(field_record)
        |> list.filter_map(fn(id) {
          case get_effective_metaobject_by_id(store, id) {
            Some(record) -> Ok(record)
            None -> Error(Nil)
          }
        })
      let window =
        paginate_connection_items(
          refs,
          field,
          dict.new(),
          fn(item, _index) { item.id },
          default_connection_window_options(),
        )
      let ConnectionWindow(
        items: page_items,
        has_next_page:,
        has_previous_page:,
      ) = window
      serialize_connection(
        field,
        SerializeConnectionConfig(
          items: page_items,
          has_next_page: has_next_page,
          has_previous_page: has_previous_page,
          get_cursor_value: fn(item, _index) { item.id },
          serialize_node: fn(item, node_field, _index) {
            serialize_metaobject_selection(store, item, node_field, fragments)
          },
          selected_field_options: SelectedFieldOptions(False),
          page_info_options: default_connection_page_info_options(),
        ),
      )
    }
    _ -> json.null()
  }
}

fn serialize_referenced_by_connection(
  store: Store,
  target_id: String,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let relations =
    list.flat_map(list_effective_metaobjects(store), fn(referencer) {
      let projected = project_metaobject_through_definition(store, referencer)
      list.filter_map(projected.fields, fn(meta_field) {
        case
          list.contains(
            read_metaobject_reference_ids_from_field(meta_field),
            target_id,
          )
        {
          True -> Ok(#(meta_field, projected))
          False -> Error(Nil)
        }
      })
    })
  let window =
    paginate_connection_items(
      relations,
      field,
      dict.new(),
      fn(item, _index) {
        let #(meta_field, referencer) = item
        referencer.id <> ":" <> meta_field.key
      },
      default_connection_window_options(),
    )
  let ConnectionWindow(items: page_items, has_next_page:, has_previous_page:) =
    window
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: page_items,
      has_next_page: has_next_page,
      has_previous_page: has_previous_page,
      get_cursor_value: fn(item, _index) {
        let #(meta_field, referencer) = item
        referencer.id <> ":" <> meta_field.key
      },
      serialize_node: fn(item, node_field, _index) {
        let #(meta_field, referencer) = item
        let relation_source =
          src_object([
            #("__typename", SrcString("MetaobjectFieldReference")),
            #("key", SrcString(meta_field.key)),
            #(
              "name",
              graphql_helpers.option_string_source(case meta_field.definition {
                Some(defn) -> defn.name
                None -> None
              }),
            ),
            #("namespace", SrcString(referencer.type_)),
            #("referencer", metaobject_source(store, referencer)),
          ])
        project_selection(relation_source, node_field, fragments)
      },
      selected_field_options: SelectedFieldOptions(False),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn definition_payload(
  field: Selection,
  fragments: FragmentMap,
  definition: Option(MetaobjectDefinitionRecord),
  user_errors: List(UserError),
) -> Json {
  let source =
    src_object([
      #("metaobjectDefinition", case definition {
        Some(defn) -> metaobject_definition_source(defn)
        None -> SrcNull
      }),
      #("userErrors", SrcList(list.map(user_errors, user_error_source))),
    ])
  project_selection(source, field, fragments)
}

fn metaobject_payload(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  metaobject: Option(MetaobjectRecord),
  user_errors: List(UserError),
) -> Json {
  let source =
    src_object([
      #("metaobject", case metaobject {
        Some(record) -> metaobject_source(store, record)
        None -> SrcNull
      }),
      #("userErrors", SrcList(list.map(user_errors, user_error_source))),
    ])
  project_selection_with_metaobject(store, source, field, fragments)
}

fn definition_delete_result(
  key: String,
  field: Selection,
  deleted_id: Option(String),
  user_errors: List(UserError),
) -> MutationFieldResult {
  let source =
    src_object([
      #("deletedId", graphql_helpers.option_string_source(deleted_id)),
      #("userErrors", SrcList(list.map(user_errors, user_error_source))),
    ])
  MutationFieldResult(
    key,
    project_selection(source, field, dict.new()),
    option_string_to_list(deleted_id),
    [],
    case deleted_id {
      Some(id) -> [log_draft("metaobjectDefinitionDelete", [id])]
      None -> []
    },
  )
}

fn metaobject_delete_result(
  key: String,
  field: Selection,
  deleted_id: Option(String),
  user_errors: List(UserError),
) -> MutationFieldResult {
  let source =
    src_object([
      #("deletedId", graphql_helpers.option_string_source(deleted_id)),
      #("userErrors", SrcList(list.map(user_errors, user_error_source))),
    ])
  MutationFieldResult(
    key,
    project_selection(source, field, dict.new()),
    option_string_to_list(deleted_id),
    [],
    case deleted_id {
      Some(id) -> [log_draft("metaobjectDelete", [id])]
      None -> []
    },
  )
}

fn bulk_delete_payload(
  field: Selection,
  fragments: FragmentMap,
  job: Option(BulkDeleteJob),
  user_errors: List(UserError),
) -> Json {
  let source =
    src_object([
      #("job", case job {
        Some(BulkDeleteJob(id:, done:)) ->
          src_object([
            #("__typename", SrcString("Job")),
            #("id", SrcString(id)),
            #("done", SrcBool(done)),
          ])
        None -> SrcNull
      }),
      #("userErrors", SrcList(list.map(user_errors, user_error_source))),
    ])
  project_selection(source, field, fragments)
}

fn project_selection(
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(source, selections, fragments)
    _ -> source_to_json(source)
  }
}

fn project_selection_with_metaobject(
  store: Store,
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case source, field {
    SrcObject(fields),
      Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..)
    ->
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          case selection {
            Field(name: name, ..) if name.value == "metaobject" ->
              case dict.get(fields, "metaobject") {
                Ok(SrcObject(meta_fields)) ->
                  project_source_field_with_metaobject(
                    store,
                    meta_fields,
                    selection,
                    fragments,
                  )
                Ok(SrcNull) -> #(key, json.null())
                _ -> #(key, json.null())
              }
            _ -> project_source_field(fields, selection, fragments)
          }
        }),
      )
    _, _ -> project_selection(source, field, fragments)
  }
}

fn project_source_field_with_metaobject(
  store: Store,
  meta_fields: Dict(String, SourceValue),
  selection: Selection,
  fragments: FragmentMap,
) -> #(String, Json) {
  let key = get_field_response_key(selection)
  let id = case dict.get(meta_fields, "id") {
    Ok(SrcString(value)) -> Some(value)
    _ -> None
  }
  case id {
    Some(metaobject_id) ->
      case get_effective_metaobject_by_id(store, metaobject_id) {
        Some(record) -> #(
          key,
          serialize_metaobject_selection(store, record, selection, fragments),
        )
        None -> #(
          key,
          project_graphql_value(
            SrcObject(meta_fields),
            graphql_helpers.field_raw_selections(selection),
            fragments,
          ),
        )
      }
    None -> #(
      key,
      project_graphql_value(
        SrcObject(meta_fields),
        graphql_helpers.field_raw_selections(selection),
        fragments,
      ),
    )
  }
}

fn project_source_field(
  source: Dict(String, SourceValue),
  selection: Selection,
  fragments: FragmentMap,
) -> #(String, Json) {
  let key = get_field_response_key(selection)
  case selection {
    Field(name: name, ..) ->
      case name.value {
        "__typename" -> #(key, case dict.get(source, "__typename") {
          Ok(value) -> source_to_json(value)
          Error(_) -> json.null()
        })
        field_name -> {
          let value = dict.get(source, field_name) |> result.unwrap(SrcNull)
          let selections = graphql_helpers.field_raw_selections(selection)
          case selections {
            [] -> #(key, source_to_json(value))
            _ -> #(key, project_graphql_value(value, selections, fragments))
          }
        }
      }
    _ -> #(key, json.null())
  }
}

fn user_error_source(error: UserError) -> SourceValue {
  src_object([
    #("field", case error.field {
      Some(parts) -> SrcList(list.map(parts, SrcString))
      None -> SrcNull
    }),
    #("message", SrcString(error.message)),
    #("code", SrcString(error.code)),
    #("elementKey", graphql_helpers.option_string_source(error.element_key)),
    #("elementIndex", graphql_helpers.option_int_source(error.element_index)),
  ])
}

// ---------------------------------------------------------------------------
// Source helpers
// ---------------------------------------------------------------------------

fn access_source(access: Dict(String, Option(String))) -> SourceValue {
  SrcObject(
    dict.to_list(access)
    |> list.map(fn(pair) {
      let #(key, value) = pair
      #(key, graphql_helpers.option_string_source(value))
    })
    |> dict.from_list,
  )
}

fn definition_capabilities_source(
  capabilities: MetaobjectDefinitionCapabilitiesRecord,
) -> SourceValue {
  src_object([
    #("publishable", definition_capability_source(capabilities.publishable)),
    #("translatable", definition_capability_source(capabilities.translatable)),
    #("renderable", definition_capability_source(capabilities.renderable)),
    #("onlineStore", definition_capability_source(capabilities.online_store)),
  ])
}

fn definition_capability_source(
  capability: Option(MetaobjectDefinitionCapabilityRecord),
) -> SourceValue {
  case capability {
    Some(MetaobjectDefinitionCapabilityRecord(enabled: enabled)) ->
      src_object([#("enabled", SrcBool(enabled))])
    None -> SrcNull
  }
}

fn field_definition_source(
  definition: MetaobjectFieldDefinitionRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("MetaobjectFieldDefinition")),
    #("key", SrcString(definition.key)),
    #("name", graphql_helpers.option_string_source(definition.name)),
    #(
      "description",
      graphql_helpers.option_string_source(definition.description),
    ),
    #("required", graphql_helpers.option_bool_source(definition.required)),
    #("type", type_source(definition.type_)),
    #(
      "validations",
      SrcList(list.map(definition.validations, validation_source)),
    ),
  ])
}

fn field_definition_reference_source(
  definition: MetaobjectFieldDefinitionReferenceRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("MetaobjectFieldDefinition")),
    #("key", SrcString(definition.key)),
    #("name", graphql_helpers.option_string_source(definition.name)),
    #("required", graphql_helpers.option_bool_source(definition.required)),
    #("type", type_source(definition.type_)),
  ])
}

fn field_definition_reference(
  definition: MetaobjectFieldDefinitionRecord,
) -> MetaobjectFieldDefinitionReferenceRecord {
  MetaobjectFieldDefinitionReferenceRecord(
    key: definition.key,
    name: definition.name,
    required: definition.required,
    type_: definition.type_,
  )
}

fn type_source(type_: MetaobjectDefinitionTypeRecord) -> SourceValue {
  src_object([
    #("name", SrcString(type_.name)),
    #("category", graphql_helpers.option_string_source(type_.category)),
  ])
}

fn validation_source(
  validation: MetaobjectFieldDefinitionValidationRecord,
) -> SourceValue {
  src_object([
    #("name", SrcString(validation.name)),
    #("value", graphql_helpers.option_string_source(validation.value)),
  ])
}

fn standard_template_source(
  template: Option(MetaobjectStandardTemplateRecord),
) -> SourceValue {
  case template {
    Some(MetaobjectStandardTemplateRecord(type_: type_, name: name)) ->
      src_object([
        #("type", graphql_helpers.option_string_source(type_)),
        #("name", graphql_helpers.option_string_source(name)),
      ])
    None -> SrcNull
  }
}

fn metaobject_capabilities_source(
  capabilities: MetaobjectCapabilitiesRecord,
  definition: Option(MetaobjectDefinitionRecord),
) -> SourceValue {
  let publishable = case definition {
    Some(defn) ->
      case defn.capabilities.publishable {
        Some(MetaobjectDefinitionCapabilityRecord(enabled: False)) -> SrcNull
        _ -> metaobject_publishable_capability_source(capabilities, definition)
      }
    None -> metaobject_publishable_capability_source(capabilities, definition)
  }
  let online_store = case capabilities.online_store {
    Some(MetaobjectOnlineStoreCapabilityRecord(template_suffix: suffix)) ->
      src_object([
        #("templateSuffix", graphql_helpers.option_string_source(suffix)),
      ])
    None -> SrcNull
  }
  src_object([
    #("publishable", publishable),
    #("onlineStore", online_store),
  ])
}

fn metaobject_publishable_capability_source(
  capabilities: MetaobjectCapabilitiesRecord,
  definition: Option(MetaobjectDefinitionRecord),
) -> SourceValue {
  case capabilities.publishable {
    Some(MetaobjectPublishableCapabilityRecord(status: status)) ->
      src_object([#("status", graphql_helpers.option_string_source(status))])
    None ->
      case definition {
        Some(defn) ->
          case defn.capabilities.publishable {
            Some(MetaobjectDefinitionCapabilityRecord(enabled: True)) ->
              src_object([#("status", SrcString("DRAFT"))])
            _ -> SrcNull
          }
        None -> SrcNull
      }
  }
}

fn metaobject_json_value_to_source(value: MetaobjectJsonValue) -> SourceValue {
  case value {
    MetaobjectNull -> SrcNull
    MetaobjectString(value) -> SrcString(value)
    MetaobjectBool(value) -> SrcBool(value)
    MetaobjectInt(value) -> SrcInt(value)
    MetaobjectFloat(value) -> SrcFloat(value)
    MetaobjectList(items) ->
      SrcList(list.map(items, metaobject_json_value_to_source))
    MetaobjectObject(fields) ->
      SrcObject(
        dict.to_list(fields)
        |> list.map(fn(pair) {
          #(pair.0, metaobject_json_value_to_source(pair.1))
        })
        |> dict.from_list,
      )
  }
}

// ---------------------------------------------------------------------------
// Readers and small utilities
// ---------------------------------------------------------------------------

fn read_id_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  read_string(graphql_helpers.field_args(field, variables), "id")
}

fn read_string_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  read_string(graphql_helpers.field_args(field, variables), key)
}

fn read_bool_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Bool {
  read_bool(graphql_helpers.field_args(field, variables), key)
  |> option.unwrap(False)
}

fn read_handle_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Option(String), Option(String)) {
  read_handle_value(read_object_arg(
    graphql_helpers.field_args(field, variables),
    "handle",
  ))
}

fn read_handle_value(
  input: Dict(String, root_field.ResolvedValue),
) -> #(Option(String), Option(String)) {
  #(read_string(input, "type"), read_string(input, "handle"))
}

fn read_string(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  read_optional_string(input, key)
}

fn read_non_blank_string(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case read_string(input, key) {
    Some(value) ->
      case string.trim(value) {
        "" -> None
        trimmed -> Some(trimmed)
      }
    None -> None
  }
}

fn read_string_if_present(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
  existing: Option(String),
) -> Option(String) {
  case dict.get(input, key) {
    Ok(root_field.StringVal(value)) -> Some(value)
    Ok(root_field.NullVal) -> None
    _ -> existing
  }
}

fn read_bool(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Bool) {
  case dict.get(input, key) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_object(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(input, key) {
    Ok(root_field.ObjectVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_object_arg(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Dict(String, root_field.ResolvedValue) {
  read_object(input, key) |> option.unwrap(dict.new())
}

fn read_list(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(root_field.ResolvedValue) {
  case dict.get(input, key) {
    Ok(root_field.ListVal(values)) -> values
    _ -> []
  }
}

fn read_type_name(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case read_string(input, "type") {
    Some(value) -> Some(value)
    None ->
      case read_object(input, "type") {
        Some(type_obj) -> read_string(type_obj, "name")
        None -> None
      }
  }
}

fn read_validation_inputs(
  values: List(root_field.ResolvedValue),
) -> List(MetaobjectFieldDefinitionValidationRecord) {
  list.filter_map(values, fn(value) {
    case value {
      root_field.ObjectVal(obj) ->
        case read_string(obj, "name") {
          Some(name) ->
            Ok(MetaobjectFieldDefinitionValidationRecord(
              name,
              read_string(obj, "value"),
            ))
          None -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
}

fn normalize_definition_capabilities(
  raw: Option(Dict(String, root_field.ResolvedValue)),
  base: MetaobjectDefinitionCapabilitiesRecord,
) -> MetaobjectDefinitionCapabilitiesRecord {
  case raw {
    None -> base
    Some(capabilities) ->
      MetaobjectDefinitionCapabilitiesRecord(
        publishable: merge_definition_capability(
          capabilities,
          "publishable",
          base.publishable,
        ),
        translatable: merge_definition_capability(
          capabilities,
          "translatable",
          base.translatable,
        ),
        renderable: merge_definition_capability(
          capabilities,
          "renderable",
          base.renderable,
        ),
        online_store: merge_definition_capability(
          capabilities,
          "onlineStore",
          base.online_store,
        ),
      )
  }
}

fn merge_definition_capability(
  raw: Dict(String, root_field.ResolvedValue),
  key: String,
  base: Option(MetaobjectDefinitionCapabilityRecord),
) -> Option(MetaobjectDefinitionCapabilityRecord) {
  case read_object(raw, key) {
    Some(capability) ->
      case read_bool(capability, "enabled") {
        Some(enabled) -> Some(MetaobjectDefinitionCapabilityRecord(enabled))
        None -> base
      }
    None -> base
  }
}

fn build_definition_access(
  raw: Option(Dict(String, root_field.ResolvedValue)),
  base: Dict(String, Option(String)),
) -> Dict(String, Option(String)) {
  case raw {
    None -> base
    Some(access) ->
      list.fold(dict.to_list(access), base, fn(acc, pair) {
        let #(key, value) = pair
        case value {
          root_field.StringVal(text) -> dict.insert(acc, key, Some(text))
          root_field.NullVal -> dict.insert(acc, key, None)
          _ -> acc
        }
      })
  }
}

fn infer_field_type_category(type_name: String) -> Option(String) {
  case
    string.contains(type_name, "text")
    || type_name == "url"
    || type_name == "color"
  {
    True -> Some("TEXT")
    False ->
      case
        string.contains(type_name, "number")
        || type_name == "rating"
        || type_name == "volume"
        || type_name == "weight"
      {
        True -> Some("NUMBER")
        False ->
          case string.contains(type_name, "reference") {
            True -> Some("REFERENCE")
            False ->
              case type_name {
                "boolean" -> Some("TRUE_FALSE")
                "date" | "date_time" -> Some("DATE_TIME")
                "json" -> Some("JSON")
                _ -> None
              }
          }
      }
  }
}

fn normalize_metaobject_value(
  type_name: String,
  value: Option(String),
) -> Option(String) {
  case value {
    None -> None
    Some(raw) ->
      case type_name {
        "boolean" -> Some(normalize_boolean_value(raw))
        "number_integer" -> Some(normalize_integer_value(raw))
        "date_time" -> Some(normalize_date_time_value(raw))
        "rating" -> Some(normalize_rating_value_string(raw))
        _ ->
          case string.starts_with(type_name, "list.") {
            True ->
              normalize_list_metaobject_value_string(
                string.drop_start(type_name, 5),
                raw,
              )
            False ->
              case is_measurement_metaobject_type(type_name) {
                True -> normalize_measurement_value_string(raw)
                False -> Some(raw)
              }
          }
      }
  }
}

fn normalize_boolean_value(raw: String) -> String {
  case raw {
    "false" -> "false"
    _ -> "true"
  }
}

fn normalize_integer_value(raw: String) -> String {
  case int.parse(raw) {
    Ok(value) -> int.to_string(value)
    Error(_) ->
      case float.parse(raw) {
        Ok(value) -> int.to_string(float.truncate(value))
        Error(_) -> "0"
      }
  }
}

fn normalize_date_time_value(value: String) -> String {
  let lower = string.lowercase(value)
  case string.ends_with(lower, "z") {
    True -> string.drop_end(value, 1) <> "+00:00"
    False ->
      case has_timezone_offset(value) {
        True -> value
        False -> value <> "+00:00"
      }
  }
}

fn has_timezone_offset(value: String) -> Bool {
  let length = string.length(value)
  case length >= 6 {
    False -> False
    True -> {
      let sign = string.slice(value, length - 6, 1)
      let colon = string.slice(value, length - 3, 1)
      case sign, colon {
        "+", ":" -> True
        "-", ":" -> True
        _, _ -> False
      }
    }
  }
}

fn normalize_list_metaobject_value_string(
  type_name: String,
  raw: String,
) -> Option(String) {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      case decode.run(dynamic, decode.list(decode.dynamic)) {
        Ok(items) ->
          case type_name {
            "number_decimal" | "float" ->
              Some(normalize_decimal_list_string(raw))
            "date_time" ->
              items
              |> list.try_map(fn(item) {
                case decode.run(item, decode.string) {
                  Ok(value) ->
                    Ok(MetaobjectString(normalize_date_time_value(value)))
                  Error(_) -> dynamic_to_metaobject_json(item)
                }
              })
              |> result.map(metaobject_json_list_to_string)
              |> option.from_result
            "rating" -> Some(normalize_rating_list_string(raw))
            _ ->
              case is_measurement_metaobject_type(type_name) {
                True ->
                  items
                  |> list.try_map(normalize_measurement_value_dynamic_to_string)
                  |> result.map(fn(parts) {
                    "[" <> string.join(parts, ",") <> "]"
                  })
                  |> option.from_result
                False -> Some(raw)
              }
          }
        Error(_) -> Some(raw)
      }
    Error(_) -> Some(raw)
  }
}

fn is_measurement_metaobject_type(type_name: String) -> Bool {
  case type_name {
    "antenna_gain"
    | "area"
    | "battery_charge_capacity"
    | "battery_energy_capacity"
    | "capacitance"
    | "concentration"
    | "data_storage_capacity"
    | "data_transfer_rate"
    | "dimension"
    | "display_density"
    | "distance"
    | "duration"
    | "electric_current"
    | "electrical_resistance"
    | "energy"
    | "frequency"
    | "illuminance"
    | "inductance"
    | "luminous_flux"
    | "mass_flow_rate"
    | "power"
    | "pressure"
    | "resolution"
    | "rotational_speed"
    | "sound_level"
    | "speed"
    | "temperature"
    | "thermal_power"
    | "voltage"
    | "volume"
    | "volumetric_flow_rate"
    | "weight" -> True
    _ -> False
  }
}

fn normalize_decimal_list_string(raw: String) -> String {
  case string.starts_with(raw, "[") && string.ends_with(raw, "]") {
    True -> {
      let inner = string.drop_start(raw, 1) |> string.drop_end(1)
      "[\"" <> inner <> "\"]"
    }
    False -> raw
  }
}

fn should_parse_metaobject_json_value(type_name: String) -> Bool {
  case type_name {
    "json" | "json_string" | "link" | "money" | "rating" | "rich_text_field" ->
      True
    _ ->
      is_measurement_metaobject_type(type_name)
      || string.starts_with(type_name, "list.")
  }
}

fn normalize_measurement_value_string(raw: String) -> Option(String) {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      normalize_measurement_value_dynamic_to_string(dynamic)
      |> option.from_result
      |> option.or(Some(raw))
    Error(_) -> Some(raw)
  }
}

fn normalize_measurement_value_dynamic_to_string(
  dynamic: Dynamic,
) -> Result(String, Nil) {
  use fields <- result.try(
    decode.run(dynamic, decode.dict(decode.string, decode.dynamic))
    |> result.replace_error(Nil),
  )
  use value <- result.try(
    dict.get(fields, "value") |> result.replace_error(Nil),
  )
  use unit <- result.try(dict.get(fields, "unit") |> result.replace_error(Nil))
  use value_string <- result.try(read_measurement_number_string(value))
  use unit_string <- result.try(
    decode.run(unit, decode.string) |> result.replace_error(Nil),
  )
  Ok(
    "{\"value\":"
    <> value_string
    <> ",\"unit\":\""
    <> string.uppercase(unit_string)
    <> "\"}",
  )
}

fn read_measurement_number_string(dynamic: Dynamic) -> Result(String, Nil) {
  case decode.run(dynamic, decode.int) {
    Ok(value) -> Ok(int.to_string(value) <> ".0")
    Error(_) ->
      case decode.run(dynamic, decode.float) {
        Ok(value) -> Ok(float.to_string(value))
        Error(_) ->
          case decode.run(dynamic, decode.string) {
            Ok(value) ->
              case int.parse(value) {
                Ok(parsed) -> Ok(int.to_string(parsed) <> ".0")
                Error(_) ->
                  case float.parse(value) {
                    Ok(parsed) -> Ok(float.to_string(parsed))
                    Error(_) -> Error(Nil)
                  }
              }
            Error(_) -> Error(Nil)
          }
      }
  }
}

fn normalize_rating_value_string(raw: String) -> String {
  case rating_parts(raw) {
    Some(parts) -> rating_parts_to_string(parts)
    None -> raw
  }
}

fn normalize_rating_list_string(raw: String) -> String {
  case string.starts_with(raw, "[") && string.ends_with(raw, "]") {
    True -> {
      let inner = string.drop_start(raw, 1) |> string.drop_end(1)
      case rating_parts(inner) {
        Some(parts) -> "[" <> rating_parts_to_string(parts) <> "]"
        None -> raw
      }
    }
    False -> raw
  }
}

fn rating_parts_to_string(parts: #(String, String, String)) -> String {
  let #(scale_min, scale_max, value) = parts
  "{\"scale_min\":\""
  <> scale_min
  <> "\",\"scale_max\":\""
  <> scale_max
  <> "\",\"value\":\""
  <> value
  <> "\"}"
}

fn rating_parts(raw: String) -> Option(#(String, String, String)) {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      case normalize_rating_dynamic(dynamic) {
        Ok(MetaobjectObject(fields)) ->
          case
            dict.get(fields, "scale_min"),
            dict.get(fields, "scale_max"),
            dict.get(fields, "value")
          {
            Ok(MetaobjectString(min)),
              Ok(MetaobjectString(max)),
              Ok(MetaobjectString(value))
            -> Some(#(min, max, value))
            _, _, _ -> None
          }
        _ -> None
      }
    Error(_) -> None
  }
}

fn normalize_rating_dynamic(
  dynamic: Dynamic,
) -> Result(MetaobjectJsonValue, Nil) {
  use fields <- result.try(
    decode.run(dynamic, decode.dict(decode.string, decode.dynamic))
    |> result.replace_error(Nil),
  )
  use scale_min <- result.try(read_dynamic_string_field(fields, "scale_min"))
  use scale_max <- result.try(read_dynamic_string_field(fields, "scale_max"))
  use value <- result.try(read_dynamic_string_field(fields, "value"))
  Ok(
    MetaobjectObject(
      dict.from_list([
        #("scale_min", MetaobjectString(scale_min)),
        #("scale_max", MetaobjectString(scale_max)),
        #("value", MetaobjectString(value)),
      ]),
    ),
  )
}

fn read_dynamic_string_field(
  fields: Dict(String, Dynamic),
  key: String,
) -> Result(String, Nil) {
  use value <- result.try(dict.get(fields, key) |> result.replace_error(Nil))
  decode.run(value, decode.string) |> result.replace_error(Nil)
}

fn metaobject_json_list_to_string(items: List(MetaobjectJsonValue)) -> String {
  "["
  <> string.join(list.map(items, metaobject_json_value_to_compact_string), ",")
  <> "]"
}

fn metaobject_json_value_to_compact_string(
  value: MetaobjectJsonValue,
) -> String {
  source_to_json(metaobject_json_value_to_source(value))
  |> json.to_string
}

fn read_metaobject_json_value(
  type_name: String,
  value: Option(String),
) -> MetaobjectJsonValue {
  case value {
    None -> MetaobjectNull
    Some(raw) ->
      case type_name {
        "date_time" -> MetaobjectString(normalize_date_time_value(raw))
        "boolean" ->
          case raw {
            "true" -> MetaobjectBool(True)
            "false" -> MetaobjectBool(False)
            _ -> MetaobjectString(raw)
          }
        "number_integer" ->
          case int.parse(raw) {
            Ok(value) -> MetaobjectInt(value)
            Error(_) -> MetaobjectString(raw)
          }
        "number_decimal" | "float" -> MetaobjectString(raw)
        "rating" -> parse_rating_json_value(raw)
        _ ->
          case string.starts_with(type_name, "list.") {
            True ->
              case
                is_measurement_metaobject_type(string.drop_start(type_name, 5))
              {
                True ->
                  parse_measurement_list_json_value(
                    raw,
                    string.drop_start(type_name, 5),
                  )
                False -> parse_json_value(raw)
              }
            False ->
              case is_measurement_metaobject_type(type_name) {
                True -> parse_measurement_json_value(raw)
                False ->
                  case should_parse_metaobject_json_value(type_name) {
                    True -> parse_json_value(raw)
                    False -> MetaobjectString(raw)
                  }
              }
          }
      }
  }
}

fn parse_measurement_json_value(raw: String) -> MetaobjectJsonValue {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      measurement_dynamic_to_metaobject_json(dynamic)
      |> result.unwrap(parse_json_value(raw))
    Error(_) -> MetaobjectString(raw)
  }
}

fn parse_measurement_list_json_value(
  raw: String,
  type_: String,
) -> MetaobjectJsonValue {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      case decode.run(dynamic, decode.list(decode.dynamic)) {
        Ok(items) ->
          items
          |> list.try_map(fn(item) {
            use value <- result.try(measurement_dynamic_to_metaobject_json(item))
            Ok(normalize_measurement_json_unit_for_list(value, type_))
          })
          |> result.map(MetaobjectList)
          |> result.unwrap(parse_json_value(raw))
        Error(_) -> parse_json_value(raw)
      }
    Error(_) -> MetaobjectString(raw)
  }
}

fn normalize_measurement_json_unit_for_list(
  value: MetaobjectJsonValue,
  type_: String,
) -> MetaobjectJsonValue {
  case value {
    MetaobjectObject(fields) ->
      case dict.get(fields, "unit") {
        Ok(MetaobjectString(unit)) ->
          MetaobjectObject(dict.insert(
            fields,
            "unit",
            MetaobjectString(normalize_measurement_list_json_unit(type_, unit)),
          ))
        _ -> value
      }
    _ -> value
  }
}

fn normalize_measurement_list_json_unit(type_: String, unit: String) -> String {
  let normalized = string.lowercase(unit)
  case type_, normalized {
    "dimension", "centimeters" -> "cm"
    "volume", "milliliters" -> "ml"
    "weight", "kilograms" -> "kg"
    _, _ -> normalized
  }
}

fn measurement_dynamic_to_metaobject_json(
  dynamic: Dynamic,
) -> Result(MetaobjectJsonValue, Nil) {
  use value_dynamic <- result.try(
    decode.run(
      dynamic,
      decode.field("value", decode.dynamic, fn(value) { decode.success(value) }),
    )
    |> result.replace_error(Nil),
  )
  use unit <- result.try(
    decode.run(
      dynamic,
      decode.field("unit", decode.string, fn(value) { decode.success(value) }),
    )
    |> result.replace_error(Nil),
  )
  use value <- result.try(dynamic_number_to_metaobject_json(value_dynamic))
  Ok(
    MetaobjectObject(
      dict.from_list([
        #("value", value),
        #("unit", MetaobjectString(unit)),
      ]),
    ),
  )
}

fn dynamic_number_to_metaobject_json(
  dynamic: Dynamic,
) -> Result(MetaobjectJsonValue, Nil) {
  case decode.run(dynamic, decode.int) {
    Ok(value) -> Ok(MetaobjectInt(value))
    Error(_) ->
      case decode.run(dynamic, decode.float) {
        Ok(value) -> Ok(whole_float_to_metaobject_number(value))
        Error(_) ->
          case decode.run(dynamic, decode.string) {
            Ok(value) ->
              case int.parse(value) {
                Ok(parsed) -> Ok(MetaobjectInt(parsed))
                Error(_) ->
                  case float.parse(value) {
                    Ok(parsed) -> Ok(whole_float_to_metaobject_number(parsed))
                    Error(_) -> Error(Nil)
                  }
              }
            Error(_) -> Error(Nil)
          }
      }
  }
}

fn whole_float_to_metaobject_number(value: Float) -> MetaobjectJsonValue {
  let truncated = float.truncate(value)
  case int.to_float(truncated) == value {
    True -> MetaobjectInt(truncated)
    False -> MetaobjectFloat(value)
  }
}

fn parse_rating_json_value(raw: String) -> MetaobjectJsonValue {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      normalize_rating_dynamic(dynamic)
      |> result.unwrap(parse_json_value(raw))
    Error(_) -> MetaobjectString(raw)
  }
}

fn parse_json_value(raw: String) -> MetaobjectJsonValue {
  case json.parse(raw, decode.dynamic) {
    Ok(dynamic) ->
      dynamic_to_metaobject_json(dynamic)
      |> result.unwrap(MetaobjectString(raw))
    Error(_) -> MetaobjectString(raw)
  }
}

fn dynamic_to_metaobject_json(
  value: Dynamic,
) -> Result(MetaobjectJsonValue, Nil) {
  case decode.run(value, decode.bool) {
    Ok(value) -> Ok(MetaobjectBool(value))
    Error(_) -> dynamic_to_metaobject_json_non_bool(value)
  }
}

fn dynamic_to_metaobject_json_non_bool(
  value: Dynamic,
) -> Result(MetaobjectJsonValue, Nil) {
  case decode.run(value, decode.optional(decode.dynamic)) {
    Ok(None) -> Ok(MetaobjectNull)
    _ -> dynamic_to_metaobject_json_present(value)
  }
}

fn dynamic_to_metaobject_json_present(
  value: Dynamic,
) -> Result(MetaobjectJsonValue, Nil) {
  case decode.run(value, decode.int) {
    Ok(n) -> Ok(MetaobjectInt(n))
    Error(_) ->
      case decode.run(value, decode.float) {
        Ok(n) -> Ok(whole_float_to_metaobject_number(n))
        Error(_) ->
          case decode.run(value, decode.string) {
            Ok(s) -> Ok(MetaobjectString(s))
            Error(_) ->
              case decode.run(value, decode.list(decode.dynamic)) {
                Ok(items) ->
                  items
                  |> list.try_map(dynamic_to_metaobject_json)
                  |> result.map(MetaobjectList)
                Error(_) ->
                  case
                    decode.run(
                      value,
                      decode.dict(decode.string, decode.dynamic),
                    )
                  {
                    Ok(fields) ->
                      fields
                      |> dict.to_list
                      |> list.try_map(fn(pair) {
                        use converted <- result.try(dynamic_to_metaobject_json(
                          pair.1,
                        ))
                        Ok(#(pair.0, converted))
                      })
                      |> result.map(fn(entries) {
                        MetaobjectObject(dict.from_list(entries))
                      })
                    Error(_) -> Error(Nil)
                  }
              }
          }
      }
  }
}

fn read_metaobject_reference_ids_from_field(
  field: MetaobjectFieldRecord,
) -> List(String) {
  case field.type_ {
    Some("metaobject_reference") ->
      case field.value {
        Some(id) -> [id]
        None -> []
      }
    Some("list.metaobject_reference") ->
      case field.json_value {
        MetaobjectList(items) ->
          list.filter_map(items, fn(item) {
            case item {
              MetaobjectString(id) -> Ok(id)
              _ -> Error(Nil)
            }
          })
        _ -> []
      }
    _ -> []
  }
}

fn read_bulk_delete_ids(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> List(String) {
  case read_bulk_delete_where(args) {
    BulkDeleteByIds(ids) -> list.take(ids, 250)
    BulkDeleteByType(type_) ->
      list.map(list_effective_metaobjects_by_type(store, type_), fn(item) {
        item.id
      })
      |> list.take(250)
    BulkDeleteNoSelector -> []
  }
}

fn read_bulk_delete_where(
  args: Dict(String, root_field.ResolvedValue),
) -> BulkDeleteWhere {
  case read_object(args, "where") {
    Some(where) ->
      case dict.has_key(where, "ids") {
        True -> BulkDeleteByIds(read_string_list(where, "ids"))
        False ->
          case read_string(where, "type") {
            Some(type_) -> BulkDeleteByType(type_)
            None -> BulkDeleteNoSelector
          }
      }
    None -> BulkDeleteNoSelector
  }
}

fn read_string_list(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(String) {
  case dict.get(input, key) {
    Ok(root_field.ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          root_field.StringVal(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn record_not_found_user_error(field: List(String)) -> UserError {
  UserError(Some(field), "Record not found", "RECORD_NOT_FOUND", None, None)
}

fn build_invalid_json_message(value: String) -> String {
  case string.starts_with(string.trim(value), "{") {
    True -> "Value is invalid JSON."
    False -> "Value is invalid JSON."
  }
}

fn find_field_definition(
  fields: List(MetaobjectFieldDefinitionRecord),
  key: String,
) -> Option(MetaobjectFieldDefinitionRecord) {
  list.find(fields, fn(field) { field.key == key }) |> option.from_result
}

fn replace_field_definition(
  fields: List(MetaobjectFieldDefinitionRecord),
  replacement: MetaobjectFieldDefinitionRecord,
) -> List(MetaobjectFieldDefinitionRecord) {
  list.map(fields, fn(field) {
    case field.key == replacement.key {
      True -> replacement
      False -> field
    }
  })
}

fn append_if(items: List(a), condition: Bool, item: a) -> List(a) {
  case condition {
    True -> list.append(items, [item])
    False -> items
  }
}

fn log_draft(root: String, ids: List(String)) -> LogDraft {
  single_root_log_draft(
    root,
    ids,
    store.Staged,
    domain_name,
    execution_name,
    None,
  )
}

fn option_string_to_list(value: Option(String)) -> List(String) {
  case value {
    Some(item) -> [item]
    None -> []
  }
}

fn enumerate_values(items: List(a)) -> List(#(Int, a)) {
  enumerate_loop(items, 0, [])
}

fn enumerate_loop(
  items: List(a),
  index: Int,
  acc: List(#(Int, a)),
) -> List(#(Int, a)) {
  case items {
    [] -> list.reverse(acc)
    [first, ..rest] -> enumerate_loop(rest, index + 1, [#(index, first), ..acc])
  }
}

fn dedupe_strings(items: List(String)) -> List(String) {
  dedupe_loop(items, dict.new(), [])
}

fn dedupe_loop(
  items: List(String),
  seen: Dict(String, Bool),
  acc: List(String),
) -> List(String) {
  case items {
    [] -> list.reverse(acc)
    [first, ..rest] ->
      case dict.has_key(seen, first) {
        True -> dedupe_loop(rest, seen, acc)
        False ->
          dedupe_loop(rest, dict.insert(seen, first, True), [first, ..acc])
      }
  }
}

fn list_to_set(items: List(String)) -> Dict(String, Bool) {
  list.fold(items, dict.new(), fn(acc, item) { dict.insert(acc, item, True) })
}
