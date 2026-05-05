# Bulk Operations Endpoint Group

The Bulk Operations group covers Shopify Admin GraphQL's root-level asynchronous export/import API. These roots create, inspect, list, and cancel `BulkOperation` jobs; they are not product variant bulk mutations, inventory bulk toggles, discount bulk roots, or metaobject bulk delete.

HAR-263 adds the shared in-memory `BulkOperation` job model plus local read/list/current/cancel handling. HAR-264 adds local `bulkOperationRunQuery` export staging with generated JSONL result records. HAR-265 adds the first mutation-import execution slice backed by the proxy's local staged upload handoff.

## Current support and limitations

### Supported roots

Local overlay reads:

- `bulkOperation`
- `bulkOperations`
- `currentBulkOperation` (deprecated in favor of `bulkOperations` with filters)

Local-staging mutations:

- `bulkOperationCancel`
- `bulkOperationRunQuery` for supported `products` and `productVariants` query exports
- `bulkOperationRunMutation` for supported Admin API inner mutation roots with local bulk-import executors

Unsupported execution posture:

- There is no intentional passthrough posture for this root group in the checked-in registry slice. Every known Bulk Operations root is intercepted locally, but support is bounded by the behavior below. Unsupported query export shapes and unsafe mutation import roots fail locally with explicit `userErrors`; they must not be normalized as permanent passthrough support or described as broad Shopify import/export support.

### Current 2026-04 behavior and local coverage

`BulkOperation` represents an asynchronous query export or mutation import job. Current documented fields are `id`, `completedAt`, `createdAt`, `errorCode`, `fileSize`, `objectCount`, `partialDataUrl`, `query`, `rootObjectCount`, `status`, `type`, and `url`. Current Shopify bulk-operation docs also note that API versions `2026-01` and higher can run up to five bulk query operations at a time per shop; older versions allowed one query and one mutation operation at a time.

Current enum inventory:

- `BulkOperationStatus`: `CANCELED`, `CANCELING`, `COMPLETED`, `CREATED`, `EXPIRED`, `FAILED`, `RUNNING`
- `BulkOperationErrorCode`: `ACCESS_DENIED`, `INTERNAL_SERVER_ERROR`, `TIMEOUT`
- `BulkOperationType`: `MUTATION`, `QUERY`

Current root behavior:

- `bulkOperation(id: ID!)` returns one job by ID. Locally, unknown valid BulkOperation GIDs return `null`; malformed non-BulkOperation IDs return a top-level invalid-id error.
- `bulkOperations` returns the app's jobs as a connection, newest first by default, with pagination, `reverse`, `sortKey`, and search filters for `created_at`, `id`, `operation_type`, and `status`. Locally, the endpoint uses shared connection helpers for cursor windows, `nodes`/`edges`, and selected `pageInfo`.
- `currentBulkOperation(type: BulkOperationType = QUERY)` is deprecated but still documents the app's most recent query or mutation job. Locally, it selects the newest effective job for the requested type and defaults to `QUERY`.
- `bulkOperationRunQuery(query: String!, groupObjects: Boolean! = false)` creates an async query export. Locally supported product exports complete immediately against effective in-memory state, write JSONL result records, accept explicit `groupObjects: true`/`false` as well as the omitted default path, and never proxy supported export requests upstream at runtime. Before staging, the proxy refuses a new query run when any effective `QUERY` BulkOperation is non-terminal (`CREATED`, `RUNNING`, or `CANCELING`), returning `bulkOperation: null` and `userErrors[{ field: null, code: "OPERATION_IN_PROGRESS" }]`.
- `bulkOperationRunMutation(mutation: String!, stagedUploadPath: String!, clientIdentifier: String, groupObjects: Boolean = true)` creates an async mutation import from uploaded JSONL variables. The `groupObjects` argument is deprecated. After inner-mutation validation and before reading staged upload content or staging a job, the proxy refuses a new mutation run when any effective `MUTATION` BulkOperation is non-terminal, using the same `OPERATION_IN_PROGRESS` userError shape.
- `bulkOperationCancel(id: ID!)` starts asynchronous cancellation. Locally, staged non-terminal jobs transition to `CANCELING`; in LiveHybrid, a cold cancel can first read the target `BulkOperation` through the cassette/upstream read seam, then stage the local cancel overlay or terminal userError without sending Shopify's cancel mutation upstream. `CANCELING` remains non-terminal locally, so canceling an in-progress job does not unblock another same-type run until a terminal status is observed or staged.

