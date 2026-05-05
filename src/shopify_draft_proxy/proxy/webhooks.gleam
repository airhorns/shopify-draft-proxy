//// Mirrors `src/proxy/webhooks.ts`.
////
//// Pass 11 landed the pure substrate. Pass 12 added the read path. Pass
//// 13 adds the three webhook subscription mutations
//// (`webhookSubscriptionCreate` / `Update` / `Delete`) plus a helper
//// for top-level GraphQL argument validation.
////
//// What's here:
//// - `endpoint_from_uri` / `uri_from_endpoint` / `webhook_subscription_uri`
////   (URI ↔ endpoint variant marshaling)
//// - `webhook_subscription_legacy_id` (trailing-segment GID pull)
//// - `matches_webhook_term` / `filter_webhook_subscriptions_by_query`
//// - `filter_webhook_subscriptions_by_field_arguments` (format / uri /
////   topics filters from the Admin GraphQL field args)
//// - `sort_webhook_subscriptions_for_connection` (CREATED_AT /
////   UPDATED_AT / TOPIC / ID, with stable tiebreak by id)
//// - `handle_webhook_subscription_query` (root-field dispatch),
////   `process` (full envelope), `is_webhook_subscription_query_root`
////   (dispatcher predicate)
//// - `process_mutation` plus the three mutation handlers
////   (`handle_webhook_subscription_create` / `_update` / `_delete`)
////   and the AST-level `validate_required_field_arguments` helper. When
////   any mutation produces top-level errors, `process_mutation` returns
////   a `{"errors": [...]}` envelope instead of `{"data": {...}}`.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order.{type Order, Eq}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, ConnectionPageInfoOptions, ConnectionWindow,
  SelectedFieldOptions, SerializeConnectionConfig, SrcList, SrcNull, SrcString,
  default_connection_page_info_options, default_connection_window_options,
  get_document_fragments, get_field_response_key, paginate_connection_items,
  project_graphql_value, serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type MutationOutcome, MutationOutcome, RequiredArgument,
  read_optional_string, read_optional_string_array, respond_to_query,
  single_root_log_draft, validate_required_field_arguments,
  validate_required_id_argument,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/search_query_parser.{
  type SearchQueryTerm, SearchQueryTermListOptions,
}
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type WebhookSubscriptionEndpoint, type WebhookSubscriptionRecord,
  WebhookEventBridgeEndpoint, WebhookHttpEndpoint, WebhookPubSubEndpoint,
  WebhookSubscriptionRecord,
}

// ----------- URI marshaling -----------

