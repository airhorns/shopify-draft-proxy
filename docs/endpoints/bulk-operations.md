# Bulk Operations Endpoint Group

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
- `bulkOperations(...)` returns jobs as a connection, newest first by default, with `first`/`last`, `after`, `before`, `reverse`, `sortKey: CREATED_AT`, `sortKey: COMPLETED_AT`, and search filters for `created_at`, `id`, `operation_type`, and `status`.
- Empty reads return Shopify-like non-null empty connections with selected `nodes`, `edges`, and `pageInfo`.
- Invalid connection windows, malformed `created_at`, malformed inline IDs, non-BulkOperation GIDs, and hidden/internal `sortKey: ID` return top-level GraphQL/BAD_REQUEST envelopes matching captured behavior.
- Invalid `status` and `operation_type` search values return the selected empty connection plus `extensions.search` warnings.
- `currentBulkOperation(type:)` returns the newest effective job for the requested type and defaults to `QUERY`.
- In LiveHybrid, cold explicit sort-key reads can pass through to Shopify until local BulkOperation state exists. Once state is staged or snapshotted, reads resolve locally so read-after-write ordering remains visible.

Query export behavior:

- `bulkOperationRunQuery(query:, groupObjects:)` validates submitted bulk query documents against captured Admin GraphQL schema rules before staging.
- Supported local JSONL synthesis roots are `products` and `productVariants`, including supported product/variant scalar selections and nested product variants with `__parentId`.
- Supported export requests complete locally against effective state, write generated JSONL results, expose a synthetic result URL under `/__meta/bulk-operations/<encoded-id>/result.jsonl`, and never proxy supported export mutations upstream at runtime.
- Immediate mutation responses return Shopify's created job shape with `status: CREATED`, `completedAt: null`, zero counters, no file/result URL, and the original query. Later reads expose a terminal completed job with counters, file size, result URL, and original query.
- `groupObjects: true`, `groupObjects: false`, and omitted `groupObjects` all stage the same local export shape; grouped JSONL ordering is not modeled as a separate result mode.
- Same-type in-progress operations return `OPERATION_IN_PROGRESS` without staging a second job. Valid nonblank `clientIdentifier` values scope that check locally; omitted identifiers keep broad app/shop collision behavior.

Mutation import behavior:

- `bulkOperationRunMutation(mutation:, stagedUploadPath:, clientIdentifier:, groupObjects:)` accepts any single inner mutation root except `bulkOperationRunMutation` and `bulkOperationRunQuery`, matching Shopify's top-level analyzer.
- A fully local import requires a proxy staged upload, a valid single-root mutation, an implemented Admin API mutation root with `stage-locally` execution, and a matching local mutation handler.
- For locally executable roots, each JSONL line is parsed as variables, stages through the same domain handler used by normal GraphQL mutations, and writes one result JSONL row.
- Accepted roots without a local executor still create an observable local BulkOperation job, but each JSONL line is sent upstream through the unsupported-mutation passthrough escape hatch and logged as `Proxied`. Those lines are Shopify-side effects and do not create local downstream read-after-write state.
- The proxy records one staged mutation-log entry per locally handled JSONL line, in original line order, with replay bodies containing the inner mutation and that line's variables. The outer bulk request is retained as audit metadata rather than as an additional commit entry.
- Missing staged upload objects, malformed inner mutation documents, non-mutation operations, multiple top-level mutation fields, disallowed bulk roots, oversized uploads, invalid `clientIdentifier`, and in-progress jobs return Shopify-like userErrors without staging a successful job. Malformed JSONL after a valid import starts stages a failed job with a result artifact for observability.

Cancel behavior:

- `bulkOperationCancel(id:)` stages non-terminal local jobs as `CANCELING` and returns selected job payloads with empty userErrors.
- LiveHybrid can hydrate a cold known job before staging a local cancellation overlay.
- Unknown IDs return `bulkOperation: null` with `field: ["id"]` userErrors. Terminal jobs return the existing job plus a `field: null` userError.
- Cancel attempts append original raw mutation bodies and staged BulkOperation IDs to the mutation log for observability.

Meta API behavior:

- `GET /__meta/state` exposes base/staged BulkOperation records, ordering, and generated result artifacts.
- `GET /__meta/log` exposes query exports, cancel attempts, and mutation-import inner line entries in replay order.
- `GET /__meta/bulk-operations/<encoded-gid>/result.jsonl` serves generated query-export and mutation-import JSONL from instance-owned memory.
- `POST /__meta/commit` replays only staged mutation-log entries. For mutation imports, commit replays inner mutation entries in JSONL line order and does not replay the outer bulk request.

### Boundaries

- Bulk Operations roots are intercepted locally; they are not permanent passthrough capabilities. Unsupported local query synthesis returns an `UNSUPPORTED_IN_PROXY` userError without a cassette/upstream seam, while cassette-backed LiveHybrid parity can replay Shopify's captured payload for evidence.
- Query JSONL synthesis is supported for `products` and `productVariants` only.
- Mutation import result-file schema, partial failure status/counter behavior, and broader per-domain imports require deeper executable evidence before being claimed as local lifecycle support.
- Accepted inner mutation roots without local executors use per-line upstream passthrough. That path is an explicit limitation and does not provide local downstream read-after-write effects.
- Daily/per-app `LIMIT_REACHED` quota behavior and Shopify's POS/product-feed client allowlist for `clientIdentifier` are not modeled.
- Registry-only support is not claimed for unsupported inner roots. Validation-only support includes schema analyzer branches and request/userError guardrails that fail before staging.

### Evidence

- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-status-catalog-cancel.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-query-schema-roots.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-query-validators.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-query-user-error-codes.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-query-group-objects.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operations-read-arg-validation.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operations-sort-key.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-mutation-user-errors.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-mutation-allowed-roots.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-mutation-created-status.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-mutation-client-identifier-validation.json`
- `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`
- `config/parity-specs/bulk-operations/bulk-operation-status-catalog-cancel.json`
- `config/parity-specs/bulk-operations/bulk-operation-run-query-created-status.json`
- `config/parity-specs/bulk-operations/bulk-operation-run-query-schema-roots.json`
- `config/parity-specs/bulk-operations/bulk-operation-run-query-validators.json`
- `config/parity-specs/bulk-operations/bulk-operation-run-query-operation-type-and-list-validators.json`
- `config/parity-specs/bulk-operations/bulk-operation-run-query-user-error-codes.json`
- `config/parity-specs/bulk-operations/bulk-operation-run-query-group-objects.json`
- `config/parity-specs/bulk-operations/bulk-operations-read-arg-validation.json`
- `config/parity-specs/bulk-operations/bulk-operations-sort-key.json`
- `config/parity-specs/bulk-operations/bulk-operation-run-mutation-user-errors.json`
- `config/parity-specs/bulk-operations/run-mutation-allowed-roots.json`
- `config/parity-specs/bulk-operations/bulk-operation-run-mutation-created-status.json`
- `config/parity-specs/bulk-operations/bulk-operation-run-mutation-client-identifier-validation.json`
- `config/parity-specs/bulk-operations/bulk-operation-run-mutation-operation-in-progress.json`

### Validation

- `corepack pnpm parity -- bulk-operation-status-catalog-cancel`
- `corepack pnpm parity -- bulk-operation-run-query-schema-roots`
- `corepack pnpm parity -- bulk-operation-run-mutation-user-errors`
- `corepack pnpm parity -- bulk-operations-read-arg-validation`
- `corepack pnpm conformance:check`
