# Webhooks

HAR-267 adds conformance evidence for Admin GraphQL webhook subscription roots. HAR-268 adds local runtime read support for API-created webhook subscription records while leaving create/update/delete staging unsupported.

## Registered Roots

- `webhookSubscription(id:)`
- `webhookSubscriptions(...)`
- `webhookSubscriptionsCount(...)`
- `webhookSubscriptionCreate(topic:, webhookSubscription:)`
- `webhookSubscriptionUpdate(id:, webhookSubscription:)`
- `webhookSubscriptionDelete(id:)`

The three query roots are registered under the `webhooks` domain as implemented `overlay-read` operations:

- `webhookSubscription(id:)`
- `webhookSubscriptions(...)`
- `webhookSubscriptionsCount(...)`

The mutation roots remain registered but unimplemented. They must not be treated as supported local staging until a webhook subscription mutation lifecycle model exists.

## Local Read Behavior

- Webhook subscription reads are backed by normalized `webhookSubscriptions` state plus `webhookSubscriptionOrder`.
- Snapshot mode returns `null` for unknown `webhookSubscription(id:)`, an empty `webhookSubscriptions` connection, and `{ count: 0, precision: "EXACT" }` for `webhookSubscriptionsCount` when no records are present.
- Local records preserve captured fields: `id`, `topic`, `format`, `includeFields`, `metafieldNamespaces`, `filter`, `createdAt`, `updatedAt`, and endpoint-specific fields for `WebhookHttpEndpoint`, `WebhookEventBridgeEndpoint`, and `WebhookPubSubEndpoint`.
- `webhookSubscriptions` uses shared connection helpers for `nodes`, `edges`, selected `pageInfo`, stable synthetic cursors, `first`/`last`, `before`/`after`, `sortKey: ID`, and `reverse`.
- `webhookSubscriptionsCount` supports count `limit` semantics with `EXACT` / `AT_LEAST` precision and simple captured query filtering such as `id:<legacy id>`.
- Live-hybrid reads hydrate upstream webhook subscription nodes into normalized base state and overlay staged local records when present.

## Captured Evidence

`fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhook-subscription-conformance.json` records:

- empty/no-data `webhookSubscriptions` catalog behavior
- empty and filtered `webhookSubscriptionsCount` behavior
- unknown-id `webhookSubscription` detail null behavior
- create/update/delete payloads for a temporary HTTP `SHOP_UPDATE` subscription
- detail read-after-create, read-after-update, and read-after-delete behavior
- missing URI validation on create
- unknown-id validation on update and delete

The capture used `WebhookSubscriptionInput.uri` with an `https://example.com/...` HTTP endpoint, `format: JSON`, selected `includeFields`, and selected `metafieldNamespaces`. The created subscription was deleted during the same script run.

## Access And Scope Notes

The capture fixture includes the active app access scopes returned by `currentAppInstallation.accessScopes`. The captured grant did not expose dedicated `read_webhooks` or `write_webhooks` handles; it could still read and manage API-created subscriptions for the app. Topic-specific requirements can still vary by topic, so future runtime work should keep scope/topic failures as conformance-backed validation rather than hardcoded assumptions.

The lifecycle capture uses `SHOP_UPDATE` because it is available in the topic enum and does not require creating or modifying products, orders, customers, inventory, or other domain records. The script does not trigger any shop update or delivery probe, so no webhook delivery is intentionally sent during HAR-267.

## Out Of Scope

App configuration/TOML webhooks remain out of scope. Shopify's Admin GraphQL subscription roots are being treated here as the API-created subscription lifecycle surface; future evidence must prove otherwise before TOML/app-config webhooks are modeled through these roots.

Runtime mutation staging is still out of scope. Future support should add create/update/delete staging, read-after-write effects from those mutation handlers, raw mutation log retention, and tests showing supported webhook mutations do not hit Shopify at runtime.

## Validation

- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
- `corepack pnpm typecheck`
- `corepack pnpm vitest run tests/integration/webhook-subscription-query-shapes.test.ts`
- `corepack pnpm vitest run tests/unit/webhook-subscription-conformance-fixture.test.ts`