/// Mirrors `endpointFromUri`. Three URI shapes:
/// - `pubsub://<project>:<topic>` → PubSub endpoint (split on first `:`;
///   if no `:`, the whole tail goes to `pub_sub_project` and topic is
///   empty)
/// - `arn:aws:events:...` → EventBridge endpoint, ARN copied verbatim
/// - anything else → HTTP endpoint with the URI as the callback URL
pub fn endpoint_from_uri(uri: String) -> WebhookSubscriptionEndpoint {
  case string.starts_with(uri, "pubsub://") {
    True -> {
      let tail = string.drop_start(uri, 9)
      case string.split_once(tail, ":") {
        Ok(#(project, topic)) ->
          WebhookPubSubEndpoint(
            pub_sub_project: Some(project),
            pub_sub_topic: Some(topic),
          )
        Error(_) ->
          WebhookPubSubEndpoint(
            pub_sub_project: Some(tail),
            pub_sub_topic: Some(""),
          )
      }
    }
    False ->
      case string.starts_with(uri, "arn:aws:events:") {
        True -> WebhookEventBridgeEndpoint(arn: Some(uri))
        False -> WebhookHttpEndpoint(callback_url: Some(uri))
      }
  }
}

/// Mirrors `uriFromEndpoint`. Returns `None` when the endpoint
/// doesn't carry the field its kind needs (e.g. PubSub with a missing
/// project or topic).
pub fn uri_from_endpoint(
  endpoint: Option(WebhookSubscriptionEndpoint),
) -> Option(String) {
  case endpoint {
    None -> None
    Some(WebhookHttpEndpoint(callback_url: u)) -> u
    Some(WebhookEventBridgeEndpoint(arn: a)) -> a
    Some(WebhookPubSubEndpoint(pub_sub_project: Some(p), pub_sub_topic: Some(t))) ->
      case p, t {
        "", _ -> None
        _, "" -> None
        _, _ -> Some("pubsub://" <> p <> ":" <> t)
      }
    Some(WebhookPubSubEndpoint(..)) -> None
  }
}

/// Mirrors `webhookSubscriptionUri`. Prefers the explicit `uri` field;
/// falls back to deriving one from the endpoint.
pub fn webhook_subscription_uri(
  record: WebhookSubscriptionRecord,
) -> Option(String) {
  case record.uri {
    Some(_) -> record.uri
    None -> uri_from_endpoint(record.endpoint)
  }
}

/// Mirrors `webhookSubscriptionLegacyId`. The legacy id is the
/// trailing path segment of the GID; if there's no `/`, the whole id
/// is returned.
pub fn webhook_subscription_legacy_id(
  record: WebhookSubscriptionRecord,
) -> String {
  case list.last(string.split(record.id, "/")) {
    Ok(tail) -> tail
    Error(_) -> record.id
  }
}

// ----------- Term matching -----------

/// Mirrors `matchesWebhookTerm` (without the negation flip — that's
/// applied by `matches_search_query_term` so this function is a pure
/// positive-term matcher).
pub fn matches_webhook_term(
  record: WebhookSubscriptionRecord,
  term: SearchQueryTerm,
) -> Bool {
  case term.field {
    None ->
      search_query_parser.matches_search_query_text(Some(record.id), term)
      || search_query_parser.matches_search_query_text(record.topic, term)
      || search_query_parser.matches_search_query_text(record.format, term)
    Some(raw_field) ->
      case string.lowercase(raw_field) {
        "id" -> {
          let expected =
            search_query_parser.normalize_search_query_value(term.value)
          let id_norm =
            search_query_parser.normalize_search_query_value(record.id)
          let legacy_norm =
            search_query_parser.normalize_search_query_value(
              webhook_subscription_legacy_id(record),
            )
          id_norm == expected || legacy_norm == expected
        }
        "topic" ->
          search_query_parser.matches_search_query_text(record.topic, term)
        "format" ->
          search_query_parser.matches_search_query_text(record.format, term)
        "uri" | "callbackurl" | "callback_url" | "endpoint" ->
          search_query_parser.matches_search_query_text(
            webhook_subscription_uri(record),
            term,
          )
        "created_at" | "createdat" ->
          search_query_parser.matches_search_query_text(record.created_at, term)
        "updated_at" | "updatedat" ->
          search_query_parser.matches_search_query_text(record.updated_at, term)
        _ -> False
      }
  }
}

// ----------- Filtering -----------

/// Mirrors `filterWebhookSubscriptionsByQuery`. Empty / whitespace-only
/// queries leave the list unchanged. Otherwise the query is parsed
/// with `AND` ignored as a connector keyword and AND-semantics applied
/// across remaining terms.
pub fn filter_webhook_subscriptions_by_query(
  records: List(WebhookSubscriptionRecord),
  raw_query: Option(String),
) -> List(WebhookSubscriptionRecord) {
  let opts =
    SearchQueryTermListOptions(
      quote_characters: ["\"", "'"],
      preserve_quotes_in_terms: False,
      ignored_keywords: ["AND"],
      drop_empty_values: False,
    )
  search_query_parser.apply_search_query_terms(
    records,
    raw_query,
    opts,
    matches_webhook_term,
  )
}

/// Mirrors `filterWebhookSubscriptionsByFieldArguments`. Each filter is
/// optional: missing or empty filters pass through.
pub fn filter_webhook_subscriptions_by_field_arguments(
  records: List(WebhookSubscriptionRecord),
  format: Option(String),
  uri: Option(String),
  topics: List(String),
) -> List(WebhookSubscriptionRecord) {
  records
  |> list.filter(fn(record) {
    case format {
      None -> True
      Some(f) -> record.format == Some(f)
    }
  })
  |> list.filter(fn(record) {
    case uri {
      None -> True
      Some(u) -> webhook_subscription_uri(record) == Some(u)
    }
  })
  |> list.filter(fn(record) {
    case topics {
      [] -> True
      _ ->
        case record.topic {
          None -> False
          Some(t) -> list.contains(topics, t)
        }
    }
  })
}

// ----------- Sorting -----------

/// The four sort keys the connection accepts. Mirrors the TS string
/// switch — anything unrecognised falls through to `Id`.
pub type WebhookSubscriptionSortKey {
  CreatedAtKey
  UpdatedAtKey
  TopicKey
  IdKey
}

pub fn parse_sort_key(raw: String) -> WebhookSubscriptionSortKey {
  case string.uppercase(raw) {
    "CREATED_AT" -> CreatedAtKey
    "UPDATED_AT" -> UpdatedAtKey
    "TOPIC" -> TopicKey
    _ -> IdKey
  }
}

/// Mirrors `sortWebhookSubscriptionsForConnection`. Stable tiebreak on
/// the GID's numeric tail keeps order deterministic when the primary
/// key has duplicates (e.g. two records sharing a topic).
pub fn sort_webhook_subscriptions_for_connection(
  records: List(WebhookSubscriptionRecord),
  sort_key: WebhookSubscriptionSortKey,
  reverse: Bool,
) -> List(WebhookSubscriptionRecord) {
  let sorted =
    list.sort(records, fn(left, right) {
      let primary = case sort_key {
        CreatedAtKey ->
          compare_optional_strings(left.created_at, right.created_at)
        UpdatedAtKey ->
          compare_optional_strings(left.updated_at, right.updated_at)
        TopicKey -> compare_optional_strings(left.topic, right.topic)
        IdKey -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
      }
      case primary {
        Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        _ -> primary
      }
    })
  case reverse {
    True -> list.reverse(sorted)
    False -> sorted
  }
}

/// Compare two `Option(String)` lexicographically with the TS
/// `(left ?? '').localeCompare(right ?? '')` semantics: `None` is
/// treated as the empty string, so it sorts before any non-empty.
fn compare_optional_strings(
  left: Option(String),
  right: Option(String),
) -> Order {
  let l = option.unwrap(left, "")
  let r = option.unwrap(right, "")
  string.compare(l, r)
}

// ---------------------------------------------------------------------------
// Query handler
// ---------------------------------------------------------------------------

/// Errors specific to the webhooks handler. Currently just surfaces
/// upstream parse errors.
pub type WebhooksError {
  ParseFailed(root_field.RootFieldError)
}

/// Predicate matching `isWebhookSubscriptionQueryRoot`. Useful for the
/// dispatcher when checking whether to delegate.
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
pub fn handle_webhook_subscription_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, WebhooksError) {
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
) -> Result(Json, WebhooksError) {
  use data <- result.try(handle_webhook_subscription_query(
    store,
    document,
    variables,
  ))
  Ok(graphql_helpers.wrap_data(data))
}

