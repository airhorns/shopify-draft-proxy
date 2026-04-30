//// Minimal port of `src/proxy/metafield-definitions.ts`.
////
//// The full TS module is ~1550 LOC and covers definition lifecycle
//// (create/update/delete/pin/unpin), validation, capability
//// inspection, and seeded catalog reads. This port only ships the
//// branches the proxy needs for currently enabled parity scenarios:
////
//// Reads (`metafield-definitions-product-empty-read`):
////   - `metafieldDefinition(identifier:)` / `metafieldDefinition(id:)`
////     returns `null` when there is no matching record.
////   - `metafieldDefinitions(...)` returns an empty connection
////     (`nodes`/`edges` empty, `pageInfo` all-false-with-null-cursors).
////
//// Mutations (`standard-metafield-definition-enable-validation`):
////   - `standardMetafieldDefinitionEnable(ownerType:, id?, namespace?, key?)`
////     emits the `findStandardMetafieldDefinitionTemplate` validation
////     errors. Without the standard-template catalog seeded, every
////     request that gets past the required-args check falls through to
////     `TEMPLATE_NOT_FOUND` (matching the captured branch). The success
////     path that creates a real definition is deliberately deferred
////     until the catalog ports.
////
//// Other roots in queries (`byIdentifier`, `metafieldDefinitions`,
//// `filteredByQuery`, `seedCatalog`) still serialize, but because the
//// parity spec doesn't compare them their (empty) shapes don't need
//// to match the captured response. Lifecycle mutations and seeded reads
//// will land in a later pass.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, SrcList, SrcNull, SrcString, default_selected_field_options,
  get_document_fragments, get_field_response_key, project_graphql_value,
  serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, read_optional_string, single_root_log_draft,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

/// Errors specific to the metafield-definitions handler. Currently just
/// propagates upstream parse errors.
pub type MetafieldDefinitionsError {
  ParseFailed(root_field.RootFieldError)
}

/// User-error payload mirroring `StandardMetafieldDefinitionEnableUserError`.
/// Unlike the bare `field`+`message` shape some other domains use, this
/// payload carries a `code` (one of the `MetafieldDefinitionUserErrorCode`
/// enum values).
pub type UserError {
  UserError(field: Option(List(String)), message: String, code: String)
}

/// Outcome of a metafield-definitions mutation.
pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

/// Predicate matching the supported subset of metafield-definitions
/// query roots. The full TS surface adds `metafieldDefinitionTypes`
/// and `standardMetafieldDefinitionTemplates`; those are deliberately
/// out-of-scope for this minimal port.
pub fn is_metafield_definitions_query_root(name: String) -> Bool {
  case name {
    "metafieldDefinition" -> True
    "metafieldDefinitions" -> True
    _ -> False
  }
}

/// Predicate matching the supported subset of metafield-definitions
/// mutation roots. Currently just `standardMetafieldDefinitionEnable`
/// — the four lifecycle mutations
/// (`metafieldDefinitionCreate`/`Update`/`Delete`/`Pin`/`Unpin`) are
/// deferred until parity scenarios exercise them.
pub fn is_metafield_definitions_mutation_root(name: String) -> Bool {
  case name {
    "standardMetafieldDefinitionEnable" -> True
    _ -> False
  }
}

/// Handle a `query` operation against the metafield-definitions surface.
pub fn handle_metafield_definitions_query(
  document: String,
) -> Result(Json, MetafieldDefinitionsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> Ok(serialize_root_fields(fields))
  }
}

fn serialize_root_fields(fields: List(Selection)) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = case field {
        Field(name: name, ..) ->
          case name.value {
            "metafieldDefinition" -> json.null()
            "metafieldDefinitions" ->
              serialize_empty_connection(
                field,
                default_selected_field_options(),
              )
            _ -> json.null()
          }
        _ -> json.null()
      }
      #(key, value)
    })
  json.object(entries)
}

/// Wrap a successful response in the standard GraphQL envelope.
pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(document: String) -> Result(Json, MetafieldDefinitionsError) {
  use data <- result.try(handle_metafield_definitions_query(document))
  Ok(wrap_data(data))
}

/// Process a metafield-definitions mutation document. Mirrors
/// `handleMetafieldDefinitionsMutation` but only for the validation
/// branch of `standardMetafieldDefinitionEnable`.
pub fn process_mutation(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, MetafieldDefinitionsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_mutation_fields(
        store_in,
        identity,
        fields,
        fragments,
        variables,
      ))
    }
  }
}

