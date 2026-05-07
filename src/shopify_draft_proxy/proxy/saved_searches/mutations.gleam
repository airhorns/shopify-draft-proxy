//// Mutation handling for saved-search roots.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Location, type ObjectField, type Selection, Field, NullValue, ObjectField,
  ObjectValue, SelectionSet,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcList, SrcNull, SrcString,
  get_document_fragments, get_field_response_key, project_graphql_value,
  src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome, find_argument, read_optional_string,
  single_root_log_draft,
}
import shopify_draft_proxy/proxy/saved_searches/queries
import shopify_draft_proxy/proxy/saved_searches/types as saved_search_types
import shopify_draft_proxy/proxy/store_properties
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/search_query_parser.{parse_search_query_term}
import shopify_draft_proxy/state/store.{
  type Store, list_effective_saved_searches,
}
import shopify_draft_proxy/state/store/shared.{dedupe_strings}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type SavedSearchRecord, SavedSearchRecord,
}

const synthetic_shop_id: String = "gid://shopify/Shop/1?shopify-draft-proxy=synthetic"

@internal
pub fn is_saved_search_mutation_root(name: String) -> Bool {
  name == "savedSearchCreate"
  || name == "savedSearchUpdate"
  || name == "savedSearchDelete"
}

/// Process a saved-search mutation document. Currently only
/// `savedSearchCreate` is implemented; other root fields produce a
/// `MutationNotImplemented` error.
@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  _upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let operation_path = get_operation_path_label(document)
      handle_mutation_fields(
        store,
        identity,
        request_path,
        document,
        operation_path,
        fields,
        fragments,
        variables,
      )
    }
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  operation_path: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let validation_errors =
    validate_required_saved_search_input_fields(
      fields,
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] ->
      MutationOutcome(
        data: json.object([
          #("errors", json.preprocessed_array(validation_errors)),
        ]),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        log_drafts: [],
      )
    [] ->
      handle_valid_mutation_fields(
        store,
        identity,
        request_path,
        document,
        fields,
        fragments,
        variables,
      )
  }
}

fn handle_valid_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial =
    MutationOutcome(
      data: json.object([]),
      store: store,
      identity: identity,
      staged_resource_ids: [],
      log_drafts: [],
    )
  let #(entries, outcome) =
    list.fold(fields, #([], initial), fn(acc, field) {
      let #(pairs, current) = acc
      case field {
        Field(name: name, ..) -> {
          case name.value {
            "savedSearchCreate" -> {
              let #(key, payload, next) =
                handle_create(
                  current.store,
                  current.identity,
                  request_path,
                  document,
                  field,
                  fragments,
                  variables,
                )
              let merged =
                MutationOutcome(
                  ..next,
                  log_drafts: list.append(current.log_drafts, next.log_drafts),
                )
              #(list.append(pairs, [#(key, payload)]), merged)
            }
            "savedSearchUpdate" -> {
              let #(key, payload, next) =
                handle_update(
                  current.store,
                  current.identity,
                  request_path,
                  document,
                  field,
                  fragments,
                  variables,
                )
              let merged =
                MutationOutcome(
                  ..next,
                  log_drafts: list.append(current.log_drafts, next.log_drafts),
                )
              #(list.append(pairs, [#(key, payload)]), merged)
            }
            "savedSearchDelete" -> {
              let #(key, payload, next) =
                handle_delete(
                  current.store,
                  current.identity,
                  request_path,
                  document,
                  field,
                  fragments,
                  variables,
                )
              let merged =
                MutationOutcome(
                  ..next,
                  log_drafts: list.append(current.log_drafts, next.log_drafts),
                )
              #(list.append(pairs, [#(key, payload)]), merged)
            }
            _ -> #(pairs, current)
          }
        }
        _ -> #(pairs, current)
      }
    })
  MutationOutcome(
    ..outcome,
    data: graphql_helpers.wrap_data(json.object(entries)),
  )
}

fn get_operation_path_label(document: String) -> String {
  case parse_operation.parse_operation(document) {
    Ok(parsed) -> {
      let kind = case parsed.type_ {
        parse_operation.QueryOperation -> "query"
        parse_operation.MutationOperation -> "mutation"
      }
      case parsed.name {
        Some(name) -> kind <> " " <> name
        None -> kind
      }
    }
    Error(_) -> "mutation"
  }
}

