import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/proxy/upstream_query.{empty_upstream_context}
import shopify_draft_proxy/proxy/webhooks
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type WebhookSubscriptionRecord, WebhookEventBridgeEndpoint,
  WebhookHttpEndpoint, WebhookPubSubEndpoint, WebhookSubscriptionRecord,
}

fn make_record(
  id: String,
  topic: option.Option(String),
  uri: option.Option(String),
  format: option.Option(String),
  created_at: option.Option(String),
  updated_at: option.Option(String),
  endpoint: option.Option(types.WebhookSubscriptionEndpoint),
) -> WebhookSubscriptionRecord {
  WebhookSubscriptionRecord(
    id: id,
    topic: topic,
    uri: uri,
    name: None,
    format: format,
    include_fields: [],
    metafield_namespaces: [],
    filter: None,
    created_at: created_at,
    updated_at: updated_at,
    endpoint: endpoint,
  )
}

// ----------- Endpoint URI marshaling -----------

pub fn endpoint_from_http_uri_test() {
  assert webhooks.endpoint_from_uri("https://example.com/hook")
    == WebhookHttpEndpoint(callback_url: Some("https://example.com/hook"))
}

pub fn endpoint_from_arn_uri_test() {
  let arn = "arn:aws:events:us-east-1:1234:event-bus/default"
  assert webhooks.endpoint_from_uri(arn)
    == WebhookEventBridgeEndpoint(arn: Some(arn))
}

pub fn endpoint_from_pubsub_uri_test() {
  assert webhooks.endpoint_from_uri("pubsub://my-project:my-topic")
    == WebhookPubSubEndpoint(
      pub_sub_project: Some("my-project"),
      pub_sub_topic: Some("my-topic"),
    )
}

pub fn endpoint_from_pubsub_uri_no_topic_test() {
  // No `:` after the scheme — project gets the whole tail, topic empty.
  assert webhooks.endpoint_from_uri("pubsub://my-project")
    == WebhookPubSubEndpoint(
      pub_sub_project: Some("my-project"),
      pub_sub_topic: Some(""),
    )
}

pub fn uri_from_http_endpoint_test() {
  assert webhooks.uri_from_endpoint(
      Some(WebhookHttpEndpoint(callback_url: Some("https://x"))),
    )
    == Some("https://x")
}

pub fn uri_from_eventbridge_endpoint_test() {
  let arn = "arn:aws:events:..."
  assert webhooks.uri_from_endpoint(
      Some(WebhookEventBridgeEndpoint(arn: Some(arn))),
    )
    == Some(arn)
}

pub fn uri_from_pubsub_endpoint_round_trip_test() {
  assert webhooks.uri_from_endpoint(
      Some(WebhookPubSubEndpoint(
        pub_sub_project: Some("p"),
        pub_sub_topic: Some("t"),
      )),
    )
    == Some("pubsub://p:t")
}

pub fn uri_from_pubsub_endpoint_missing_topic_test() {
  // Mirrors the TS `pubSubProject && pubSubTopic` short-circuit.
  assert webhooks.uri_from_endpoint(
      Some(WebhookPubSubEndpoint(
        pub_sub_project: Some("p"),
        pub_sub_topic: Some(""),
      )),
    )
    == None
}

pub fn uri_from_endpoint_none_test() {
  assert webhooks.uri_from_endpoint(None) == None
}

// ----------- webhook_subscription_uri -----------

pub fn subscription_uri_prefers_explicit_uri_test() {
  let r =
    make_record(
      "gid://shopify/WebhookSubscription/1",
      Some("topic"),
      Some("https://explicit"),
      Some("JSON"),
      None,
      None,
      Some(WebhookHttpEndpoint(callback_url: Some("https://endpoint"))),
    )
  // Explicit `uri` wins over the endpoint-derived URI.
  assert webhooks.webhook_subscription_uri(r) == Some("https://explicit")
}

pub fn subscription_uri_falls_back_to_endpoint_test() {
  let r =
    make_record(
      "gid://shopify/WebhookSubscription/1",
      Some("topic"),
      None,
      Some("JSON"),
      None,
      None,
      Some(WebhookHttpEndpoint(callback_url: Some("https://from-endpoint"))),
    )
  assert webhooks.webhook_subscription_uri(r) == Some("https://from-endpoint")
}

// ----------- legacy_id -----------

pub fn legacy_id_extracts_trailing_segment_test() {
  let r =
    make_record(
      "gid://shopify/WebhookSubscription/123",
      None,
      None,
      None,
      None,
      None,
      None,
    )
  assert webhooks.webhook_subscription_legacy_id(r) == "123"
}

pub fn legacy_id_no_slash_returns_whole_test() {
  let r = make_record("plain", None, None, None, None, None, None)
  assert webhooks.webhook_subscription_legacy_id(r) == "plain"
}

// ----------- Filter by query -----------

fn http(url: String) -> option.Option(types.WebhookSubscriptionEndpoint) {
  Some(WebhookHttpEndpoint(callback_url: Some(url)))
}

