//// Mutation handling for metaobject definitions and metaobjects.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Location, type ObjectField, type Selection, Argument, Field, ObjectField,
  ObjectValue, SelectionSet,
}
import shopify_draft_proxy/graphql/location as graphql_location
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/graphql/source as graphql_source
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcList, SrcNull, SrcObject,
  SrcString, get_document_fragments, get_field_response_key, src_object,
}
import shopify_draft_proxy/proxy/metaobject_definitions/serializers.{
  metaobject_definition_source, project_selection, project_source_field,
  serialize_metaobject_mutation_selection,
}
import shopify_draft_proxy/proxy/metaobject_definitions/types as metaobject_definition_types
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type MutationOutcome, MutationOutcome,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{
  type Store, delete_staged_metaobject, delete_staged_metaobject_definition,
  find_effective_metaobject_by_handle,
  find_effective_metaobject_definition_by_type, get_effective_metaobject_by_id,
  get_effective_metaobject_definition_by_id,
  list_effective_metaobject_definitions, list_effective_metaobjects_by_type,
  upsert_base_metaobject_definitions, upsert_base_metaobjects,
  upsert_staged_metaobject, upsert_staged_metaobject_definition,
  upsert_staged_url_redirect,
}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type MetaobjectCapabilitiesRecord, type MetaobjectDefinitionCapabilitiesRecord,
  type MetaobjectDefinitionCapabilityRecord, type MetaobjectDefinitionRecord,
  type MetaobjectDefinitionTypeRecord,
  type MetaobjectFieldDefinitionCapabilitiesRecord,
  type MetaobjectFieldDefinitionRecord,
  type MetaobjectFieldDefinitionReferenceRecord,
  type MetaobjectFieldDefinitionValidationRecord, type MetaobjectFieldRecord,
  type MetaobjectJsonValue, type MetaobjectOnlineStoreCapabilityRecord,
  type MetaobjectPublishableCapabilityRecord, type MetaobjectRecord,
  type MetaobjectStandardTemplateRecord, MetaobjectBool,
  MetaobjectCapabilitiesRecord, MetaobjectDefinitionCapabilitiesRecord,
  MetaobjectDefinitionCapabilityRecord, MetaobjectDefinitionRecord,
  MetaobjectDefinitionTypeRecord, MetaobjectFieldDefinitionCapabilitiesRecord,
  MetaobjectFieldDefinitionRecord, MetaobjectFieldDefinitionReferenceRecord,
  MetaobjectFieldDefinitionValidationRecord, MetaobjectFieldRecord,
  MetaobjectInt, MetaobjectList, MetaobjectNull, MetaobjectObject,
  MetaobjectOnlineStoreCapabilityRecord, MetaobjectPublishableCapabilityRecord,
  MetaobjectRecord, MetaobjectStandardTemplateRecord, MetaobjectString,
  UrlRedirectRecord,
}

@internal
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

@internal
pub type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
    log_drafts: List(LogDraft),
  )
}

@internal
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