fn validate_required_saved_search_input_fields(
  fields: List(Selection),
  operation_path: String,
  document: String,
) -> List(Json) {
  list.fold(fields, [], fn(errors, field) {
    case field {
      Field(name: name, ..) -> {
        let field_errors =
          required_input_field_errors(
            name.value,
            operation_path,
            document,
            field,
          )
        case field_errors {
          [] -> errors
          _ -> list.append(errors, field_errors)
        }
      }
      _ -> errors
    }
  })
}

fn required_input_field_errors(
  root_name: String,
  operation_path: String,
  document: String,
  field: Selection,
) -> List(Json) {
  let required_fields = case root_name {
    "savedSearchCreate" -> [
      #("resourceType", "SearchResultType!", "SavedSearchCreateInput"),
      #("name", "String!", "SavedSearchCreateInput"),
      #("query", "String!", "SavedSearchCreateInput"),
    ]
    "savedSearchUpdate" -> [#("id", "ID!", "SavedSearchUpdateInput")]
    "savedSearchDelete" -> [#("id", "ID!", "SavedSearchDeleteInput")]
    _ -> []
  }
  case required_fields {
    [] -> []
    _ ->
      case inline_input_object(field) {
        None -> []
        Some(input_object) -> {
          let InlineInputObject(fields: input_fields, loc: loc) = input_object
          list.filter_map(required_fields, fn(required) {
            let #(field_name, expected_type, input_object_type) = required
            case find_object_field(input_fields, field_name) {
              None ->
                Ok(build_missing_required_input_field_error(
                  root_name,
                  field_name,
                  expected_type,
                  input_object_type,
                  operation_path,
                  loc,
                  document,
                ))
              Some(ObjectField(value: NullValue(..), ..)) ->
                Ok(build_null_required_input_field_error(
                  root_name,
                  field_name,
                  expected_type,
                  input_object_type,
                  operation_path,
                  loc,
                  document,
                ))
              Some(_) -> Error(Nil)
            }
          })
        }
      }
  }
}

type InlineInputObject {
  InlineInputObject(fields: List(ObjectField), loc: Option(Location))
}

fn inline_input_object(field: Selection) -> Option(InlineInputObject) {
  let arguments = case field {
    Field(arguments: args, ..) -> args
    _ -> []
  }
  case find_argument(arguments, "input") {
    Some(argument) ->
      case argument.value {
        ObjectValue(fields: fields, loc: loc) ->
          Some(InlineInputObject(fields: fields, loc: loc))
        _ -> None
      }
    None -> None
  }
}

fn find_object_field(
  fields: List(ObjectField),
  name: String,
) -> Option(ObjectField) {
  case fields {
    [] -> None
    [first, ..rest] -> {
      let ObjectField(name: field_name, ..) = first
      case field_name.value == name {
        True -> Some(first)
        False -> find_object_field(rest, name)
      }
    }
  }
}

fn build_missing_required_input_field_error(
  root_name: String,
  field_name: String,
  expected_type: String,
  input_object_type: String,
  operation_path: String,
  loc: Option(Location),
  document: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Argument '"
        <> field_name
        <> "' on InputObject '"
        <> input_object_type
        <> "' is required. Expected type "
        <> expected_type,
      ),
    ),
  ]
  let with_locations = case loc {
    Some(location) ->
      list.append(base, [
        #("locations", graphql_helpers.locations_json(location, document)),
      ])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array(
          [operation_path, root_name, "input", field_name],
          json.string,
        ),
      ),
      #(
        "extensions",
        json.object([
          #("code", json.string("missingRequiredInputObjectAttribute")),
          #("argumentName", json.string(field_name)),
          #("argumentType", json.string(expected_type)),
          #("inputObjectType", json.string(input_object_type)),
        ]),
      ),
    ]),
  )
}

