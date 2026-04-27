# Bulk Operations Endpoint Group

The Bulk Operations group covers Shopify Admin GraphQL's root-level asynchronous export/import API. These roots create, inspect, list, and cancel `BulkOperation` jobs; they are not product variant bulk mutations, inventory bulk toggles, discount bulk roots, or metaobject bulk delete.

HAR-263 adds the shared in-memory `BulkOperation` job model plus local read/list/current/cancel handling. It does not execute query exports, mutation imports, JSONL result generation, or staged upload processing.

## Supported roots

Local overlay reads:

- `bulkOperation`
- `bulkOperations`
- `currentBulkOperation` (deprecated in favor of `bulkOperations` with filters)

Local-staging mutations:

- `bulkOperationCancel`

Unsupported execution roots:

- `bulkOperationRunQuery`
- `bulkOperationRunMutation`

The run roots remain `implemented: false`; they may proxy through as unsupported mutations, but they must not be treated as permanent passthrough support.

## Current 2026-04 behavior and local coverage

`BulkOperation` represents an asynchronous query export or mutation import job. Current documented fields are `id`, `completedAt`, `createdAt`, `errorCode`, `fileSize`, `objectCount`, `partialDataUrl`, `query`, `rootObjectCount`, `status`, `type`, and `url`.

Current enum inventory:

- `BulkOperationStatus`: `CANCELED`, `CANCELING`, `COMPLETED`, `CREATED`, `EXPIRED`, `FAILED`, `RUNNING`
- `BulkOperationErrorCode`: `ACCESS_DENIED`, `INTERNAL_SERVER_ERROR`, `TIMEOUT`
- `BulkOperationType`: `MUTATION`, `QUERY`

Current root behavior:

- `bulkOperation(id: ID!)` returns one job by ID. Locally, unknown valid BulkOperation GIDs return `null`; malformed non-BulkOperation IDs return a top-level invalid-id error.
- `bulkOperations` returns the app's jobs as a connection, newest first by default, with pagination, `reverse`, `sortKey`, and search filters for `created_at`, `id`, `operation_type`, and `status`. Locally, the endpoint uses shared connection helpers for cursor windows, `nodes`/`edges`, and selected `pageInfo`.
- `currentBulkOperation(type: BulkOperationType = QUERY)` is deprecated but still documents the app's most recent query or mutation job. Locally, it selects the newest effective job for the requested type and defaults to `QUERY`.
- `bulkOperationRunQuery(query: String!, groupObjects: Boolean! = false)` creates an async query export. Shopify documents one bulk query operation and one bulk mutation operation at a time per shop.
- `bulkOperationRunMutation(mutation: String!, stagedUploadPath: String!, clientIdentifier: String, groupObjects: Boolean = true)` creates an async mutation import from uploaded JSONL variables. The `groupObjects` argument is deprecated.
- `bulkOperationCancel(id: ID!)` starts asynchronous cancellation. Locally, staged non-terminal jobs transition to `CANCELING`; terminal and unknown jobs return captured userErrors without upstream access.

## Version drift

The checked-in 2025-01 root introspection fixture contains `bulkOperationRunQuery`, `bulkOperationRunMutation`, and `bulkOperationCancel` under `mutationRoot`. It does not contain the current documented read roots `bulkOperation`, `bulkOperations`, or deprecated `currentBulkOperation`.

Future fixture refresh should confirm whether those reads were added after the captured 2025-01 schema, were unavailable to that conformance app/token, or were omitted for another version/scoping reason. Until then, HAR-261 records the query roots from 2026-04 docs and the mutation roots from both docs and the checked-in fixture.

## Coverage boundaries

- Do not model these roots through product variant bulk operations. `productVariantsBulkCreate`, `productVariantsBulkUpdate`, and `productVariantsBulkDelete` are product-domain staging roots with immediate product read-after-write expectations.
- Do not model these roots through inventory bulk toggles. `inventoryBulkToggleActivation` changes inventory activation state, not Admin API export/import jobs.
- Do not model these roots through discount bulk roots. Discount bulk activation/deactivation/delete and redeem-code bulk operations have discount-specific selector semantics and partial local guardrails.
- Do not model these roots through `metaobjectBulkDelete`. Metaobject bulk delete is custom-data deletion behavior, not the generic `BulkOperation` job controller.
- Do not add planned-only parity specs or parity request placeholders for this group. Add parity specs only after captured Shopify interactions can run as executable evidence with strict comparison targets.

## Local state and behavior

