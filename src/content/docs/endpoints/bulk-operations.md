---
title: 'Bulk Operations Endpoint Group'
description: 'Coverage notes and fidelity boundaries for Bulk Operations Endpoint Group.'
---

This endpoint group covers Shopify Admin GraphQL's root-level asynchronous export/import API for `BulkOperation` jobs. It does not cover product variant bulk mutations, inventory bulk toggles, discount bulk roots, or metaobject bulk delete.

## Current support and limitations

### Supported roots

Read roots:

- `bulkOperation(id:)`
- `bulkOperations(...)`
- `currentBulkOperation(type:)`

Mutation roots:

- `bulkOperationCancel`
- `bulkOperationRunQuery`
- `bulkOperationRunMutation`

### Local behavior

The proxy stores normalized `BulkOperation` jobs with `id`, `status`, `type`, `errorCode`, `createdAt`, `completedAt`, `objectCount`, `rootObjectCount`, `fileSize`, `url`, `partialDataUrl`, `query`, optional internal client-identifier throttle metadata, and optional cursor metadata. Snapshot loading can seed base jobs, local staging can add jobs, and `POST /__meta/reset` restores the startup base snapshot while clearing staged jobs, result records, staged uploads, and logs.

Read behavior:

- `bulkOperation(id:)` returns one job by ID. Unknown valid BulkOperation GIDs return `null`; malformed non-GID strings and non-BulkOperation GIDs return captured top-level invalid-ID errors.
- `bulkOperations(...)` returns jobs as a connection, newest first by default, with `first`/`last`, `after`, `before`, `reverse`, `sortKey: CREATED_AT`, `sortKey: COMPLETED_AT`, and search filters for `created_at`, `id`, `operation_type`, and `status`. `status` and operation-type values match case-insensitively. `created_at` accepts date-only and timestamp values with Shopify-style comparator prefixes (`>`, `<`, `>=`, `<=`, `=`).
- Empty reads return Shopify-like non-null empty connections with selected `nodes`, `edges`, and `pageInfo`.
- Invalid connection windows, malformed `created_at`, malformed inline IDs, non-BulkOperation GIDs, and hidden/internal `sortKey: ID` return top-level GraphQL/BAD_REQUEST envelopes matching captured behavior.
- Invalid `status` and `operation_type` search values return the selected empty connection plus `extensions.search` `invalid_value` warnings. Unsupported search keys intentionally keep Shopify's fail-open match-all behavior but include an `invalid_field` warning in `extensions.search` instead of silently broadening the result set.
- `currentBulkOperation(type:)` returns the newest effective job for the requested type and defaults to `QUERY`.
- In LiveHybrid, `bulkOperations`, `currentBulkOperation`, and `bulkOperation(id:)` hydrate missing upstream jobs before local projection. Observed upstream jobs become base state, staged jobs overlay that base by ID, by-ID misses forward upstream, and list reads with staged jobs render the effective base-plus-staged connection locally so real historical jobs stay visible beside local jobs.

Query export behavior:

- `bulkOperationRunQuery(query:, groupObjects:)` dispatches by the root field and arguments, independent of the client's GraphQL operation name, and validates submitted bulk query documents against captured Admin GraphQL schema rules before staging, including connection `nodes` selections whose userError example names the offending connection field.
- Omitted required `query` arguments return the same top-level GraphQL `missingRequiredArguments` envelope as other locally validated Admin mutations. Blank query strings return a selected payload with `bulkOperation: null` and a `field: ["query"]` userError.
- Submitted bulk query text is measured after Shopify-style single-quoted newline escaping and rejects escaped UTF-8 byte sizes above 65,535 with `field: ["query"]`, message `Query is too large (<bytes> bytes; maximum is 65535 bytes)`, `code: INVALID`, `bulkOperation: null`, and no staged job.
- Supported local JSONL synthesis roots are `products` and `productVariants`. Product exports support nested `collections`, `images`, `media`, `metafields`, and `variants`; product-variant exports support nested `media` and `metafields`. Nested rows include `__parentId`.
- Snapshot exports serialize the complete effective local catalog. LiveHybrid exports first hydrate the complete upstream product or product-variant baseline, selecting only identity, requested output fields, parent relations, and fields required by the corresponding local search predicate. Top-level catalog pages and selected nested connection pages are fetched in 250-row windows until Shopify explicitly returns `hasNextPage: false`.
- Hydrated rows merge into base state without replacing staged state. Staged creates and updates therefore overlay the complete upstream baseline, while staged product and variant tombstones remain absent from the generated JSONL.
- Product search/filter evaluation uses the same effective-catalog predicate as non-bulk product reads. Fields needed by `status`, vendor, product type, title, handle, tags, SKU, barcode, gift-card, collection, publication, and timestamp terms are included in hydration even when they are not selected for JSONL output.
- LiveHybrid hydration uses ordinary Admin GraphQL read queries only. The supported `bulkOperationRunQuery` mutation itself is never sent upstream.
- An upstream transport failure, GraphQL error, malformed connection, missing pagination proof, repeated cursor, or page-limit exhaustion returns `bulkOperation: null` with a `field: ["query"]`, `code: INVALID` userError. No job or result artifact is staged, so a cold proxy cannot silently publish an unobserved catalog as an empty export.
- Supported export requests complete locally against effective state, write generated JSONL results, expose a synthetic absolute `https://localhost:<proxy-port>/__meta/bulk-operations/<encoded-id>/result.jsonl` result URL, and never proxy supported export mutations upstream at runtime.
- Immediate mutation responses return Shopify's created job shape with `status: CREATED`, `completedAt: null`, zero counters, no file/result URL, and the original query. Later reads expose a terminal completed job with counters, file size, result URL, and original query. For query exports, `objectCount` counts every emitted JSONL object, while `rootObjectCount` counts only root-level objects, so product exports with nested variants can report a larger `objectCount` than `rootObjectCount`.
- `groupObjects: true`, `groupObjects: false`, and omitted `groupObjects` all stage the same local export shape; grouped JSONL ordering is not modeled as a separate result mode.
- Same-type in-progress operations return `OPERATION_IN_PROGRESS` without staging a second job once the version-specific concurrency limit is reached. Admin API versions before 2026-01 allow one non-terminal operation per type; 2026-01 and newer allow five non-terminal query operations and five non-terminal mutation operations before the sixth same-type run throttles. Valid nonblank `clientIdentifier` values scope that check locally; omitted identifiers keep broad app/shop collision behavior.

Mutation import behavior:

- `bulkOperationRunMutation(mutation:, stagedUploadPath:, clientIdentifier:, groupObjects:)` dispatches by the root field and arguments, independent of the client's GraphQL operation name, and accepts any single inner mutation root except `bulkOperationRunMutation` and `bulkOperationRunQuery`, matching Shopify's top-level analyzer.
- Submitted inner mutation text is measured with the same escaped UTF-8 storage limit. Sizes above 65,535 bytes return `field: ["query"]`, message `is too large (<bytes> bytes; maximum is 65535 bytes)`, `code: INVALID_MUTATION`, `bulkOperation: null`, and no staged job.
- Inner mutation selection validation uses the captured Admin output schema to reject more than one selected connection field before staging. Public nested-connection mutation documents captured across product, shop, collection, customer, order, and line-item paths all surface Shopify's count error before the deeper nesting message, so they return `field: ["mutation"]`, message `Bulk mutations cannot contain more than 1 connection.`, `code: null`, `bulkOperation: null`, and no staged job. A single shallow connection selection is accepted.
- A fully local import requires a proxy staged upload whose HTTP PUT/POST body bytes were captured by the same proxy instance, a valid single-root mutation, an implemented Admin API mutation root with `stage-locally` execution, and a matching local mutation handler. The `stagedUploadPath` must be a key produced by the instance's `stagedUploadsCreate` state; literal placeholder paths such as `valid` are treated as missing uploads.
- For locally executable roots, each non-empty JSONL line is parsed as variables, stages through the same domain handler used by normal GraphQL mutations, and writes one result JSONL row with the row response plus `__lineNumber`. Completed import counters reflect processed row count, and `fileSize` reflects the generated result JSONL byte length.
- A root that passes Shopify's bulk-document analyzer but has no implemented `stage-locally` resolver still receives the immediate accepted job shape with `status: CREATED`. The effective job then becomes `FAILED`; every valid JSONL variables line produces an explicit result-row error naming the unsupported root, no line enters the commit log, and no mutation is sent upstream during draft staging.
- The proxy records one staged mutation-log entry per locally handled JSONL line, in original line order, with replay bodies containing the inner mutation and that line's variables. The outer bulk request is retained as audit metadata rather than as an additional commit entry.
- Missing staged upload objects, malformed inner mutation documents, non-mutation operations, multiple top-level mutation fields, disallowed bulk roots, zero-byte uploads, oversized uploads, invalid `clientIdentifier`, and in-progress jobs return Shopify-like userErrors without staging a successful job. Missing staged uploads return `field: null`, the `NO_SUCH_FILE` message, `code: NO_SUCH_FILE`, and `bulkOperation: null` before same-type in-progress throttling. Zero-byte uploads return `field: null`, message `The input file is empty.`, `code: INVALID_STAGED_UPLOAD_FILE`, and `bulkOperation: null` before same-type in-progress throttling. Malformed JSONL after a valid import starts stages a failed job with a result artifact for observability.

