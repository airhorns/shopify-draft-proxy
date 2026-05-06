//// Webhook subscription URI, filtering, and sorting helpers.

import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order.{type Order, Eq}
import gleam/string
import shopify_draft_proxy/search_query_parser.{
  type SearchQueryTerm, SearchQueryTermListOptions,
}
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/types.{
  type WebhookSubscriptionEndpoint, type WebhookSubscriptionRecord,
  WebhookEventBridgeEndpoint, WebhookHttpEndpoint, WebhookPubSubEndpoint,
}

// ----------- URI marshaling -----------

/// Mirrors `endpointFromUri`. Three URI shapes:
/// - `pubsub://<project>:<topic>` → PubSub endpoint (split on first `:`;
///   if no `:`, the whole tail goes to `pub_sub_project` and topic is
///   empty)
/// - `arn:aws:events:...` → EventBridge endpoint, ARN copied verbatim
/// - anything else → HTTP endpoint with the URI as the callback URL
@internal
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
@internal
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
@internal
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
@internal
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
@internal
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
@internal
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
@internal
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
@internal
pub type WebhookSubscriptionSortKey {
  CreatedAtKey
  UpdatedAtKey
  TopicKey
  IdKey
}

@internal
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
@internal
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