fn build_null_required_input_field_error(
  root_name: String,
  field_name: String,
  expected_type: String,
  input_object_type: String,
  operation_path: String,
  loc: Option(Location),
  document: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Argument '"
        <> field_name
        <> "' on InputObject '"
        <> input_object_type
        <> "' has an invalid value (null). Expected type '"
        <> expected_type
        <> "'.",
      ),
    ),
  ]
  let with_locations = case loc {
    Some(location) ->
      list.append(base, [
        #("locations", graphql_helpers.locations_json(location, document)),
      ])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "path",
        json.array(
          [operation_path, root_name, "input", field_name],
          json.string,
        ),
      ),
      #(
        "extensions",
        json.object([
          #("code", json.string("argumentLiteralsIncompatible")),
          #("typeName", json.string("InputObject")),
          #("argumentName", json.string(field_name)),
        ]),
      ),
    ]),
  )
}

fn handle_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let args = case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
  let input = read_input(args)
  let errors =
    validate_saved_search_input(input, RequireResourceType(True))
    |> validate_saved_search_query_grammar(input, None, CreateValidation)
    |> validate_reserved_saved_search_name(store, input, None)
    |> validate_unique_saved_search_name(store, input, None)
  let #(record_opt, store_after, identity_after, staged_ids) = case
    input,
    errors
  {
    Some(input_dict), [] -> {
      let #(record, identity_after) =
        make_saved_search(identity, input_dict, None)
      let #(_, store_after) = store.upsert_staged_saved_search(store, record)
      #(Some(record), store_after, identity_after, [record.id])
    }
    _, _ -> #(None, store, identity, [])
  }
  let payload =
    project_create_payload(record_opt, input, errors, field, fragments)
  let draft =
    single_root_log_draft(
      "savedSearchCreate",
      staged_ids,
      case errors {
        [] -> store_types.Staged
        _ -> store_types.Failed
      },
      "saved-searches",
      "stage-locally",
      Some("Locally staged savedSearchCreate in shopify-draft-proxy."),
    )
  let outcome =
    MutationOutcome(
      data: json.object([]),
      store: store_after,
      identity: identity_after,
      staged_resource_ids: staged_ids,
      log_drafts: [draft],
    )
  #(key, payload, outcome)
}

fn handle_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let args = case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
  let input = read_input(args)
  let id_from_input = case input {
    Some(d) -> read_optional_string(d, "id")
    None -> None
  }
  let existing = case id_from_input {
    Some(id) -> store.get_effective_saved_search_by_id(store, id)
    None -> None
  }
  let errors = case existing {
    Some(record) ->
      validate_saved_search_input(input, RequireResourceType(False))
      |> validate_saved_search_query_grammar(
        input,
        Some(record.resource_type),
        UpdateValidation,
      )
      |> validate_reserved_saved_search_name(store, input, Some(record.id))
      |> validate_unique_saved_search_name(store, input, Some(record.id))
    None -> [
      saved_search_types.UserError(
        field: Some(["input", "id"]),
        message: "Saved Search does not exist",
      ),
    ]
  }
  let sanitized_input = case input, existing {
    Some(d), Some(_) -> Some(sanitized_update_input(d, errors))
    _, _ -> None
  }
  let keep_payload_without_staging =
    has_duplicate_name_error(errors) || has_query_grammar_error(errors)
  let #(record_opt, store_after, identity_after, staged_ids) = case
    sanitized_input,
    existing,
    keep_payload_without_staging
  {
    Some(d), Some(existing_record), True -> {
      let #(record, identity_after) =
        make_saved_search(identity, d, Some(existing_record))
      #(Some(record), store, identity_after, [])
    }
    Some(d), Some(existing_record), False -> {
      let #(record, identity_after) =
        make_saved_search(identity, d, Some(existing_record))
      let #(_, store_after) = store.upsert_staged_saved_search(store, record)
      #(Some(record), store_after, identity_after, [record.id])
    }
    _, _, _ -> #(None, store, identity, [])
  }
  let payload_record = case record_opt {
    Some(_) -> record_opt
    None -> existing
  }
  let projection_input = case record_opt {
    Some(_) -> sanitized_input
    None -> None
  }
  let payload =
    project_create_payload(
      payload_record,
      projection_input,
      errors,
      field,
      fragments,
    )
  let draft =
    single_root_log_draft(
      "savedSearchUpdate",
      staged_ids,
      case errors {
        [] -> store_types.Staged
        _ -> store_types.Failed
      },
      "saved-searches",
      "stage-locally",
      Some("Locally staged savedSearchUpdate in shopify-draft-proxy."),
    )
  let outcome =
    MutationOutcome(
      data: json.object([]),
      store: store_after,
      identity: identity_after,
      staged_resource_ids: staged_ids,
      log_drafts: [draft],
    )
  #(key, payload, outcome)
}