fn sample_records() -> List(WebhookSubscriptionRecord) {
  [
    make_record(
      "gid://shopify/WebhookSubscription/1",
      Some("orders/create"),
      None,
      Some("JSON"),
      Some("2024-01-01T00:00:00Z"),
      Some("2024-01-02T00:00:00Z"),
      http("https://a.example.com"),
    ),
    make_record(
      "gid://shopify/WebhookSubscription/2",
      Some("orders/updated"),
      None,
      Some("XML"),
      Some("2024-02-01T00:00:00Z"),
      Some("2024-02-02T00:00:00Z"),
      http("https://b.example.com"),
    ),
    make_record(
      "gid://shopify/WebhookSubscription/3",
      Some("products/create"),
      None,
      Some("JSON"),
      Some("2024-03-01T00:00:00Z"),
      Some("2024-03-02T00:00:00Z"),
      http("https://c.example.com"),
    ),
  ]
}

pub fn filter_by_query_empty_returns_all_test() {
  let records = sample_records()
  assert webhooks.filter_webhook_subscriptions_by_query(records, None)
    == records
  assert webhooks.filter_webhook_subscriptions_by_query(records, Some(""))
    == records
  assert webhooks.filter_webhook_subscriptions_by_query(records, Some("   "))
    == records
}

pub fn filter_by_query_topic_field_test() {
  let records = sample_records()
  let result =
    webhooks.filter_webhook_subscriptions_by_query(
      records,
      Some("topic:orders"),
    )
  // "orders/create" and "orders/updated" both contain "orders".
  assert list_length(result) == 2
}

pub fn filter_by_query_format_field_test() {
  let records = sample_records()
  let result =
    webhooks.filter_webhook_subscriptions_by_query(records, Some("format:JSON"))
  assert list_length(result) == 2
}

pub fn filter_by_query_uri_field_test() {
  let records = sample_records()
  let result =
    webhooks.filter_webhook_subscriptions_by_query(
      records,
      Some("uri:b.example.com"),
    )
  assert list_length(result) == 1
}

pub fn filter_by_query_id_exact_test() {
  // The `id` field uses exact-match on the normalized GID, not
  // substring — so "1" alone won't match "gid://shopify/WebhookSubscription/1".
  let records = sample_records()
  let result_substring =
    webhooks.filter_webhook_subscriptions_by_query(records, Some("id:1"))
  assert list_length(result_substring) == 1
  // …because the TS code falls back to comparing the *legacy* id (the
  // trailing path segment), which is "1" for the first record.
}

pub fn filter_by_query_id_full_gid_test() {
  let records = sample_records()
  let result =
    webhooks.filter_webhook_subscriptions_by_query(
      records,
      Some("id:gid://shopify/WebhookSubscription/2"),
    )
  assert list_length(result) == 1
}

pub fn filter_by_query_and_keyword_dropped_test() {
  // "AND" is in the ignored_keywords list and gets stripped.
  let records = sample_records()
  let result =
    webhooks.filter_webhook_subscriptions_by_query(
      records,
      Some("topic:orders AND format:JSON"),
    )
  // Only the first record matches both topic:orders and format:JSON.
  assert list_length(result) == 1
}

pub fn filter_by_query_negation_test() {
  let records = sample_records()
  let result =
    webhooks.filter_webhook_subscriptions_by_query(
      records,
      Some("-format:JSON"),
    )
  assert list_length(result) == 1
}

// ----------- Filter by field arguments -----------

pub fn filter_by_field_args_format_test() {
  let records = sample_records()
  let result =
    webhooks.filter_webhook_subscriptions_by_field_arguments(
      records,
      Some("JSON"),
      None,
      [],
    )
  assert list_length(result) == 2
}

pub fn filter_by_field_args_uri_exact_test() {
  let records = sample_records()
  let result =
    webhooks.filter_webhook_subscriptions_by_field_arguments(
      records,
      None,
      Some("https://b.example.com"),
      [],
    )
  assert list_length(result) == 1
}

pub fn filter_by_field_args_topics_test() {
  let records = sample_records()
  let result =
    webhooks.filter_webhook_subscriptions_by_field_arguments(
      records,
      None,
      None,
      ["orders/create", "products/create"],
    )
  assert list_length(result) == 2
}

pub fn filter_by_field_args_combine_test() {
  let records = sample_records()
  let result =
    webhooks.filter_webhook_subscriptions_by_field_arguments(
      records,
      Some("JSON"),
      None,
      ["orders/create"],
    )
  assert list_length(result) == 1
}

pub fn filter_by_field_args_no_filters_returns_all_test() {
  let records = sample_records()
  assert webhooks.filter_webhook_subscriptions_by_field_arguments(
      records,
      None,
      None,
      [],
    )
    == records
}

// ----------- Sorting -----------

pub fn sort_by_id_default_test() {
  // Numeric trailing-int sort — ID/2 < ID/10 even though "2" > "10"
  // lexicographically.
  let records = [
    make_record(
      "gid://shopify/WebhookSubscription/10",
      None,
      None,
      None,
      None,
      None,
      None,
    ),
    make_record(
      "gid://shopify/WebhookSubscription/2",
      None,
      None,
      None,
      None,
      None,
      None,
    ),
  ]
  let sorted =
    webhooks.sort_webhook_subscriptions_for_connection(
      records,
      webhooks.IdKey,
      False,
    )
  case sorted {
    [first, _] -> {
      assert first.id == "gid://shopify/WebhookSubscription/2"
    }
    _ -> panic as "expected 2 records"
  }
}

