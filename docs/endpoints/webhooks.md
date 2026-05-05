# Webhooks

HAR-267 adds conformance evidence for Admin GraphQL webhook subscription roots. HAR-268 adds local runtime read support for API-created webhook subscription records. HAR-269 adds local staging for the captured create/update registration subset without firing webhooks or registering real subscriptions upstream during supported runtime handling. HAR-270 adds local deregistration staging for the same captured API-created subset without unsubscribing real Shopify webhook subscriptions at runtime.

## Current support and limitations

### Registered Roots

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

### Local Read Behavior

- Webhook subscription reads are backed by normalized `webhookSubscriptions` state plus `webhookSubscriptionOrder`.
- Snapshot mode returns `null` for unknown `webhookSubscription(id:)`, an empty `webhookSubscriptions` connection, and `{ count: 0, precision: "EXACT" }` for `webhookSubscriptionsCount` when no records are present.
- Local records preserve captured fields: `id`, `topic`, `uri`, `name`, `format`, `includeFields`, `metafieldNamespaces`, `filter`, `createdAt`, `updatedAt`, and deprecated endpoint-specific fields for `WebhookHttpEndpoint`, `WebhookEventBridgeEndpoint`, and `WebhookPubSubEndpoint`.
- `webhookSubscriptions` uses shared connection helpers for `nodes`, `edges`, selected `pageInfo`, stable synthetic cursors, `first`/`last`, `before`/`after`, `sortKey: ID`, and `reverse`. It also applies current Shopify catalog filters for `uri`, deprecated `callbackUrl`, `format`, and `topics`.
- `webhookSubscriptionsCount` supports count `limit` semantics with `EXACT` / `AT_LEAST` precision and simple captured query filtering such as `id:<legacy id>`, `topic:<topic>`, `format:<format>`, `uri:<uri>`, and `endpoint:<uri fragment>`.
- Live-hybrid reads hydrate upstream webhook subscription nodes into normalized base state and overlay staged local records when present.

### Local Mutation Behavior

- `webhookSubscriptionCreate(topic:, webhookSubscription:)` stages a new normalized webhook subscription with a proxy synthetic `WebhookSubscription` GID, stable synthetic timestamps, the requested `topic`, `uri`, `name`, `format`, `includeFields`, `metafieldNamespaces`, and `filter`. The deprecated `endpoint` field is derived from `WebhookSubscriptionInput.uri`: HTTPS-like strings become `WebhookHttpEndpoint.callbackUrl`, `pubsub://{project-id}:{topic-id}` becomes `WebhookPubSubEndpoint`, and `arn:aws:events:...` becomes `WebhookEventBridgeEndpoint`.
- `webhookSubscriptionUpdate(id:, webhookSubscription:)` updates an existing staged or hydrated webhook subscription in place, preserving `topic` and `createdAt` while replacing the captured mutable fields, `name`, and endpoint URI when present. Current Admin GraphQL docs describe `uri` as the unified field; `endpoint` and `callbackUrl` are kept only as deprecated compatibility projections.
- `webhookSubscriptionDelete(id:)` stages a deletion for staged, synthetic, or hydrated/local webhook subscriptions. The successful payload returns `deletedWebhookSubscriptionId` and an empty `userErrors` list; subsequent `webhookSubscription(id:)` reads return `null`, and list/count reads omit the deleted subscription.
- Successful create/update/delete mutations append staged entries to the meta mutation log with the original request body intact for commit replay. Commit replay replaces synthetic IDs with upstream IDs from earlier successful replay attempts before replaying later raw request bodies.
- Captured validation branches are handled locally without upstream writes: create without `uri` returns `webhookSubscription: null` and `userErrors: [{ field: ["webhookSubscription", "callbackUrl"], message: "Address can't be blank" }]`; update of an unknown ID returns `webhookSubscription: null` and `userErrors: [{ field: ["id"], message: "Webhook subscription does not exist" }]`; delete of an unknown or already deleted ID returns `deletedWebhookSubscriptionId: null` and `userErrors: [{ field: ["id"], message: "Webhook subscription does not exist" }]`.
- Missing or null required `webhookSubscriptionCreate(topic:, webhookSubscription:)`, `webhookSubscriptionUpdate(id:, webhookSubscription:)`, and `webhookSubscriptionDelete(id:)` arguments return Shopify-like GraphQL validation errors locally and do not append mutation-log entries.
- Local registration/update/delete does not deliver webhook payloads and does not create, update, or unsubscribe real Shopify webhook subscriptions at runtime.

