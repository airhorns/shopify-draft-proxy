---
title: 'Events'
description: 'Coverage notes and fidelity boundaries for Events.'
---

<!-- Mirrored from docs/endpoints/events.md so the Starlight site exposes the canonical endpoint notes. -->

This endpoint group covers the top-level Shopify Admin GraphQL Event catalog roots: `event`, `events`, and `eventsCount`.

## Current support and limitations

### Supported roots

Read roots:

- `event(id:)`
- `events(...)`
- `eventsCount(...)`

Mutation roots:

- None. Top-level event emission is not modeled as a mutation surface in this endpoint group.

### Local behavior

Snapshot mode models the checked-in no-data branch only:

- `event(id:)` returns `null` for absent Event GIDs.
- `events(...)` returns a non-null empty connection with selected `nodes`, `edges`, and `pageInfo` fields, false page booleans, and null cursors.
- `eventsCount(...)` returns `{ count: 0, precision: "EXACT" }`.

The captured empty Event selection includes `id`, `action`, `appTitle`, `attributeToApp`, `attributeToUser`, `createdAt`, `criticalAlert`, and `message`, plus `BasicEvent` fields such as `additionalContent`, `additionalData`, `arguments`, `author`, `hasAdditionalContent`, `secondaryMessage`, `subjectId`, and `subjectType`. Because the evidence is empty/null, the local handler must not invent values for those fields.

### Boundaries

- Non-empty event catalogs, search/filter/sort behavior, count precision beyond exact zero, and pagination over real events remain unsupported.
- Supported mutations in other domains do not write into a shared top-level Event catalog. Domain-owned event surfaces, such as discount detail events and fulfillment events, remain documented and modeled by their owning endpoint groups.
- No event root is registry-only or validation-only in this group; the supported read roots are intentionally limited to the no-data shape above.

### Evidence

- `config/parity-specs/events/event-empty-read.json`
- `config/parity-requests/events/event-empty-read.graphql`
- `config/parity-requests/events/event-empty-read.variables.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/events/event-empty-read.json`
- `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`

### Validation

- `corepack pnpm parity -- event-empty-read`
- `corepack pnpm conformance:check`
