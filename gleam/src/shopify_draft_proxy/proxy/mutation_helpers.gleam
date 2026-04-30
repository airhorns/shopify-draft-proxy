//// Helpers shared across mutation handlers.
////
//// Pass 13 introduced AST-level argument validation in
//// `proxy/webhooks.gleam` to mirror the structured top-level GraphQL
//// error envelope TS emits (`extensions.code` =
//// `missingRequiredArguments` / `argumentLiteralsIncompatible` /
//// `INVALID_VARIABLE`). Pass 14 lifts the validator + its three error
//// builders + the resolved-value readers here so future domain
//// handlers don't have to copy them.
////
//// What's here:
//// - `RequiredArgument` and `validate_required_field_arguments` —
////   the generic AST validator. Mirrors `validateRequiredFieldArguments`
////   in `src/proxy/webhooks.ts`.
//// - `validate_required_id_argument` — single-`id`-arg variant used
////   by `*Delete` mutations whose only top-level requirement is `id`.
//// - `build_missing_required_argument_error` /
////   `build_null_argument_error` / `build_missing_variable_error` —
////   the three error builders, directly reusable.
//// - `read_optional_string` / `read_optional_string_array` —
////   resolved-arg readers that ignore non-matching variants. Both
////   `webhooks` and `saved_searches` use these.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Argument, type Location, type Selection, Argument, Field, NullValue,
  VariableValue,
}
import shopify_draft_proxy/graphql/location as graphql_location
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/graphql/source as graphql_source
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

/// One required top-level argument on a mutation field. `name` is the
/// argument name as it appears in the schema; `expected_type` is the
/// type string used in error messages (e.g. `"WebhookSubscriptionTopic!"`).
pub type RequiredArgument {
  RequiredArgument(name: String, expected_type: String)
}

/// Validate the AST-level arguments on a mutation field. Returns one
/// JSON error object per problem; an empty list means "all good".
///
/// Mirrors TS `validateRequiredFieldArguments`. The split between
/// "validate against AST" and "execute against resolved arg dict" is
/// intentional and necessary: only the AST distinguishes
/// `omitted` / `literal null` / `unbound variable`, each of which maps
/// to a distinct GraphQL error code.
///
/// `operation_path` is the operation path label
/// (e.g. `"mutation"` / `"mutation Foo"`) — formed from the parsed
/// operation upstream and threaded down here.
pub fn validate_required_field_arguments(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  operation_name: String,
  required_arguments: List(RequiredArgument),
  operation_path: String,
  source_body: String,
) -> List(Json) {
  let arguments = case field {
    Field(arguments: args, ..) -> args
    _ -> []
  }
  let field_loc = field_location(field)
  let #(missing_names, errors) =
    list.fold(required_arguments, #([], []), fn(acc, required) {
      let #(missing, errs) = acc
      case find_argument(arguments, required.name) {
        None -> #(list.append(missing, [required.name]), errs)
        Some(argument) ->
          case argument.value {
            NullValue(..) -> #(
              missing,
              list.append(errs, [
                build_null_argument_error(
                  operation_name,
                  required.name,
                  required.expected_type,
                  operation_path,
                  field_loc,
                  source_body,
                ),
              ]),
            )
            VariableValue(variable: var) ->
              case dict.get(variables, var.name.value) {
                Ok(root_field.NullVal) | Error(_) -> #(
                  missing,
                  list.append(errs, [
                    build_missing_variable_error(
                      var.name.value,
                      required.expected_type,
                    ),
                  ]),
                )
                _ -> #(missing, errs)
              }
            _ -> #(missing, errs)
          }
      }
    })
  case missing_names {
    [] -> errors
    _ -> [
      build_missing_required_argument_error(
        operation_name,
        string.join(missing_names, ", "),
        operation_path,
        field_loc,
        source_body,
      ),
      ..errors
    ]
  }
}

