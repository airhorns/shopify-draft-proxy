//// Saved-search runtime implementation for the Gleam port.
////
//// Covers the connection-shaped `*SavedSearches` read pipeline,
//// `savedSearchCreate`, `savedSearchUpdate`, `savedSearchDelete`,
//// query grammar normalization, mutation-log drafts, and the static
//// Shopify default order/draft-order saved searches. Live-hybrid
//// upstream hydration remains outside the current Gleam substrate.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, ConnectionPageInfoOptions, ConnectionWindow,
  SerializeConnectionConfig, SrcList, SrcNull, SrcString, build_synthetic_cursor,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  paginate_connection_items, project_graphql_value, serialize_connection,
  serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome,
  read_optional_string, single_root_log_draft,
}
import shopify_draft_proxy/search_query_parser.{
  parse_search_query_term, search_query_term_value,
  strip_search_query_value_quotes,
}
import shopify_draft_proxy/state/store.{
  type Store, list_effective_saved_searches,
}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type SavedSearchFilter, type SavedSearchRecord, SavedSearchFilter,
  SavedSearchRecord,
}

/// Errors specific to the saved-searches handler. Currently just
/// surfaces upstream parse errors.
pub type SavedSearchesError {
  ParseFailed(root_field.RootFieldError)
}

/// Map from saved-search root field name to the resource type the store
/// keys defaults under. Mirrors `SAVED_SEARCH_ROOT_RESOURCE_TYPES`.
pub fn root_field_resource_type(name: String) -> Result(String, Nil) {
  case name {
    "automaticDiscountSavedSearches" -> Ok("PRICE_RULE")
    "codeDiscountSavedSearches" -> Ok("PRICE_RULE")
    "collectionSavedSearches" -> Ok("COLLECTION")
    "customerSavedSearches" -> Ok("CUSTOMER")
    "discountRedeemCodeSavedSearches" -> Ok("DISCOUNT_REDEEM_CODE")
    "draftOrderSavedSearches" -> Ok("DRAFT_ORDER")
    "fileSavedSearches" -> Ok("FILE")
    "orderSavedSearches" -> Ok("ORDER")
    "productSavedSearches" -> Ok("PRODUCT")
    _ -> Error(Nil)
  }
}

/// Default saved searches for ORDER. Mirrors `ORDER_SAVED_SEARCHES`.
pub fn order_saved_searches() -> List(SavedSearchRecord) {
  [
    SavedSearchRecord(
      id: "gid://shopify/SavedSearch/3634391515442",
      legacy_resource_id: "3634391515442",
      name: "Unfulfilled",
      query: "status:open fulfillment_status:unshipped,partial",
      resource_type: "ORDER",
      search_terms: "",
      filters: [],
      cursor: None,
    ),
    SavedSearchRecord(
      id: "gid://shopify/SavedSearch/3634391548210",
      legacy_resource_id: "3634391548210",
      name: "Unpaid",
      query: "status:open financial_status:unpaid",
      resource_type: "ORDER",
      search_terms: "",
      filters: [],
      cursor: None,
    ),
    SavedSearchRecord(
      id: "gid://shopify/SavedSearch/3634391580978",
      legacy_resource_id: "3634391580978",
      name: "Open",
      query: "status:open",
      resource_type: "ORDER",
      search_terms: "",
      filters: [],
      cursor: None,
    ),
    SavedSearchRecord(
      id: "gid://shopify/SavedSearch/3634391613746",
      legacy_resource_id: "3634391613746",
      name: "Archived",
      query: "status:closed",
      resource_type: "ORDER",
      search_terms: "",
      filters: [],
      cursor: None,
    ),
  ]
}