fn handle_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json, MutationOutcome) {
  let key = get_field_response_key(field)
  let args = case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
  let input = read_input(args)
  let id_from_input = case input {
    Some(d) -> read_optional_string(d, "id")
    None -> None
  }
  let existing = case id_from_input {
    Some(id) -> store.get_effective_saved_search_by_id(store, id)
    None -> None
  }
  let errors = case existing {
    Some(_) -> []
    None -> [
      saved_search_types.UserError(
        field: Some(["input", "id"]),
        message: "Saved Search does not exist",
      ),
    ]
  }
  let store_after = case id_from_input, existing {
    Some(id), Some(_) -> store.delete_staged_saved_search(store, id)
    _, _ -> store
  }
  let deleted_id = case errors {
    [] -> id_from_input
    _ -> None
  }
  let payload =
    project_delete_payload(store_after, deleted_id, errors, field, fragments)
  let draft =
    single_root_log_draft(
      "savedSearchDelete",
      [],
      case errors {
        [] -> store_types.Staged
        _ -> store_types.Failed
      },
      "saved-searches",
      "stage-locally",
      Some("Locally staged savedSearchDelete in shopify-draft-proxy."),
    )
  let outcome =
    MutationOutcome(
      data: json.object([]),
      store: store_after,
      identity: identity,
      staged_resource_ids: [],
      log_drafts: [draft],
    )
  #(key, payload, outcome)
}

fn validate_unique_saved_search_name(
  errors: List(saved_search_types.UserError),
  store: Store,
  input: Option(dict.Dict(String, root_field.ResolvedValue)),
  excluded_id: Option(String),
) -> List(saved_search_types.UserError) {
  case errors, input {
    [], Some(fields) ->
      case read_optional_string(fields, "name") {
        Some(name) ->
          case read_uniqueness_resource_type(store, fields, excluded_id) {
            Some(resource_type) ->
              case
                saved_search_name_taken(store, resource_type, name, excluded_id)
              {
                True -> [duplicate_name_error()]
                False -> []
              }
            None -> []
          }
        None -> []
      }
    _, _ -> errors
  }
}

fn validate_reserved_saved_search_name(
  errors: List(saved_search_types.UserError),
  store: Store,
  input: Option(dict.Dict(String, root_field.ResolvedValue)),
  excluded_id: Option(String),
) -> List(saved_search_types.UserError) {
  case errors, input {
    [], Some(fields) ->
      case read_optional_string(fields, "name") {
        Some(name) ->
          case read_uniqueness_resource_type(store, fields, excluded_id) {
            Some(resource_type) ->
              case reserved_saved_search_name(resource_type, name) {
                True -> [duplicate_name_error()]
                False -> []
              }
            None -> []
          }
        None -> []
      }
    _, _ -> errors
  }
}

fn read_uniqueness_resource_type(
  store: Store,
  fields: dict.Dict(String, root_field.ResolvedValue),
  excluded_id: Option(String),
) -> Option(String) {
  case read_optional_string(fields, "resourceType") {
    Some(resource_type) -> Some(resource_type)
    None ->
      case excluded_id {
        Some(id) ->
          case store.get_effective_saved_search_by_id(store, id) {
            Some(record) -> Some(record.resource_type)
            None -> None
          }
        None -> None
      }
  }
}

fn saved_search_name_taken(
  store: Store,
  resource_type: String,
  name: String,
  excluded_id: Option(String),
) -> Bool {
  let local_records = list_effective_saved_searches(store)
  let records =
    list.append(
      queries.defaults_for_resource_type(resource_type),
      local_records,
    )
  list.any(records, fn(record) {
    record.resource_type == resource_type
    && record.name == name
    && case excluded_id {
      Some(id) -> record.id != id
      None -> True
    }
  })
}