/// Uniform query entrypoint matching the dispatcher's signature.
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
    Some(r) -> project_webhook_subscription(r, field, fragments)
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
        project_webhook_subscription(record, node_field, fragments)
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

fn project_webhook_subscription(
  record: WebhookSubscriptionRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let source = webhook_subscription_to_source(record)
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(source, selections, fragments)
    _ -> json.object([])
  }
}

fn webhook_subscription_to_source(
  record: WebhookSubscriptionRecord,
) -> graphql_helpers.SourceValue {
  let uri_source = case webhook_subscription_uri(record) {
    Some(s) -> SrcString(s)
    None -> SrcNull
  }
  let topic_source = graphql_helpers.option_string_source(record.topic)
  let name_source = graphql_helpers.option_string_source(record.name)
  let format_source = graphql_helpers.option_string_source(record.format)
  let filter_source = graphql_helpers.option_string_source(record.filter)
  let created_at_source =
    graphql_helpers.option_string_source(record.created_at)
  let updated_at_source =
    graphql_helpers.option_string_source(record.updated_at)
  let endpoint_source = endpoint_to_source(record.endpoint)
  let include_fields_source =
    SrcList(list.map(record.include_fields, fn(s) { SrcString(s) }))
  let metafield_namespaces_source =
    SrcList(list.map(record.metafield_namespaces, fn(s) { SrcString(s) }))
  let legacy_id = webhook_subscription_legacy_id(record)
  src_object([
    #("__typename", SrcString("WebhookSubscription")),
    #("id", SrcString(record.id)),
    #("legacyResourceId", SrcString(legacy_id)),
    #("topic", topic_source),
    #("uri", uri_source),
    #("callbackUrl", uri_source),
    #("name", name_source),
    #("format", format_source),
    #("includeFields", include_fields_source),
    #("metafieldNamespaces", metafield_namespaces_source),
    #("filter", filter_source),
    #("createdAt", created_at_source),
    #("updatedAt", updated_at_source),
    #("endpoint", endpoint_source),
  ])
}

fn endpoint_to_source(
  endpoint: Option(WebhookSubscriptionEndpoint),
) -> graphql_helpers.SourceValue {
  case endpoint {
    None -> SrcNull
    Some(WebhookHttpEndpoint(callback_url: c)) ->
      src_object([
        #("__typename", SrcString("WebhookHttpEndpoint")),
        #("callbackUrl", graphql_helpers.option_string_source(c)),
      ])
    Some(WebhookEventBridgeEndpoint(arn: a)) ->
      src_object([
        #("__typename", SrcString("WebhookEventBridgeEndpoint")),
        #("arn", graphql_helpers.option_string_source(a)),
      ])
    Some(WebhookPubSubEndpoint(pub_sub_project: p, pub_sub_topic: t)) ->
      src_object([
        #("__typename", SrcString("WebhookPubSubEndpoint")),
        #("pubSubProject", graphql_helpers.option_string_source(p)),
        #("pubSubTopic", graphql_helpers.option_string_source(t)),
      ])
  }
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Outcome of a webhook-subscription mutation. Mirrors the saved-search
/// outcome shape: a JSON envelope (`{"data": ...}` or `{"errors": ...}`),
/// the updated store and identity registry, and the staged GIDs.
/// User-error payload emitted on validation failure. Mirrors the
/// `WebhookSubscriptionUserError` shape in TS.
pub type UserError {
  UserError(field: List(String), message: String)
}

type UriInput {
  UriAbsent
  UriBlank
  UriPresent(String)
}

/// Predicate matching `isWebhookSubscriptionMutationRoot`. Three
/// top-level mutations the TS handler dispatches.
pub fn is_webhook_subscription_mutation_root(name: String) -> Bool {
  name == "webhookSubscriptionCreate"
  || name == "webhookSubscriptionUpdate"
  || name == "webhookSubscriptionDelete"
}

/// Process a webhook-subscription mutation document and return a
/// `MutationOutcome`. Mirrors `handleWebhookSubscriptionMutation`.
/// Top-level argument-validation errors short-circuit the whole
/// envelope to `{"errors": [...]}`; otherwise per-field payloads are
/// collected into `{"data": {...}}`.
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  process_mutation_with_api_client(
    store,
    identity,
    request_path,
    document,
    variables,
    None,
  )
}