/// Default saved searches for DRAFT_ORDER. Mirrors
/// `DRAFT_ORDER_SAVED_SEARCHES` from `proxy/orders/shared.ts`.
pub fn draft_order_saved_searches() -> List(SavedSearchRecord) {
  [
    SavedSearchRecord(
      id: "gid://shopify/SavedSearch/3634390597938",
      legacy_resource_id: "3634390597938",
      name: "Open and invoice sent",
      query: "status:open_and_invoice_sent",
      resource_type: "DRAFT_ORDER",
      search_terms: "",
      filters: [],
      cursor: None,
    ),
    SavedSearchRecord(
      id: "gid://shopify/SavedSearch/3634390630706",
      legacy_resource_id: "3634390630706",
      name: "Open",
      query: "status:open",
      resource_type: "DRAFT_ORDER",
      search_terms: "",
      filters: [],
      cursor: None,
    ),
    SavedSearchRecord(
      id: "gid://shopify/SavedSearch/3634390663474",
      legacy_resource_id: "3634390663474",
      name: "Invoice sent",
      query: "status:invoice_sent",
      resource_type: "DRAFT_ORDER",
      search_terms: "",
      filters: [],
      cursor: None,
    ),
    SavedSearchRecord(
      id: "gid://shopify/SavedSearch/3634390696242",
      legacy_resource_id: "3634390696242",
      name: "Completed",
      query: "status:completed",
      resource_type: "DRAFT_ORDER",
      search_terms: "",
      filters: [],
      cursor: None,
    ),
    SavedSearchRecord(
      id: "gid://shopify/SavedSearch/3634390729010",
      legacy_resource_id: "3634390729010",
      name: "Submitted for review",
      query: "status:open source:online_store",
      resource_type: "DRAFT_ORDER",
      search_terms: "",
      filters: [],
      cursor: None,
    ),
  ]
}

fn defaults_for_resource_type(
  resource_type: String,
) -> List(SavedSearchRecord) {
  let raw = case resource_type {
    "ORDER" -> order_saved_searches()
    "DRAFT_ORDER" -> draft_order_saved_searches()
    _ -> []
  }
  list.map(raw, derive_default_saved_search_query_parts)
}

fn derive_default_saved_search_query_parts(
  record: SavedSearchRecord,
) -> SavedSearchRecord {
  let parsed = parse_saved_search_query(record.query)
  SavedSearchRecord(
    ..record,
    query: parsed.canonical_query,
    search_terms: parsed.search_terms,
    filters: parsed.filters,
  )
}

/// Process a saved-searches query document and return a JSON `data`
/// envelope. Mirrors `handleSavedSearchQuery`. The `Store` argument
/// supplies effective (base + staged) records; static defaults are
/// merged in for resource types that have them.
pub fn handle_saved_search_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, SavedSearchesError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = case field {
        Field(name: name, ..) ->
          case root_field_resource_type(name.value) {
            Ok(resource_type) ->
              serialize_saved_search_connection(
                store,
                field,
                resource_type,
                fragments,
                variables,
              )
            Error(_) ->
              // The TS handler skips unknown fields entirely; the
              // proxy is dispatched once per domain so anything else
              // here is unreachable under the current dispatcher. Be
              // safe and emit an empty connection.
              serialize_empty_connection(
                field,
                default_selected_field_options(),
              )
          }
        _ -> json.null()
      }
      #(key, value)
    })
  json.object(entries)
}

fn serialize_saved_search_connection(
  store: Store,
  field: Selection,
  resource_type: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let records = list_saved_searches(store, field, resource_type, variables)
  let window =
    paginate_connection_items(
      records,
      field,
      dict.new(),
      saved_search_cursor_value,
      default_connection_window_options(),
    )
  let ConnectionWindow(
    items: items,
    has_next_page: has_next,
    has_previous_page: has_prev,
  ) = window
  let page_info_options =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      include_inline_fragments: False,
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: items,
      has_next_page: has_next,
      has_previous_page: has_prev,
      get_cursor_value: saved_search_cursor_value,
      serialize_node: fn(record, node_field, _index) {
        project_saved_search(record, node_field, fragments)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: page_info_options,
    ),
  )
}

