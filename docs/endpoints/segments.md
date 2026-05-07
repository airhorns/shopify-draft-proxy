# Segments Endpoint Group

The segments group has an implemented read baseline for segment catalog, detail, count, and filter metadata roots.
Segment lifecycle mutations are staged locally. Customer segment member query jobs and member reads have a narrow
implementation for captured customer targeting flows.

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
- `segmentCreate` stages a local segment with a stable synthetic Segment GID, creation/last-edit timestamps, name, and query. Duplicate names follow Shopify's suffix behavior by bumping an existing trailing ` (N)` counter or appending ` (2)` when there is no counter. After 10 duplicate retries the proxy returns `Name has already been taken` without staging a segment.
- Locally staged Segment payloads project Shopify-like defaults for schema non-null fields: `tagMigrated: false` and `valid: true`. Computed Shopify-owned fields that the proxy cannot derive locally are selected as null for staged segments: `percentageSnapshot`, `percentageSnapshotUpdatedAt`, `translation`, and `author`.
- `segmentCreate` rejects names longer than 255 characters, queries longer than 5000 characters, and shops that already have 6000 effective local segments before staging a new segment. Length validation runs before query grammar validation so overlong queries return Shopify's length error rather than parser errors.
- `segmentCreate` and `segmentUpdate` accept the broader Shopify segment query grammar at save time for supported filter names, comparison operators, `IS NULL` / `IS NOT NULL`, `CONTAINS` / `NOT CONTAINS`, `BETWEEN`, boolean `AND` / `OR`, parentheses, escaped string literals, and relative date literals. This is storage validation only; it does not mean local member evaluation understands every accepted filter.
- `segmentUpdate` stages name/query replacement on an existing base or staged segment and preserves the original creation timestamp while advancing `lastEditDate`. Rename collisions use the same bump-then-Taken behavior as `segmentCreate`.
- `segmentUpdate` applies the same 255-character name and 5000-character query limits before staging updates.
- `segmentDelete` records local deletion state and removes the segment from downstream detail, catalog, and count reads.
- Captured validation coverage currently includes blank names, overlong names, blank/invalid/overlong query strings, unknown IDs, missing required GraphQL arguments, segment-limit rejection, and delete-after-delete/unknown delete behavior.
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
  as `email_subscription_status = 'SUBSCRIBED'`, `companies IS NULL`, or `customer_countries CONTAINS 'CA'`.
- `customerSegmentMembersQueryCreate(input: { segmentId })` resolves the staged or hydrated segment query at creation
  time, returns Shopify's captured async creation shape, and stores an immediately readable local job. The mutation
  does not revalidate the resolved segment's stored query grammar; Shopify hands this branch to CDP after selector
  validation. Unknown valid Segment GIDs return the captured CDP user error `field: null`, `code: INVALID`,
  `message: "Invalid segment ID."`. Integration coverage verifies that direct `customerSegmentMembers(query:)` reads
  and `queryId` reads agree for the supported grammar.
- Segment search, sort, and uncaptured member-query grammar are not inferred beyond the captured request arguments.
- Segment filter and value suggestion roots are captured metadata payloads, not a dynamic local suggestion engine. They
  are useful for shape fidelity and empty/no-data behavior, but new suggestion search semantics should be backed by fresh
  conformance evidence before being claimed.

## Historical and developer notes

### Validation anchors

- Conformance fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/segments/segments-baseline.json`
- Segment lifecycle validation fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segments/segment-lifecycle-validation.json`
- Segment length/limit validation fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segments/segments-create-update-validation-limits.json`
- Segment payload non-null field fixture: `fixtures/conformance/local-runtime/2026-04/segments/segment-payload-non-null-fields.json`
- Customer segment member fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segments/customer-segment-members-query-lifecycle.json`
- Segment query grammar fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segments/segment-query-grammar-not-contains.json`
- Segment create/update query grammar fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segments/segment-create-update-query-grammar.json`
- Member-query segmentId branch fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segments/customer-segment-members-query-create-segment-id-paths.json`
- Conformance request/spec: `config/parity-requests/segments/segments-baseline-read.graphql`, `config/parity-specs/segments/segments-baseline-read.json`
- Segment lifecycle validation specs: `config/parity-specs/segments/segment-create-invalid-query-validation.json`, `config/parity-specs/segments/segment-update-unknown-id-validation.json`, `config/parity-specs/segments/segment-delete-unknown-id-validation.json`
- Segment length/limit validation spec: `config/parity-specs/segments/segments-create-update-validation-limits.json`
- Segment payload non-null field spec: `config/parity-specs/segments/segment-payload-non-null-fields.json`
- Customer segment member parity spec: `config/parity-specs/segments/customer-segment-members-query-lifecycle.json`
- Segment query grammar parity spec: `config/parity-specs/segments/segment-query-grammar-not-contains.json`
- Segment create/update query grammar parity spec: `config/parity-specs/segments/segment-create-update-query-grammar.json`
- Member-query segmentId branch parity spec: `config/parity-specs/segments/customer-segment-members-query-create-segment-id-paths.json`
- Segment query grammar capture script: `scripts/capture-segment-query-grammar-conformance.ts`
- Member-query segmentId branch capture script: `scripts/capture-customer-segment-members-query-create-segment-id-paths-conformance.ts`
- Review coverage includes segmentId-backed member query jobs, direct query reads, and accepted-but-unmodeled filter
  storage boundaries.