### Local mutation import support

`bulkOperationRunMutation` is local-only for inner mutation roots that the operation registry classifies as locally staged. A successful local import requires all of the following:

- The JSONL variables file was uploaded to a proxy staged upload target returned by local `stagedUploadsCreate`.
- The caller passes the target `parameters { name: "key" }` value as `stagedUploadPath`.
- The `mutation` argument parses as a single-root mutation.
- The inner root is an implemented Admin API mutation root with `stage-locally` execution in the operation registry and a matching local mutation handler.

When those requirements are met, the proxy parses each non-empty JSONL line as one variables object, calls the same local domain mutation handler used by normal GraphQL mutations, stages downstream state, and creates a terminal local `BulkOperation` with `type: MUTATION` for subsequent reads. The mutation response itself returns Shopify's freshly-created job shape: `status: CREATED`, `completedAt: null`, zero counters, and no `fileSize` or `url`. The local result JSONL is stored in the in-memory job record and becomes visible through the synthetic `BulkOperation.url` under `/__meta/bulk-operations/<encoded-id>/result.jsonl` on later `bulkOperation(id:)`, `bulkOperations`, or `currentBulkOperation` reads.

Mutation log semantics are intentionally commit-oriented:

- The proxy records one staged mutation-log entry per JSONL line, in original line order.
- Each entry's replay body is the original inner mutation document plus that line's variables, so `__meta/commit` can preserve synthetic-to-authoritative ID mapping with the existing commit executor.
- Each entry also carries `interpreted.bulkOperationImport` metadata with the local BulkOperation ID, line number, staged upload path, original outer bulk request body, and inner mutation text.
- The outer `bulkOperationRunMutation` request itself is preserved as metadata rather than as an additional staged commit entry, because replaying the outer request would require recreating Shopify-hosted staged upload storage during commit.

Unsupported or unsafe imports fail locally without upstream Shopify writes. Argument and validation branches such as a missing staged upload object, malformed inner mutation, non-mutation operation, multiple top-level mutation fields, or unsupported inner root return Shopify-like `userErrors` with `bulkOperation: null` and do not stage a local job. Once a valid uploaded JSONL import starts, per-line failures such as malformed JSONL still stage a failed local `BulkOperation` with a local result artifact for observability.

Per-line result behavior is locally modeled but still needs deeper live Shopify evidence for every branch. Domain mutation validation userErrors are represented in the corresponding result JSONL row and do not increment `objectCount`; invalid JSONL lines create result rows with `errors` and mark the local job `FAILED`. Dedicated live conformance for Shopify's exact import result-file schema, partial failure status semantics, and counter/file-size edge cases remains needed before broadening this beyond already supported local mutation roots.

Failed local mutation-import jobs created after a valid import starts remain normal BulkOperation records. They are readable by `bulkOperation(id:)`, appear in `bulkOperations(first:, query: "status:FAILED operation_type:MUTATION")`, can be returned by `currentBulkOperation(type: MUTATION)` when newest, expose a local `url` result artifact, and keep `partialDataUrl: null` until a fixture proves Shopify returns partial-data artifacts for the corresponding branch.

## Parity promotion status