### Draft Delivery Policy

Default runtime policy: supported draft-mode mutations must never send webhook deliveries to external systems. This includes HTTP callback URLs, EventBridge ARNs, Pub/Sub topics, app-config/TOML subscriptions, and any other destination Shopify can target. The proxy should treat registered webhook subscriptions as local subscription metadata during normal runtime handling, not as permission to notify the outside world.

The recommended implementation policy is an in-memory, pull-based webhook outbox exposed through the meta API. When a supported local mutation eventually maps to a supported webhook topic, the proxy should append a synthetic payload record to the outbox after the domain command is successfully staged. Tests can inspect that deterministic record through meta endpoints, but the proxy does not POST to callback URLs, publish to AWS/GCP destinations, retry, or forward any delivery auth/secrets.

Rejected alternatives:

- Never fire and never record: safest, but too little observability once webhook subscriptions are modeled. Apps cannot assert that a staged product/order/customer mutation would have produced a relevant Shopify event.
- Fire synthetic local callbacks: creates real HTTP side effects, duplicate notifications, retry and timeout behavior, HMAC/signing questions, and destination availability problems during tests.
- Deliver to EventBridge or Pub/Sub: requires cloud credentials and can escape the local test boundary even more easily than HTTP callbacks.
- Deliver during `__meta/commit`: commit replay may cause real Shopify to deliver real webhooks for the replayed mutations; the proxy must not also replay synthetic outbox entries or emit separate notifications, because that would create duplicate side effects.
- Allow opt-in external delivery now: useful only after the outbox, payload-shape fixtures, topic coverage rules, isolation controls, and credential redaction rules are implemented. It should stay a separate future design/implementation slice, not the default HAR-271 policy.

#### Outbox Observability Contract

The future meta API should expose webhook payload records separately from the existing mutation log, for example:

- `GET /__meta/webhooks/outbox` returns ordered synthetic webhook payload records.
- `POST /__meta/webhooks/outbox/reset` clears only the webhook outbox.
- `POST /__meta/reset` clears the webhook outbox together with staged state, caches, synthetic identities, and the mutation log.

Each outbox record should be JSON-serializable and deterministic:

- `id`: stable synthetic delivery ID, suitable for a Shopify-like webhook ID/header value.
- `sequence`: monotonically increasing integer in append order.
- `recordedAt`: proxy timestamp from the same clock source used for staged mutation timestamps.
- `topic`: Shopify topic enum value such as `PRODUCTS_CREATE`.
- `subscriptionId`: local or hydrated `WebhookSubscription` GID that matched the topic.
- `endpoint`: cloned subscription endpoint metadata; HTTP callback URLs, EventBridge ARNs, and Pub/Sub coordinates are recorded as destination metadata only.
- `format`, `includeFields`, `metafieldNamespaces`, and `filter`: subscription fields used to derive the payload.
- `sourceMutationLogEntryId` and `sourceMutationLogIndex`: link back to the staged mutation that generated the payload.
- `resourceGid`: primary resource ID affected by the staged mutation.
- `payload`: Shopify-shaped JSON payload for the selected topic and format.
- `headers`: deterministic, secret-free preview of delivery headers such as topic, shop domain, API version, synthetic webhook ID, and trigger timestamp. Do not copy incoming Admin API auth headers. Do not expose or derive real app secrets; HMAC should be absent or explicitly `null` unless a later isolated test mode introduces a test-only signing secret.
- `delivery`: `{ mode: "recorded", status: "recorded", attempts: [] }` for the default policy.