pub fn process_mutation_with_headers(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  request_headers: Dict(String, String),
) -> MutationOutcome {
  process_mutation_with_api_client(
    store,
    identity,
    request_path,
    document,
    variables,
    read_requesting_api_client_id(request_headers),
  )
}

fn process_mutation_with_api_client(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
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
        requesting_api_client_id,
      )
    }
  }
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

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
    log_drafts: List(LogDraft),
  )
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
  requesting_api_client_id: Option(String),
) -> MutationOutcome {
  let initial = #([], [], store, identity, [], [])
  let #(
    data_entries,
    all_errors,
    final_store,
    final_identity,
    all_staged_ids,
    all_drafts,
  ) =
    list.fold(fields, initial, fn(acc, field) {
      let #(
        entries,
        errors,
        current_store,
        current_identity,
        staged_ids,
        drafts,
      ) = acc
      case field {
        Field(name: name, ..) -> {
          let dispatch = case name.value {
            "webhookSubscriptionCreate" ->
              Some(handle_create(
                current_store,
                current_identity,
                request_path,
                document,
                operation_path,
                field,
                fragments,
                variables,
                requesting_api_client_id,
              ))
            "webhookSubscriptionUpdate" ->
              Some(handle_update(
                current_store,
                current_identity,
                request_path,
                document,
                operation_path,
                field,
                fragments,
                variables,
                requesting_api_client_id,
              ))
            "webhookSubscriptionDelete" ->
              Some(handle_delete(
                current_store,
                current_identity,
                request_path,
                document,
                operation_path,
                field,
                fragments,
                variables,
              ))
            _ -> None
          }
          case dispatch {
            None -> acc
            Some(#(result, next_store, next_identity)) -> {
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> entries
              }
              let next_staged_ids = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = list.append(drafts, result.log_drafts)
              #(
                next_entries,
                next_errors,
                next_store,
                next_identity,
                next_staged_ids,
                next_drafts,
              )
            }
          }
        }
        _ -> acc
      }
    })
  let envelope = case all_errors {
    [] -> json.object([#("data", json.object(data_entries))])
    _ ->
      json.object([
        #("errors", json.preprocessed_array(all_errors)),
      ])
  }
  let final_staged_ids = case all_errors {
    [] -> all_staged_ids
    _ -> []
  }
  MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: final_staged_ids,
    log_drafts: all_drafts,
  )
}

// ---- Per-field handlers ---------------------------------------------------

fn handle_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "webhookSubscriptionCreate",
      [
        RequiredArgument(
          name: "topic",
          expected_type: "WebhookSubscriptionTopic!",
        ),
        RequiredArgument(
          name: "webhookSubscription",
          expected_type: "WebhookSubscriptionInput!",
        ),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> {
      let result =
        MutationFieldResult(
          key: key,
          payload: json.null(),
          staged_resource_ids: [],
          top_level_errors: validation_errors,
          log_drafts: [],
        )
      #(result, store, identity)
    }
    [] -> {
      let args = case root_field.get_field_arguments(field, variables) {
        Ok(d) -> d
        Error(_) -> dict.new()
      }
      let topic = case dict.get(args, "topic") {
        Ok(root_field.StringVal(s)) -> Some(s)
        _ -> None
      }
      let input = read_webhook_subscription_input(args)
      let user_errors = case input {
        Some(input_dict) ->
          validate_webhook_subscription_input(
            input_dict,
            require_uri: True,
            requesting_api_client_id: requesting_api_client_id,
          )
        None -> []
      }
      let #(record_opt, store_after, identity_after, staged_ids) = case
        input,
        user_errors
      {
        Some(input_dict), [] -> {
          let #(record, identity_after) =
            build_webhook_from_create_input(identity, topic, input_dict)
          let #(_, store_after) =
            store.upsert_staged_webhook_subscription(store, record)
          #(Some(record), store_after, identity_after, [record.id])
        }
        _, _ -> #(None, store, identity, [])
      }
      let payload =
        project_create_payload(record_opt, user_errors, field, fragments)
      let draft =
        single_root_log_draft(
          "webhookSubscriptionCreate",
          staged_ids,
          case user_errors {
            [] -> store.Staged
            _ -> store.Failed
          },
          "webhooks",
          "stage-locally",
          Some(
            "Locally staged webhookSubscriptionCreate in shopify-draft-proxy.",
          ),
        )
      let result =
        MutationFieldResult(
          key: key,
          payload: payload,
          staged_resource_ids: staged_ids,
          top_level_errors: [],
          log_drafts: [draft],
        )
      #(result, store_after, identity_after)
    }
  }
}