fn reserved_saved_search_name(resource_type: String, name: String) -> Bool {
  let normalized_name = string.lowercase(name)
  reserved_names_for_resource_type(resource_type)
  |> list.any(fn(reserved_name) {
    string.lowercase(reserved_name) == normalized_name
  })
}

fn reserved_names_for_resource_type(resource_type: String) -> List(String) {
  case resource_type {
    "PRODUCT" -> ["All products"]
    "COLLECTION" -> ["All collections"]
    "ORDER" -> ["All"]
    "DRAFT_ORDER" -> ["All Drafts"]
    "FILE" -> ["All Files"]
    "CUSTOMER" -> ["All Customers"]
    _ -> []
  }
}

fn duplicate_name_error() -> saved_search_types.UserError {
  saved_search_types.UserError(
    field: Some(["input", "name"]),
    message: "Name has already been taken",
  )
}

fn has_duplicate_name_error(
  errors: List(saved_search_types.UserError),
) -> Bool {
  list.any(errors, fn(error) {
    error.field == Some(["input", "name"])
    && error.message == "Name has already been taken"
  })
}

fn has_query_grammar_error(errors: List(saved_search_types.UserError)) -> Bool {
  list.any(errors, fn(error) {
    case error.field {
      Some(["input", "query"]) | Some(["input", "searchTerms"]) ->
        string.starts_with(error.message, "Search terms is invalid")
        || string.starts_with(error.message, "Query has incompatible filters")
        || string.starts_with(error.message, "Query is invalid,")
        || error.message == "Query is invalid"
      _ -> False
    }
  })
}

fn sanitized_update_input(
  input: dict.Dict(String, root_field.ResolvedValue),
  errors: List(saved_search_types.UserError),
) -> dict.Dict(String, root_field.ResolvedValue) {
  list.fold(errors, input, fn(acc, error) {
    case error.field {
      Some(parts) ->
        case list.last(parts) {
          Ok("name") -> dict.delete(acc, "name")
          _ -> acc
        }
      None -> acc
    }
  })
}

fn project_delete_payload(
  store: Store,
  deleted_id: Option(String),
  errors: List(saved_search_types.UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let id_source = case deleted_id {
    Some(s) -> SrcString(s)
    None -> SrcNull
  }
  let user_errors_source = SrcList(list.map(errors, user_error_to_source))
  let payload =
    src_object([
      #("deletedSavedSearchId", id_source),
      #("shop", current_shop_source(store)),
      #("userErrors", user_errors_source),
    ])
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(payload, selections, fragments)
    _ -> json.object([])
  }
}

fn current_shop_source(store: Store) -> SourceValue {
  case store.get_effective_shop(store) {
    Some(shop) -> store_properties.shop_source(shop)
    None -> synthetic_shop_source()
  }
}

fn synthetic_shop_source() -> SourceValue {
  src_object([
    #("__typename", SrcString("Shop")),
    #("id", SrcString(synthetic_shop_id)),
    #("name", SrcString("Shopify Draft Proxy")),
    #("myshopifyDomain", SrcString("shopify-draft-proxy.myshopify.com")),
  ])
}

type RequireResourceType {
  RequireResourceType(Bool)
}

type SavedSearchValidationOperation {
  CreateValidation
  UpdateValidation
}

fn read_input(
  args: dict.Dict(String, root_field.ResolvedValue),
) -> Option(dict.Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, "input") {
    Ok(root_field.ObjectVal(fields)) -> Some(fields)
    _ -> None
  }
}

// `read_optional_string` is now imported from `proxy/mutation_helpers`
// (Pass 14 lift).

