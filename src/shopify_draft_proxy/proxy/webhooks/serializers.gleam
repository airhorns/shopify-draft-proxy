//// Serializers and GraphQL projection helpers for webhook subscriptions.

import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, SrcList, SrcNull, SrcString, project_graphql_value,
  src_object,
}
import shopify_draft_proxy/proxy/webhooks/filters.{
  webhook_subscription_legacy_id, webhook_subscription_uri,
}
import shopify_draft_proxy/proxy/webhooks/types as webhook_types
import shopify_draft_proxy/state/types.{
  type WebhookSubscriptionEndpoint, type WebhookSubscriptionRecord,
  WebhookEventBridgeEndpoint, WebhookHttpEndpoint, WebhookPubSubEndpoint,
}

@internal
pub fn project_webhook_subscription(
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

@internal
pub fn webhook_subscription_to_source(
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

// ---- Mutation projection --------------------------------------------------

@internal
pub fn project_create_payload(
  record: Option(WebhookSubscriptionRecord),
  user_errors: List(webhook_types.UserError),
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

@internal
pub fn project_update_payload(
  record: Option(WebhookSubscriptionRecord),
  user_errors: List(webhook_types.UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_create_payload(record, user_errors, field, fragments)
}

@internal
pub fn project_delete_payload(
  deleted_id: Option(String),
  user_errors: List(webhook_types.UserError),
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

fn user_error_to_source(
  error: webhook_types.UserError,
) -> graphql_helpers.SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", SrcList(list.map(error.field, fn(part) { SrcString(part) }))),
    #("message", SrcString(error.message)),
  ])
}