pub fn sort_by_topic_test() {
  let records = sample_records()
  let sorted =
    webhooks.sort_webhook_subscriptions_for_connection(
      records,
      webhooks.TopicKey,
      False,
    )
  case sorted {
    [a, b, c] -> {
      assert a.topic == Some("orders/create")
      assert b.topic == Some("orders/updated")
      assert c.topic == Some("products/create")
    }
    _ -> panic as "expected 3 records"
  }
}

pub fn sort_by_created_at_test() {
  let records = sample_records()
  let sorted =
    webhooks.sort_webhook_subscriptions_for_connection(
      records,
      webhooks.CreatedAtKey,
      False,
    )
  case sorted {
    [a, b, c] -> {
      assert a.created_at == Some("2024-01-01T00:00:00Z")
      assert b.created_at == Some("2024-02-01T00:00:00Z")
      assert c.created_at == Some("2024-03-01T00:00:00Z")
    }
    _ -> panic as "expected 3 records"
  }
}

pub fn sort_reverse_test() {
  let records = sample_records()
  let sorted =
    webhooks.sort_webhook_subscriptions_for_connection(
      records,
      webhooks.IdKey,
      True,
    )
  case sorted {
    [a, _, _] -> {
      assert a.id == "gid://shopify/WebhookSubscription/3"
    }
    _ -> panic as "expected 3 records"
  }
}

pub fn sort_tiebreak_by_id_test() {
  // Two records with the same topic — secondary sort by GID numeric
  // tail breaks the tie deterministically.
  let records = [
    make_record(
      "gid://shopify/WebhookSubscription/10",
      Some("same"),
      None,
      None,
      None,
      None,
      None,
    ),
    make_record(
      "gid://shopify/WebhookSubscription/2",
      Some("same"),
      None,
      None,
      None,
      None,
      None,
    ),
  ]
  let sorted =
    webhooks.sort_webhook_subscriptions_for_connection(
      records,
      webhooks.TopicKey,
      False,
    )
  case sorted {
    [first, _] -> {
      assert first.id == "gid://shopify/WebhookSubscription/2"
    }
    _ -> panic as "expected 2 records"
  }
}

pub fn parse_sort_key_test() {
  assert webhooks.parse_sort_key("CREATED_AT") == webhooks.CreatedAtKey
  assert webhooks.parse_sort_key("UPDATED_AT") == webhooks.UpdatedAtKey
  assert webhooks.parse_sort_key("TOPIC") == webhooks.TopicKey
  assert webhooks.parse_sort_key("ID") == webhooks.IdKey
  assert webhooks.parse_sort_key("created_at") == webhooks.CreatedAtKey
  // Anything unknown falls through to ID.
  assert webhooks.parse_sort_key("nonsense") == webhooks.IdKey
}

// Helper.
fn list_length(xs: List(a)) -> Int {
  case xs {
    [] -> 0
    [_, ..rest] -> 1 + list_length(rest)
  }
}

// ----------- Query handler smoke tests -----------

pub fn is_webhook_subscription_query_root_test() {
  assert webhooks.is_webhook_subscription_query_root("webhookSubscription")
  assert webhooks.is_webhook_subscription_query_root("webhookSubscriptions")
  assert webhooks.is_webhook_subscription_query_root(
    "webhookSubscriptionsCount",
  )
  assert !webhooks.is_webhook_subscription_query_root("savedSearches")
  assert !webhooks.is_webhook_subscription_query_root("event")
}

fn handle(store: store.Store, query: String) -> String {
  let assert Ok(data) =
    webhooks.handle_webhook_subscription_query(store, query, dict.new())
  json.to_string(data)
}

fn seed_basic_store() -> store.Store {
  let r1 =
    make_record(
      "gid://shopify/WebhookSubscription/1",
      Some("ORDERS_CREATE"),
      Some("https://hook-1"),
      Some("JSON"),
      Some("2024-01-01T00:00:00Z"),
      Some("2024-01-02T00:00:00Z"),
      Some(WebhookHttpEndpoint(callback_url: Some("https://hook-1"))),
    )
  let r2 =
    make_record(
      "gid://shopify/WebhookSubscription/2",
      Some("ORDERS_UPDATE"),
      Some("https://hook-2"),
      Some("JSON"),
      Some("2024-01-03T00:00:00Z"),
      Some("2024-01-04T00:00:00Z"),
      Some(WebhookHttpEndpoint(callback_url: Some("https://hook-2"))),
    )
  store.upsert_base_webhook_subscriptions(store.new(), [r1, r2])
}

pub fn webhook_subscriptions_connection_returns_nodes_test() {
  let result =
    handle(
      seed_basic_store(),
      "{ webhookSubscriptions(first: 5) { nodes { id topic uri } } }",
    )
  assert result
    == "{\"webhookSubscriptions\":{\"nodes\":[{\"id\":\"gid://shopify/WebhookSubscription/1\",\"topic\":\"ORDERS_CREATE\",\"uri\":\"https://hook-1\"},{\"id\":\"gid://shopify/WebhookSubscription/2\",\"topic\":\"ORDERS_UPDATE\",\"uri\":\"https://hook-2\"}]}}"
}

pub fn webhook_subscription_single_returns_record_test() {
  let result =
    handle(
      seed_basic_store(),
      "{ webhookSubscription(id: \"gid://shopify/WebhookSubscription/2\") { id topic } }",
    )
  assert result
    == "{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/2\",\"topic\":\"ORDERS_UPDATE\"}}"
}