fn handle_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "webhookSubscriptionUpdate",
      [
        RequiredArgument(name: "id", expected_type: "ID!"),
        RequiredArgument(
          name: "webhookSubscription",
          expected_type: "WebhookSubscriptionInput!",
        ),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> {
      let result =
        MutationFieldResult(
          key: key,
          payload: json.null(),
          staged_resource_ids: [],
          top_level_errors: validation_errors,
          log_drafts: [],
        )
      #(result, store, identity)
    }
    [] -> {
      let args = case root_field.get_field_arguments(field, variables) {
        Ok(d) -> d
        Error(_) -> dict.new()
      }
      let id = case dict.get(args, "id") {
        Ok(root_field.StringVal(s)) -> Some(s)
        _ -> None
      }
      let input = read_webhook_subscription_input(args)
      let existing = case id {
        Some(gid) -> store.get_effective_webhook_subscription_by_id(store, gid)
        None -> None
      }
      let user_errors = case existing, input {
        Some(_), Some(input_dict) ->
          validate_webhook_subscription_input(
            input_dict,
            require_uri: False,
            requesting_api_client_id: requesting_api_client_id,
          )
        Some(_), None -> []
        None, _ -> [
          UserError(
            field: ["id"],
            message: "Webhook subscription does not exist",
          ),
        ]
      }
      let #(record_opt, store_after, identity_after, staged_ids) = case
        input,
        existing,
        user_errors
      {
        Some(input_dict), Some(existing_record), [] -> {
          let #(record, identity_after) =
            apply_webhook_update_input(identity, existing_record, input_dict)
          let #(_, store_after) =
            store.upsert_staged_webhook_subscription(store, record)
          #(Some(record), store_after, identity_after, [record.id])
        }
        _, _, _ -> #(None, store, identity, [])
      }
      let payload =
        project_update_payload(record_opt, user_errors, field, fragments)
      let draft =
        single_root_log_draft(
          "webhookSubscriptionUpdate",
          staged_ids,
          case user_errors {
            [] -> store.Staged
            _ -> store.Failed
          },
          "webhooks",
          "stage-locally",
          Some(
            "Locally staged webhookSubscriptionUpdate in shopify-draft-proxy.",
          ),
        )
      let result =
        MutationFieldResult(
          key: key,
          payload: payload,
          staged_resource_ids: staged_ids,
          top_level_errors: [],
          log_drafts: [draft],
        )
      #(result, store_after, identity_after)
    }
  }
}

fn handle_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let #(resolved_id, id_errors) =
    validate_required_id_argument(
      field,
      variables,
      "webhookSubscriptionDelete",
      operation_path,
      document,
    )
  case id_errors {
    [_, ..] -> {
      let result =
        MutationFieldResult(
          key: key,
          payload: json.null(),
          staged_resource_ids: [],
          top_level_errors: id_errors,
          log_drafts: [],
        )
      #(result, store, identity)
    }
    [] -> {
      let existing = case resolved_id {
        Some(gid) -> store.get_effective_webhook_subscription_by_id(store, gid)
        None -> None
      }
      let user_errors = case resolved_id, existing {
        Some(_), Some(_) -> []
        _, _ -> [
          UserError(
            field: ["id"],
            message: "Webhook subscription does not exist",
          ),
        ]
      }
      let store_after = case resolved_id, existing, user_errors {
        Some(gid), Some(_), [] ->
          store.delete_staged_webhook_subscription(store, gid)
        _, _, _ -> store
      }
      let deleted_id = case user_errors {
        [] -> resolved_id
        _ -> None
      }
      let payload =
        project_delete_payload(deleted_id, user_errors, field, fragments)
      let draft =
        single_root_log_draft(
          "webhookSubscriptionDelete",
          [],
          case user_errors {
            [] -> store.Staged
            _ -> store.Failed
          },
          "webhooks",
          "stage-locally",
          Some(
            "Locally staged webhookSubscriptionDelete in shopify-draft-proxy.",
          ),
        )
      let result =
        MutationFieldResult(
          key: key,
          payload: payload,
          staged_resource_ids: [],
          top_level_errors: [],
          log_drafts: [draft],
        )
      #(result, store_after, identity)
    }
  }
}

// ---- Top-level argument validation ----------------------------------------
//
// Lifted to `proxy/mutation_helpers` in Pass 14. Handlers above call
// `validate_required_field_arguments` and `validate_required_id_argument`
// directly through the import.

// ---- Builders -------------------------------------------------------------

fn build_webhook_from_create_input(
  identity: SyntheticIdentityRegistry,
  topic: Option(String),
  input: Dict(String, root_field.ResolvedValue),
) -> #(WebhookSubscriptionRecord, SyntheticIdentityRegistry) {
  let #(timestamp, identity_after_ts) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(id, identity_after_id) =
    synthetic_identity.make_proxy_synthetic_gid(
      identity_after_ts,
      "WebhookSubscription",
    )
  let uri = option.unwrap(normalize_uri_from_input(input), "")
  let record =
    WebhookSubscriptionRecord(
      id: id,
      topic: topic,
      uri: Some(uri),
      name: read_optional_string(input, "name"),
      format: case read_optional_string(input, "format") {
        Some(s) -> Some(s)
        None -> Some("JSON")
      },
      include_fields: option.unwrap(
        read_optional_string_array(input, "includeFields"),
        [],
      ),
      metafield_namespaces: option.unwrap(
        read_optional_string_array(input, "metafieldNamespaces"),
        [],
      ),
      filter: case read_optional_string(input, "filter") {
        Some(s) -> Some(s)
        None -> Some("")
      },
      created_at: Some(timestamp),
      updated_at: Some(timestamp),
      endpoint: Some(endpoint_from_uri(uri)),
    )
  #(record, identity_after_id)
}