fn list_saved_searches(
  store: Store,
  field: Selection,
  resource_type: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(SavedSearchRecord) {
  let args = case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
  let query_arg = case dict.get(args, "query") {
    Ok(root_field.StringVal(s)) -> Some(s)
    _ -> None
  }
  let reverse = case dict.get(args, "reverse") {
    Ok(root_field.BoolVal(True)) -> True
    _ -> False
  }
  let local_records = list_effective_saved_searches(store)
  let local_ids = list.map(local_records, fn(record) { record.id })
  let defaults =
    defaults_for_resource_type(resource_type)
    |> list.filter(fn(default) { !list.contains(local_ids, default.id) })
  let filtered =
    list.append(defaults, local_records)
    |> list.filter(fn(record) { record.resource_type == resource_type })
    |> list.filter(fn(record) { matches_query(record, query_arg) })
  case reverse {
    True -> list.reverse(filtered)
    False -> filtered
  }
}

fn matches_query(record: SavedSearchRecord, query: Option(String)) -> Bool {
  case query {
    None -> True
    Some(raw) -> {
      let trimmed = string.trim(raw)
      case trimmed {
        "" -> True
        normalized -> {
          let needle = string.lowercase(normalized)
          let haystacks = [
            record.id,
            record.name,
            record.query,
            record.search_terms,
            record.resource_type,
          ]
          list.any(haystacks, fn(value) {
            string.contains(string.lowercase(value), needle)
          })
        }
      }
    }
  }
}

fn saved_search_cursor_value(record: SavedSearchRecord, _index: Int) -> String {
  record.id
}

fn project_saved_search(
  record: SavedSearchRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source = saved_search_to_source(record)
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(source, selections, fragments)
    _ -> json.object([])
  }
}