pub fn webhook_subscription_single_returns_null_when_missing_test() {
  let result =
    handle(
      seed_basic_store(),
      "{ webhookSubscription(id: \"gid://shopify/WebhookSubscription/999\") { id } }",
    )
  assert result == "{\"webhookSubscription\":null}"
}

pub fn webhook_subscriptions_filter_by_topic_test() {
  let result =
    handle(
      seed_basic_store(),
      "{ webhookSubscriptions(first: 5, topics: [\"ORDERS_UPDATE\"]) { nodes { id topic } } }",
    )
  assert result
    == "{\"webhookSubscriptions\":{\"nodes\":[{\"id\":\"gid://shopify/WebhookSubscription/2\",\"topic\":\"ORDERS_UPDATE\"}]}}"
}

pub fn webhook_subscriptions_count_test() {
  let result =
    handle(
      seed_basic_store(),
      "{ webhookSubscriptionsCount { count precision } }",
    )
  assert result
    == "{\"webhookSubscriptionsCount\":{\"count\":2,\"precision\":\"EXACT\"}}"
}

pub fn webhook_subscriptions_count_with_limit_at_least_test() {
  let result =
    handle(
      seed_basic_store(),
      "{ webhookSubscriptionsCount(limit: 1) { count precision } }",
    )
  assert result
    == "{\"webhookSubscriptionsCount\":{\"count\":1,\"precision\":\"AT_LEAST\"}}"
}

pub fn webhook_subscriptions_endpoint_typename_routes_test() {
  let r =
    make_record(
      "gid://shopify/WebhookSubscription/3",
      Some("ORDERS_CREATE"),
      None,
      Some("JSON"),
      None,
      None,
      Some(
        WebhookEventBridgeEndpoint(arn: Some(
          "arn:aws:events:us-east-1:1:event-bus/default",
        )),
      ),
    )
  let s = store.upsert_base_webhook_subscriptions(store.new(), [r])
  let result =
    handle(
      s,
      "{ webhookSubscription(id: \"gid://shopify/WebhookSubscription/3\") { id endpoint { __typename ... on WebhookEventBridgeEndpoint { arn } } } }",
    )
  assert result
    == "{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/3\",\"endpoint\":{\"__typename\":\"WebhookEventBridgeEndpoint\",\"arn\":\"arn:aws:events:us-east-1:1:event-bus/default\"}}}"
}

pub fn webhook_subscriptions_uri_falls_back_to_endpoint_test() {
  let r =
    make_record(
      "gid://shopify/WebhookSubscription/4",
      Some("PRODUCTS_CREATE"),
      None,
      Some("JSON"),
      None,
      None,
      Some(WebhookPubSubEndpoint(
        pub_sub_project: Some("p"),
        pub_sub_topic: Some("t"),
      )),
    )
  let s = store.upsert_base_webhook_subscriptions(store.new(), [r])
  let result =
    handle(
      s,
      "{ webhookSubscription(id: \"gid://shopify/WebhookSubscription/4\") { id uri } }",
    )
  assert result
    == "{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/4\",\"uri\":\"pubsub://p:t\"}}"
}