@internal
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
            case metaobject_definition_types.read_object(args, "where") {
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
  let input = metaobject_definition_types.read_object_arg(args, "definition")
  let validation_errors =
    metaobject_definition_types.build_create_definition_validation_errors(
      input,
      requesting_api_client_id,
    )
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
      metaobject_definition_types.build_create_definition_uniqueness_errors(
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
        metaobject_definition_types.build_definition_from_create_input(
          identity,
          input,
          requesting_api_client_id,
        )
      let #(staged, next_store) =
        upsert_staged_metaobject_definition(store, definition)
      let payload = definition_payload(field, fragments, Some(staged), [])
      #(
        MutationFieldResult(key, payload, [staged.id], [], [
          metaobject_definition_types.log_draft("metaobjectDefinitionCreate", [
            staged.id,
          ]),
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
  let id = metaobject_definition_types.read_string(args, "id")
  let input = metaobject_definition_types.read_object_arg(args, "definition")
  case id {
    None -> {
      let payload =
        definition_payload(field, fragments, None, [
          metaobject_definition_types.record_not_found_user_error(["id"]),
        ])
      #(MutationFieldResult(key, payload, [], [], []), store, identity)
    }
    Some(definition_id) ->
      case get_effective_metaobject_definition_by_id(store, definition_id) {
        None -> {
          let payload =
            definition_payload(field, fragments, None, [
              metaobject_definition_types.record_not_found_user_error(["id"]),
            ])
          #(MutationFieldResult(key, payload, [], [], []), store, identity)
        }
        Some(existing) -> {
          let reset_field_order =
            metaobject_definition_types.read_bool_arg(
              field,
              variables,
              "resetFieldOrder",
            )
            || option.unwrap(
              metaobject_definition_types.read_bool(input, "resetFieldOrder"),
              False,
            )
          let #(updated, next_identity, user_errors) =
            metaobject_definition_types.apply_definition_update(
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
                  metaobject_definition_types.log_draft(
                    "metaobjectDefinitionUpdate",
                    [staged.id],
                  ),
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
  let id =
    metaobject_definition_types.read_string(
      graphql_helpers.field_args(field, variables),
      "id",
    )
  case id {
    None -> #(
      definition_delete_result(key, field, None, [
        metaobject_definition_types.record_not_found_user_error(["id"]),
      ]),
      store,
      identity,
    )
    Some(definition_id) ->
      case get_effective_metaobject_definition_by_id(store, definition_id) {
        None -> #(
          definition_delete_result(key, field, None, [
            metaobject_definition_types.record_not_found_user_error(["id"]),
          ]),
          store,
          identity,
        )
        Some(definition) -> {
          case
            metaobject_definition_types.build_definition_delete_guard_user_errors(
              definition,
            )
          {
            [_, ..] as user_errors -> #(
              definition_delete_result(key, field, None, user_errors),
              store,
              identity,
            )
            [] -> {
              let cascaded_metaobject_ids =
                list_effective_metaobjects_by_type(store, definition.type_)
                |> list.map(fn(metaobject) { metaobject.id })
              let store_after_entries =
                list.fold(
                  cascaded_metaobject_ids,
                  store,
                  fn(acc, metaobject_id) {
                    delete_staged_metaobject(acc, metaobject_id)
                  },
                )
              let next_store =
                delete_staged_metaobject_definition(
                  store_after_entries,
                  definition_id,
                )
              let staged_ids =
                list.append([definition_id], cascaded_metaobject_ids)
              #(
                definition_delete_result_with_staged_ids(
                  key,
                  field,
                  Some(definition_id),
                  [],
                  staged_ids,
                ),
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
  case metaobject_definition_types.read_string(args, "type") {
    None -> {
      let payload =
        definition_payload(field, fragments, None, [
          standard_template_record_not_found_error(),
        ])
      #(MutationFieldResult(key, payload, [], [], []), store, identity)
    }
    Some(type_) ->
      case metaobject_definition_types.standard_template(type_) {
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
                metaobject_definition_types.build_standard_definition(
                  identity,
                  template,
                )
              let #(staged, next_store) =
                upsert_staged_metaobject_definition(store, definition)
              let payload =
                definition_payload(field, fragments, Some(staged), [])
              #(
                MutationFieldResult(key, payload, [staged.id], [], [
                  metaobject_definition_types.log_draft(
                    "standardMetaobjectDefinitionEnable",
                    [staged.id],
                  ),
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

fn standard_template_record_not_found_error() -> metaobject_definition_types.UserError {
  metaobject_definition_types.UserError(
    Some(["type"]),
    "Record not found",
    "RECORD_NOT_FOUND",
    None,
    None,
  )
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
    metaobject_definition_types.read_object_arg(
      graphql_helpers.field_args(field, variables),
      "metaobject",
    )
  let type_ = metaobject_definition_types.read_string(input, "type")
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
      metaobject_definition_types.find_effective_metaobject_definition_by_input_type(
        store,
        t,
        requesting_api_client_id,
      )
    None -> None
  }
  let user_errors =
    metaobject_definition_types.build_create_metaobject_user_errors(
      type_,
      definition,
    )
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
        metaobject_definition_types.build_metaobject_from_create_input(
          store,
          identity,
          input,
          defn,
        )
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
            metaobject_definition_types.adjust_definition_count(
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
              [
                metaobject_definition_types.log_draft("metaobjectCreate", [
                  staged.id,
                ]),
              ],
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
  let input = metaobject_definition_types.read_object_arg(args, "metaobject")
  let redirect_new_handle =
    metaobject_definition_types.read_bool(input, "redirectNewHandle")
    |> option.or(metaobject_definition_types.read_bool(
      args,
      "redirectNewHandle",
    ))
    |> option.unwrap(False)
  case metaobject_definition_types.read_string(args, "id") {
    None -> #(
      MutationFieldResult(
        key,
        metaobject_payload(store, field, fragments, None, [
          metaobject_definition_types.record_not_found_user_error(["id"]),
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
              metaobject_definition_types.record_not_found_user_error(["id"]),
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
                metaobject_definition_types.UserError(
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
                metaobject_definition_types.apply_metaobject_update_input(
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
                  let #(final_store, final_identity, staged_ids) =
                    maybe_stage_metaobject_handle_redirect(
                      next_store,
                      next_identity,
                      definition,
                      existing,
                      staged,
                      redirect_new_handle,
                    )
                  #(
                    MutationFieldResult(
                      key,
                      metaobject_payload(
                        final_store,
                        field,
                        fragments,
                        Some(staged),
                        [],
                      ),
                      staged_ids,
                      [],
                      [
                        metaobject_definition_types.log_draft(
                          "metaobjectUpdate",
                          staged_ids,
                        ),
                      ],
                    ),
                    final_store,
                    final_identity,
                  )
                }
              }
            }
          }
      }
    }
  }
}

fn maybe_stage_metaobject_handle_redirect(
  store: Store,
  identity: SyntheticIdentityRegistry,
  definition: MetaobjectDefinitionRecord,
  before: MetaobjectRecord,
  after: MetaobjectRecord,
  redirect_new_handle: Bool,
) -> #(Store, SyntheticIdentityRegistry, List(String)) {
  let base_ids = [after.id]
  case redirect_new_handle && before.handle != after.handle {
    False -> #(store, identity, base_ids)
    True ->
      case
        metaobject_storefront_path(definition, before),
        metaobject_storefront_path(definition, after)
      {
        Some(path), Some(target) -> {
          let #(id, after_id) =
            synthetic_identity.make_proxy_synthetic_gid(identity, "UrlRedirect")
          let #(now, next_identity) =
            synthetic_identity.make_synthetic_timestamp(after_id)
          let redirect =
            UrlRedirectRecord(
              id: id,
              path: path,
              target: target,
              cursor: None,
              created_at: Some(now),
              updated_at: Some(now),
            )
          let #(staged, next_store) =
            upsert_staged_url_redirect(store, redirect)
          #(next_store, next_identity, [after.id, staged.id])
        }
        _, _ -> #(store, identity, base_ids)
      }
  }
}

fn metaobject_storefront_path(
  definition: MetaobjectDefinitionRecord,
  metaobject: MetaobjectRecord,
) -> Option(String) {
  case
    definition_capability_enabled(definition.capabilities.online_store),
    definition_capability_enabled(definition.capabilities.renderable),
    metaobject.capabilities.online_store,
    definition.online_store_url_handle |> option.or(definition.display_name_key)
  {
    True, True, Some(_), Some(url_handle) ->
      Some("/pages/" <> url_handle <> "/" <> metaobject.handle)
    _, _, _, _ -> None
  }
}

fn definition_capability_enabled(
  capability: Option(MetaobjectDefinitionCapabilityRecord),
) -> Bool {
  case capability {
    Some(MetaobjectDefinitionCapabilityRecord(enabled: True)) -> True
    _ -> False
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
  let #(type_, handle) =
    metaobject_definition_types.read_handle_value(
      metaobject_definition_types.read_object_arg(args, "handle"),
    )
  let input = read_upsert_payload_input(args)
  let handle_errors = case handle {
    Some(h) ->
      case string.trim(h) {
        "" -> []
        _ ->
          metaobject_definition_types.validate_explicit_metaobject_handle_with_field(
            h,
            ["handle", "handle"],
            False,
          )
      }
    None -> []
  }
  case type_, handle, handle_errors {
    Some(_), Some(_), [_, ..] -> {
      #(
        MutationFieldResult(
          key,
          metaobject_payload(store, field, fragments, None, handle_errors),
          [],
          [],
          [],
        ),
        store,
        identity,
      )
    }
    Some(t), Some(h), [] -> {
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
        metaobject_definition_types.find_effective_metaobject_definition_by_input_type(
          store,
          t,
          requesting_api_client_id,
        )
      {
        None -> {
          let err =
            metaobject_definition_types.UserError(
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
                  metaobject_definition_types.read_string(input, "handle")
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
                metaobject_definition_types.apply_metaobject_update_input(
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
                          [
                            metaobject_definition_types.log_draft(
                              "metaobjectUpsert",
                              [staged.id],
                            ),
                          ],
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
              let limit_errors =
                metaobject_definition_types.append_metaobjects_per_type_user_errors(
                  [],
                  Some(definition),
                )
              case limit_errors {
                [_, ..] -> #(
                  MutationFieldResult(
                    key,
                    metaobject_payload(
                      store,
                      field,
                      fragments,
                      None,
                      limit_errors,
                    ),
                    [],
                    [],
                    [],
                  ),
                  store,
                  identity,
                )
                [] -> {
                  let create_input =
                    dict.insert(input, "type", root_field.StringVal(t))
                  let create_input =
                    dict.insert(create_input, "handle", root_field.StringVal(h))
                  let #(created, next_identity, field_errors) =
                    metaobject_definition_types.build_metaobject_from_create_input(
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
                        metaobject_definition_types.adjust_definition_count(
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
                          [
                            metaobject_definition_types.log_draft(
                              "metaobjectUpsert",
                              [staged.id],
                            ),
                          ],
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
      }
    }
    _, _, _ -> {
      let err =
        metaobject_definition_types.UserError(
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
  case metaobject_definition_types.read_object(args, "metaobject") {
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

fn partition_upsert_user_errors(
  errors: List(metaobject_definition_types.UserError),
) -> List(metaobject_definition_types.UserError) {
  list.map(errors, fn(error) {
    let metaobject_definition_types.UserError(
      field,
      message,
      code,
      element_key,
      element_index,
    ) = error
    metaobject_definition_types.UserError(
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
  let id =
    metaobject_definition_types.read_string(
      graphql_helpers.field_args(field, variables),
      "id",
    )
  case id {
    None -> #(
      metaobject_delete_result(key, field, None, [
        metaobject_definition_types.record_not_found_user_error(["id"]),
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
            metaobject_definition_types.record_not_found_user_error(["id"]),
          ]),
          store,
          identity,
        )
        Some(metaobject) -> {
          let staged_store = delete_staged_metaobject(store, metaobject_id)
          let #(next_store, next_identity) =
            metaobject_definition_types.adjust_definition_count(
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
  case metaobject_definition_types.read_bulk_delete_where(args) {
    metaobject_definition_types.BulkDeleteByType(type_) ->
      case find_effective_metaobject_definition_by_type(store, type_) {
        None -> {
          let err =
            metaobject_definition_types.UserError(
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
          let ids =
            metaobject_definition_types.read_bulk_delete_ids(store, args)
          bulk_delete_selected_ids(store, identity, key, field, fragments, ids)
        }
      }
    metaobject_definition_types.BulkDeleteByIds(ids) ->
      bulk_delete_selected_ids(
        store,
        identity,
        key,
        field,
        fragments,
        list.take(ids, 250),
      )
    metaobject_definition_types.BulkDeleteNoSelector ->
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
      let job =
        metaobject_definition_types.BulkDeleteJob(id: job_id, done: False)
      #(
        MutationFieldResult(
          key,
          bulk_delete_payload(field, fragments, Some(job), []),
          [job_id],
          [],
          [
            metaobject_definition_types.log_draft("metaobjectBulkDelete", [
              job_id,
            ]),
          ],
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
      let job =
        metaobject_definition_types.BulkDeleteJob(id: job_id, done: True)
      #(
        MutationFieldResult(
          key,
          bulk_delete_payload(field, fragments, Some(job), user_errors),
          list.append([job_id], deleted_ids),
          [],
          [
            metaobject_definition_types.log_draft(
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
) -> #(
  Store,
  List(metaobject_definition_types.UserError),
  List(String),
  SyntheticIdentityRegistry,
) {
  list.fold(ids, #(store, [], [], identity), fn(acc, id) {
    let #(current_store, errors, deleted_ids, current_identity) = acc
    case get_effective_metaobject_by_id(current_store, id) {
      None -> {
        let err =
          metaobject_definition_types.UserError(
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
          metaobject_definition_types.adjust_definition_count(
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
        metaobject_definition_types.normalize_definition_type(
          metaobject_type,
          requesting_api_client_id,
        )
      case
        metaobject_definition_types.find_effective_metaobject_definition_by_normalized_type(
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
  case metaobject_definition_types.read_string(input, "type") {
    None -> store
    Some(raw_type) -> {
      let normalized_type =
        metaobject_definition_types.normalize_definition_type(
          raw_type,
          requesting_api_client_id,
        )
      case
        raw_type == normalized_type
        || metaobject_definition_types.is_app_reserved_definition_type_input(
          raw_type,
        )
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
  let explicit_ids = case
    metaobject_definition_types.read_object(args, "where")
  {
    Some(where) ->
      metaobject_definition_types.read_string_list(where, "ids")
      |> list.take(250)
    None -> []
  }
  case explicit_ids {
    [_, ..] ->
      list.fold(explicit_ids, store, fn(current, id) {
        maybe_hydrate_metaobject_by_id(current, Some(id), upstream)
      })
    [] ->
      case metaobject_definition_types.read_object(args, "where") {
        Some(where) ->
          case metaobject_definition_types.read_string(where, "type") {
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
    online_store_url_handle: definition_online_store_url_handle_from_json(
      json_get(value, "capabilities"),
    ),
    has_thumbnail_field: json_get_bool(value, "hasThumbnailField"),
    metaobjects_count: json_get_int(value, "metaobjectsCount"),
    standard_template: standard_template_from_json(json_get(
      value,
      "standardTemplate",
    )),
    standard_template_id: json_get_string(value, "standardTemplateId"),
    standard_template_dependent_on_app: json_get_bool(
      value,
      "standardTemplateDependentOnApp",
    )
      |> option.unwrap(False),
    app_config_managed: json_get_bool(value, "appConfigManaged")
      |> option.unwrap(False),
    linked_metafields: [],
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
    capabilities: field_definition_capabilities_from_json(json_get(
      value,
      "capabilities",
    )),
    validations: json_array(json_get(value, "validations"))
      |> list.filter_map(validation_from_json),
  ))
}

fn field_definition_capabilities_from_json(
  value: Option(commit.JsonValue),
) -> MetaobjectFieldDefinitionCapabilitiesRecord {
  MetaobjectFieldDefinitionCapabilitiesRecord(
    admin_filterable: definition_capability_from_json(value, "adminFilterable"),
  )
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
    _ -> metaobject_definition_types.default_definition_access()
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

fn definition_online_store_url_handle_from_json(
  value: Option(commit.JsonValue),
) -> Option(String) {
  case value {
    None -> None
    Some(object) ->
      case json_get(object, "onlineStore") {
        Some(online_store) ->
          case json_get(online_store, "data") {
            Some(data) -> json_get_string(data, "urlHandle")
            None -> None
          }
        None -> None
      }
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
        Some(type_name) ->
          metaobject_definition_types.read_metaobject_json_value(
            type_name,
            raw_value,
          )
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
    commit.JsonFloat(value) ->
      metaobject_definition_types.whole_float_to_metaobject_number(value)
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
  "query MetaobjectDefinitionHydrateByType($type: String!) { metaobjectDefinitionByType(type: $type) { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } capabilities { adminFilterable { enabled } } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } }"
}

fn metaobject_hydrate_query() -> String {
  "query MetaobjectHydrateById($id: ID!) { metaobject(id: $id) { id handle type displayName createdAt updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } definition { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } capabilities { adminFilterable { enabled } } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } } }"
}

fn metaobject_hydrate_by_handle_query() -> String {
  "query MetaobjectHydrateByHandle($type: String!, $handle: String!) { metaobjectByHandle(handle: { type: $type, handle: $handle }) { id handle type displayName createdAt updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } definition { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } capabilities { adminFilterable { enabled } } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } } }"
}

fn metaobject_bulk_delete_hydrate_query() -> String {
  "query MetaobjectBulkDeleteHydrateByType($type: String!) { catalog: metaobjects(type: $type, first: 250) { nodes { id handle type displayName createdAt updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } } } definition: metaobjectDefinitionByType(type: $type) { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } capabilities { adminFilterable { enabled } } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } }"
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

fn definition_payload(
  field: Selection,
  fragments: FragmentMap,
  definition: Option(MetaobjectDefinitionRecord),
  user_errors: List(metaobject_definition_types.UserError),
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
  user_errors: List(metaobject_definition_types.UserError),
) -> Json {
  let source_fields =
    dict.from_list([
      #("userErrors", SrcList(list.map(user_errors, user_error_source))),
    ])
  let source = SrcObject(source_fields)
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      json.object(
        list.map(selections, fn(selection) {
          let key = get_field_response_key(selection)
          case selection {
            Field(name: name, ..) if name.value == "metaobject" ->
              case metaobject {
                Some(record) -> #(
                  key,
                  serialize_metaobject_mutation_selection(
                    store,
                    record,
                    selection,
                    fragments,
                  ),
                )
                None -> #(key, json.null())
              }
            _ -> project_source_field(source_fields, selection, fragments)
          }
        }),
      )
    _ -> project_selection(source, field, fragments)
  }
}

fn definition_delete_result(
  key: String,
  field: Selection,
  deleted_id: Option(String),
  user_errors: List(metaobject_definition_types.UserError),
) -> MutationFieldResult {
  definition_delete_result_with_staged_ids(
    key,
    field,
    deleted_id,
    user_errors,
    metaobject_definition_types.option_string_to_list(deleted_id),
  )
}

fn definition_delete_result_with_staged_ids(
  key: String,
  field: Selection,
  deleted_id: Option(String),
  user_errors: List(metaobject_definition_types.UserError),
  staged_resource_ids: List(String),
) -> MutationFieldResult {
  let source =
    src_object([
      #("deletedId", graphql_helpers.option_string_source(deleted_id)),
      #("userErrors", SrcList(list.map(user_errors, user_error_source))),
    ])
  MutationFieldResult(
    key,
    project_selection(source, field, dict.new()),
    staged_resource_ids,
    [],
    case deleted_id {
      Some(_) -> [
        metaobject_definition_types.log_draft(
          "metaobjectDefinitionDelete",
          staged_resource_ids,
        ),
      ]
      None -> []
    },
  )
}

fn metaobject_delete_result(
  key: String,
  field: Selection,
  deleted_id: Option(String),
  user_errors: List(metaobject_definition_types.UserError),
) -> MutationFieldResult {
  let source =
    src_object([
      #("deletedId", graphql_helpers.option_string_source(deleted_id)),
      #("userErrors", SrcList(list.map(user_errors, user_error_source))),
    ])
  MutationFieldResult(
    key,
    project_selection(source, field, dict.new()),
    metaobject_definition_types.option_string_to_list(deleted_id),
    [],
    case deleted_id {
      Some(id) -> [
        metaobject_definition_types.log_draft("metaobjectDelete", [id]),
      ]
      None -> []
    },
  )
}

fn bulk_delete_payload(
  field: Selection,
  fragments: FragmentMap,
  job: Option(metaobject_definition_types.BulkDeleteJob),
  user_errors: List(metaobject_definition_types.UserError),
) -> Json {
  let source =
    src_object([
      #("job", case job {
        Some(metaobject_definition_types.BulkDeleteJob(id:, done:)) ->
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

fn user_error_source(
  error: metaobject_definition_types.UserError,
) -> SourceValue {
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
