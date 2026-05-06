//// Public entrypoint for webhook subscription handling.
////
//// Implementation is split across webhooks/* submodules; this file keeps
//// the original public API surface stable for callers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option.{type Option}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/mutation_helpers.{type MutationOutcome}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/proxy/webhooks/filters
import shopify_draft_proxy/proxy/webhooks/mutations
import shopify_draft_proxy/proxy/webhooks/queries
import shopify_draft_proxy/proxy/webhooks/types as webhook_types
import shopify_draft_proxy/search_query_parser.{type SearchQueryTerm}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type WebhookSubscriptionEndpoint, type WebhookSubscriptionRecord,
}

pub type WebhooksError =
  webhook_types.WebhooksError

pub type UserError =
  webhook_types.UserError

pub fn endpoint_from_uri(uri: String) -> WebhookSubscriptionEndpoint {
  filters.endpoint_from_uri(uri)
}

pub fn uri_from_endpoint(
  endpoint: Option(WebhookSubscriptionEndpoint),
) -> Option(String) {
  filters.uri_from_endpoint(endpoint)
}

pub fn webhook_subscription_uri(
  record: WebhookSubscriptionRecord,
) -> Option(String) {
  filters.webhook_subscription_uri(record)
}

pub fn webhook_subscription_legacy_id(
  record: WebhookSubscriptionRecord,
) -> String {
  filters.webhook_subscription_legacy_id(record)
}

pub fn matches_webhook_term(
  record: WebhookSubscriptionRecord,
  term: SearchQueryTerm,
) -> Bool {
  filters.matches_webhook_term(record, term)
}

pub fn filter_webhook_subscriptions_by_query(
  records: List(WebhookSubscriptionRecord),
  raw_query: Option(String),
) -> List(WebhookSubscriptionRecord) {
  filters.filter_webhook_subscriptions_by_query(records, raw_query)
}

pub fn filter_webhook_subscriptions_by_field_arguments(
  records: List(WebhookSubscriptionRecord),
  format: Option(String),
  uri: Option(String),
  topics: List(String),
) -> List(WebhookSubscriptionRecord) {
  filters.filter_webhook_subscriptions_by_field_arguments(
    records,
    format,
    uri,
    topics,
  )
}

pub type WebhookSubscriptionSortKey {
  CreatedAtKey
  UpdatedAtKey
  TopicKey
  IdKey
}

pub fn parse_sort_key(raw: String) -> WebhookSubscriptionSortKey {
  case filters.parse_sort_key(raw) {
    filters.CreatedAtKey -> CreatedAtKey
    filters.UpdatedAtKey -> UpdatedAtKey
    filters.TopicKey -> TopicKey
    filters.IdKey -> IdKey
  }
}

pub fn sort_webhook_subscriptions_for_connection(
  records: List(WebhookSubscriptionRecord),
  sort_key: WebhookSubscriptionSortKey,
  reverse: Bool,
) -> List(WebhookSubscriptionRecord) {
  filters.sort_webhook_subscriptions_for_connection(
    records,
    to_internal_sort_key(sort_key),
    reverse,
  )
}

fn to_internal_sort_key(
  sort_key: WebhookSubscriptionSortKey,
) -> filters.WebhookSubscriptionSortKey {
  case sort_key {
    CreatedAtKey -> filters.CreatedAtKey
    UpdatedAtKey -> filters.UpdatedAtKey
    TopicKey -> filters.TopicKey
    IdKey -> filters.IdKey
  }
}

pub fn is_webhook_subscription_query_root(name: String) -> Bool {
  queries.is_webhook_subscription_query_root(name)
}

pub fn handle_webhook_subscription_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, webhook_types.WebhooksError) {
  queries.handle_webhook_subscription_query(store, document, variables)
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, webhook_types.WebhooksError) {
  queries.process(store, document, variables)
}

pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  queries.handle_query_request(
    proxy,
    request,
    parsed,
    primary_root_field,
    document,
    variables,
  )
}

pub fn is_webhook_subscription_mutation_root(name: String) -> Bool {
  mutations.is_webhook_subscription_mutation_root(name)
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  mutations.process_mutation(
    store,
    identity,
    request_path,
    document,
    variables,
    upstream,
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
  mutations.process_mutation_with_headers(
    store,
    identity,
    request_path,
    document,
    variables,
    request_headers,
  )
}
