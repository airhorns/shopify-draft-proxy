---
title: 'Segments Endpoint Group'
description: 'Coverage notes and fidelity boundaries for Segments Endpoint Group.'
---

This endpoint group covers Shopify Admin GraphQL segment catalog, filter metadata, segment lifecycle, and customer segment member query roots.

## Current support and limitations

### Supported roots

Read roots:

- `segment(id:)`
- `segments(...)`
- `segmentsCount(...)`
- `segmentFilters(...)`
- `segmentFilterSuggestions(...)`
- `segmentValueSuggestions(...)`
- `segmentMigrations(...)`
- `customerSegmentMembers(...)`
- `customerSegmentMembersQuery(id:)`
- `customerSegmentMembership(customerId:, segmentIds:)`

Mutation roots:

- `segmentCreate`
- `segmentUpdate`
- `segmentDelete`
- `customerSegmentMembersQueryCreate`

### Local behavior

Segment reads are capture-driven and intentionally narrow:

- `segment(id:)`, `segments`, and `segmentsCount` use normalized segment records for the selected catalog/detail/count fields in checked-in captures. Locally staged `segments(sortKey:)` supports `CREATION_DATE`, `ID`, and `LAST_EDIT_DATE`, applies `reverse` and cursor windows through the shared connection helper, and uses id as the stable secondary tiebreaker. `RELEVANCE` uses deterministic id ordering for local staged segments because Shopify's search score is opaque.
- Snapshot mode returns Shopify-like empty connections and exact zero counts when no segment data has been hydrated.
- Unknown `segment(id:)` returns `null` with Shopify's captured `NOT_FOUND` error shape.
- `segmentFilters`, `segmentFilterSuggestions`, `segmentValueSuggestions`, and `segmentMigrations` preserve captured metadata/suggestion payloads. They are not dynamic suggestion engines.

Segment lifecycle mutations stage locally and retain the original raw mutation for ordered commit replay:

- `segmentCreate` stages a synthetic Segment GID, timestamps, name, and query. Duplicate names follow Shopify's suffix behavior by incrementing an existing trailing ` (N)` counter or appending ` (2)`, then return `Name has already been taken` after the supported retry window.
- Locally staged Segment payloads project Shopify-like defaults for non-null fields: `tagMigrated: false` and `valid: true`. Shopify-owned fields the proxy cannot derive locally, including `percentageSnapshot`, `percentageSnapshotUpdatedAt`, `translation`, and `author`, are returned as `null` when selected.
- Missing required top-level mutation arguments fail as GraphQL coercion errors before resolver behavior. Blank strings still reach the resolver and return payload-level userErrors.
- `segmentCreate` and `segmentUpdate` enforce stripped-name and raw-query length limits, segment count limits, duplicate-name behavior, and query grammar validation before staging. Query grammar validation runs only after the Change-level name/query presence and length checks pass; query blank/too-long errors can still aggregate with name errors, but grammatically invalid query text is not reported when the name itself is blank or too long.
- Accepted query strings are stored with Shopify-like whitespace behavior: blankness is checked on the trimmed string, length is measured before trimming, and accepted query values are returned verbatim.
- `segmentUpdate` updates an existing base or staged segment, preserves the creation timestamp, and advances `lastEditDate`.
- `segmentDelete` records local deletion state and removes the segment from detail, catalog, and count reads.

Segment query grammar support has two tiers:

- Save-time validation for `segmentCreate`, `segmentUpdate`, and direct member query creation accepts the captured grammar for supported filter names, comparison operators, `IS NULL` / `IS NOT NULL`, `CONTAINS` / `NOT CONTAINS`, `BETWEEN`, boolean `AND` / `OR`, parentheses, escaped string literals, and relative date literals.
- Member evaluation is narrower. The proxy evaluates `number_of_orders` comparisons and exact customer tag `CONTAINS` / `NOT CONTAINS` against normalized local customer state. Other save-accepted filters can be stored but do not imply local membership evaluation.

Customer segment member query behavior:

- `customerSegmentMembersQueryCreate(input: { query })` stages Shopify's captured async creation shape with `status: INITIALIZED`, `currentCount: 0`, and `done: false`.
- `customerSegmentMembersQueryCreate(input: { segmentId })` resolves an effective segment query at creation time, returns the same async shape, and stores a readable local job. Malformed or empty GIDs fail input-object coercion with a top-level `INVALID_VARIABLE` error before resolver behavior; wrong-resource Shopify GIDs return top-level `RESOURCE_NOT_FOUND` with `data.customerSegmentMembersQueryCreate: null`. Unknown valid Segment GIDs still return the captured CDP userError `field: null`, `code: INVALID`, and `message: "Invalid segment ID."`.
- `customerSegmentMembers(query:)`, `customerSegmentMembers(queryId:)`, and `customerSegmentMembers(segmentId:)` return Shopify-like totals, `statistics.attributeStatistics(...){ average sum }`, `edges`, and `pageInfo` for the supported evaluator grammar.
- `customerSegmentMembersQuery(id:)` returns a staged job or `null` with Shopify's captured `INTERNAL_SERVER_ERROR`-shaped error for unknown query IDs.
- `customerSegmentMembership(customerId:, segmentIds:)` returns rows only for segments present in effective local state. Missing segment IDs are skipped, and missing or non-matching customers return `isMember: false` for known segments.

### Boundaries

- Search and uncaptured request arguments for seeded segment catalog roots are not inferred beyond checked-in captures; staged local segment catalogs only model the sort keys listed above.
- Segment filter and value suggestion roots are static captured metadata payloads until separate executable evidence supports dynamic behavior.
- Public Admin GraphQL Segment parity is limited to `id`, `name`, `query`, `creationDate`, and `lastEditDate`, which are the fields exposed by 2025-01 and 2026-04 conformance-shop introspection. Private Core Segment fields such as `tagMigrated`, `valid`, `percentageSnapshot`, `percentageSnapshotUpdatedAt`, `translation`, and `author` are covered by Rust integration tests rather than Shopify parity fixtures.
- Accepted segment filters such as `email_subscription_status = 'SUBSCRIBED'`, `companies IS NULL`, and `customer_countries CONTAINS 'CA'` do not have local member evaluation.
- CDP validation failures outside the captured malformed direct-query branch are not guessed; accepted-but-unmodeled filters produce readable async jobs with zero local members rather than fabricated parser messages.
- No segment root in this document is registry-only. The main validation-only distinction is save-time grammar acceptance for filters that are not evaluated by local member reads.
