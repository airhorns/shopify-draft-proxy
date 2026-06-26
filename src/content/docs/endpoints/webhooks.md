---
title: 'Webhooks'
description: 'Coverage notes and fidelity boundaries for Webhooks.'
---

This endpoint group covers Shopify Admin GraphQL API-created webhook subscription roots for catalog reads, count reads, local subscription staging, and local deregistration. It does not cover app configuration/TOML webhook subscriptions or actual webhook delivery.

## Current support and limitations

### Supported roots

Read roots:

- `webhookSubscription(id:)`
- `webhookSubscriptions(...)`
- `webhookSubscriptionsCount(...)`

Mutation roots:

- `webhookSubscriptionCreate(topic:, webhookSubscription:)`
- `webhookSubscriptionUpdate(id:, webhookSubscription:)`
- `webhookSubscriptionDelete(id:)`
- `pubSubWebhookSubscriptionCreate(topic:, webhookSubscription:)`
- `pubSubWebhookSubscriptionUpdate(id:, webhookSubscription:)`
- `eventBridgeWebhookSubscriptionCreate(topic:, webhookSubscription:)`
- `eventBridgeWebhookSubscriptionUpdate(id:, webhookSubscription:)`

### Local behavior

Webhook subscription reads are backed by normalized `webhookSubscriptions` state plus `webhookSubscriptionOrder`:

- Snapshot mode returns `null` for unknown `webhookSubscription(id:)`, an empty `webhookSubscriptions` connection, and `{ count: 0, precision: "EXACT" }` for `webhookSubscriptionsCount` when no records are present.
- Local records preserve captured fields: `id`, `topic`, `uri`, `name`, `format`, `includeFields`, `metafieldNamespaces`, `metafields`, `filter`, `apiVersion`, `createdAt`, `updatedAt`, and deprecated endpoint-specific fields for HTTP, EventBridge, and Pub/Sub endpoints.
- `webhookSubscriptions(...)` uses shared connection helpers for `nodes`, `edges`, selected `pageInfo`, stable synthetic cursors, `first`/`last`, `before`/`after`, `sortKey: ID`, and `reverse`.
- Catalog filters cover captured Shopify filters for `uri`, deprecated `callbackUrl`, `format`, and `topics`.
- `webhookSubscriptionsCount(...)` supports `limit` precision semantics and captured query filtering for IDs, topic, format, URI, and endpoint fragments.
- LiveHybrid reads hydrate upstream webhook subscription nodes into normalized base state and overlay staged local records.

Subscription lifecycle mutations stage locally and retain the original raw mutation for commit replay:

- `webhookSubscriptionCreate` rejects blank or malformed `WebhookSubscriptionTopic` values before resolver side effects, but it does not freeze topic validation to a captured enum snapshot. Non-empty uppercase enum-shaped topic names stage a synthetic local `WebhookSubscription` record after address, format, name, duplicate, filter, and namespace validation passes.
- `webhookSubscriptionUpdate` updates an existing staged or hydrated subscription in place, preserving `topic` and `createdAt` while replacing supported mutable fields.
- `webhookSubscriptionDelete` records local deletion state. Downstream detail reads return `null`, and list/count reads omit deleted subscriptions.
- `$app:<suffix>` `metafieldNamespaces` entries resolve through request-owned `x-shopify-draft-proxy-api-client-id` when available. Without a caller API client ID, the proxy preserves `$app:` input unchanged rather than fabricating an identity.
- `metafields` accepts and stores the webhook payload metafield identifier list as `[{ namespace, key }]`. Create/update payloads, detail reads, and list reads project the stored identifiers, and omitted input projects Shopify's non-null empty list `[]`.
- Unified `uri` input derives endpoint projections: HTTPS URIs keep the same top-level deprecated `callbackUrl` and become `WebhookHttpEndpoint.callbackUrl`; valid `pubsub://project:topic` URIs and Shopify partner EventBridge ARNs keep the real address in `uri` and `endpoint`, while the top-level deprecated `callbackUrl` returns Shopify's `https://eventbridge.arn` placeholder.
- Dedicated Pub/Sub create/update roots normalize `pubSubProject` plus `pubSubTopic` into the stored `pubsub://project:topic` URI while preserving dedicated validation field paths.
- Pub/Sub GCP project validation accepts all-numeric project numbers in addition to lowercase alpha-start project IDs. Topic validation requires an ASCII letter first character and accepts literal percent signs when represented by a valid percent-encoded `%25` sequence; encoded invalid characters such as `%20` are rejected like Shopify.
- Dedicated EventBridge create/update roots normalize `arn` into the stored URI/address while preserving dedicated validation field paths.
- Commit replay replaces synthetic IDs with upstream IDs from prior successful replay attempts before replaying subsequent raw request bodies.

Validation and no-side-effect behavior:

