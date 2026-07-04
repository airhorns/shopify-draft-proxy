---
title: 'Events'
description: 'Coverage notes and fidelity boundaries for Events.'
---

This endpoint group covers the top-level Shopify Admin GraphQL Event catalog roots: `event`, `events`, and `eventsCount`.

## Current support and limitations

### Supported roots

Read roots:

- `event(id:)`
- `events(...)`
- `eventsCount(...)`

Mutation roots:

- None. Top-level event emission is not modeled as a mutation surface in this endpoint group.

### Runtime behavior

In LiveHybrid mode, top-level Event reads pass through to Shopify until the proxy has a real local Event catalog model. This keeps real store timelines, non-empty event catalogs, and `eventsCount` values visible instead of asserting the checked-in no-data shape for every live store. The `event-non-empty-read` parity scenario captures a real store where `events(first: 3, sortKey: ID, reverse: true)` returns `BasicEvent` nodes and `eventsCount` returns `precision: "AT_LEAST"`.

### Local snapshot behavior

Snapshot mode models the checked-in no-data branch only:

- `event(id:)` returns `null` for absent Event GIDs.
- `events(...)` returns a non-null empty connection with selected `nodes`, `edges`, and `pageInfo` fields, false page booleans, and null cursors.
- `eventsCount(...)` returns `{ count: 0, precision: "EXACT" }`.

The captured empty Event selection includes `id`, `action`, `appTitle`, `attributeToApp`, `attributeToUser`, `createdAt`, `criticalAlert`, and `message`, plus `BasicEvent` fields such as `additionalContent`, `additionalData`, `arguments`, `author`, `hasAdditionalContent`, `secondaryMessage`, `subjectId`, and `subjectType`. Because the evidence is empty/null, the local handler must not invent values for those fields.

### Boundaries

- Local modeling for non-empty event catalogs, search/filter/sort behavior, count precision beyond exact zero, and pagination over real events remains unsupported outside the live-hybrid passthrough path.
- Supported mutations in other domains do not write into a shared top-level Event catalog. Domain-owned event surfaces, such as discount detail events and fulfillment events, remain documented and modeled by their owning endpoint groups.
- No event root is registry-only or validation-only in this group; the supported read roots are intentionally limited to the no-data shape above.
