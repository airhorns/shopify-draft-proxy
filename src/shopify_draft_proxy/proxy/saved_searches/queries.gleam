//// Query handling for saved-search roots.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, ConnectionPageInfoOptions, ConnectionWindow,
  SerializeConnectionConfig, SrcList, SrcNull, SrcString, build_synthetic_cursor,
  default_connection_page_info_options, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  paginate_connection_items, project_graphql_value, serialize_connection,
  serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{respond_to_query}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/saved_searches/types as saved_search_types
import shopify_draft_proxy/state/store.{
  type Store, list_effective_saved_searches,
}
import shopify_draft_proxy/state/types.{
  type SavedSearchRecord, SavedSearchRecord,
}

@internal
pub type SavedSearchesError {
  ParseFailed(root_field.RootFieldError)
}

/// Map from saved-search root field name to the resource type the store
/// keys defaults under. Mirrors `SAVED_SEARCH_ROOT_RESOURCE_TYPES`.
@internal
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
@internal
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
@internal
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

@internal
pub fn defaults_for_resource_type(
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
  let parsed = saved_search_types.parse_saved_search_query(record.query)
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
@internal
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
@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, SavedSearchesError) {
  use data <- result.try(handle_saved_search_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}

/// Uniform query entrypoint matching the dispatcher's signature.
@internal
pub fn handle_query_request(
  proxy: DraftProxy,
  _request: Request,
  _parsed: parse_operation.ParsedOperation,
  _primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  respond_to_query(
    proxy,
    process(proxy.store, document, variables),
    "Failed to handle saved searches query",
  )
}

/// Predicate matching the TS `isSavedSearchQueryRoot`. Useful for the
/// dispatcher when checking whether to delegate.
@internal
pub fn is_saved_search_query_root(name: String) -> Bool {
  case root_field_resource_type(name) {
    Ok(_) -> True
    Error(_) -> False
  }
}

/// Build the synthetic cursor for a saved-search record. Exposed for
/// tests.
@internal
pub fn saved_search_cursor(record: SavedSearchRecord) -> String {
  build_synthetic_cursor(record.id)
}
/// Outcome of a saved-search mutation: a JSON `data` envelope plus the
/// updated store and synthetic identity registry. Callers thread these
/// forward.
// ---------------------------------------------------------------------------
// Mutation: savedSearchCreate
// ---------------------------------------------------------------------------
