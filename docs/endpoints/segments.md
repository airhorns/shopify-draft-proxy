# Segments Endpoint Group

The segments group has an implemented read baseline for segment catalog, detail, count, and filter metadata roots. Segment lifecycle mutations are staged locally. Segment member queries remain future work.

## Implemented roots

Overlay reads:

- `segment`
- `segments`
- `segmentsCount`
- `segmentFilters`
- `segmentFilterSuggestions`
- `segmentValueSuggestions`
- `segmentMigrations`

Staged mutations:

- `segmentCreate`
- `segmentUpdate`
- `segmentDelete`

## Unsupported roots still tracked by the registry

- `customerSegmentMembers`
- `customerSegmentMembersQuery`
- `customerSegmentMembership`
- `customerSegmentMembersQueryCreate`

## Behavior notes

- Segment read support is capture-driven and intentionally narrow.
- `segment(id:)`, `segments`, and `segmentsCount` use normalized segment records for the selected catalog/detail/count fields captured in HAR-215.
- `segmentFilters`, `segmentFilterSuggestions`, `segmentValueSuggestions`, and `segmentMigrations` preserve captured root payloads for selected metadata/suggestion/migration fields.
- Snapshot mode returns Shopify-like empty connections and `EXACT` zero counts when no segment data has been hydrated.
- Unknown `segment(id:)` returns `null` with Shopify's captured `NOT_FOUND` error shape.
- `segmentCreate` stages a local segment with a stable synthetic Segment GID, creation/last-edit timestamps, name, and query. Duplicate names follow Shopify's captured suffix behavior by returning the requested name with ` (2)`, ` (3)`, and so on rather than a user error.
- `segmentUpdate` stages name/query replacement on an existing base or staged segment and preserves the original creation timestamp while advancing `lastEditDate`.
- `segmentDelete` records local deletion state and removes the segment from downstream detail, catalog, and count reads.
- Captured validation coverage currently includes blank names, blank/invalid query strings, unknown IDs, missing required GraphQL arguments, and delete-after-delete/unknown delete behavior.
- Segment search, sort, pagination beyond the captured page, and member-query grammar are not inferred beyond the captured request arguments.
- Customer member evaluation is intentionally not expanded by lifecycle staging; broader member matching belongs to the customer-segment member query work.

## Validation anchors

- Segment reads: `tests/integration/segment-query-shapes.test.ts`
- Segment lifecycle: `tests/integration/segment-lifecycle-flow.test.ts`
- Conformance fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/segments-baseline.json`
- Segment lifecycle validation fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segment-lifecycle-validation.json`
- Conformance request/spec: `config/parity-requests/segments-baseline-read.graphql`, `config/parity-specs/segments-baseline-read.json`
- Segment lifecycle validation specs: `config/parity-specs/segment-create-invalid-query-validation.json`, `config/parity-specs/segment-update-unknown-id-validation.json`, `config/parity-specs/segment-delete-unknown-id-validation.json`