The checked-in executable parity scenario is `config/parity-specs/bulk-operations/bulk-operation-status-catalog-cancel.json`, backed by the 2026-04 fixture at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-status-catalog-cancel.json`. It is a cassette-backed `captured-vs-proxy-request` scenario with `strict-json` comparison targets, so it is discovered and run by the Gleam parity runner on both JavaScript and Erlang targets.

The scenario compares whole selected BulkOperation payload slices for empty reads, empty filtered connections, current-operation null behavior, cancel userErrors, local staged cancel, captured `bulkOperationRunQuery` validation for submitted queries with no connection, the immediate `CREATED` query-export response, terminal product query export completion on read-after-local-run, and downstream read behavior. The LiveHybrid cassette includes Pattern 2 reads for prior `BulkOperation` hydration and upstream product count hydration; supported mutations still stage locally and do not send Shopify mutations upstream. Runtime integration coverage exercises mutation-import execution because the generic parity runner does not yet replay upload handoff flows: product, product variant, customer, and location imports run through the same local staged mutation handlers as normal Admin API mutations; unsupported roots and malformed JSONL fail locally without upstream passthrough and remain visible through downstream BulkOperation reads.

Path-scoped expected differences are limited to local infrastructure differences for staged query exports: synthetic BulkOperation IDs, local timestamps, local result URLs, and generated JSONL byte-size reporting. Those differences are not accepted at the scenario level and are checked per target.

### Version drift

The checked-in 2025-01 root introspection fixture contains `bulkOperationRunQuery`, `bulkOperationRunMutation`, and `bulkOperationCancel` under `mutationRoot`. It does not contain the current documented read roots `bulkOperation`, `bulkOperations`, or deprecated `currentBulkOperation`.

Future fixture refresh should confirm whether those reads were added after the captured 2025-01 schema, were unavailable to that conformance app/token, or were omitted for another version/scoping reason. Until then, HAR-261 records the query roots from 2026-04 docs and the mutation roots from both docs and the checked-in fixture.

### Coverage boundaries

- Do not model these roots through product variant bulk operations. `productVariantsBulkCreate`, `productVariantsBulkUpdate`, and `productVariantsBulkDelete` are product-domain staging roots with immediate product read-after-write expectations.
- Do not model these roots through inventory bulk toggles. `inventoryBulkToggleActivation` changes inventory activation state, not Admin API export/import jobs.
- Do not model these roots through discount bulk roots. Discount bulk activation/deactivation/delete and redeem-code bulk operations have discount-specific selector semantics and partial local guardrails.
- Do not model these roots through `metaobjectBulkDelete`. Metaobject bulk delete is custom-data deletion behavior, not the generic `BulkOperation` job controller.
- Do not add planned-only parity specs or parity request placeholders for this group. Add parity specs only after captured Shopify interactions can run as executable evidence with strict comparison targets.

### Local state and behavior

The normalized job model stores `id`, `status`, `type`, `errorCode`, `createdAt`, `completedAt`, `objectCount`, `rootObjectCount`, `fileSize`, `url`, `partialDataUrl`, `query`, and optional cursor metadata in base and staged state. Snapshot loading can seed base jobs, direct staging can add local jobs, and `POST /__meta/reset` restores the startup base snapshot while clearing staged jobs and logs.

Local `bulkOperations` supports:

- empty connections with `edges: []`, `nodes: []`, `hasNextPage: false`, `hasPreviousPage: false`, and null cursors
- `first`/`last`, `after`, and `before` cursor windowing through `paginateConnectionItems(...)`
- selected `nodes`, `edges`, and `pageInfo` serialization through `serializeConnection(...)`
- default newest-first `CREATED_AT` ordering, `sortKey: ID`, `reverse`, and search filters for `created_at`, `id`, `operation_type`, and `status`

Local `bulkOperationRunQuery` supports:

- one top-level connection rooted at `products` or `productVariants`
- product scalar selections already supported by local product reads
- nested `products { ... variants { ... } }` exports with flat JSONL output where each variant line receives `__parentId`
- root `productVariants` exports with product-variant scalar selections already supported by local product variant reads
- effective local/snapshot state as the export source, including staged products and variants
- LiveHybrid product-export count hydration from upstream when the local product store is cold, so the staged job counters reflect the upstream store while the export mutation itself remains local-only
- immediate mutation responses with Shopify's created job shape: `status: CREATED`, `completedAt: null`, zero counters, `fileSize: null`, `url: null`, `partialDataUrl: null`, and original `query`
- terminal staged `BulkOperation` rows on subsequent reads with `status: COMPLETED`, `type: QUERY`, `completedAt`, `objectCount`, `rootObjectCount`, `fileSize`, `url`, `partialDataUrl: null`, and original `query`
- `groupObjects: true`, `groupObjects: false`, and omitted `groupObjects` arguments all stage the same supported local export shape; grouped JSONL ordering is not modeled as a separate local result mode
- local result URLs at `https://shopify-draft-proxy.local/__meta/bulk-operations/<encoded-gid>/result.jsonl`; the JS adapter serves the matching path as `application/jsonl` from instance-owned memory until reset
- `fileSize` is the byte length of the generated local JSONL payload. Captured Shopify `fileSize` values can differ from the downloaded JSONL byte length because Shopify reports its stored artifact size.
- original raw mutation bodies in the meta mutation log for commit/replay observability

