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
- `customerSegmentMembersQuery(id:)`

Mutation roots:

- `segmentCreate`
- `segmentUpdate`
- `segmentDelete`
- `customerSegmentMembersQueryCreate`

### Registry-only roots

These query roots are known to the operation registry but do not have local runtime handlers and are not classified as supported:

- `segmentFilters(...)`
- `segmentFilterSuggestions(...)`
- `segmentValueSuggestions(...)`
- `segmentMigrations(...)`
- `customerSegmentMembers(...)`
- `customerSegmentMembership(customerId:, segmentIds:)`

### Local behavior

Segment reads use an effective base/staged segment set:

- In LiveHybrid, `segment(id:)`, `segments`, and `segmentsCount` observe upstream segment rows/counts and overlay local segment creates, updates, and tombstones. Staged records win by ID, local tombstones suppress matching upstream/base rows, and unrelated upstream segments remain visible after a local segment mutation.
- `segmentsCount` preserves Shopify's upstream `count` and `precision` as the base total, then applies known local creates, updates, and tombstones. Without an upstream count response, local fallback counts use the shared `limit`/precision helper.
- Locally projected `segments(sortKey:)` supports `CREATION_DATE`, `ID`, and `LAST_EDIT_DATE`, applies `query`, `reverse`, and cursor windows through the shared connection helper, and uses id as the stable secondary tiebreaker. `RELEVANCE` uses deterministic id ordering for local staged/base segments because Shopify's search score is opaque.
- Snapshot mode returns Shopify-like empty connections and exact zero counts when no segment data has been hydrated.
- `segment(id:)` local misses forward upstream in LiveHybrid instead of fabricating `NOT_FOUND`. Local tombstones remain authoritative and return `null` with Shopify's top-level `NOT_FOUND` error; list and count reads omit tombstoned segments without that detail-read error.

Segment lifecycle mutations stage locally and retain the original raw mutation for ordered commit replay:

- `segmentCreate` stages a synthetic Segment GID, timestamps, name, and query. Duplicate names follow Shopify's suffix behavior by incrementing an existing trailing ` (N)` counter or appending ` (2)`, then return `Name has already been taken` after the supported retry window.
- Before a valid LiveHybrid create, the proxy obtains only the authoritative prerequisites it needs. One combined query-only request reads `segmentsCount(limit: 6000)` when no count baseline is cached and runs one bounded name search per distinct duplicate-suffix base. A name probe is considered complete only when Shopify returns `hasNextPage: false`; an overfull or malformed probe remains unresolved instead of being treated as an authoritative absence. The count, matching rows, and completed probe scopes are cached, and subsequent creates apply staged create/delete deltas to the baseline without enumerating the catalog.
- Locally staged Segment payloads project Shopify-like defaults for non-null fields: `tagMigrated: false` and `valid: true`. Shopify-owned fields the proxy cannot derive locally, including `percentageSnapshot`, `percentageSnapshotUpdatedAt`, `translation`, and `author`, are returned as `null` when selected.
- Missing required top-level mutation arguments fail as GraphQL coercion errors before resolver behavior. Blank strings still reach the resolver and return payload-level userErrors.
- `segmentCreate` and `segmentUpdate` enforce stripped-name and raw-query length limits, segment count limits, duplicate-name behavior, and query grammar validation before staging. Query grammar validation runs only after the Change-level name/query presence and length checks pass; query blank/too-long errors can still aggregate with name errors, but grammatically invalid query text is not reported when the name itself is blank or too long.
- Accepted query strings are stored with Shopify-like whitespace behavior: blankness is checked on the trimmed string, length is measured before trimming, and accepted query values are returned verbatim.
- In LiveHybrid, `segmentUpdate`, `segmentDelete`, and `customerSegmentMembersQueryCreate(input: { segmentId })` hydrate Segment prerequisites that are absent from effective state before deciding that they do not exist. IDs submitted by all applicable roots in the operation are deduplicated and loaded in one query-only batch; the single-ID mutation-first path retains its targeted `segment(id:)` request. The lookup preserves the caller's versioned path and auth headers and never forwards the caller mutation. Snapshot mode performs no supplemental upstream lookup.
- Authoritative Segment hits and confirmed misses are cached. Transport failures, GraphQL failures, incomplete batches, and mismatched nodes remain unresolved and are returned without caching a miss, so a later request can retry.
- `segmentUpdate` updates an existing base or staged segment, preserves the creation timestamp, and advances `lastEditDate`.
- `segmentDelete` records local deletion state; subsequent detail reads return Shopify's null-plus-`NOT_FOUND` envelope, while catalog and count reads hide the target.
- Accepted lifecycle writes update the effective Segment records, name index, count delta, tombstones, member-job state, and replay entry together. Validation failures and unresolved prerequisites do not allocate an ID, append a replay entry, or change staged lifecycle state. Supported mutations reach Shopify only through explicit ordered `POST /__meta/commit` replay of their original documents and variables.
- Dump/restore includes the authoritative Segment count/name/identity caches, base and staged Segment records, tombstones, base and staged member-query jobs, known job misses, synthetic allocator, and replay log. Reset preserves authoritative base observations while discarding staged records/jobs, tombstones, replay entries, and allocator progress.