- Missing or null required arguments return Shopify-like GraphQL validation errors and do not append mutation-log entries.
- Create/update reject blank addresses, non-HTTPS HTTP callback URLs, malformed Pub/Sub/project/topic values, malformed EventBridge ARNs, wrong EventBridge API client IDs when known, public Kafka URIs, Shopify/internal callback hosts, invalid topic/format combinations, invalid names, duplicate webhook names, and duplicate active `(topic, uri, format, filter, apiPermissionId)` registrations without staging.
- EventBridge ARN validation requires the captured Shopify partner event-source shape `arn:aws:events:<region>::event-source/aws.partner/shopify.com(.test)?/<api_client_id>/<event_source_name>`. The embedded `api_client_id` is compared only when the request includes `x-shopify-draft-proxy-api-client-id`; without that caller identity, the proxy still rejects malformed or non-partner ARNs but cannot prove wrong-app ownership.
- Unknown update/delete IDs return captured userErrors and leave local state unchanged.
- Whitespace-only `uri` is treated as blank; leading/trailing whitespace around a valid HTTPS URI is trimmed before storage.
- Callback address byte-size validation uses Shopify's MySQL text-column maximum of 65,535 bytes.
- Filter byte-size validation uses the same 65,535-byte maximum and takes precedence over filter syntax validation.
- Shop-owned callback host validation uses effective shop state or a LiveHybrid upstream shop baseline when available. The proxy rejects the effective non-static `primaryDomain.host` as shop-owned and keeps exact-host matching only.
- When a webhook record has no hydrated API-version metadata, the local `apiVersion` projection derives from the Admin route or `x-shopify-draft-proxy-api-version`: `handle` and `displayName` use the requested handle, and `supported` is `false` only for `unstable`.

Supported create/update/delete operations do not deliver webhook payloads and do not create, update, or unsubscribe real Shopify webhook subscriptions at runtime.

### Boundaries

- App configuration/TOML webhook subscriptions are out of scope for these Admin GraphQL roots.
- Topic-specific permission/business-rule failures are not hardcoded unless captured for a concrete topic.
- Destination-specific validation beyond captured structural URI, Shopify/internal host, Pub/Sub, EventBridge, and Kafka branches is not generalized.
- HTTP endpoint reachability, cloud destination existence, app configuration, provider credentials, and denied plural shop-domain visibility are not modeled.
- Webhook payload delivery, retry scheduling, HMAC signing, and external HTTP/EventBridge/Pub/Sub publishing are not emulated.
- A webhook outbox is the intended default direction for deterministic local observability, but no outbox root is documented here as supported until topic-specific payload fixtures and meta API behavior exist.
- No listed webhook root is registry-only. Validation-only branches are captured create/update/delete failures that return errors without staging.

### Evidence

- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-conformance.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-topic-enum-validation.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-cloud-uri-validation.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-dedicated-cloud-destinations.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/gcp-project-topic-char-rules.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-uri-validation.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-uri-whitespace.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-address-byte-size-validation.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-metafield-namespaces-resolution.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-metafields-lifecycle.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-topic-format-name-validation.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/eventbridge-cloud-format-json-only.json`
- `config/parity-specs/webhooks/webhook-subscription-catalog-read.json`
- `config/parity-specs/webhooks/webhook-subscription-conformance.json`
- `config/parity-specs/webhooks/webhook-subscription-required-argument-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-topic-enum-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-cloud-uri-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-dedicated-cloud-destinations.json`
- `config/parity-specs/webhooks/eventbridge-cloud-format-json-only.json`
- `config/parity-specs/webhooks/gcp-project-topic-char-rules.json`
- `config/parity-specs/webhooks/webhook-subscription-update-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-uri-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-uri-whitespace.json`
- `config/parity-specs/webhooks/webhook-subscription-address-byte-size-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-metafield-namespaces-resolution.json`
- `config/parity-specs/webhooks/webhook-subscription-metafields-lifecycle.json`
- `config/parity-specs/webhooks/webhook-subscription-topic-format-name-validation.json`
- `scripts/capture-webhook-subscription-conformance.ts`
- `scripts/capture-webhook-subscription-topic-enum-validation.ts`
- `scripts/capture-webhook-cloud-uri-validation-conformance.ts`
- `scripts/capture-webhook-dedicated-cloud-destinations-conformance.ts`
- `scripts/capture-webhook-eventbridge-cloud-format-json-only-conformance.ts`
- `scripts/capture-webhook-gcp-project-topic-char-rules-conformance.ts`
- `scripts/capture-webhook-subscription-metafields-conformance.ts`
- `scripts/capture-webhook-subscription-uri-whitespace.ts`
- `scripts/capture-webhook-subscription-address-byte-size-validation.ts`
- Runtime coverage: `cargo test --test graphql_routes admin_graphql_webhooks::`

### Validation

- `corepack pnpm parity -- webhook-subscription-catalog-read`
- `corepack pnpm parity -- webhook-subscription-conformance`
- `corepack pnpm parity -- webhook-subscription-cloud-uri-validation`
- `corepack pnpm parity -- webhook-subscription-dedicated-cloud-destinations`
- `corepack pnpm parity -- webhook-subscription-metafields-lifecycle`
- `corepack pnpm parity -- eventbridge-cloud-format-json-only`
- `corepack pnpm parity -- gcp-project-topic-char-rules`
- `cargo test --test graphql_routes admin_graphql_webhooks::`
- `corepack pnpm conformance:check`