fn saved_search_to_source(
  record: SavedSearchRecord,
) -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("SavedSearch")),
    #("id", SrcString(record.id)),
    #("legacyResourceId", SrcString(record.legacy_resource_id)),
    #("name", SrcString(record.name)),
    #("query", SrcString(record.query)),
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
    #("cursor", case record.cursor {
      Some(c) -> SrcString(c)
      None -> SrcNull
    }),
  ])
}

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, SavedSearchesError) {
  use data <- result.try(handle_saved_search_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}

/// Predicate matching the TS `isSavedSearchQueryRoot`. Useful for the
/// dispatcher when checking whether to delegate.
pub fn is_saved_search_query_root(name: String) -> Bool {
  case root_field_resource_type(name) {
    Ok(_) -> True
    Error(_) -> False
  }
}

/// Build the synthetic cursor for a saved-search record. Exposed for
/// tests.
pub fn saved_search_cursor(record: SavedSearchRecord) -> String {
  build_synthetic_cursor(record.id)
}

// ---------------------------------------------------------------------------
// Mutation: savedSearchCreate
// ---------------------------------------------------------------------------

/// Outcome of a saved-search mutation: a JSON `data` envelope plus the
/// updated store and synthetic identity registry. Callers thread these
/// forward.
/// User-error payload emitted on validation failure. Mirrors the
/// `UserError` shape in TS (`field` may be `null`).
pub type UserError {
  UserError(field: Option(List(String)), message: String)
}

/// Predicate matching the three saved-search mutation roots.
pub fn is_saved_search_mutation_root(name: String) -> Bool {
  name == "savedSearchCreate"
  || name == "savedSearchUpdate"
  || name == "savedSearchDelete"
}

/// Process a saved-search mutation document. Currently only
/// `savedSearchCreate` is implemented; other root fields produce a
/// `MutationNotImplemented` error.
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, SavedSearchesError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      handle_mutation_fields(
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
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, SavedSearchesError) {
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
  Ok(
    MutationOutcome(
      ..outcome,
      data: graphql_helpers.wrap_data(json.object(entries)),
    ),
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
  let errors = validate_saved_search_input(input, RequireResourceType(True))
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
        [] -> store.Staged
        _ -> store.Failed
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
    Some(_) -> validate_saved_search_input(input, RequireResourceType(False))
    None -> [
      UserError(
        field: Some(["input", "id"]),
        message: "Saved Search does not exist",
      ),
    ]
  }
  let sanitized_input = case input, existing {
    Some(d), Some(_) -> Some(sanitized_update_input(d, errors))
    _, _ -> None
  }
  let #(record_opt, store_after, identity_after, staged_ids) = case
    sanitized_input,
    existing
  {
    Some(d), Some(existing_record) -> {
      let #(record, identity_after) =
        make_saved_search(identity, d, Some(existing_record))
      let #(_, store_after) = store.upsert_staged_saved_search(store, record)
      #(Some(record), store_after, identity_after, [record.id])
    }
    _, _ -> #(None, store, identity, [])
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
        [] -> store.Staged
        _ -> store.Failed
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
      UserError(
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
  let payload = project_delete_payload(deleted_id, errors, field, fragments)
  let draft =
    single_root_log_draft(
      "savedSearchDelete",
      [],
      case errors {
        [] -> store.Staged
        _ -> store.Failed
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

fn sanitized_update_input(
  input: dict.Dict(String, root_field.ResolvedValue),
  errors: List(UserError),
) -> dict.Dict(String, root_field.ResolvedValue) {
  list.fold(errors, input, fn(acc, error) {
    case error.field {
      Some(parts) ->
        case list.last(parts) {
          Ok("name") -> dict.delete(acc, "name")
          Ok("query") -> dict.delete(acc, "query")
          _ -> acc
        }
      None -> acc
    }
  })
}

fn project_delete_payload(
  deleted_id: Option(String),
  errors: List(UserError),
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
      #("userErrors", user_errors_source),
    ])
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(payload, selections, fragments)
    _ -> json.object([])
  }
}

type RequireResourceType {
  RequireResourceType(Bool)
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
) -> List(UserError) {
  case input {
    None -> [UserError(field: Some(["input"]), message: "Input is required")]
    Some(fields) -> {
      let RequireResourceType(require) = require_resource_type
      let errors = []
      let errors = case dict.has_key(fields, "name") {
        True ->
          case read_optional_string(fields, "name") {
            None ->
              list.append(errors, [
                UserError(
                  field: Some(["input", "name"]),
                  message: "Name can't be blank",
                ),
              ])
            Some(name) ->
              case string.trim(name), string.length(name) {
                "", _ ->
                  list.append(errors, [
                    UserError(
                      field: Some(["input", "name"]),
                      message: "Name can't be blank",
                    ),
                  ])
                _, n if n > 40 ->
                  list.append(errors, [
                    UserError(
                      field: Some(["input", "name"]),
                      message: "Name is too long (maximum is 40 characters)",
                    ),
                  ])
                _, _ -> errors
              }
          }
        False -> errors
      }
      let errors = case dict.has_key(fields, "query") {
        True ->
          case read_optional_string(fields, "query") {
            None ->
              list.append(errors, [
                UserError(
                  field: Some(["input", "query"]),
                  message: "Query can't be blank",
                ),
              ])
            Some(query) ->
              case string.trim(query) {
                "" ->
                  list.append(errors, [
                    UserError(
                      field: Some(["input", "query"]),
                      message: "Query can't be blank",
                    ),
                  ])
                _ -> errors
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

fn validate_resource_type(
  fields: dict.Dict(String, root_field.ResolvedValue),
  errors: List(UserError),
) -> List(UserError) {
  case read_optional_string(fields, "resourceType") {
    None ->
      list.append(errors, [
        UserError(
          field: Some(["input", "resourceType"]),
          message: "Resource type can't be blank",
        ),
      ])
    Some("CUSTOMER") ->
      list.append(errors, [
        UserError(
          field: None,
          message: "Customer saved searches have been deprecated. Use Segmentation API instead.",
        ),
      ])
    Some(rt) ->
      case is_supported_resource_type(rt) {
        True -> errors
        False ->
          list.append(errors, [
            UserError(field: Some(["input", "resourceType"]), message: case rt {
              "URL_REDIRECT" ->
                "URL redirect saved searches require online-store navigation conformance before local support"
              _ ->
                "Resource type is not supported by the local saved search model"
            }),
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
  let parsed = parse_saved_search_query(query)
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
/// structured shape Shopify exposes on `SavedSearch`.
pub type ParsedSavedSearchQuery {
  ParsedSavedSearchQuery(
    filters: List(SavedSearchFilter),
    search_terms: String,
    canonical_query: String,
  )
}

/// Mirrors `parseSavedSearchQuery` in `src/proxy/saved-searches.ts`.
/// Splits a raw query into top-level tokens, classifies each as filter
/// vs search-term, and recomputes the canonical stored shape
/// (`<terms> <filters>`).
pub fn parse_saved_search_query(raw_query: String) -> ParsedSavedSearchQuery {
  let tokens = split_saved_search_top_level_tokens(raw_query)
  let has_boolean_expression =
    list.any(tokens, fn(t) { is_boolean_token(t) || is_grouped_token(t) })
  let #(filters_rev, search_terms_rev) =
    list.fold(tokens, #([], []), fn(acc, token) {
      let #(filters, terms) = acc
      let term = parse_search_query_term(token)
      let take_as_filter = case term.field, term.value {
        Some(_), v if v != "" ->
          is_filter_candidate(token)
          && { term.negated || !has_boolean_expression }
        _, _ -> False
      }
      case take_as_filter, term.field {
        True, Some(field) -> {
          let key = case term.negated {
            True -> field <> "_not"
            False -> field
          }
          let value = filter_value_for_term(token)
          #([SavedSearchFilter(key: key, value: value), ..filters], terms)
        }
        _, _ -> #(filters, [token, ..terms])
      }
    })
  let filters = list.reverse(filters_rev)
  let search_terms_tokens = list.reverse(search_terms_rev)
  let search_terms_text = string.join(search_terms_tokens, " ")
  let stored_search_terms_text =
    search_terms_tokens
    |> list.map(normalize_saved_search_term)
    |> string.join(" ")
  let rendered_filters = list.map(filters, render_saved_search_filter)
  let query_parts = case stored_search_terms_text {
    "" -> rendered_filters
    s -> [s, ..rendered_filters]
  }
  ParsedSavedSearchQuery(
    filters: filters,
    search_terms: normalize_saved_search_quoted_values(search_terms_text),
    canonical_query: string.join(query_parts, " "),
  )
}

fn split_saved_search_top_level_tokens(raw_query: String) -> List(String) {
  let chars = string.to_graphemes(string.trim(raw_query))
  let final_state =
    list.fold(
      chars,
      TokenizerState(current: "", quote: None, depth: 0, tokens: []),
      fn(state, ch) {
        case is_quote(ch) {
          True ->
            case state.quote {
              None ->
                TokenizerState(
                  ..state,
                  current: state.current <> ch,
                  quote: Some(ch),
                )
              Some(open) ->
                case open == ch {
                  True ->
                    TokenizerState(
                      ..state,
                      current: state.current <> ch,
                      quote: None,
                    )
                  False -> TokenizerState(..state, current: state.current <> ch)
                }
            }
          False ->
            case state.quote {
              Some(_) -> TokenizerState(..state, current: state.current <> ch)
              None ->
                case ch {
                  "(" ->
                    TokenizerState(
                      ..state,
                      current: state.current <> ch,
                      depth: state.depth + 1,
                    )
                  ")" ->
                    case state.depth > 0 {
                      True ->
                        TokenizerState(
                          ..state,
                          current: state.current <> ch,
                          depth: state.depth - 1,
                        )
                      False ->
                        TokenizerState(..state, current: state.current <> ch)
                    }
                  _ ->
                    case state.depth == 0 && is_whitespace(ch) {
                      True -> flush_current_token(state)
                      False ->
                        TokenizerState(..state, current: state.current <> ch)
                    }
                }
            }
        }
      },
    )
  let flushed = flush_current_token(final_state)
  list.reverse(flushed.tokens)
}

type TokenizerState {
  TokenizerState(
    current: String,
    quote: Option(String),
    depth: Int,
    tokens: List(String),
  )
}

fn flush_current_token(state: TokenizerState) -> TokenizerState {
  let value = string.trim(state.current)
  case value {
    "" -> TokenizerState(..state, current: "")
    _ -> TokenizerState(..state, current: "", tokens: [value, ..state.tokens])
  }
}

fn is_quote(ch: String) -> Bool {
  ch == "\"" || ch == "'"
}

fn is_whitespace(ch: String) -> Bool {
  ch == " " || ch == "\t" || ch == "\n" || ch == "\r"
}

fn is_grouped_token(token: String) -> Bool {
  string.contains(token, "(") || string.contains(token, ")")
}

fn is_boolean_token(token: String) -> Bool {
  string.uppercase(token) == "OR"
}

fn is_filter_candidate(token: String) -> Bool {
  !is_boolean_token(token) && !is_grouped_token(token)
}

fn filter_value_for_term(token: String) -> String {
  let term = parse_search_query_term(token)
  strip_search_query_value_quotes(search_query_term_value(term))
}

fn render_saved_search_filter(filter: SavedSearchFilter) -> String {
  let negated = string.ends_with(filter.key, "_not")
  let key = case negated {
    True -> string.drop_end(filter.key, 4)
    False -> filter.key
  }
  let value = case contains_whitespace(filter.value) {
    True -> "\"" <> filter.value <> "\""
    False -> filter.value
  }
  let prefix = case negated {
    True -> "-"
    False -> ""
  }
  prefix <> key <> ":" <> value
}

fn contains_whitespace(s: String) -> Bool {
  string.contains(s, " ")
  || string.contains(s, "\t")
  || string.contains(s, "\n")
  || string.contains(s, "\r")
}

fn normalize_saved_search_term(token: String) -> String {
  let normalized = normalize_saved_search_quoted_values(token)
  case
    string.contains(normalized, ":")
    || string.contains(normalized, "\"")
    || is_boolean_token(normalized)
    || is_grouped_token(normalized)
  {
    True -> normalized
    False -> escape_saved_search_term_for_stored_query(normalized)
  }
}

fn escape_saved_search_term_for_stored_query(token: String) -> String {
  string.replace(token, "-", "\\-")
}

fn normalize_saved_search_quoted_values(value: String) -> String {
  let chars = string.to_graphemes(value)
  let final_state =
    list.fold(chars, NormalizeQuotesState(out: "", quote: None), fn(state, ch) {
      case is_quote(ch) {
        True ->
          case state.quote {
            None ->
              NormalizeQuotesState(out: state.out <> "\"", quote: Some(ch))
            Some(open) ->
              case open == ch {
                True ->
                  NormalizeQuotesState(out: state.out <> "\"", quote: None)
                False -> NormalizeQuotesState(..state, out: state.out <> ch)
              }
          }
        False -> NormalizeQuotesState(..state, out: state.out <> ch)
      }
    })
  final_state.out
}

type NormalizeQuotesState {
  NormalizeQuotesState(out: String, quote: Option(String))
}

fn project_create_payload(
  record: Option(SavedSearchRecord),
  input: Option(dict.Dict(String, root_field.ResolvedValue)),
  errors: List(UserError),
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

fn user_error_to_source(error: UserError) -> graphql_helpers.SourceValue {
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