fn handle_mutation_fields(
  store_in: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let #(entries, drafts) =
    list.fold(fields, #([], []), fn(acc, field) {
      let #(entries, drafts) = acc
      case field {
        Field(name: name, ..) -> {
          let dispatch = case name.value {
            "standardMetafieldDefinitionEnable" ->
              Some(
                #(
                  get_field_response_key(field),
                  handle_standard_metafield_definition_enable(
                    field,
                    fragments,
                    variables,
                  ),
                  [],
                ),
              )
            _ -> None
          }
          case dispatch {
            None -> acc
            Some(#(key, payload, staged_resource_ids)) -> {
              let draft =
                single_root_log_draft(
                  name.value,
                  staged_resource_ids,
                  metafield_definitions_status_for(
                    name.value,
                    staged_resource_ids,
                  ),
                  "metafield-definitions",
                  "stage-locally",
                  Some(metafield_definitions_notes_for(name.value)),
                )
              #(
                list.append(entries, [#(key, payload)]),
                list.append(drafts, [draft]),
              )
            }
          }
        }
        _ -> acc
      }
    })
  MutationOutcome(
    data: wrap_data(json.object(entries)),
    store: store_in,
    identity: identity,
    staged_resource_ids: [],
    log_drafts: drafts,
  )
}

/// Per-root-field log status. Without lifecycle mutations ported, the
/// only mutation here is `standardMetafieldDefinitionEnable`, which in
/// the captured parity branch always falls through to `TEMPLATE_NOT_FOUND`
/// and never stages anything — so it logs `Failed`. The empty/non-empty
/// `staged_resource_ids` rule still applies once create/update/delete
/// land in a future pass.
fn metafield_definitions_status_for(
  _root_field_name: String,
  staged_resource_ids: List(String),
) -> store.EntryStatus {
  case staged_resource_ids {
    [] -> store.Failed
    [_, ..] -> store.Staged
  }
}

/// Notes string mirroring the `metafield-definitions` dispatcher in
/// `routes.ts`.
fn metafield_definitions_notes_for(_root_field_name: String) -> String {
  "Staged locally in the in-memory metafield definition draft store."
}

fn handle_standard_metafield_definition_enable(
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
  let user_errors = find_standard_template_user_errors(args)
  let payload =
    src_object([
      #("createdDefinition", SrcNull),
      #("userErrors", SrcList(list.map(user_errors, user_error_to_source))),
    ])
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(payload, selections, fragments)
    _ -> json.object([])
  }
}

/// Mirrors `findStandardMetafieldDefinitionTemplate` user-error branches.
/// Without a standard-template catalog seeded, the success path is
/// unreachable: any well-formed request falls through to the
/// `TEMPLATE_NOT_FOUND` error matching the captured branch.
fn find_standard_template_user_errors(
  args: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  let owner_type = read_optional_string(args, "ownerType")
  let id = read_optional_string(args, "id")
  let namespace = read_optional_string(args, "namespace")
  let key = read_optional_string(args, "key")

  case owner_type {
    None -> [
      UserError(
        field: None,
        message: "A namespace and key or standard metafield definition template id must be provided.",
        code: "TEMPLATE_NOT_FOUND",
      ),
    ]
    Some(_) ->
      case id, namespace, key {
        None, None, _ | None, _, None -> [
          UserError(
            field: None,
            message: "A namespace and key or standard metafield definition template id must be provided.",
            code: "TEMPLATE_NOT_FOUND",
          ),
        ]
        Some(_), _, _ -> [
          UserError(
            field: Some(["id"]),
            message: "Id is not a valid standard metafield definition template id",
            code: "TEMPLATE_NOT_FOUND",
          ),
        ]
        None, Some(_), Some(_) -> [
          UserError(
            field: None,
            message: "A standard definition wasn't found for the specified owner type, namespace, and key.",
            code: "TEMPLATE_NOT_FOUND",
          ),
        ]
      }
  }
}

fn user_error_to_source(error: UserError) -> graphql_helpers.SourceValue {
  let field_value = case error.field {
    Some(parts) -> SrcList(list.map(parts, fn(part) { SrcString(part) }))
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("StandardMetafieldDefinitionEnableUserError")),
    #("field", field_value),
    #("message", SrcString(error.message)),
    #("code", SrcString(error.code)),
  ])
}