/// Validate the single `id` argument on a `*Delete` mutation field
/// and return the resolved id alongside any top-level errors.
/// Mirrors the per-mutation pattern in `*Delete` TS handlers, where
/// the validator both surfaces structured errors *and* hands the
/// caller the resolved id when validation passed.
///
/// On a literal string id with no errors, returns `#(Some(id), [])`.
/// On any validation failure or unresolvable variable, returns
/// `#(None, [errors...])`.
pub fn validate_required_id_argument(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  operation_name: String,
  operation_path: String,
  source_body: String,
) -> #(Option(String), List(Json)) {
  let arguments = case field {
    Field(arguments: args, ..) -> args
    _ -> []
  }
  let field_loc = field_location(field)
  case find_argument(arguments, "id") {
    None -> #(None, [
      build_missing_required_argument_error(
        operation_name,
        "id",
        operation_path,
        field_loc,
        source_body,
      ),
    ])
    Some(argument) ->
      case argument.value {
        NullValue(..) -> #(None, [
          build_null_argument_error(
            operation_name,
            "id",
            "ID!",
            operation_path,
            field_loc,
            source_body,
          ),
        ])
        VariableValue(variable: var) ->
          case dict.get(variables, var.name.value) {
            Ok(root_field.NullVal) | Error(_) -> #(None, [
              build_missing_variable_error(var.name.value, "ID!"),
            ])
            Ok(root_field.StringVal(s)) -> #(Some(s), [])
            _ -> #(None, [])
          }
        _ -> {
          // Literal value or coercible — fall back to the resolved-arg
          // dict to pick up the string.
          let args = case root_field.get_field_arguments(field, variables) {
            Ok(d) -> d
            Error(_) -> dict.new()
          }
          let id = case dict.get(args, "id") {
            Ok(root_field.StringVal(s)) -> Some(s)
            _ -> None
          }
          #(id, [])
        }
      }
  }
}

/// Look up a named argument in a list. Public so domain handlers can
/// inspect specific arguments after validation has passed.
pub fn find_argument(
  arguments: List(Argument),
  name: String,
) -> Option(Argument) {
  case arguments {
    [] -> None
    [first, ..rest] ->
      case first {
        Argument(name: arg_name, ..) ->
          case arg_name.value == name {
            True -> Some(first)
            False -> find_argument(rest, name)
          }
      }
  }
}

/// Build the structured error for one or more missing required
/// arguments. `argument_names_joined` is comma-separated per TS.
pub fn build_missing_required_argument_error(
  operation_name: String,
  argument_names_joined: String,
  operation_path: String,
  field_loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Field '"
        <> operation_name
        <> "' is missing required arguments: "
        <> argument_names_joined,
      ),
    ),
  ]
  let with_locations = case locations_payload(field_loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #("path", json.array([operation_path, operation_name], json.string)),
      #(
        "extensions",
        json.object([
          #("code", json.string("missingRequiredArguments")),
          #("className", json.string("Field")),
          #("name", json.string(operation_name)),
          #("arguments", json.string(argument_names_joined)),
        ]),
      ),
    ]),
  )
}

/// Build the structured error for an argument bound to a literal
/// `null` AST node.
pub fn build_null_argument_error(
  operation_name: String,
  argument_name: String,
  expected_type: String,
  operation_path: String,
  field_loc: Option(Location),
  source_body: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Argument '"
        <> argument_name
        <> "' on Field '"
        <> operation_name
        <> "' has an invalid value (null). Expected type '"
        <> expected_type
        <> "'.",
      ),
    ),
  ]
  let with_locations = case locations_payload(field_loc, source_body) {
    Some(locs) -> list.append(base, [#("locations", locs)])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array([operation_path, operation_name, argument_name], json.string),
      ),
      #(
        "extensions",
        json.object([
          #("code", json.string("argumentLiteralsIncompatible")),
          #("typeName", json.string("Field")),
          #("argumentName", json.string(argument_name)),
        ]),
      ),
    ]),
  )
}

/// Build the structured error for an argument bound to a variable
/// that resolved to `null` or wasn't supplied.
pub fn build_missing_variable_error(
  variable_name: String,
  variable_type: String,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Variable $"
        <> variable_name
        <> " of type "
        <> variable_type
        <> " was provided invalid value",
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("INVALID_VARIABLE")),
        #("value", json.null()),
        #(
          "problems",
          json.preprocessed_array([
            json.object([
              #("path", json.preprocessed_array([])),
              #("explanation", json.string("Expected value to not be null")),
            ]),
          ]),
        ),
      ]),
    ),
  ])
}

/// Read an optional string from a resolved-arg dict. Returns `None`
/// if the key is absent or bound to a non-string value.
pub fn read_optional_string(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case dict.get(input, key) {
    Ok(root_field.StringVal(s)) -> Some(s)
    _ -> None
  }
}