fn validate_saved_search_input(
  input: Option(dict.Dict(String, root_field.ResolvedValue)),
  require_resource_type: RequireResourceType,
) -> List(saved_search_types.UserError) {
  case input {
    None -> [
      saved_search_types.UserError(
        field: Some(["input"]),
        message: "Input is required",
      ),
    ]
    Some(fields) -> {
      let RequireResourceType(require) = require_resource_type
      let errors = []
      let errors = case dict.has_key(fields, "name") {
        True ->
          case read_optional_string(fields, "name") {
            None ->
              list.append(errors, [
                saved_search_types.UserError(
                  field: Some(["input", "name"]),
                  message: "Name can't be blank",
                ),
              ])
            Some(name) ->
              case string.trim(name), string.length(name) {
                "", _ ->
                  list.append(errors, [
                    saved_search_types.UserError(
                      field: Some(["input", "name"]),
                      message: "Name can't be blank",
                    ),
                  ])
                _, n if n > 40 ->
                  list.append(errors, [
                    saved_search_types.UserError(
                      field: Some(["input", "name"]),
                      message: "Name is too long (maximum is 40 characters)",
                    ),
                  ])
                _, _ -> errors
              }
          }
        False -> errors
      }
      case require {
        True -> validate_resource_type(fields, errors)
        False -> errors
      }
    }
  }
}

fn validate_saved_search_query_grammar(
  errors: List(saved_search_types.UserError),
  input: Option(dict.Dict(String, root_field.ResolvedValue)),
  existing_resource_type: Option(String),
  operation: SavedSearchValidationOperation,
) -> List(saved_search_types.UserError) {
  case errors, input {
    [], Some(fields) ->
      case read_optional_string(fields, "query") {
        Some(query) -> {
          let resource_type = case
            read_optional_string(fields, "resourceType")
          {
            Some(rt) -> Some(rt)
            None -> existing_resource_type
          }
          case resource_type {
            Some(rt) -> validate_query_for_resource_type(query, rt, operation)
            None -> []
          }
        }
        None -> []
      }
    _, _ -> errors
  }
}

fn validate_query_for_resource_type(
  query: String,
  resource_type: String,
  operation: SavedSearchValidationOperation,
) -> List(saved_search_types.UserError) {
  let keys = saved_search_query_filter_keys(query)
  case first_reserved_filter(resource_type, keys) {
    Some(filter) -> [reserved_filter_error(operation, filter)]
    None ->
      case unknown_filter_errors(resource_type, keys, operation) {
        [_, ..] as errors -> errors
        [] ->
          case product_incompatible_filter_pair(resource_type, keys) {
            Some(pair) -> {
              let #(left, right) = pair
              [
                saved_search_types.UserError(
                  field: Some(["input", "query"]),
                  message: "Query has incompatible filters: "
                    <> left
                    <> ", "
                    <> right,
                ),
              ]
            }
            None -> []
          }
      }
  }
}

fn saved_search_query_filter_keys(query: String) -> List(String) {
  saved_search_types.split_saved_search_top_level_tokens(query)
  |> list.filter_map(fn(token) {
    case is_top_level_filter_token(token) {
      True -> {
        let term = parse_search_query_term(token)
        case term.field {
          Some(field) if field != "" -> Ok(field)
          _ -> Error(Nil)
        }
      }
      False -> Error(Nil)
    }
  })
}

fn is_top_level_filter_token(token: String) -> Bool {
  let normalized = string.trim(token)
  normalized != ""
  && string.uppercase(normalized) != "OR"
  && !string.contains(normalized, "(")
  && !string.contains(normalized, ")")
}

fn first_reserved_filter(
  resource_type: String,
  keys: List(String),
) -> Option(String) {
  case
    reserved_filters_for_resource_type(resource_type)
    |> list.find(fn(filter) { list.contains(keys, filter) })
  {
    Ok(filter) -> Some(filter)
    Error(_) -> None
  }
}

fn reserved_filters_for_resource_type(resource_type: String) -> List(String) {
  case resource_type {
    "ORDER" -> ["reference_location_id"]
    _ -> []
  }
}

fn reserved_filter_error(
  operation: SavedSearchValidationOperation,
  filter: String,
) -> saved_search_types.UserError {
  let field = case operation {
    CreateValidation -> ["input", "query"]
    UpdateValidation -> ["input", "searchTerms"]
  }
  saved_search_types.UserError(
    field: Some(field),
    message: "Search terms is invalid, '"
      <> filter
      <> "' is a reserved filter name",
  )
}

