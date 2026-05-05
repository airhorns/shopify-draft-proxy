# Segments Endpoint Group

The segments group has an implemented read baseline for segment catalog, detail, count, and filter metadata roots.
Segment lifecycle mutations are staged locally. Customer segment member query jobs and member reads have a narrow
HAR-217 implementation for captured customer targeting flows.

## Current support and limitations

### Implemented roots

Overlay reads:

- `segment`
- `segments`
- `segmentsCount`
- `segmentFilters`
- `segmentFilterSuggestions`
- `segmentValueSuggestions`
- `segmentMigrations`
- `customerSegmentMembers`
- `customerSegmentMembersQuery`
- `customerSegmentMembership`

Staged mutations:

- `customerSegmentMembersQueryCreate`
- `segmentCreate`
- `segmentUpdate`
- `segmentDelete`

### Behavior notes

- Segment read support is capture-driven and intentionally narrow.
- `segment(id:)`, `segments`, and `segmentsCount` use normalized segment records for the selected catalog/detail/count fields captured in HAR-215.
- `segmentFilters`, `segmentFilterSuggestions`, `segmentValueSuggestions`, and `segmentMigrations` preserve captured root payloads for selected metadata/suggestion/migration fields.
- Snapshot mode returns Shopify-like empty connections and `EXACT` zero counts when no segment data has been hydrated.
- Unknown `segment(id:)` returns `null` with Shopify's captured `NOT_FOUND` error shape.
- `segmentCreate` stages a local segment with a stable synthetic Segment GID, creation/last-edit timestamps, name, and query. Duplicate names follow Shopify's captured suffix behavior by returning the requested name with ` (2)`, ` (3)`, and so on rather than a user error.
- `segmentUpdate` stages name/query replacement on an existing base or staged segment and preserves the original creation timestamp while advancing `lastEditDate`.
- `segmentDelete` records local deletion state and removes the segment from downstream detail, catalog, and count reads.
- Captured validation coverage currently includes blank names, blank/invalid query strings, unknown IDs, missing required GraphQL arguments, and delete-after-delete/unknown delete behavior.
- `customerSegmentMembersQueryCreate` stages a local query job and retains the original raw mutation request in the
  mutation log for commit replay. New jobs follow Shopify's initial async shape in both the creation payload and
  immediate downstream lookup (`status: INITIALIZED`, `currentCount: 0`, `done: false`).
- Member-query evaluation is intentionally narrow and evidence-backed. The proxy currently supports:
  - `number_of_orders = N`, `>`, `>=`, `<`, and `<=`
  - `customer_tags CONTAINS 'tag'`
  - `customer_tags NOT CONTAINS 'tag'`
- `customerSegmentMembers(query:)`, `customerSegmentMembers(queryId:)`, and `customerSegmentMembers(segmentId:)`
  return Shopify-like `totalCount`, `statistics.attributeStatistics(...){ average sum }`, `edges`, and `pageInfo`.
  Connection pagination uses local stable cursors rather than Shopify's opaque cursor encoding.
- `customerSegmentMembersQuery(id:)` returns the staged job or `null` with Shopify's captured
  `INTERNAL_SERVER_ERROR`-shaped error for unknown query IDs.
- `customerSegmentMembership(customerId:, segmentIds:)` returns membership rows only for segment IDs that exist in
  effective local segment state. Missing segment IDs are skipped, and missing/non-matching customers return
  `isMember: false` for known segments.
- Customer member evaluation observes staged `customerCreate` / customer updates and staged segment definitions for
  the supported query grammar. Tag membership is evaluated against normalized local customer `tags` with exact string
  equality, and order-count membership is evaluated against local `numberOfOrders`. The proxy does not infer customer
  membership for filters that are accepted for segment storage but do not have a modeled customer-state dependency, such
  as `email_subscription_status = 'SUBSCRIBED'`. Broader Shopify segment grammar is not claimed.
- `customerSegmentMembersQueryCreate(input: { segmentId })` resolves the staged or hydrated segment query at creation
  time, returns Shopify's captured async creation shape, and stores an immediately readable local job. HAR-458
  integration coverage verifies that direct `customerSegmentMembers(query:)` reads and `queryId` reads agree for the
  supported grammar.
- Segment search, sort, and uncaptured member-query grammar are not inferred beyond the captured request arguments.
- Segment filter and value suggestion roots are captured metadata payloads, not a dynamic local suggestion engine. They
  are useful for shape fidelity and empty/no-data behavior, but new suggestion search semantics should be backed by fresh
  conformance evidence before being claimed.

## Historical and developer notes

### Validation anchors

- Segment reads: `tests/integration/segment-query-shapes.test.ts`
- Segment lifecycle: `tests/integration/segment-lifecycle-flow.test.ts`
- Customer segment members: `tests/integration/customer-segment-member-flow.test.ts`
- Conformance fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/segments/segments-baseline.json`
- Segment lifecycle validation fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segments/segment-lifecycle-validation.json`
- Customer segment member fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segments/customer-segment-members-query-lifecycle.json`
- Segment query grammar fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segments/segment-query-grammar-not-contains.json`
- Conformance request/spec: `config/parity-requests/segments/segments-baseline-read.graphql`, `config/parity-specs/segments/segments-baseline-read.json`
- Segment lifecycle validation specs: `config/parity-specs/segments/segment-create-invalid-query-validation.json`, `config/parity-specs/segments/segment-update-unknown-id-validation.json`, `config/parity-specs/segments/segment-delete-unknown-id-validation.json`
- Customer segment member parity spec: `config/parity-specs/segments/customer-segment-members-query-lifecycle.json`
- Segment query grammar parity spec: `config/parity-specs/segments/segment-query-grammar-not-contains.json`
- Segment query grammar capture script: `scripts/capture-segment-query-grammar-conformance.ts`
- HAR-458 review coverage: `tests/integration/customer-segment-member-flow.test.ts` covers segmentId-backed member query
  jobs, direct query reads, and accepted-but-unmodeled filter storage boundaries.