pub fn webhook_subscriptions_legacy_resource_id_test() {
  let result =
    handle(
      seed_basic_store(),
      "{ webhookSubscription(id: \"gid://shopify/WebhookSubscription/2\") { legacyResourceId } }",
    )
  assert result == "{\"webhookSubscription\":{\"legacyResourceId\":\"2\"}}"
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

pub fn is_webhook_subscription_mutation_root_test() {
  assert webhooks.is_webhook_subscription_mutation_root(
    "webhookSubscriptionCreate",
  )
  assert webhooks.is_webhook_subscription_mutation_root(
    "webhookSubscriptionUpdate",
  )
  assert webhooks.is_webhook_subscription_mutation_root(
    "webhookSubscriptionDelete",
  )
  assert !webhooks.is_webhook_subscription_mutation_root("webhookSubscription")
  assert !webhooks.is_webhook_subscription_mutation_root("savedSearchCreate")
}

fn run_mutation(store_in: store.Store, document: String) -> String {
  let identity = synthetic_identity.new()
  let outcome =
    webhooks.process_mutation(
      store_in,
      identity,
      "/admin/api/2025-01/graphql.json",
      document,
      dict.new(),
      empty_upstream_context(),
    )
  json.to_string(outcome.data)
}

fn run_mutation_with_api_client_id(
  store_in: store.Store,
  document: String,
  api_client_id: String,
) -> String {
  let identity = synthetic_identity.new()
  let outcome =
    webhooks.process_mutation_with_headers(
      store_in,
      identity,
      "/admin/api/2025-01/graphql.json",
      document,
      dict.new(),
      dict.from_list([#("x-shopify-draft-proxy-api-client-id", api_client_id)]),
    )
  json.to_string(outcome.data)
}

fn run_mutation_outcome(
  store_in: store.Store,
  document: String,
) -> mutation_helpers.MutationOutcome {
  let identity = synthetic_identity.new()
  let outcome =
    webhooks.process_mutation(
      store_in,
      identity,
      "/admin/api/2025-01/graphql.json",
      document,
      dict.new(),
      empty_upstream_context(),
    )
  outcome
}

pub fn webhook_subscription_create_stages_record_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \"https://hooks.example.com/orders\", format: JSON }) { webhookSubscription { id topic uri format } userErrors { field message } } }"
  let outcome = run_mutation_outcome(store.new(), document)
  let body = json.to_string(outcome.data)
  // Body should be wrapped in {"data": {...}} with one staged subscription
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1?shopify-draft-proxy=synthetic\",\"topic\":\"ORDERS_CREATE\",\"uri\":\"https://hooks.example.com/orders\",\"format\":\"JSON\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids
    == ["gid://shopify/WebhookSubscription/1?shopify-draft-proxy=synthetic"]
  // The store should now have one effective record
  let records = store.list_effective_webhook_subscriptions(outcome.store)
  assert list.length(records) == 1
}

pub fn webhook_subscription_create_omitted_filter_stores_null_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { uri: \"https://hooks.example.com/shop\" }) { webhookSubscription { id filter } userErrors { field message } } }"
  let outcome = run_mutation_outcome(store.new(), document)
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1?shopify-draft-proxy=synthetic\",\"filter\":null},\"userErrors\":[]}}}"

  let records = store.list_effective_webhook_subscriptions(outcome.store)
  assert records
    == [
      WebhookSubscriptionRecord(
        id: "gid://shopify/WebhookSubscription/1?shopify-draft-proxy=synthetic",
        topic: Some("SHOP_UPDATE"),
        uri: Some("https://hooks.example.com/shop"),
        name: None,
        format: Some("JSON"),
        include_fields: [],
        metafield_namespaces: [],
        filter: None,
        created_at: Some("2024-01-01T00:00:00.000Z"),
        updated_at: Some("2024-01-01T00:00:00.000Z"),
        endpoint: Some(
          WebhookHttpEndpoint(callback_url: Some(
            "https://hooks.example.com/shop",
          )),
        ),
      ),
    ]

  assert handle(
      outcome.store,
      "{ webhookSubscription(id: \"gid://shopify/WebhookSubscription/1?shopify-draft-proxy=synthetic\") { id filter } }",
    )
    == "{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1?shopify-draft-proxy=synthetic\",\"filter\":null}}"
}

pub fn webhook_subscription_create_empty_filter_stores_empty_string_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { uri: \"https://hooks.example.com/shop\", filter: \"\" }) { webhookSubscription { id filter } userErrors { field message } } }"
  let outcome = run_mutation_outcome(store.new(), document)
  assert json.to_string(outcome.data)
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1?shopify-draft-proxy=synthetic\",\"filter\":\"\"},\"userErrors\":[]}}}"

  let records = store.list_effective_webhook_subscriptions(outcome.store)
  assert list.map(records, fn(record) { record.filter }) == [Some("")]
}

pub fn webhook_subscription_create_blank_uri_user_error_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \"\", format: JSON }) { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  // Empty URI → user-error; webhookSubscription is null
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address can't be blank\"}]}}}"
}

pub fn webhook_subscription_create_rejects_pubsub_no_topic_test() {
  assert_create_uri_rejected("pubsub://my-project", [
    "Address protocol pubsub:// is not supported",
    "Address is not a valid GCP pub/sub format. Format should be pubsub://project:topic",
  ])
}

pub fn webhook_subscription_create_rejects_bad_pubsub_project_test() {
  assert_create_uri_rejected("pubsub://-bad:topic", [
    "Address is invalid",
    "Address is not a valid GCP project id.",
  ])
}

pub fn webhook_subscription_create_rejects_bad_pubsub_topics_test() {
  assert_create_uri_rejected("pubsub://valid-project:goog-prefixed", [
    "Address is invalid",
    "Address is not a valid GCP topic id.",
  ])
  assert_create_uri_rejected("pubsub://valid-project:go", [
    "Address is invalid",
    "Address is not a valid GCP topic id.",
  ])
  assert_create_uri_rejected(
    "pubsub://valid-project:a" <> string.repeat("b", times: 255),
    ["Address is invalid", "Address is not a valid GCP topic id."],
  )
  assert_create_uri_rejected("pubsub://valid-project:bad/topic", [
    "Address is invalid",
    "Address is not a valid GCP topic id.",
  ])
}

pub fn webhook_subscription_create_rejects_malformed_arn_test() {
  assert_create_uri_rejected("arn:aws:events:bogus", [
    "Address is invalid",
    "Address is not a valid AWS ARN",
  ])
}

pub fn webhook_subscription_create_rejects_wrong_eventbridge_api_client_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \"arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/1/source\", format: JSON }) { webhookSubscription { id } userErrors { field message } } }"
  let body =
    run_mutation_with_api_client_id(store.new(), document, "347082227713")
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address is invalid\"},{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address is an AWS ARN and includes api_client_id '1' instead of '347082227713'\"}]}}}"
}

pub fn webhook_subscription_create_rejects_kafka_uri_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \"kafka://broker/topic\", format: JSON }) { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address protocol kafka:// is not supported\"},{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address is not a valid kafka topic\"}]}}}"
}

pub fn webhook_subscription_create_http_uri_user_error_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \"http://example.com\", format: JSON }) { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address protocol http:// is not supported\"}]}}}"
}

pub fn webhook_subscription_create_json_only_topic_xml_user_error_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: RETURNS_APPROVE, webhookSubscription: { uri: \"https://hooks.example.com/returns\", format: XML }) { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"format\"],\"message\":\"Format 'xml' is invalid for this webhook topic. Allowed formats: json\"}]}}}"
}