/// Read an optional `[String]` array from a resolved-arg dict.
/// Non-string list elements are dropped silently to mirror the TS
/// `filter`-then-`map` pattern. Returns `None` for absent or
/// non-list values.
pub fn read_optional_string_array(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(List(String)) {
  case dict.get(input, key) {
    Ok(root_field.ListVal(items)) ->
      Some(
        list.filter_map(items, fn(value) {
          case value {
            root_field.StringVal(s) -> Ok(s)
            _ -> Error(Nil)
          }
        }),
      )
    _ -> None
  }
}

fn field_location(field: Selection) -> Option(Location) {
  case field {
    Field(loc: loc, ..) -> loc
    _ -> None
  }
}

fn locations_payload(
  field_loc: Option(Location),
  source_body: String,
) -> Option(Json) {
  case field_loc {
    None -> None
    Some(loc) -> {
      let source = graphql_source.new(source_body)
      let computed = graphql_location.get_location(source, position: loc.start)
      Some(
        json.preprocessed_array([
          json.object([
            #("line", json.int(computed.line)),
            #("column", json.int(computed.column)),
          ]),
        ]),
      )
    }
  }
}

// ---------------------------------------------------------------------------
// Mutation log drafts
// ---------------------------------------------------------------------------

/// Per-domain description of a mutation log entry that needs to be
/// recorded. The dispatcher (`draft_proxy.route_mutation`) is the only
/// site that actually mints the synthetic id/timestamp and calls
/// `store.record_mutation_log_entry`. Domain `process_mutation`
/// implementations return one `LogDraft` per logical entry they want
/// in the buffer (typically one per root mutation field).
///
/// Centralising the record call here means a domain that forgets to
/// build a draft has its mutation invisible — but the four-domain
/// regression that prompted this design (gift_cards / localization /
/// metafield_definitions / segments shipping no log entries) cannot
/// recur without a structurally-empty `MutationOutcome.log_drafts`,
/// which is much easier to spot in code review and in domain unit
/// tests than a missing per-handler `record_mutation_log_entry` call.
pub type LogDraft {
  LogDraft(
    operation_name: Option(String),
    root_fields: List(String),
    primary_root_field: Option(String),
    domain: String,
    execution: String,
    staged_resource_ids: List(String),
    status: store.EntryStatus,
    notes: Option(String),
  )
}

/// Build a `LogDraft` for a single-root-field mutation. Mirrors the
/// shape that webhooks/apps/saved_searches/functions all
/// historically produced inline. `domain` and `execution` are the
/// `Capability` fields the entry should record; they're domain
/// constants like `"webhooks"` / `"stage-locally"`.
pub fn single_root_log_draft(
  root_field: String,
  staged_resource_ids: List(String),
  status: store.EntryStatus,
  domain: String,
  execution: String,
  notes: Option(String),
) -> LogDraft {
  LogDraft(
    operation_name: Some(root_field),
    root_fields: [root_field],
    primary_root_field: Some(root_field),
    domain: domain,
    execution: execution,
    staged_resource_ids: staged_resource_ids,
    status: status,
    notes: notes,
  )
}

/// Record each draft into the store, threading the synthetic-identity
/// registry through `make_synthetic_gid` + `make_synthetic_timestamp`
/// for the entry id and `received_at`. Returns the updated store and
/// identity registry.
pub fn record_log_drafts(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  drafts: List(LogDraft),
) -> #(Store, SyntheticIdentityRegistry) {
  list.fold(drafts, #(store, identity), fn(acc, draft) {
    let #(current_store, current_identity) = acc
    let #(log_id, identity_after_id) =
      synthetic_identity.make_synthetic_gid(current_identity, "MutationLogEntry")
    let #(received_at, identity_after_ts) =
      synthetic_identity.make_synthetic_timestamp(identity_after_id)
    let entry =
      store.MutationLogEntry(
        id: log_id,
        received_at: received_at,
        operation_name: draft.operation_name,
        path: request_path,
        query: document,
        variables: dict.new(),
        staged_resource_ids: draft.staged_resource_ids,
        status: draft.status,
        interpreted: store.InterpretedMetadata(
          operation_type: store.Mutation,
          operation_name: draft.operation_name,
          root_fields: draft.root_fields,
          primary_root_field: draft.primary_root_field,
          capability: store.Capability(
            operation_name: draft.operation_name,
            domain: draft.domain,
            execution: draft.execution,
          ),
        ),
        notes: draft.notes,
      )
    #(store.record_mutation_log_entry(current_store, entry), identity_after_ts)
  })
}
