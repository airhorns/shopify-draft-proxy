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
- Local records preserve captured fields: `id`, `topic`, `uri`, `name`, `format`, `includeFields`, `metafieldNamespaces`, `filter`, `createdAt`, `updatedAt`, and deprecated endpoint-specific fields for HTTP, EventBridge, and Pub/Sub endpoints.
- `webhookSubscriptions(...)` uses shared connection helpers for `nodes`, `edges`, selected `pageInfo`, stable synthetic cursors, `first`/`last`, `before`/`after`, `sortKey: ID`, and `reverse`.
- Catalog filters cover captured Shopify filters for `uri`, deprecated `callbackUrl`, `format`, and `topics`.
- `webhookSubscriptionsCount(...)` supports `limit` precision semantics and captured query filtering for IDs, topic, format, URI, and endpoint fragments.
- LiveHybrid reads hydrate upstream webhook subscription nodes into normalized base state and overlay staged local records.

Subscription lifecycle mutations stage locally and retain the original raw mutation for commit replay:

- `webhookSubscriptionCreate` rejects unknown or non-public `WebhookSubscriptionTopic` values before resolver side effects. Accepted public topics stage a synthetic local `WebhookSubscription` record after address, format, name, duplicate, filter, and namespace validation passes.
- `webhookSubscriptionUpdate` updates an existing staged or hydrated subscription in place, preserving `topic` and `createdAt` while replacing supported mutable fields.
- `webhookSubscriptionDelete` records local deletion state. Downstream detail reads return `null`, and list/count reads omit deleted subscriptions.
- `$app:<suffix>` `metafieldNamespaces` entries resolve through request-owned `x-shopify-draft-proxy-api-client-id` when available. Without a caller API client ID, the proxy preserves `$app:` input unchanged rather than fabricating an identity.
- Unified `uri` input derives deprecated endpoint projections: HTTPS URIs become `WebhookHttpEndpoint.callbackUrl`, valid `pubsub://project:topic` URIs become `WebhookPubSubEndpoint`, and valid EventBridge ARNs become `WebhookEventBridgeEndpoint`.
- Dedicated Pub/Sub create/update roots normalize `pubSubProject` plus `pubSubTopic` into the stored `pubsub://project:topic` URI while preserving dedicated validation field paths.
- Dedicated EventBridge create/update roots normalize `arn` into the stored URI/address while preserving dedicated validation field paths.
- Commit replay replaces synthetic IDs with upstream IDs from prior successful replay attempts before replaying subsequent raw request bodies.

Validation and no-side-effect behavior:

- Missing or null required arguments return Shopify-like GraphQL validation errors and do not append mutation-log entries.
- Create/update reject blank addresses, non-HTTPS HTTP callback URLs, malformed Pub/Sub/project/topic values, malformed EventBridge ARNs, wrong EventBridge API client IDs when known, public Kafka URIs, Shopify/internal callback hosts, invalid topic/format combinations, invalid names, duplicate webhook names, and duplicate active `(topic, uri, format, filter)` registrations without staging.
- Unknown update/delete IDs return captured userErrors and leave local state unchanged.
- Whitespace-only `uri` is treated as blank; leading/trailing whitespace around a valid HTTPS URI is trimmed before storage.
- Callback address byte-size validation uses Shopify's MySQL text-column maximum of 65,535 bytes.
- Shop-owned callback host validation uses effective shop state or a LiveHybrid upstream shop baseline when available. The proxy rejects the effective non-static `primaryDomain.host` as shop-owned and keeps exact-host matching only.

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
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-uri-validation.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-uri-whitespace.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-address-byte-size-validation.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-metafield-namespaces-resolution.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-topic-format-name-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-catalog-read.json`
- `config/parity-specs/webhooks/webhook-subscription-conformance.json`
- `config/parity-specs/webhooks/webhook-subscription-required-argument-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-topic-enum-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-cloud-uri-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-dedicated-cloud-destinations.json`
- `config/parity-specs/webhooks/webhook-subscription-update-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-uri-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-uri-whitespace.json`
- `config/parity-specs/webhooks/webhook-subscription-address-byte-size-validation.json`
- `config/parity-specs/webhooks/webhook-subscription-metafield-namespaces-resolution.json`
- `config/parity-specs/webhooks/webhook-subscription-topic-format-name-validation.json`
- `scripts/capture-webhook-subscription-conformance.ts`
- `scripts/capture-webhook-subscription-topic-enum-validation.ts`
- `scripts/capture-webhook-cloud-uri-validation-conformance.ts`
- `scripts/capture-webhook-dedicated-cloud-destinations-conformance.ts`
- `scripts/capture-webhook-subscription-uri-whitespace.ts`
- `scripts/capture-webhook-subscription-address-byte-size-validation.ts`

### Validation

- `corepack pnpm parity -- webhook-subscription-catalog-read`
- `corepack pnpm parity -- webhook-subscription-conformance`
- `corepack pnpm parity -- webhook-subscription-cloud-uri-validation`
- `corepack pnpm parity -- webhook-subscription-dedicated-cloud-destinations`
- `corepack pnpm conformance:check`