Local `bulkOperationRunQuery` rejects these branches locally with `userErrors` and no upstream runtime request:

- missing `query`, matching the captured top-level `missingRequiredArguments` shape
- malformed submitted bulk query strings, including an empty string returning `Invalid bulk query: syntax error, unexpected end of file`
- no connection, using the captured message `Bulk queries must contain at least one connection.`
- multiple top-level fields, top-level `node`/`nodes`, unsupported roots, more than five detected connections, and connections deeper than two levels
- unsupported nested connections other than product `variants`
- selected `BulkOperationUserError.code` values are serialized for these local validation branches as `INVALID`, matching the captured 2026-04 Admin API behavior instead of dropping the selected field
- same-type in-progress operations: if any effective `QUERY` job has a non-terminal status, local `bulkOperationRunQuery` returns `bulkOperation: null` plus `userErrors[{ field: null, message: "A bulk query operation for this app and shop is already in progress: <id>.", code: "OPERATION_IN_PROGRESS" }]` without staging a second job. Daily/per-app `LIMIT_REACHED` quota modeling is not implemented.

Local `bulkOperationRunMutation` rejects same-type in-progress operations with `bulkOperation: null` plus `userErrors[{ field: null, message: "A bulk mutation operation for this app and shop is already in progress: <id>.", code: "OPERATION_IN_PROGRESS" }]` without reading staged upload content, importing JSONL variables, staging a failed job, or calling upstream Shopify. Daily/per-app `LIMIT_REACHED` quota modeling is not implemented for mutation imports either.

Local `bulkOperationCancel` supports:

- `RUNNING`/`CREATED`/`CANCELING` staged jobs returning a selected `bulkOperation` payload and empty `userErrors`, with non-terminal staged jobs stored as `CANCELING`
- LiveHybrid prior-operation hydration for cold known IDs, then local staging of a `CANCELING` overlay for non-terminal jobs
- unknown IDs returning `bulkOperation: null` plus `userErrors[{ field: ["id"], message: "Bulk operation does not exist" }]`
- terminal jobs returning the selected existing job plus a `field: null` userError such as `A bulk operation cannot be canceled when it is completed`
- meta log entries with original raw mutation bodies and staged BulkOperation IDs for observability

## Meta API observability

BulkOperation jobs are inspectable through the standard meta surfaces:

- `GET /__meta/state` returns `baseState.bulkOperations`, `baseState.bulkOperationOrder`, `stagedState.bulkOperations`, `stagedState.bulkOperationOrder`, and the generated query-export `bulkOperationResults` map. Mutation import jobs also store `resultJsonl` on the staged BulkOperation record so the result artifact is visible next to the job metadata.
- `GET /__meta/log` returns the original staged mutation log in replay order. Query exports and cancel attempts appear as their original root mutation request. Mutation imports appear as one staged inner mutation entry per JSONL line, preserving the original line variables and carrying `interpreted.bulkOperationImport` metadata for the outer bulk request, staged upload path, inner mutation text, and line number.
- `GET /__meta/bulk-operations/<encoded-gid>/result.jsonl` serves generated query-export JSONL from the in-memory `bulkOperationResults` map and mutation-import result JSONL from the staged BulkOperation record.
- `POST /__meta/reset` restores the startup snapshot and clears staged BulkOperation jobs, generated result records, staged uploads, and mutation logs.
- `POST /__meta/commit` replays only staged mutation-log entries. For mutation imports, that means the inner mutation entries are sent upstream in JSONL line order; the outer `bulkOperationRunMutation` request is not replayed because it is stored as audit metadata for commit review.

