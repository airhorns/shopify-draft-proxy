# Webhooks

HAR-267 adds conformance evidence for Admin GraphQL webhook subscription roots. HAR-268 adds local runtime read support for API-created webhook subscription records. HAR-269 adds local staging for the captured create/update registration subset without firing webhooks or registering real subscriptions upstream during supported runtime handling. HAR-270 adds local deregistration staging for the same captured API-created subset without unsubscribing real Shopify webhook subscriptions at runtime.

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

`webhookSubscriptionCreate`, `webhookSubscriptionUpdate`, and `webhookSubscriptionDelete` are registered under the `webhooks` domain as implemented `stage-locally` operations for the captured Admin API-created HTTP URI subset.

## Local Read Behavior

- Webhook subscription reads are backed by normalized `webhookSubscriptions` state plus `webhookSubscriptionOrder`.
- Snapshot mode returns `null` for unknown `webhookSubscription(id:)`, an empty `webhookSubscriptions` connection, and `{ count: 0, precision: "EXACT" }` for `webhookSubscriptionsCount` when no records are present.
- Local records preserve captured fields: `id`, `topic`, `format`, `includeFields`, `metafieldNamespaces`, `filter`, `createdAt`, `updatedAt`, and endpoint-specific fields for `WebhookHttpEndpoint`, `WebhookEventBridgeEndpoint`, and `WebhookPubSubEndpoint`.
- `webhookSubscriptions` uses shared connection helpers for `nodes`, `edges`, selected `pageInfo`, stable synthetic cursors, `first`/`last`, `before`/`after`, `sortKey: ID`, and `reverse`.
- `webhookSubscriptionsCount` supports count `limit` semantics with `EXACT` / `AT_LEAST` precision and simple captured query filtering such as `id:<legacy id>`.
- Live-hybrid reads hydrate upstream webhook subscription nodes into normalized base state and overlay staged local records when present.

## Local Mutation Behavior

- `webhookSubscriptionCreate(topic:, webhookSubscription:)` stages a new normalized webhook subscription with a proxy synthetic `WebhookSubscription` GID, stable synthetic timestamps, the requested `topic`, `format`, `includeFields`, `metafieldNamespaces`, `filter`, and a `WebhookHttpEndpoint.callbackUrl` derived from the captured `WebhookSubscriptionInput.uri` shape.
- `webhookSubscriptionUpdate(id:, webhookSubscription:)` updates an existing staged or hydrated webhook subscription in place, preserving `topic` and `createdAt` while replacing the captured mutable fields and endpoint URI when present.
- `webhookSubscriptionDelete(id:)` stages a deletion for staged, synthetic, or hydrated/local webhook subscriptions. The successful payload returns `deletedWebhookSubscriptionId` and an empty `userErrors` list; subsequent `webhookSubscription(id:)` reads return `null`, and list/count reads omit the deleted subscription.
- Successful create/update/delete mutations append staged entries to the meta mutation log with the original request body intact for commit replay. Commit replay replaces synthetic IDs with upstream IDs from earlier successful replay attempts before replaying later raw request bodies.
- Captured validation branches are handled locally without upstream writes: create without `uri` returns `webhookSubscription: null` and `userErrors: [{ field: ["webhookSubscription", "callbackUrl"], message: "Address can't be blank" }]`; update of an unknown ID returns `webhookSubscription: null` and `userErrors: [{ field: ["id"], message: "Webhook subscription does not exist" }]`; delete of an unknown or already deleted ID returns `deletedWebhookSubscriptionId: null` and `userErrors: [{ field: ["id"], message: "Webhook subscription does not exist" }]`.
- Missing or null `webhookSubscriptionDelete(id:)` arguments return Shopify-like GraphQL validation errors locally and do not append mutation-log entries.
- Local registration/update/delete does not deliver webhook payloads and does not create, update, or unsubscribe real Shopify webhook subscriptions at runtime.

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

## Validation

- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
- `corepack pnpm typecheck`
- `corepack pnpm vitest run tests/integration/webhook-subscription-mutation-flow.test.ts`
- `corepack pnpm vitest run tests/integration/webhook-subscription-query-shapes.test.ts`
- `corepack pnpm vitest run tests/unit/webhook-subscription-conformance-fixture.test.ts`