fn unknown_filter_errors(
  resource_type: String,
  keys: List(String),
  operation: SavedSearchValidationOperation,
) -> List(saved_search_types.UserError) {
  case valid_filter_fields_for_resource_type(resource_type) {
    Some(valid_fields) ->
      keys
      |> dedupe_strings
      |> list.filter(fn(key) {
        !list.contains(valid_fields, string.lowercase(key))
      })
      |> list.sort(by: string.compare)
      |> list.map(fn(key) { unknown_filter_error(operation, key) })
    None -> []
  }
}

fn unknown_filter_error(
  operation: SavedSearchValidationOperation,
  filter: String,
) -> saved_search_types.UserError {
  let field = case operation {
    CreateValidation -> ["input", "query"]
    UpdateValidation -> ["input", "searchTerms"]
  }
  saved_search_types.UserError(
    field: Some(field),
    message: "Query is invalid, '" <> filter <> "' is not a valid filter",
  )
}

fn valid_filter_fields_for_resource_type(
  resource_type: String,
) -> Option(List(String)) {
  case resource_type {
    "PRODUCT" ->
      Some([
        "collection_id", "created_at", "error_feedback", "handle", "id",
        "inventory_total", "product_type", "published_at", "published_status",
        "sku", "status", "tag", "title", "updated_at", "vendor",
      ])
    "COLLECTION" ->
      Some([
        "collection_type", "handle", "id", "product_id",
        "product_publication_status", "publishable_status", "published_at",
        "published_status", "title", "updated_at",
      ])
    "ORDER" ->
      Some([
        "channel_id", "created_at", "customer_id", "email", "financial_status",
        "fulfillment_status", "id", "location_id", "name", "processed_at",
        "sales_channel", "status", "tag", "test", "updated_at",
      ])
    "DRAFT_ORDER" ->
      Some([
        "created_at", "customer_id", "email", "id", "name", "status", "tag",
        "updated_at",
      ])
    "FILE" ->
      Some([
        "created_at", "filename", "id", "media_type", "original_source",
        "status", "updated_at",
      ])
    "DISCOUNT_REDEEM_CODE" ->
      Some(["code", "created_at", "discount_id", "id", "status", "updated_at"])
    _ -> None
  }
}

fn product_incompatible_filter_pair(
  resource_type: String,
  keys: List(String),
) -> Option(#(String, String)) {
  case resource_type, list.contains(keys, "collection_id") {
    "PRODUCT", True -> {
      let incompatible = ["tag", "error_feedback", "published_status"]
      case list.find(incompatible, fn(key) { list.contains(keys, key) }) {
        Ok(key) -> Some(#("collection_id", key))
        Error(_) -> None
      }
    }
    _, _ -> None
  }
}

fn validate_resource_type(
  fields: dict.Dict(String, root_field.ResolvedValue),
  errors: List(saved_search_types.UserError),
) -> List(saved_search_types.UserError) {
  case read_optional_string(fields, "resourceType") {
    None ->
      list.append(errors, [
        saved_search_types.UserError(
          field: Some(["input", "resourceType"]),
          message: "Resource type can't be blank",
        ),
      ])
    Some("CUSTOMER") ->
      list.append(errors, [
        saved_search_types.UserError(
          field: None,
          message: "Customer saved searches have been deprecated. Use Segmentation API instead.",
        ),
      ])
    Some(rt) ->
      case is_supported_resource_type(rt) {
        True -> errors
        False ->
          list.append(errors, [
            saved_search_types.UserError(
              field: Some(["input", "resourceType"]),
              message: case rt {
                "URL_REDIRECT" ->
                  "URL redirect saved searches require online-store navigation conformance before local support"
                _ ->
                  "Resource type is not supported by the local saved search model"
              },
            ),
          ])
      }
  }
}

fn is_supported_resource_type(value: String) -> Bool {
  case value {
    "PRICE_RULE"
    | "COLLECTION"
    | "CUSTOMER"
    | "DISCOUNT_REDEEM_CODE"
    | "DRAFT_ORDER"
    | "FILE"
    | "ORDER"
    | "PRODUCT" -> True
    _ -> False
  }
}

fn make_saved_search(
  identity: SyntheticIdentityRegistry,
  input: dict.Dict(String, root_field.ResolvedValue),
  existing: Option(SavedSearchRecord),
) -> #(SavedSearchRecord, SyntheticIdentityRegistry) {
  let #(id, identity_after) = case existing {
    Some(record) -> #(record.id, identity)
    None -> synthetic_identity.make_proxy_synthetic_gid(identity, "SavedSearch")
  }
  let name = case read_optional_string(input, "name") {
    Some(s) -> s
    None ->
      case existing {
        Some(record) -> record.name
        None -> ""
      }
  }
  let query = case read_optional_string(input, "query") {
    Some(s) -> s
    None ->
      case existing {
        Some(record) -> record.query
        None -> ""
      }
  }
  let resource_type = case existing {
    Some(record) -> record.resource_type
    None ->
      case read_optional_string(input, "resourceType") {
        Some(s) -> s
        None -> ""
      }
  }
  let legacy_resource_id = case existing {
    Some(record) -> record.legacy_resource_id
    None -> read_legacy_resource_id(id)
  }
  let cursor = case existing {
    Some(record) -> record.cursor
    None -> None
  }
  let parsed = saved_search_types.parse_saved_search_query(query)
  let record =
    SavedSearchRecord(
      id: id,
      legacy_resource_id: legacy_resource_id,
      name: name,
      query: parsed.canonical_query,
      resource_type: resource_type,
      search_terms: parsed.search_terms,
      filters: parsed.filters,
      cursor: cursor,
    )
  #(record, identity_after)
}

