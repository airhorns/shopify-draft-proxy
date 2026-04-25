# Webhooks

HAR-267 adds conformance evidence for Admin GraphQL webhook subscription roots, but does not implement local runtime support yet.

## Registered Roots

- `webhookSubscription(id:)`
- `webhookSubscriptions(...)`
- `webhookSubscriptionsCount(...)`
- `webhookSubscriptionCreate(topic:, webhookSubscription:)`
- `webhookSubscriptionUpdate(id:, webhookSubscription:)`
- `webhookSubscriptionDelete(id:)`

All six roots are registered under the `webhooks` domain with `implemented: false`. The query roots remain unknown-operation passthrough at runtime, and the mutation roots must not be treated as supported local staging until a webhook subscription state model exists.

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

Runtime staging is also out of scope for this ticket. Future support should add a normalized webhook subscription graph, local connection/count serialization, create/update/delete staging, read-after-write effects, raw mutation log retention, and tests showing supported mutations do not hit Shopify at runtime.

## Validation

- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
- `corepack pnpm typecheck`
- `corepack pnpm vitest run tests/unit/webhook-subscription-conformance-fixture.test.ts`