pub fn webhook_subscription_create_pubsub_xml_user_error_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { uri: \"pubsub://valid-project:topic\", format: XML }) { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"format\"],\"message\":\"Format can only be used with format: 'json'\"}]}}}"
}

pub fn webhook_subscription_create_empty_name_user_error_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { uri: \"https://hooks.example.com/shop\", name: \"\" }) { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"name\"],\"message\":\"Name is too short (minimum is 1 character)\"},{\"field\":[\"webhookSubscription\",\"name\"],\"message\":\"Name name field can only contain alphanumeric characters, underscores, and hyphens\"}]}}}"
}

pub fn webhook_subscription_create_bad_name_format_user_error_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { uri: \"https://hooks.example.com/shop\", name: \"has spaces\" }) { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"name\"],\"message\":\"Name name field can only contain alphanumeric characters, underscores, and hyphens\"}]}}}"
}

pub fn webhook_subscription_create_long_name_user_error_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { uri: \"https://hooks.example.com/shop\", name: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\" }) { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"name\"],\"message\":\"Name is too long (maximum is 50 characters)\"}]}}}"
}

pub fn webhook_subscription_create_duplicate_topic_uri_user_error_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { uri: \"https://hooks.example.com/dup\", format: JSON, filter: \"\" }) { webhookSubscription { id topic uri format filter } userErrors { field message } } }"
  let first_outcome = run_mutation_outcome(store.new(), document)
  let first_body = json.to_string(first_outcome.data)
  assert first_body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1?shopify-draft-proxy=synthetic\",\"topic\":\"SHOP_UPDATE\",\"uri\":\"https://hooks.example.com/dup\",\"format\":\"JSON\",\"filter\":\"\"},\"userErrors\":[]}}}"

  let second_body = run_mutation(first_outcome.store, document)
  assert second_body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address for this topic has already been taken\"}]}}}"
}

pub fn webhook_subscription_create_accepts_callback_url_alias_test() {
  // Real Shopify accepts `callbackUrl` on `WebhookSubscriptionInput` as a
  // legacy alias for `uri`. The proxy used to read only `uri`, fabricating
  // a misleading `["webhookSubscription", "callbackUrl"], "Address can't be
  // blank"` userError when the field WAS populated under its legacy name.
  let document =
    "mutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { callbackUrl: \"https://hooks.example.com/orders\", format: JSON }) { webhookSubscription { id topic uri format } userErrors { field message } } }"
  let outcome = run_mutation_outcome(store.new(), document)
  let body = json.to_string(outcome.data)
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1?shopify-draft-proxy=synthetic\",\"topic\":\"ORDERS_CREATE\",\"uri\":\"https://hooks.example.com/orders\",\"format\":\"JSON\"},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids
    == ["gid://shopify/WebhookSubscription/1?shopify-draft-proxy=synthetic"]
}

pub fn webhook_subscription_create_missing_topic_top_level_error_test() {
  // No `topic` argument → top-level GraphQL error
  let document =
    "mutation { webhookSubscriptionCreate(webhookSubscription: { uri: \"https://hooks.example.com/orders\" }) { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  // Body should be the {"errors": [...]} envelope, no data key
  assert body
    == "{\"errors\":[{\"message\":\"Field 'webhookSubscriptionCreate' is missing required arguments: topic\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"mutation\",\"webhookSubscriptionCreate\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"webhookSubscriptionCreate\",\"arguments\":\"topic\"}}]}"
}

pub fn webhook_subscription_create_null_topic_top_level_error_test() {
  let document =
    "mutation { webhookSubscriptionCreate(topic: null, webhookSubscription: { uri: \"https://hooks.example.com/orders\" }) { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"errors\":[{\"message\":\"Argument 'topic' on Field 'webhookSubscriptionCreate' has an invalid value (null). Expected type 'WebhookSubscriptionTopic!'.\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"mutation\",\"webhookSubscriptionCreate\",\"topic\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"Field\",\"argumentName\":\"topic\"}}]}"
}

fn seed_update_store() -> store.Store {
  let r =
    make_record(
      "gid://shopify/WebhookSubscription/1",
      Some("ORDERS_CREATE"),
      Some("https://old.example.com/hook"),
      Some("JSON"),
      Some("2024-01-01T00:00:00Z"),
      Some("2024-01-01T00:00:00Z"),
      Some(
        WebhookHttpEndpoint(callback_url: Some("https://old.example.com/hook")),
      ),
    )
  store.upsert_base_webhook_subscriptions(store.new(), [r])
}

pub fn webhook_subscription_update_modifies_record_test() {
  let s = seed_update_store()
  let document =
    "mutation { webhookSubscriptionUpdate(id: \"gid://shopify/WebhookSubscription/1\", webhookSubscription: { uri: \"https://new.example.com/hook\" }) { webhookSubscription { id uri } userErrors { field message } } }"
  let body = run_mutation(s, document)
  assert body
    == "{\"data\":{\"webhookSubscriptionUpdate\":{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1\",\"uri\":\"https://new.example.com/hook\"},\"userErrors\":[]}}}"
}