/// Strip the synthetic-identity query suffix from a gid and return the
/// trailing numeric segment. Mirrors `readLegacyResourceId` in TS.
fn read_legacy_resource_id(id: String) -> String {
  let without_query = case string.split(id, "?") {
    [head, ..] -> head
    [] -> id
  }
  case list.last(string.split(without_query, "/")) {
    Ok(part) -> part
    Error(_) -> id
  }
}

/// Result of splitting a raw saved-search query string into the
fn project_create_payload(
  record: Option(SavedSearchRecord),
  input: Option(dict.Dict(String, root_field.ResolvedValue)),
  errors: List(saved_search_types.UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let saved_search_source = case record {
    Some(r) -> mutation_record_source(r, input)
    None -> SrcNull
  }
  let user_errors_source = SrcList(list.map(errors, user_error_to_source))
  let payload =
    src_object([
      #("savedSearch", saved_search_source),
      #("userErrors", user_errors_source),
    ])
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(payload, selections, fragments)
    _ -> json.object([])
  }
}

fn user_error_to_source(
  error: saved_search_types.UserError,
) -> graphql_helpers.SourceValue {
  let field_value = case error.field {
    Some(parts) -> SrcList(list.map(parts, fn(part) { SrcString(part) }))
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", field_value),
    #("message", SrcString(error.message)),
  ])
}

fn mutation_record_source(
  record: SavedSearchRecord,
  input: Option(dict.Dict(String, root_field.ResolvedValue)),
) -> graphql_helpers.SourceValue {
  // The TS handler echoes the *input* `query` rather than the stored
  // (re-rendered) query so callers see exactly what they sent. We
  // already store the input verbatim in this pass, so the values
  // coincide; preserve the override for fidelity once the search-query
  // parser ports.
  let effective_query = case input {
    Some(d) ->
      case read_optional_string(d, "query") {
        Some(s) -> s
        None -> record.query
      }
    None -> record.query
  }
  src_object([
    #("__typename", SrcString("SavedSearch")),
    #("id", SrcString(record.id)),
    #("legacyResourceId", SrcString(record.legacy_resource_id)),
    #("name", SrcString(record.name)),
    #("query", SrcString(effective_query)),
    #("resourceType", SrcString(record.resource_type)),
    #("searchTerms", SrcString(record.search_terms)),
    #(
      "filters",
      SrcList(
        list.map(record.filters, fn(f) {
          src_object([
            #("__typename", SrcString("SavedSearchFilter")),
            #("key", SrcString(f.key)),
            #("value", SrcString(f.value)),
          ])
        }),
      ),
    ),
  ])
}
