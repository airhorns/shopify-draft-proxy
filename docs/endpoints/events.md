# Events

## Current support and limitations

### Supported read roots

- `event`
- `events`
- `eventsCount`

### Snapshot behavior

- Snapshot mode currently models the no-data branch only.
- `event(id:)` returns `null` for absent event IDs.
- `events(...)` returns a non-null empty connection with selected `nodes`, `edges`, and `pageInfo` fields, false page booleans, and null cursors.
- `eventsCount(...)` returns `{ count: 0, precision: "EXACT" }`.
- Events read handling has cut over to the Gleam port; the legacy TypeScript runtime handler has been removed.

## Historical and developer notes

### Captured scope and gaps

Root-operation introspection confirms the Admin GraphQL `event`, `events`, and `eventsCount` roots exist in the 2025-01 captured schema inventory. HAR-323 also captured the top-level no-data payload shape against `harry-test-heelo.myshopify.com` on 2026-04-26: unknown `event(id:)` returns `null`, `events(first:, query:, sortKey: ID, reverse:)` returns an empty connection for an impossible `id:` query, and `eventsCount(query:)` returns exact zero.

The captured top-level `Event` interface selected `id`, `action`, `appTitle`, `attributeToApp`, `attributeToUser`, `createdAt`, `criticalAlert`, and `message`, with a `BasicEvent` fragment for `additionalContent`, `additionalData`, `arguments`, `author`, `hasAdditionalContent`, `secondaryMessage`, `subjectId`, and `subjectType`. Because the capture is intentionally empty/null, local snapshot mode must not invent values for those fields.

Staged mutations in other domains do not yet write into a shared top-level Event catalog. Domain-specific event surfaces that already have conformance-backed models, such as discount detail events and fulfillment events, remain owned by those endpoint implementations. Broader top-level event emission should wait for a dedicated live capture that establishes event type, subject, message, argument, filter, sort, count, and pagination behavior.

### Validation anchors

- Runtime shape coverage: `gleam/test/shopify_draft_proxy/proxy/events_test.gleam` and `gleam/test/shopify_draft_proxy/proxy/draft_proxy_test.gleam`
- Executable parity: `config/parity-specs/events/event-empty-read.json`
- Live fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/events/event-empty-read.json`
- Root presence evidence: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`