pub fn webhook_subscription_update_preserves_existing_null_filter_test() {
  let document =
    "mutation { webhookSubscriptionUpdate(id: \"gid://shopify/WebhookSubscription/1\", webhookSubscription: { uri: \"https://new.example.com/hook\" }) { webhookSubscription { id uri filter } userErrors { field message } } }"
  let outcome = run_mutation_outcome(seed_update_store(), document)
  assert json.to_string(outcome.data)
    == "{\"data\":{\"webhookSubscriptionUpdate\":{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1\",\"uri\":\"https://new.example.com/hook\",\"filter\":null},\"userErrors\":[]}}}"

  assert handle(
      outcome.store,
      "{ webhookSubscription(id: \"gid://shopify/WebhookSubscription/1\") { id filter } }",
    )
    == "{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1\",\"filter\":null}}"
}

pub fn webhook_subscription_update_empty_filter_sets_empty_string_test() {
  let document =
    "mutation { webhookSubscriptionUpdate(id: \"gid://shopify/WebhookSubscription/1\", webhookSubscription: { filter: \"\" }) { webhookSubscription { id filter } userErrors { field message } } }"
  let outcome = run_mutation_outcome(seed_update_store(), document)
  assert json.to_string(outcome.data)
    == "{\"data\":{\"webhookSubscriptionUpdate\":{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1\",\"filter\":\"\"},\"userErrors\":[]}}}"

  assert handle(
      outcome.store,
      "{ webhookSubscription(id: \"gid://shopify/WebhookSubscription/1\") { id filter } }",
    )
    == "{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1\",\"filter\":\"\"}}"
}

pub fn webhook_subscription_update_blank_uri_user_error_leaves_record_test() {
  let document =
    "mutation { webhookSubscriptionUpdate(id: \"gid://shopify/WebhookSubscription/1\", webhookSubscription: { uri: \"\" }) { webhookSubscription { id uri } userErrors { field message } } }"
  let outcome = run_mutation_outcome(seed_update_store(), document)
  assert json.to_string(outcome.data)
    == "{\"data\":{\"webhookSubscriptionUpdate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address can't be blank\"}]}}}"
  assert outcome.staged_resource_ids == []
  assert handle(
      outcome.store,
      "{ webhookSubscription(id: \"gid://shopify/WebhookSubscription/1\") { id uri } }",
    )
    == "{\"webhookSubscription\":{\"id\":\"gid://shopify/WebhookSubscription/1\",\"uri\":\"https://old.example.com/hook\"}}"
}

