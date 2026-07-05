---
title: 'Events'
description: 'Coverage notes and fidelity boundaries for Events.'
---

This endpoint group covers the top-level Shopify Admin GraphQL Event catalog roots: `event`, `events`, and `eventsCount`.

## Current support and limitations

### Implemented roots

Read roots:

- `event(id:)`
- `events(...)`
- `eventsCount(...)`

Mutation roots:

- None. Top-level event emission is not a standalone mutation surface in this endpoint group.

### Local behavior

Cold LiveHybrid top-level Event reads forward the original GraphQL request to Shopify when the proxy has no staged Events state. Successful upstream responses hydrate observed `Event`, `BasicEvent`, and `CommentEvent` nodes into the base store, so later local Event reads can project those observed records alongside staged records.

The local Event catalog supports `event(id:)`, `events(...)`, and `eventsCount(...)` from store state. Product and variant lifecycle mutations synthesize `BasicEvent` records for create, update, and destroy actions, including product default-variant replacement and product-delete variant cleanup. Generic `node(id:)` and `nodes(ids:)` resolve locally for stored Event records when they appear in an Events-dispatched document.

`events(...)` and `eventsCount(...)` use the same underlying filtered set. The local query parser supports the documented `action`, `comments`, `created_at`, `id`, and `subject_type` search fields, plus free-text matching across Event text/id fields. Local connections support `sortKey: ID`, `sortKey: CREATED_AT`, `sortKey: RELEVANCE`, `reverse`, cursor pagination, and `pageInfo` from staged store state. Local counts return exact precision unless a `limit` argument truncates the reported count, in which case precision is `AT_LEAST`.

Snapshot mode with no stored Event records models Shopify's no-data branch:

- `event(id:)` returns `null` for absent Event GIDs.
- `events(...)` returns a non-null empty connection with selected `nodes`, `edges`, and `pageInfo` fields, false page booleans, and null cursors.
- `eventsCount(...)` returns `{ count: 0, precision: "EXACT" }`.

The captured empty Event selection includes `id`, `action`, `appTitle`, `attributeToApp`, `attributeToUser`, `createdAt`, `criticalAlert`, and `message`, plus `BasicEvent` fields such as `additionalContent`, `additionalData`, `arguments`, `author`, `hasAdditionalContent`, `secondaryMessage`, `subjectId`, and `subjectType`. Because the evidence is empty/null, the local handler must not invent values for those fields.

The `event-non-empty-read` parity scenario captures a real store where `events(first: 3, sortKey: ID, reverse: true)` returns `BasicEvent` nodes and `eventsCount` returns `precision: "AT_LEAST"`, proving the cold LiveHybrid path reads through to Shopify rather than fabricating an empty catalog.

### Boundaries

- Event synthesis currently covers product and product-variant lifecycle side effects only. Event generation for other Admin API domains remains owned by those endpoint groups or unmodeled at the top-level Event catalog.
- Local read-after-write projections are based on observed upstream Event records plus staged Event records. They are not a full historical scrape of the live shop unless the relevant upstream Events were already observed through the public GraphQL read path.
- Shopify-authored Event message text and app attribution can be opaque. Locally synthesized `BasicEvent` records use deterministic proxy attribution and do not attempt to reproduce hidden Shopify rendering details.