Ordering follows the mutation log: records are appended only for successful supported local mutations, after validation passes and after the domain command has staged local state. If one mutation matches multiple local subscriptions for the same topic, append one outbox record per matching subscription in deterministic subscription order. Validation-only branches and unsupported passthrough mutations must not create synthetic outbox records; unsupported passthrough may still cause real Shopify side effects upstream and should remain visible through existing observability.

`includeFields`, `metafieldNamespaces`, and `filter` must be applied before writing the outbox record once those semantics are modeled. Until they are conformance-backed for a topic, the topic should remain unsupported for outbox generation rather than emitting a broad guessed payload.

#### Topic Mapping Policy

Webhook payload generation should be driven by domain events emitted by supported local mutation handlers, not by patching GraphQL responses. A domain handler that stages a resource change should expose enough normalized before/after state for the webhook outbox mapper to decide whether a topic is eligible and to serialize the payload.

First viable slice:

- `PRODUCTS_CREATE` from staged `productCreate`.
- `PRODUCTS_UPDATE` from staged product update/editing mutations once the changed product payload is conformance-backed.
- `PRODUCTS_DELETE` from staged product deletion once deletion payload shape is captured.

These topics are the first viable webhook outbox slice, but they still require payload fixtures before implementation. Product-adjacent topics such as variant, collection, inventory, publication, media, and metafield events should not be inferred from product mutations until specific Shopify payload evidence exists and the local domain event can identify the affected resource precisely.

Later slices should follow the same rule:

- Orders: map only after the order-domain mutation already stages the relevant lifecycle transition locally and has payload evidence for topics such as order create/update/cancel/payment/fulfillment events.
- Customers: map only after customer-domain staging can provide the Shopify-shaped customer payload and evidence for customer create/update/delete topics.
- Draft orders, refunds, fulfillments, discounts, files, markets, metaobjects, shop policies, and privacy topics remain unsupported for webhook outbox generation until their owning endpoint group has both local lifecycle fidelity and topic-specific payload fixtures.
- App lifecycle topics, compliance topics, and subscription lifecycle topics remain unsupported by default because they are not caused by ordinary local draft-mode resource mutations.

#### Source Alignment

This policy was reviewed against Shopify Admin GraphQL 2026-04 webhook subscription docs and the current app webhook delivery docs. Relevant Shopify surfaces include `webhookSubscriptionCreate`, `WebhookSubscriptionInput.uri`, `WebhookSubscriptionTopic`, `WebhookSubscriptionFormat`, HTTP/EventBridge/PubSub endpoint variants, delivery headers, HMAC signing, retry behavior, and Shopify's warning that webhook ordering and duplicate delivery cannot be assumed. The draft proxy should record enough metadata for tests to assert intended local behavior, while explicitly not emulating network delivery, retry scheduling, or cloud destination semantics until a later isolated delivery mode is designed.

## Historical and developer notes

### Captured Evidence

`fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-conformance.json` records:

- empty/no-data `webhookSubscriptions` catalog behavior
- empty and filtered `webhookSubscriptionsCount` behavior
- unknown-id `webhookSubscription` detail null behavior
- create/update/delete payloads for a temporary HTTP `SHOP_UPDATE` subscription
- detail read-after-create, read-after-update, and read-after-delete behavior
- missing URI validation on create
- unknown-id validation on update and delete
- GraphQL validation errors for missing required `webhookSubscriptionCreate(topic:)` and null `webhookSubscriptionUpdate(webhookSubscription:)`

