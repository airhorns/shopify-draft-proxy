---
title: 'Segments Endpoint Group'
description: 'Coverage notes and fidelity boundaries for Segments Endpoint Group.'
---

<!-- Mirrored from docs/endpoints/segments.md so the Starlight site exposes the canonical endpoint notes. -->

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

- `segment(id:)`, `segments`, and `segmentsCount` use normalized segment records for the selected catalog/detail/count fields in checked-in captures.
- Snapshot mode returns Shopify-like empty connections and exact zero counts when no segment data has been hydrated.
- Unknown `segment(id:)` returns `null` with Shopify's captured `NOT_FOUND` error shape.
- `segmentFilters`, `segmentFilterSuggestions`, `segmentValueSuggestions`, and `segmentMigrations` preserve captured metadata/suggestion payloads. They are not dynamic suggestion engines.

Segment lifecycle mutations stage locally and retain the original raw mutation for ordered commit replay:

- `segmentCreate` stages a synthetic Segment GID, timestamps, name, and query. Duplicate names follow Shopify's suffix behavior by incrementing an existing trailing ` (N)` counter or appending ` (2)`, then return `Name has already been taken` after the supported retry window.
- Locally staged Segment payloads project Shopify-like defaults for non-null fields: `tagMigrated: false` and `valid: true`. Shopify-owned fields the proxy cannot derive locally, including `percentageSnapshot`, `percentageSnapshotUpdatedAt`, `translation`, and `author`, are returned as `null` when selected.
- Missing required top-level mutation arguments fail as GraphQL coercion errors before resolver behavior. Blank strings still reach the resolver and return payload-level userErrors.
- `segmentCreate` and `segmentUpdate` enforce stripped-name and raw-query length limits, segment count limits, duplicate-name behavior, and query grammar validation before staging.
- Accepted query strings are stored with Shopify-like whitespace behavior: blankness is checked on the trimmed string, length is measured before trimming, and accepted query values are returned verbatim.
- `segmentUpdate` updates an existing base or staged segment, preserves the creation timestamp, and advances `lastEditDate`.
- `segmentDelete` records local deletion state and removes the segment from detail, catalog, and count reads.

Segment query grammar support has two tiers:

- Save-time validation for `segmentCreate`, `segmentUpdate`, and direct member query creation accepts the captured grammar for supported filter names, comparison operators, `IS NULL` / `IS NOT NULL`, `CONTAINS` / `NOT CONTAINS`, `BETWEEN`, boolean `AND` / `OR`, parentheses, escaped string literals, and relative date literals.
- Member evaluation is narrower. The proxy evaluates `number_of_orders` comparisons and exact customer tag `CONTAINS` / `NOT CONTAINS` against normalized local customer state. Other save-accepted filters can be stored but do not imply local membership evaluation.

Customer segment member query behavior:

- `customerSegmentMembersQueryCreate(input: { query })` stages Shopify's captured async creation shape with `status: INITIALIZED`, `currentCount: 0`, and `done: false`.
- `customerSegmentMembersQueryCreate(input: { segmentId })` resolves an effective segment query at creation time, returns the same async shape, and stores a readable local job. Unknown valid Segment GIDs return the captured CDP userError `field: null`, `code: INVALID`, and `message: "Invalid segment ID."`.
- `customerSegmentMembers(query:)`, `customerSegmentMembers(queryId:)`, and `customerSegmentMembers(segmentId:)` return Shopify-like totals, `statistics.attributeStatistics(...){ average sum }`, `edges`, and `pageInfo` for the supported evaluator grammar.
- `customerSegmentMembersQuery(id:)` returns a staged job or `null` with Shopify's captured `INTERNAL_SERVER_ERROR`-shaped error for unknown query IDs.
- `customerSegmentMembership(customerId:, segmentIds:)` returns rows only for segments present in effective local state. Missing segment IDs are skipped, and missing or non-matching customers return `isMember: false` for known segments.

### Boundaries

- Search, sort, and uncaptured request arguments for segment catalog roots are not inferred beyond checked-in captures.
- Segment filter and value suggestion roots are static captured metadata payloads until separate executable evidence supports dynamic behavior.
- Accepted segment filters such as `email_subscription_status = 'SUBSCRIBED'`, `companies IS NULL`, and `customer_countries CONTAINS 'CA'` do not have local member evaluation.
- CDP validation failures outside the captured malformed direct-query branch are not guessed; accepted-but-unmodeled filters produce readable async jobs with zero local members rather than fabricated parser messages.
- No segment root in this document is registry-only. The main validation-only distinction is save-time grammar acceptance for filters that are not evaluated by local member reads.

### Evidence

- `config/parity-specs/segments/segments-baseline-read.json`
- `config/parity-specs/segments/segment-create-invalid-query-validation.json`
- `config/parity-specs/segments/segment-update-unknown-id-validation.json`
- `config/parity-specs/segments/segment-delete-unknown-id-validation.json`
- `config/parity-specs/segments/segments-create-update-validation-limits.json`
- `config/parity-specs/segments/segment-create-update-length-edge-cases.json`
- `config/parity-specs/segments/segment-mutations-required-argument-validation.json`
- `config/parity-specs/segments/segment-payload-non-null-fields.json`
- `config/parity-specs/segments/customer-segment-members-query-lifecycle.json`
- `config/parity-specs/segments/segment-query-grammar-not-contains.json`
- `config/parity-specs/segments/segment-create-update-query-grammar.json`
- `config/parity-specs/segments/segment-query-whitespace-preservation.json`
- `config/parity-specs/segments/customer-segment-members-query-create-segment-id-paths.json`
- `config/parity-specs/segments/customer-segment-members-query-create-direct-query-grammar.json`
- `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/segments/segments-baseline.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segments/segment-lifecycle-validation.json`
- `fixtures/conformance/local-runtime/2026-04/segments/segment-payload-non-null-fields.json`
- `scripts/capture-segment-query-grammar-conformance.ts`
- `scripts/capture-segment-query-whitespace-preservation-conformance.ts`
- `scripts/capture-customer-segment-members-query-create-segment-id-paths-conformance.ts`
- `scripts/capture-customer-segment-members-query-create-direct-query-grammar-conformance.ts`

### Validation

- `corepack pnpm parity -- segments-baseline-read`
- `corepack pnpm parity -- segment-create-update-query-grammar`
- `corepack pnpm parity -- customer-segment-members-query-lifecycle`
- `corepack pnpm conformance:check`