Segment query grammar support covers save-time behavior:

- Save-time validation for `segmentCreate`, `segmentUpdate`, and direct member query creation accepts the captured grammar for supported filter names, comparison operators, `IS NULL` / `IS NOT NULL`, `CONTAINS` / `NOT CONTAINS`, `BETWEEN`, boolean `AND` / `OR`, parentheses, escaped string literals, and relative date literals.
- Malformed save-time segment queries return payload-level userErrors derived from the query text, including token/column messages for unexpected tokens and unknown filters, with an input-derived fallback for unsupported malformed values.
- Save-time acceptance does not imply support for the registry-only member connection or membership query roots.

Customer segment member query behavior:

- `customerSegmentMembersQueryCreate(input: { query })` stages Shopify's captured async creation shape with `status: INITIALIZED`, `currentCount: 0`, and `done: false`.
- `customerSegmentMembersQueryCreate(input: { segmentId })` resolves an effective or authoritatively hydrated Segment query at creation time, returns the same async shape, and stores a readable local job. Malformed or empty GIDs fail input-object coercion with a top-level `INVALID_VARIABLE` error before resolver behavior; wrong-resource Shopify GIDs return top-level `RESOURCE_NOT_FOUND` with `data.customerSegmentMembersQueryCreate: null`. A confirmed unknown Segment GID returns the captured CDP userError `field: null`, `code: INVALID`, and `message: "Invalid segment ID."`; an unresolved prerequisite failure is not converted into that definitive error.
- `customerSegmentMembersQuery(id:)` returns a staged job first, then a cached persisted job. Cold persisted job IDs in one operation are deduplicated into one `nodes(ids:)` query-only hydration call. Confirmed misses are cached and retain Shopify's captured `INTERNAL_SERVER_ERROR`-shaped response; transport and malformed-response failures remain retryable.

### Boundaries

- Registry-only filter, suggestion, migration, member-connection, and membership roots are not local capability claims. Checked-in captures for those roots document Shopify behavior but do not make them supported.
- Local segment list overlay supports simple free-text matching across `id`, `name`, and `query`, plus keyed `id:`, `name:`, and `query:` terms; unsupported local search terms remain permissive while upstream owns real catalog filtering.
- Public Admin GraphQL Segment parity is limited to `id`, `name`, `query`, `creationDate`, and `lastEditDate`, which are the fields exposed by 2025-01 and 2026-04 conformance-shop introspection. Private Core Segment fields such as `tagMigrated`, `valid`, `percentageSnapshot`, `percentageSnapshotUpdatedAt`, `translation`, and `author` are covered by Rust integration tests rather than Shopify parity fixtures.
- Accepted segment filters such as `email_subscription_status = 'SUBSCRIBED'`, `companies IS NULL`, and `customer_countries CONTAINS 'CA'` can be stored on Segment or member-query job records, but the registry-only member connection and membership roots do not claim local evaluation of those filters.
- CDP validation failures outside the captured malformed direct-query branch are not guessed; accepted-but-unmodeled filters still produce readable async job records rather than fabricated parser messages.