Cancel behavior:

- `bulkOperationCancel(id:)` looks up the target job from effective BulkOperation state before deciding the response.
- Unknown valid BulkOperation GIDs return `bulkOperation: null` with `field: ["id"]` userErrors and do not stage a record or append a mutation-log entry.
- Terminal jobs (`COMPLETED`, `CANCELED`, `FAILED`, and `EXPIRED`) return the existing job unchanged plus a `field: null` userError and do not append a mutation-log entry.
- Non-terminal jobs stage a status-only `CANCELING` overlay, preserving the stored job's counters, artifact fields, timestamps, query, and type, then return selected job payloads with empty `userErrors` and append the original raw mutation body plus the staged BulkOperation ID to the mutation log for commit replay and observability.
- LiveHybrid can hydrate a cold known job before applying the same stored-status cancel decision locally.

Meta API behavior:

- `GET /__meta/state` exposes base/staged BulkOperation records, ordering, and generated result artifacts.
- `GET /__meta/log` exposes query exports, cancel attempts, and mutation-import inner line entries in replay order.
- `GET /__meta/bulk-operations/<encoded-gid>/result.jsonl` serves generated query-export and mutation-import JSONL from instance-owned memory.
- `POST /__meta/commit` replays only staged mutation-log entries. For mutation imports, commit replays inner mutation entries in JSONL line order and does not replay the outer bulk request.

### Boundaries

- Bulk Operations roots are intercepted locally; they are not permanent passthrough capabilities. Unsupported local query synthesis returns an explicit proxy-only userError in every read mode without calling the upstream transport. The error identifies the unsupported query root at `field: ["query"]` and leaves `code` null because Shopify's `BulkOperationUserErrorCode` enum has no proxy-unsupported value. Normal proxy handling never sends `bulkOperationRunQuery` or a substitute mutation to Shopify before explicit commit.
- Query JSONL synthesis is supported for `products` and `productVariants` only.
- Product and product-variant exports require a provably complete baseline in LiveHybrid. Callers receive an `INVALID` userError rather than partial JSONL when that read-side hydration cannot complete.
- Mutation import result-file schema, partial failure status/counter behavior, and broader per-domain imports require deeper executable evidence before being claimed as local lifecycle support.
- Accepted inner mutation roots without local executors fail as local BulkOperation jobs with per-line result errors. They never use the unsupported-mutation passthrough escape hatch, do not provide local downstream read-after-write effects, and are not replayed by commit.
- Daily/per-app `LIMIT_REACHED` quota behavior and Shopify's POS/product-feed client allowlist for `clientIdentifier` are not modeled.
- Registry-only support is not claimed for unsupported inner roots. Validation-only support includes schema analyzer branches and request/userError guardrails that fail before staging.
