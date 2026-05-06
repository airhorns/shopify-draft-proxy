//// Query handling for webhook subscription roots.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{None, Some}
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, ConnectionPageInfoOptions, ConnectionWindow,
  SelectedFieldOptions, SerializeConnectionConfig,
  default_connection_page_info_options, default_connection_window_options,
  get_document_fragments, get_field_response_key, paginate_connection_items,
  serialize_connection,
}
import shopify_draft_proxy/proxy/mutation_helpers.{respond_to_query}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/webhooks/filters.{
  IdKey, filter_webhook_subscriptions_by_field_arguments,
  filter_webhook_subscriptions_by_query, parse_sort_key,
  sort_webhook_subscriptions_for_connection,
}
import shopify_draft_proxy/proxy/webhooks/serializers
import shopify_draft_proxy/proxy/webhooks/types as webhook_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{type WebhookSubscriptionRecord}

// ---------------------------------------------------------------------------
// Query handler
// ---------------------------------------------------------------------------

/// Errors specific to the webhooks handler. Currently just surfaces
/// upstream parse errors.
/// Predicate matching `isWebhookSubscriptionQueryRoot`. Useful for the
/// dispatcher when checking whether to delegate.
@internal
pub fn is_webhook_subscription_query_root(name: String) -> Bool {
  case name {
    "webhookSubscription" -> True
    "webhookSubscriptions" -> True
    "webhookSubscriptionsCount" -> True
    _ -> False
  }
}

/// Process a webhooks query document and return a JSON `data` envelope.
/// Mirrors `handleWebhookSubscriptionQuery`. The `Store` argument
/// supplies effective (base + staged) records.
@internal
pub fn handle_webhook_subscription_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, webhook_types.WebhooksError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(webhook_types.ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

/// Convenience: parse + handle + wrap, for the dispatcher.
@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, webhook_types.WebhooksError) {
  use data <- result.try(handle_webhook_subscription_query(
    store,
    document,
    variables,
  ))
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
    "Failed to handle webhooks query",
  )
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
        "webhookSubscription" ->
          serialize_single_webhook_subscription(
            store,
            field,
            fragments,
            variables,
          )
        "webhookSubscriptions" ->
          serialize_webhook_subscriptions_connection(
            store,
            field,
            fragments,
            variables,
          )
        "webhookSubscriptionsCount" ->
          serialize_webhook_subscriptions_count(store, field, variables)
        _ -> json.null()
      }
    _ -> json.null()
  }
}

fn serialize_single_webhook_subscription(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
  let id = case dict.get(args, "id") {
    Ok(root_field.StringVal(s)) -> Some(s)
    _ -> None
  }
  let record = case id {
    Some(gid) -> store.get_effective_webhook_subscription_by_id(store, gid)
    None -> None
  }
  case record {
    Some(r) -> serializers.project_webhook_subscription(r, field, fragments)
    None -> json.null()
  }
}

fn serialize_webhook_subscriptions_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
  let format = case dict.get(args, "format") {
    Ok(root_field.StringVal(s)) -> Some(s)
    _ -> None
  }
  let uri = case dict.get(args, "uri") {
    Ok(root_field.StringVal(s)) -> Some(s)
    _ -> None
  }
  let topics = case dict.get(args, "topics") {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(value) {
        case value {
          root_field.StringVal(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
  let raw_query = case dict.get(args, "query") {
    Ok(root_field.StringVal(s)) -> Some(s)
    _ -> None
  }
  let sort_key = case dict.get(args, "sortKey") {
    Ok(root_field.StringVal(s)) -> parse_sort_key(s)
    _ -> IdKey
  }
  let reverse = case dict.get(args, "reverse") {
    Ok(root_field.BoolVal(True)) -> True
    _ -> False
  }
  let records =
    store.list_effective_webhook_subscriptions(store)
    |> filter_webhook_subscriptions_by_field_arguments(format, uri, topics)
    |> filter_webhook_subscriptions_by_query(raw_query)
    |> sort_webhook_subscriptions_for_connection(sort_key, reverse)
  let window =
    paginate_connection_items(
      records,
      field,
      dict.new(),
      webhook_cursor_value,
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
      get_cursor_value: webhook_cursor_value,
      serialize_node: fn(record, node_field, _index) {
        serializers.project_webhook_subscription(record, node_field, fragments)
      },
      selected_field_options: selected_field_options,
      page_info_options: page_info_options,
    ),
  )
}

fn serialize_webhook_subscriptions_count(
  store: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
  let raw_query = case dict.get(args, "query") {
    Ok(root_field.StringVal(s)) -> Some(s)
    _ -> None
  }
  let limit = case dict.get(args, "limit") {
    Ok(root_field.IntVal(n)) ->
      case n >= 0 {
        True -> Some(n)
        False -> None
      }
    _ -> None
  }
  let filtered =
    store.list_effective_webhook_subscriptions(store)
    |> filter_webhook_subscriptions_by_query(raw_query)
  let total = list.length(filtered)
  let count = case limit {
    None -> total
    Some(l) ->
      case total < l {
        True -> total
        False -> l
      }
  }
  let precision = case limit {
    Some(l) ->
      case total > l {
        True -> "AT_LEAST"
        False -> "EXACT"
      }
    None -> "EXACT"
  }
  let selections = case field {
    Field(selection_set: Some(SelectionSet(selections: ss, ..)), ..) -> ss
    _ -> []
  }
  let entries =
    list.filter_map(selections, fn(selection) {
      case selection {
        Field(name: name, ..) -> {
          let key = get_field_response_key(selection)
          case name.value {
            "count" -> Ok(#(key, json.int(count)))
            "precision" -> Ok(#(key, json.string(precision)))
            _ -> Ok(#(key, json.null()))
          }
        }
        _ -> Error(Nil)
      }
    })
  json.object(entries)
}

fn webhook_cursor_value(
  record: WebhookSubscriptionRecord,
  _index: Int,
) -> String {
  record.id
}