fn apply_webhook_update_input(
  identity: SyntheticIdentityRegistry,
  existing: WebhookSubscriptionRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(WebhookSubscriptionRecord, SyntheticIdentityRegistry) {
  let #(timestamp, identity_after_ts) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let new_uri = normalize_uri_from_input(input)
  let resolved_uri = case new_uri {
    Some(u) -> Some(u)
    None ->
      case existing.uri {
        Some(_) -> existing.uri
        None -> uri_from_endpoint(existing.endpoint)
      }
  }
  let new_endpoint = case new_uri {
    Some(u) -> Some(endpoint_from_uri(u))
    None -> existing.endpoint
  }
  let record =
    WebhookSubscriptionRecord(
      ..existing,
      uri: resolved_uri,
      name: case read_optional_string(input, "name") {
        Some(s) -> Some(s)
        None -> existing.name
      },
      format: case read_optional_string(input, "format") {
        Some(s) -> Some(s)
        None -> existing.format
      },
      include_fields: option.unwrap(
        read_optional_string_array(input, "includeFields"),
        existing.include_fields,
      ),
      metafield_namespaces: option.unwrap(
        read_optional_string_array(input, "metafieldNamespaces"),
        existing.metafield_namespaces,
      ),
      filter: case read_optional_string(input, "filter") {
        Some(s) -> Some(s)
        None -> existing.filter
      },
      updated_at: Some(timestamp),
      endpoint: new_endpoint,
    )
  #(record, identity_after_ts)
}

// ---- Input readers --------------------------------------------------------

fn read_webhook_subscription_input(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, "webhookSubscription") {
    Ok(root_field.ObjectVal(fields)) -> Some(fields)
    _ -> None
  }
}

// `read_optional_string` and `read_optional_string_array` come from
// `proxy/mutation_helpers` (Pass 14 lift).

fn validate_webhook_subscription_input(
  input: Dict(String, root_field.ResolvedValue),
  require_uri require_uri: Bool,
  requesting_api_client_id requesting_api_client_id: Option(String),
) -> List(UserError) {
  list.append(
    validate_webhook_uri_input(input, require_uri, requesting_api_client_id),
    validate_webhook_filter_input(input),
  )
}

fn validate_webhook_uri_input(
  input: Dict(String, root_field.ResolvedValue),
  require_uri: Bool,
  requesting_api_client_id: Option(String),
) -> List(UserError) {
  case read_uri_input(input), require_uri {
    UriAbsent, True -> [blank_address_user_error()]
    UriAbsent, False -> []
    UriBlank, _ -> [blank_address_user_error()]
    UriPresent(uri), _ -> validate_webhook_uri(uri, requesting_api_client_id)
  }
}

fn read_uri_input(input: Dict(String, root_field.ResolvedValue)) -> UriInput {
  case read_uri_input_field(input, "uri") {
    UriAbsent -> read_uri_input_field(input, "callbackUrl")
    found -> found
  }
}

fn read_uri_input_field(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> UriInput {
  case dict.get(input, name) {
    Ok(root_field.StringVal(raw)) ->
      case string.trim(raw) {
        "" -> UriBlank
        trimmed -> UriPresent(trimmed)
      }
    Ok(root_field.NullVal) -> UriBlank
    _ -> UriAbsent
  }
}

fn blank_address_user_error() -> UserError {
  UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address can't be blank",
  )
}

fn invalid_url_user_error() -> UserError {
  UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is not a valid URL",
  )
}

fn unsupported_protocol_user_error(protocol: String) -> UserError {
  UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address protocol " <> protocol <> " is not supported",
  )
}

fn invalid_pubsub_user_error() -> UserError {
  UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is not a valid GCP pub/sub format. Format should be pubsub://project:topic",
  )
}

fn address_too_long_user_error() -> UserError {
  UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is too long",
  )
}

fn validate_webhook_uri(
  uri: String,
  requesting_api_client_id: Option(String),
) -> List(UserError) {
  case string.byte_size(uri) > 4096 {
    True -> [address_too_long_user_error()]
    False ->
      case string.starts_with(uri, "pubsub://") {
        True -> validate_pubsub_uri(uri)
        False ->
          case string.starts_with(uri, "arn:aws:events:") {
            True -> validate_eventbridge_arn(uri, requesting_api_client_id)
            False ->
              case string.starts_with(uri, "kafka://") {
                True -> kafka_user_errors()
                False -> validate_https_uri(uri)
              }
          }
      }
  }
}