pub fn webhook_subscription_update_blank_callback_url_user_error_test() {
  let document =
    "mutation { webhookSubscriptionUpdate(id: \"gid://shopify/WebhookSubscription/1\", webhookSubscription: { callbackUrl: \"\" }) { webhookSubscription { id uri } userErrors { field message } } }"
  let body = run_mutation(seed_update_store(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionUpdate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address can't be blank\"}]}}}"
}

pub fn webhook_subscription_update_http_uri_user_error_test() {
  let document =
    "mutation { webhookSubscriptionUpdate(id: \"gid://shopify/WebhookSubscription/1\", webhookSubscription: { uri: \"http://example.com\" }) { webhookSubscription { id uri } userErrors { field message } } }"
  let body = run_mutation(seed_update_store(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionUpdate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address protocol http:// is not supported\"}]}}}"
}

pub fn webhook_subscription_update_invalid_pubsub_uri_user_error_test() {
  let document =
    "mutation { webhookSubscriptionUpdate(id: \"gid://shopify/WebhookSubscription/1\", webhookSubscription: { uri: \"pubsub://valid-project:\" }) { webhookSubscription { id uri } userErrors { field message } } }"
  let body = run_mutation(seed_update_store(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionUpdate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address protocol pubsub:// is not supported\"},{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address is not a valid GCP pub/sub format. Format should be pubsub://project:topic\"}]}}}"
}

pub fn webhook_subscription_update_invalid_filter_user_error_test() {
  let document =
    "mutation { webhookSubscriptionUpdate(id: \"gid://shopify/WebhookSubscription/1\", webhookSubscription: { filter: \"invalid_field:*\" }) { webhookSubscription { id filter } userErrors { field message } } }"
  let body = run_mutation(seed_update_store(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionUpdate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"filter\"],\"message\":\"The specified filter is invalid, please ensure you specify the field(s) you wish to filter on.\"}]}}}"
}

pub fn webhook_subscription_update_rejects_cloud_uri_validation_errors_test() {
  assert_update_uri_rejected("pubsub://my-project", [
    "Address protocol pubsub:// is not supported",
    "Address is not a valid GCP pub/sub format. Format should be pubsub://project:topic",
  ])
  assert_update_uri_rejected("pubsub://abc:topic", [
    "Address is invalid",
    "Address is not a valid GCP project id.",
  ])
  assert_update_uri_rejected("pubsub://valid-project:goog-prefixed", [
    "Address is invalid",
    "Address is not a valid GCP topic id.",
  ])
  assert_update_uri_rejected("arn:aws:events:bogus", [
    "Address is invalid",
    "Address is not a valid AWS ARN",
  ])
}

pub fn webhook_subscription_update_rejects_kafka_uri_test() {
  let document =
    "mutation { webhookSubscriptionUpdate(id: \"gid://shopify/WebhookSubscription/1\", webhookSubscription: { uri: \"kafka://broker/topic\" }) { webhookSubscription { id uri } userErrors { field message } } }"
  let body = run_mutation(seed_webhook_subscription(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionUpdate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address protocol kafka:// is not supported\"},{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address is not a valid kafka topic\"}]}}}"
}

pub fn webhook_subscription_update_rejects_wrong_eventbridge_api_client_test() {
  let document =
    "mutation { webhookSubscriptionUpdate(id: \"gid://shopify/WebhookSubscription/1\", webhookSubscription: { uri: \"arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/1/source\" }) { webhookSubscription { id uri } userErrors { field message } } }"
  let body =
    run_mutation_with_api_client_id(
      seed_webhook_subscription(),
      document,
      "347082227713",
    )
  assert body
    == "{\"data\":{\"webhookSubscriptionUpdate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address is invalid\"},{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\"Address is an AWS ARN and includes api_client_id '1' instead of '347082227713'\"}]}}}"
}

pub fn webhook_subscription_update_unknown_id_user_error_test() {
  let document =
    "mutation { webhookSubscriptionUpdate(id: \"gid://shopify/WebhookSubscription/999\", webhookSubscription: { uri: \"https://new\" }) { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionUpdate\":{\"webhookSubscription\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Webhook subscription does not exist\"}]}}}"
}

pub fn webhook_subscription_update_missing_id_top_level_error_test() {
  // Missing `id` → top-level GraphQL error (and missing webhookSubscription too)
  let document =
    "mutation { webhookSubscriptionUpdate { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"errors\":[{\"message\":\"Field 'webhookSubscriptionUpdate' is missing required arguments: id, webhookSubscription\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"mutation\",\"webhookSubscriptionUpdate\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"webhookSubscriptionUpdate\",\"arguments\":\"id, webhookSubscription\"}}]}"
}

fn assert_create_uri_rejected(uri: String, messages: List(String)) {
  let document =
    "mutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \""
    <> uri
    <> "\", format: JSON }) { webhookSubscription { id } userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionCreate\":{\"webhookSubscription\":null,\"userErrors\":"
    <> user_errors_json(messages)
    <> "}}}"
}

fn assert_update_uri_rejected(uri: String, messages: List(String)) {
  let document =
    "mutation { webhookSubscriptionUpdate(id: \"gid://shopify/WebhookSubscription/1\", webhookSubscription: { uri: \""
    <> uri
    <> "\" }) { webhookSubscription { id uri } userErrors { field message } } }"
  let body = run_mutation(seed_webhook_subscription(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionUpdate\":{\"webhookSubscription\":null,\"userErrors\":"
    <> user_errors_json(messages)
    <> "}}}"
}

fn user_errors_json(messages: List(String)) -> String {
  "[" <> string.join(list.map(messages, user_error_json), ",") <> "]"
}

fn user_error_json(message: String) -> String {
  "{\"field\":[\"webhookSubscription\",\"callbackUrl\"],\"message\":\""
  <> message
  <> "\"}"
}

fn seed_webhook_subscription() -> store.Store {
  let r =
    make_record(
      "gid://shopify/WebhookSubscription/1",
      Some("ORDERS_CREATE"),
      Some("https://old"),
      Some("JSON"),
      Some("2024-01-01T00:00:00Z"),
      Some("2024-01-01T00:00:00Z"),
      Some(WebhookHttpEndpoint(callback_url: Some("https://old"))),
    )
  store.upsert_base_webhook_subscriptions(store.new(), [r])
}

pub fn webhook_subscription_delete_removes_record_test() {
  let r =
    make_record(
      "gid://shopify/WebhookSubscription/1",
      Some("ORDERS_CREATE"),
      Some("https://hooks"),
      Some("JSON"),
      Some("2024-01-01T00:00:00Z"),
      Some("2024-01-01T00:00:00Z"),
      Some(WebhookHttpEndpoint(callback_url: Some("https://hooks"))),
    )
  let s = store.upsert_base_webhook_subscriptions(store.new(), [r])
  let document =
    "mutation { webhookSubscriptionDelete(id: \"gid://shopify/WebhookSubscription/1\") { deletedWebhookSubscriptionId userErrors { field message } } }"
  let outcome = run_mutation_outcome(s, document)
  assert json.to_string(outcome.data)
    == "{\"data\":{\"webhookSubscriptionDelete\":{\"deletedWebhookSubscriptionId\":\"gid://shopify/WebhookSubscription/1\",\"userErrors\":[]}}}"
  // The store should now hide the record
  assert store.get_effective_webhook_subscription_by_id(
      outcome.store,
      "gid://shopify/WebhookSubscription/1",
    )
    == None
}

pub fn webhook_subscription_delete_unknown_id_user_error_test() {
  let document =
    "mutation { webhookSubscriptionDelete(id: \"gid://shopify/WebhookSubscription/999\") { deletedWebhookSubscriptionId userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"data\":{\"webhookSubscriptionDelete\":{\"deletedWebhookSubscriptionId\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Webhook subscription does not exist\"}]}}}"
}

pub fn webhook_subscription_delete_null_id_top_level_error_test() {
  let document =
    "mutation { webhookSubscriptionDelete(id: null) { deletedWebhookSubscriptionId userErrors { field message } } }"
  let body = run_mutation(store.new(), document)
  assert body
    == "{\"errors\":[{\"message\":\"Argument 'id' on Field 'webhookSubscriptionDelete' has an invalid value (null). Expected type 'ID!'.\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"mutation\",\"webhookSubscriptionDelete\",\"id\"],\"extensions\":{\"code\":\"argumentLiteralsIncompatible\",\"typeName\":\"Field\",\"argumentName\":\"id\"}}]}"
}
