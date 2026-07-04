---
title: 'Events'
description: 'Coverage notes and fidelity boundaries for Events.'
---

This endpoint group covers the top-level Shopify Admin GraphQL Event catalog roots: `event`, `events`, and `eventsCount`.

## Current support and limitations

### Implemented roots

Read roots:

- None.

Tracked but unimplemented read roots:

- `event(id:)`
- `events(...)`
- `eventsCount(...)`

Mutation roots:

- None. Top-level event emission is not modeled as a mutation surface in this endpoint group.

### Local behavior

The proxy does not locally model Shopify's top-level Event catalog. These roots are kept in the operation registry as unimplemented coverage-map entries, so callers do not receive a synthetic local event catalog.

- In LiveHybrid and passthrough modes, `event`, `events`, and `eventsCount` forward upstream with the original query, variables, pagination, filters, sort key, and `reverse` arguments intact.
- In snapshot mode, there is no upstream Event catalog to consult, so these roots surface the standard unsupported-read dispatcher error rather than a fabricated empty payload.
- Supported mutations in other endpoint groups do not append records to a shared top-level Event catalog.

### Boundaries

- Local read-after-write effects for top-level events remain unsupported. The proxy does not synthesize Event records from staged mutations.
- Local search/filter/sort behavior, count precision, and pagination over Event records remain unsupported because they are not modeled from store state.
- Domain-owned event surfaces, such as discount detail events and fulfillment events, remain documented and modeled by their owning endpoint groups.