The normalized job model stores `id`, `status`, `type`, `errorCode`, `createdAt`, `completedAt`, `objectCount`, `rootObjectCount`, `fileSize`, `url`, `partialDataUrl`, `query`, and optional cursor metadata in base and staged state. Snapshot loading can seed base jobs, direct staging can add local jobs, and `POST /__meta/reset` restores the startup base snapshot while clearing staged jobs and logs.

Local `bulkOperations` supports:

- empty connections with `edges: []`, `nodes: []`, `hasNextPage: false`, `hasPreviousPage: false`, and null cursors
- `first`/`last`, `after`, and `before` cursor windowing through `paginateConnectionItems(...)`
- selected `nodes`, `edges`, and `pageInfo` serialization through `serializeConnection(...)`
- default newest-first `CREATED_AT` ordering, `sortKey: ID`, `reverse`, and search filters for `created_at`, `id`, `operation_type`, and `status`

Local `bulkOperationCancel` supports:

- `RUNNING`/`CREATED`/`CANCELING` staged jobs returning a selected `bulkOperation` payload and empty `userErrors`, with non-terminal staged jobs stored as `CANCELING`
- unknown IDs returning `bulkOperation: null` plus `userErrors[{ field: ["id"], message: "Bulk operation does not exist" }]`
- terminal jobs returning the selected existing job plus a `field: null` userError such as `A bulk operation cannot be canceled when it is completed`
- meta log entries with original raw mutation bodies and staged BulkOperation IDs for observability

## Conformance evidence still needed before run support

- Validation/userErrors for malformed export queries, unsupported connections, nesting limits, overlapping active jobs, missing staged upload paths, and invalid mutation documents.
- Full status transition behavior across `CREATED`, `RUNNING`, `CANCELING`, `CANCELED`, `COMPLETED`, `EXPIRED`, and `FAILED`, including result URL/partial-data URL expiry, counters, file sizes, and error codes.
- Read-after-write behavior from locally staged `bulkOperationRunQuery` and `bulkOperationRunMutation` through `bulkOperation`, `bulkOperations`, and `currentBulkOperation`.

## Captured 2026-04 evidence

HAR-262 adds a live 2026-04 fixture at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operation-status-catalog-cancel.json`, produced by `corepack pnpm tsx scripts/capture-bulk-operation-status-conformance.ts`. The fixture is registered by `config/parity-specs/bulk-operation-status-catalog-cancel.json` and enforced by `tests/integration/bulk-operation-conformance-flow.test.ts`; HAR-263 reuses that evidence for local null/empty/userError/list/current behavior while leaving export/import execution unsupported.

The fixture captures these read and validation branches:

- `bulkOperation(id: "gid://shopify/BulkOperation/0")` returns `bulkOperation: null`; a malformed non-GID string returns a top-level invalid-id error.
- Running-query and running-mutation `bulkOperations` filters return an empty connection with `edges: []`, `nodes: []`, and all cursors `null`.
- `currentBulkOperation(type: MUTATION)` returns `null` on the captured store, while `currentBulkOperation(type: QUERY)` can return the most recent query job even when it is terminal.
- Missing `bulkOperation(id:)`, missing `bulkOperationCancel(id:)`, and missing `bulkOperationRunQuery(query:)` fail as top-level GraphQL `missingRequiredArguments` errors.
- `bulkOperations` without `first`/`last`, with both `first` and `last`, or with an invalid `created_at` timestamp filter fails as top-level `BAD_REQUEST`.
- `bulkOperationCancel(id:)` for an unknown ID returns `bulkOperation: null` plus `userErrors[{ field: ["id"], message: "Bulk operation does not exist" }]`.
- `bulkOperationRunQuery(query:)` with no connection returns `bulkOperation: null` plus `userErrors[{ field: ["query"], message: "Bulk queries must contain at least one connection." }]`.

The fixture also captures two safe no-write product export lifecycles:

- A query export transitioned from `CREATED` to `COMPLETED` and populated `completedAt`, `objectCount`, `rootObjectCount`, `fileSize`, `url`, and `partialDataUrl: null`; canceling that terminal operation returned a userError with `field: null`.
- A second query export was canceled immediately, returning `CANCELING` from `bulkOperationCancel` and later `CANCELED` from `bulkOperation(id:)`; its counters and result URL behavior are fixture-backed and should not be guessed from the completed branch.

## Validation anchors

- Registry and schema checks: `tests/unit/operation-registry.test.ts`, `tests/unit/json-file-schemas.test.ts`
- Root inventory discovery: `tests/unit/graphql-operation-coverage.test.ts`
- Captured root inventory: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-graphql-root-operation-introspection.json`