fn validate_https_uri(uri: String) -> List(UserError) {
  case string.starts_with(uri, "https://") {
    False ->
      case string.split_once(uri, "://") {
        Ok(#(protocol, _)) -> [
          unsupported_protocol_user_error(protocol <> "://"),
        ]
        Error(_) -> [invalid_url_user_error()]
      }
    True -> {
      let host = https_uri_host(uri)
      case host == "" || has_url_whitespace(uri) || is_disallowed_host(host) {
        True -> [invalid_url_user_error()]
        False -> []
      }
    }
  }
}

fn https_uri_host(uri: String) -> String {
  let without_scheme = string.drop_start(uri, 8)
  without_scheme
  |> split_before("/")
  |> split_before("?")
  |> split_before("#")
  |> split_before(":")
  |> string.lowercase
}

fn split_before(value: String, separator: String) -> String {
  case string.split_once(value, separator) {
    Ok(#(left, _)) -> left
    Error(_) -> value
  }
}

fn has_url_whitespace(value: String) -> Bool {
  string.contains(value, " ")
  || string.contains(value, "\t")
  || string.contains(value, "\n")
  || string.contains(value, "\r")
}

fn is_disallowed_host(host: String) -> Bool {
  host == "localhost"
  || host == "127.0.0.1"
  || host == "0.0.0.0"
  || host == "::1"
  || string.ends_with(host, ".local")
}

fn validate_pubsub_uri(uri: String) -> List(UserError) {
  let tail = string.drop_start(uri, 9)
  case string.split_once(tail, ":") {
    Ok(#(project, topic)) ->
      case project, topic {
        "", _ -> pubsub_format_user_errors()
        _, "" -> pubsub_format_user_errors()
        _, _ ->
          case valid_gcp_project_id(project) {
            False -> [invalid_address_user_error(), gcp_project_id_user_error()]
            True ->
              case valid_gcp_topic_id(topic) {
                True -> []
                False -> [
                  invalid_address_user_error(),
                  gcp_topic_id_user_error(),
                ]
              }
          }
      }
    Error(_) -> pubsub_format_user_errors()
  }
}

fn pubsub_format_user_errors() -> List(UserError) {
  [
    unsupported_protocol_user_error("pubsub://"),
    invalid_pubsub_user_error(),
  ]
}

fn valid_gcp_project_id(project: String) -> Bool {
  case all_digits(project) {
    True -> True
    False -> {
      let length = string.length(project)
      length >= 6
      && length <= 30
      && starts_with_lowercase_alpha(project)
      && !string.ends_with(project, "-")
      && all_gcp_project_chars(project)
    }
  }
}

fn valid_gcp_topic_id(topic: String) -> Bool {
  let length = string.length(topic)
  length >= 3
  && length <= 255
  && starts_with_alpha(topic)
  && !string.starts_with(string.lowercase(topic), "goog")
  && all_gcp_topic_chars(topic)
}

fn validate_eventbridge_arn(
  uri: String,
  requesting_api_client_id: Option(String),
) -> List(UserError) {
  let tail = string.drop_start(uri, 15)
  case string.split_once(tail, "::event-source/aws.partner/shopify.com") {
    Ok(#(region, after_domain)) ->
      case valid_aws_region(region), eventbridge_source_tail(after_domain) {
        True, Some(#(api_client_id, event_source_name)) ->
          case all_digits(api_client_id) && event_source_name != "" {
            True ->
              case requesting_api_client_id {
                Some(expected) if api_client_id != expected -> [
                  invalid_address_user_error(),
                  eventbridge_wrong_api_client_user_error(
                    api_client_id,
                    expected,
                  ),
                ]
                _ -> []
              }
            False -> eventbridge_arn_user_errors()
          }
        _, _ -> eventbridge_arn_user_errors()
      }
    Error(_) -> eventbridge_arn_user_errors()
  }
}

fn eventbridge_source_tail(tail: String) -> Option(#(String, String)) {
  let source = case string.starts_with(tail, ".test/") {
    True -> Some(string.drop_start(tail, 6))
    False ->
      case string.starts_with(tail, "/") {
        True -> Some(string.drop_start(tail, 1))
        False -> None
      }
  }
  case source {
    Some(rest) -> {
      case string.split_once(rest, "/") {
        Ok(#(api_client_id, event_source_name)) ->
          Some(#(api_client_id, event_source_name))
        Error(_) -> None
      }
    }
    None -> None
  }
}

fn valid_aws_region(region: String) -> Bool {
  case string.split(region, on: "-") {
    [country, zone, number] ->
      string.length(country) == 2
      && all_lowercase_alpha(country)
      && zone != ""
      && all_lowercase_alpha(zone)
      && all_digits(number)
    _ -> False
  }
}

fn starts_with_lowercase_alpha(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Ok(#(first, _)) -> is_lowercase_alpha(first)
    Error(_) -> False
  }
}

fn starts_with_alpha(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Ok(#(first, _)) -> is_alpha(first)
    Error(_) -> False
  }
}

fn all_gcp_project_chars(value: String) -> Bool {
  list.all(string.to_graphemes(value), fn(grapheme) {
    is_lowercase_alpha(grapheme) || is_digit(grapheme) || grapheme == "-"
  })
}

fn all_gcp_topic_chars(value: String) -> Bool {
  list.all(string.to_graphemes(value), fn(grapheme) {
    is_alpha(grapheme)
    || is_digit(grapheme)
    || string.contains("-_.~+%", grapheme)
  })
}

fn all_lowercase_alpha(value: String) -> Bool {
  list.all(string.to_graphemes(value), is_lowercase_alpha)
}

fn all_digits(value: String) -> Bool {
  value != "" && list.all(string.to_graphemes(value), is_digit)
}

fn is_alpha(grapheme: String) -> Bool {
  is_lowercase_alpha(grapheme)
  || string.contains("ABCDEFGHIJKLMNOPQRSTUVWXYZ", grapheme)
}

fn is_lowercase_alpha(grapheme: String) -> Bool {
  string.contains("abcdefghijklmnopqrstuvwxyz", grapheme)
}

fn is_digit(grapheme: String) -> Bool {
  string.contains("0123456789", grapheme)
}

fn invalid_address_user_error() -> UserError {
  UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is invalid",
  )
}

fn gcp_project_id_user_error() -> UserError {
  UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is not a valid GCP project id.",
  )
}

fn gcp_topic_id_user_error() -> UserError {
  UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is not a valid GCP topic id.",
  )
}

fn eventbridge_arn_user_errors() -> List(UserError) {
  [
    invalid_address_user_error(),
    UserError(
      field: ["webhookSubscription", "callbackUrl"],
      message: "Address is not a valid AWS ARN",
    ),
  ]
}

fn eventbridge_wrong_api_client_user_error(
  actual: String,
  expected: String,
) -> UserError {
  UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is an AWS ARN and includes api_client_id '"
      <> actual
      <> "' instead of '"
      <> expected
      <> "'",
  )
}

fn kafka_user_errors() -> List(UserError) {
  [
    unsupported_protocol_user_error("kafka://"),
    UserError(
      field: ["webhookSubscription", "callbackUrl"],
      message: "Address is not a valid kafka topic",
    ),
  ]
}

fn read_requesting_api_client_id(
  request_headers: Dict(String, String),
) -> Option(String) {
  let found =
    dict.to_list(request_headers)
    |> list.find_map(fn(header) {
      let #(name, value) = header
      case string.lowercase(name) == "x-shopify-draft-proxy-api-client-id" {
        True -> Ok(value)
        False -> Error(Nil)
      }
    })
  case found {
    Ok(value) -> Some(value)
    Error(_) -> None
  }
}

fn validate_webhook_filter_input(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case dict.has_key(input, "filter") {
    False -> []
    True ->
      case read_optional_string(input, "filter") {
        Some(raw) ->
          case string.trim(raw) {
            "" -> []
            _ -> [
              UserError(
                field: ["webhookSubscription", "filter"],
                message: "The specified filter is invalid, please ensure you specify the field(s) you wish to filter on.",
              ),
            ]
          }
        None -> []
      }
  }
}

fn normalize_uri_from_input(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  // Real Shopify exposes `callbackUrl` on `WebhookSubscriptionInput` as a
  // (deprecated) alias for `uri` — confirmed by introspecting the 2025-01
  // schema with `includeDeprecated: true`. Reading only `uri` made the
  // proxy fabricate a misleading "Address can't be blank" userError when
  // callers used the deprecated field name. (`callback_url` and `endpoint`
  // are NOT real input fields and were stripped after the introspection
  // check; they were artifacts of the search-query alias list in this
  // module.)
  case read_uri_input(input) {
    UriPresent(uri) -> Some(uri)
    UriAbsent | UriBlank -> None
  }
}

// ---- Mutation projection --------------------------------------------------

fn project_create_payload(
  record: Option(WebhookSubscriptionRecord),
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let webhook_source = case record {
    Some(r) -> webhook_subscription_to_source(r)
    None -> SrcNull
  }
  let user_errors_source = SrcList(list.map(user_errors, user_error_to_source))
  let payload =
    src_object([
      #("webhookSubscription", webhook_source),
      #("userErrors", user_errors_source),
    ])
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(payload, selections, fragments)
    _ -> json.object([])
  }
}

fn project_update_payload(
  record: Option(WebhookSubscriptionRecord),
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_create_payload(record, user_errors, field, fragments)
}

fn project_delete_payload(
  deleted_id: Option(String),
  user_errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let id_source = case deleted_id {
    Some(s) -> SrcString(s)
    None -> SrcNull
  }
  let user_errors_source = SrcList(list.map(user_errors, user_error_to_source))
  let payload =
    src_object([
      #("deletedWebhookSubscriptionId", id_source),
      #("userErrors", user_errors_source),
    ])
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(payload, selections, fragments)
    _ -> json.object([])
  }
}

fn user_error_to_source(error: UserError) -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", SrcList(list.map(error.field, fn(part) { SrcString(part) }))),
    #("message", SrcString(error.message)),
  ])
}