The capture used `WebhookSubscriptionInput.uri` with an `https://example.com/...` HTTP endpoint, `format: JSON`, selected `includeFields`, and selected `metafieldNamespaces`. The created subscription was deleted during the same script run. HAR-399 reviewed the current 2026-04 docs and 2025-10 webhook changelog and added local executable coverage for unified `uri` projection and filtering across HTTPS, Google Pub/Sub (`pubsub://{project-id}:{topic-id}`), and Amazon EventBridge ARN endpoint shapes without adding any delivery/outbox side effects.

HAR-356 promotes the captured evidence into two executable strict parity contracts:

- `webhook-subscription-catalog-read` compares the captured empty `webhookSubscriptions` connection, count precision, filtered count, and unknown-detail null behavior against the local proxy in snapshot mode.
- `webhook-subscription-conformance` replays the captured create/update/delete lifecycle through local staging, then compares mutation payloads, detail read-after-write responses, read-after-delete absence, and captured validation branches. Live Shopify IDs and timestamps are accepted only through path-scoped matchers because the proxy uses synthetic IDs and a deterministic synthetic clock.
- `webhook-subscription-required-argument-validation` compares captured GraphQL validation errors for missing create topic and null update input against the local proxy. These requests fail before resolver side effects and do not stage webhook subscriptions.

### HAR-461 Fidelity Review

The 2026-04 and latest Admin GraphQL webhook subscription docs and public usage examples were re-reviewed for HAR-461. Current examples use `WebhookSubscriptionInput.uri` for HTTP callback URLs, Google Pub/Sub `pubsub://{project-id}:{topic-id}` destinations, and Amazon EventBridge ARNs; deprecated endpoint-specific projections are still preserved by the proxy only as read compatibility fields. The existing parity contracts cover API-created subscription lifecycle, empty catalog/count reads, unknown-detail null behavior, missing URI userErrors, unknown-id update/delete userErrors, required-argument GraphQL validation, and downstream read-after-write/read-after-delete effects.

Remaining fidelity gaps are intentionally narrower than webhook delivery:

- Topic-specific permission and business-rule failures are not hardcoded unless captured for a concrete topic. The current lifecycle evidence uses `SHOP_UPDATE` because it avoids mutating resource data or triggering deliveries.
- Destination-specific validation beyond the captured missing-URI branch is not generalized. HTTP, Pub/Sub, and EventBridge URI projections are modeled for local state and reads, but cloud destination existence, app configuration, and provider credentials are out of scope for runtime staging.
- App configuration/TOML webhook subscriptions are still out of scope for these Admin GraphQL subscription roots.
- No webhook payload delivery, retry scheduling, HMAC signing, or external callback/cloud publishing is emulated. Those behaviors require a separately scoped webhook outbox/delivery issue with topic-specific payload fixtures.
- The conformance capture script reads the checked-in webhook parity requests from `config/parity-requests/webhooks` so recorded evidence can be refreshed without drift from the executable parity contracts.

### Access And Scope Notes

The capture fixture includes the active app access scopes returned by `currentAppInstallation.accessScopes`. The captured grant did not expose dedicated `read_webhooks` or `write_webhooks` handles; it could still read and manage API-created subscriptions for the app. Topic-specific requirements can still vary by topic, so future runtime work should keep scope/topic failures as conformance-backed validation rather than hardcoded assumptions.

The lifecycle capture uses `SHOP_UPDATE` because it is available in the topic enum and does not require creating or modifying products, orders, customers, inventory, or other domain records. The script does not trigger any shop update or delivery probe, so no webhook delivery is intentionally sent during HAR-267.

### Out Of Scope

App configuration/TOML webhooks remain out of scope. Shopify's Admin GraphQL subscription roots are being treated here as the API-created subscription lifecycle surface; future evidence must prove otherwise before TOML/app-config webhooks are modeled through these roots.

### Validation

- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
- `corepack pnpm typecheck`
- `corepack pnpm vitest run test/parity_test.gleam`
- `corepack pnpm vitest run test/parity_test.gleam`
- `corepack pnpm vitest run test/parity_test.gleam`
