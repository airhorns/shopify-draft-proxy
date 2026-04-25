# Segments Endpoint Group

The segments group has an implemented read baseline for segment catalog, detail, count, and filter metadata roots. Segment member queries and mutations remain future work.

## Implemented roots

Overlay reads:

- `segment`
- `segments`
- `segmentsCount`
- `segmentFilters`
- `segmentFilterSuggestions`
- `segmentValueSuggestions`
- `segmentMigrations`

## Unsupported roots still tracked by the registry

- `customerSegmentMembers`
- `customerSegmentMembersQuery`
- `customerSegmentMembership`
- `customerSegmentMembersQueryCreate`
- `segmentCreate`
- `segmentUpdate`
- `segmentDelete`

## Behavior notes

- Segment read support is capture-driven and intentionally narrow.
- `segment(id:)`, `segments`, and `segmentsCount` use normalized segment records for the selected catalog/detail/count fields captured in HAR-215.
- `segmentFilters`, `segmentFilterSuggestions`, `segmentValueSuggestions`, and `segmentMigrations` preserve captured root payloads for selected metadata/suggestion/migration fields.
- Snapshot mode returns Shopify-like empty connections and `EXACT` zero counts when no segment data has been hydrated.
- Unknown `segment(id:)` returns `null` with Shopify's captured `NOT_FOUND` error shape.
- Segment search, sort, pagination beyond the captured page, and member-query grammar are not inferred beyond the captured request arguments.

## Validation anchors

- Segment reads: `tests/integration/segment-query-shapes.test.ts`
- Conformance fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/segments-baseline.json`
- Conformance request/spec: `config/parity-requests/segments-baseline-read.graphql`, `config/parity-specs/segments-baseline-read.json`