## Historical and developer notes

### Conformance evidence still needed

- Validation/userErrors for malformed export queries, unsupported connections, nesting limits, overlapping active jobs, missing staged upload paths, and invalid mutation documents.
- Full status transition behavior across `CREATED`, `RUNNING`, `CANCELING`, `CANCELED`, `COMPLETED`, `EXPIRED`, and `FAILED`, including result URL/partial-data URL expiry, counters, file sizes, and error codes.
- Read-after-write behavior from locally staged `bulkOperationRunQuery` and broader `bulkOperationRunMutation` import families through `bulkOperation`, `bulkOperations`, and `currentBulkOperation`.
- Shopify's exact `bulkOperationRunMutation` result JSONL schema and partial-failure status/counter semantics for product import validation and malformed-line branches.
- `bulkOperationRunQuery` parity for non-product roots, grouped output, active-job limits, failure/partial-data branches, and exact Shopify result URL expiry semantics.

### Captured 2026-04 evidence

HAR-262 adds a live 2026-04 fixture at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-status-catalog-cancel.json`, produced by `corepack pnpm tsx scripts/capture-bulk-operation-status-conformance.ts`. The fixture is registered by `config/parity-specs/bulk-operations/bulk-operation-status-catalog-cancel.json`. HAR-346 promotes the local read/cancel slice from fixture-only evidence to `captured-vs-proxy-request` parity: the parity runner strictly compares unknown-id reads, empty running-query/running-mutation lists, empty `currentBulkOperation(type: MUTATION)`, unknown/terminal cancel userErrors, staged local cancel, and read-after-local-cancel. HAR-264 extends that same fixture with downloaded product-export JSONL records, replays `bulkOperationRunQuery`, and compares the immediate created job plus downstream terminal `bulkOperation(id:)` read to the captured Shopify lifecycle with only synthetic IDs/timestamps/result URLs/file-size infrastructure differences allowed. HAR-396 adds runtime coverage for failed mutation-import job visibility after malformed JSONL, including result URL serving and downstream BulkOperation status reads. HAR-528 migrates the scenario to cassette-backed Gleam parity: prior BulkOperation records and product counts are hand-synthesized into `upstreamCalls` from the checked-in capture evidence, replacing the retired base-state seeding pattern. HAR-750 adds focused CREATED-on-submit parity at `config/parity-specs/bulk-operations/bulk-operation-run-query-created-status.json` and captures mutation-import submit behavior at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-mutation-created-status.json`.

HAR-725 adds focused 2026-04 validation evidence at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-mutation-user-errors.json`, produced by `corepack pnpm conformance:capture -- --run bulk-operation-run-mutation-user-errors`. The strict parity spec `config/parity-specs/bulk-operations/bulk-operation-run-mutation-user-errors.json` proves that missing staged upload files return `bulkOperation: null`, `field: null`, and `code: "NO_SUCH_FILE"`; parser failures return `field: null` and `code: "INVALID_MUTATION"`; and valid-but-disallowed inner roots use Shopify's allowlist validator message.

HAR-733 adds a focused 2026-04 fixture at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-query-user-error-codes.json`, produced by `corepack pnpm tsx scripts/capture-bulk-operation-run-query-user-error-codes-conformance.ts`. The strict parity spec `config/parity-specs/bulk-operations/bulk-operation-run-query-user-error-codes.json` proves that selecting `userErrors { field message code }` returns `code: "INVALID"` for both no-connection validation and empty-query malformed-query branches.

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
- The completed query export fixture includes the downloaded JSONL records for `products { edges { node { id title } } }`; integration coverage replays those exact records through the local result URL.
- A second query export was canceled immediately, returning `CANCELING` from `bulkOperationCancel` and later `CANCELED` from `bulkOperation(id:)`; its counters and result URL behavior are fixture-backed and should not be guessed from the completed branch.

### Validation anchors

- Captured root inventory: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`
