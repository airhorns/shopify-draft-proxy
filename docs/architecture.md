# Architecture

## Overview

`shopify-draft-proxy` is an embeddable Shopify Admin GraphQL draft proxy. The
current implementation direction is the Gleam port in `gleam/`, which compiles
the same core domain model to JavaScript and Erlang. The legacy TypeScript/Koa
runtime in `src/` remains as a temporary compatibility baseline until the port
reaches parity and is deleted.

Across both implementations, the runtime supports local read/write staging,
versioned Shopify Admin GraphQL routes, and the same supported/unsupported
mutation split.

### Read execution modes

1. **live-hybrid**
   - read requests are sent to Shopify
   - returned payloads are patched with staged local effects
2. **snapshot**
   - reads are resolved from a startup snapshot plus staged state
   - absent data should behave like Shopify behaves when no matching backend data exists
3. **passthrough / live-only debugging**
   - the legacy TypeScript runtime names this mode `passthrough`
   - the Gleam API has a reserved `Live` read-mode variant for the same final
     cutover need
   - fully documented live-only server behavior is still part of the remaining
     port work

### Mutation execution paths

1. **supported mutation**
   - do not send to Shopify immediately
   - interpret the mutation into a domain command
   - apply the command to local staged state
   - synthesize a Shopify-like response
   - append to the mutation log
2. **unsupported mutation**
   - proxy through to Shopify unchanged
   - this is an intentional escape hatch and should be visible in observability

## High-level request flow

```text
App/test harness -> DraftProxy instance -> operation classifier
                                      ├─ Query path
                                      │   ├─ live Shopify read (optional)
                                      │   ├─ normalized overlay engine
                                      │   └─ GraphQL response serializer
                                      ├─ Mutation path
                                      │   ├─ supported? yes -> local stage + synthesized response
                                      │   └─ supported? no  -> passthrough to Shopify
                                      └─ Meta API path
                                          ├─ reset/log/state/config/health
                                          └─ commit replay

HTTP adapter -> DraftProxy instance
```

The HTTP adapter is still Koa in the legacy TypeScript runtime. The Gleam core
already accepts HTTP-shaped requests directly through `process_request`; the JS
HTTP server adapter and BEAM-facing serving story are separate cutover tasks.

## Primary modules

### `gleam/`

- current implementation track for the proxy core
- compiles to JavaScript and Erlang from the same domain implementation
- exposes `shopify_draft_proxy/proxy/draft_proxy.gleam` as the main runtime
  entry point
- owns explicit `DraftProxy` state threading: request processing returns both a
  `Response` and the next `DraftProxy`
- includes the TypeScript compatibility shim under `gleam/js/` and the Elixir
  shipment smoke project under `gleam/elixir_smoke/`

### `src/config.ts`

- parse environment/configuration
- select runtime mode
- hold Shopify upstream URL/version settings
- legacy TypeScript runtime only during the port; the final package cutover
  should not add new runtime authority here

### `src/app.ts`

- build Koa app
- register body parser and mount incoming HTTP requests onto a `DraftProxy`
  instance
- legacy TypeScript HTTP adapter kept until the Gleam JS HTTP adapter reaches
  route parity

### `src/proxy-instance.ts`

- public embeddable API for creating isolated proxy instances
- owns the runtime `AppConfig`, in-memory store, and synthetic ID/timestamp
  registry for that instance
- exposes request-form processing for versioned Shopify Admin GraphQL routes
  plus public meta-equivalent methods for config, log, state, reset, and commit
- provides the public object used by the Koa webservice adapter
- temporary compatibility baseline for the JS-facing `createDraftProxy(...)`
  shape; the Gleam JS shim is expected to preserve that public shape after the
  TypeScript runtime is removed

### `src/logger.ts`

- create the shared pino structured logger for runtime proxy logs
- use `pino-pretty` single-line output for local development
- provide child loggers for server and proxy modules
- keep unsupported mutation passthrough visible through structured warning logs

### `src/server.ts`

- start HTTP server
- legacy Koa entry point during the port

### `src/graphql/`

- parse GraphQL documents
- identify operation type and operation name
- eventually map known operations to capability records

### `src/search-query-parser.ts`

- parse Shopify-style Admin search query strings into shared typed terms and expression trees
- provide reusable term metadata (`field`, comparator, value, negation) and common text/number/date match helpers so endpoint groups do not maintain separate query parsers or comparator implementations

### `src/state/`

- define normalized object graph
- state store interface + in-memory implementation
- mutation log
- synthetic ID/timestamp generation

### `src/proxy/`

- request classifier
- read pipeline
- mutation pipeline
- response overlay engine
- GraphQL route dispatch keeps HTTP validation, auth/upstream wiring, and unsupported fallback passthrough in `routes.ts`; domain behavior is selected through a `DomainDispatcher` table whose entries own `canHandle`, mutation handling, query handling, and live-hybrid hydration decisions for their resource area.

### `src/shopify/`

- upstream HTTP client
- request serialization
- commit executor
- conformance helpers later

### `src/meta/`

- reset, commit, state, log endpoints
- shared meta handlers used by both the Koa webservice and the embeddable
  `DraftProxy` API

### `src/testing/`

- scenario fixtures
- recorder/replayer helpers
- parity comparators

### `scripts/conformance-scenario-registry.ts`

- discovers standard conformance scenarios recursively from `config/parity-specs/**/*.json`
- keeps scenario-to-operation mapping in parity specs instead of the runtime operation registry
- builds conformance status JSON for CI comments from discovered specs
- supports optional override config only for unusual scenario shapes

### `scripts/conformance-parity-lib.ts`

- classifies conformance scenarios by capture/proxy-request/comparison-contract readiness
- executes contract-ready proxy requests against local product proxy handlers in snapshot mode
- blocks live Shopify access during parity execution by rejecting unsupported operations instead of proxying them upstream
- compares captured Shopify payload slices to proxy payload slices with strict JSON semantics
- allows nondeterministic values only through explicit path-scoped rules in parity specs

## State model

The runtime should use a normalized object graph rather than raw GraphQL blobs.

### Core state containers

- `baseState`
  - snapshot-derived entities and/or normalized entities learned from reads
- `stagedState`
  - local inserts/updates/deletes/derived indexes not yet committed
- `mutationLog`
  - ordered list of raw mutation requests plus interpreted commands
- `syntheticIdentityRegistry`
  - stable generated GIDs, timestamps, handles, temp IDs
- `queryCache` (optional)
  - normalized read-through cache useful for overlay operations

Current implementation note:

- Each `createDraftProxy(config)` call creates an isolated in-memory runtime
  store, mutation log, and synthetic identity registry. Embedded callers should
  treat the returned `DraftProxy` object as the owner of runtime APIs such as
  request processing, state/log inspection, reset, and commit.
- Embedded callers can persist an instance with `DraftProxy.dumpState()` and
  rehydrate it with `DraftProxy.restoreState(...)` or
  `createDraftProxy(config, { state })`. The dump is a plain JSON-compatible,
  versioned envelope containing the instance-owned store state, mutation log,
  snapshot/reset baselines, runtime-only caches, and synthetic identity cursor.
  The store portion is serialized as a full own-field state map rather than a
  hand-maintained subset so newly added in-memory buckets cannot be silently
  omitted from persistence. Unknown envelope metadata is ignored by v1 restore
  so future dump writers can add extension data without invalidating the current
  reader.
- The Koa server creates a fresh `DraftProxy` instance when `createApp(config)`
  is called, unless the caller explicitly provides one to mount. The server does
  not use a process-wide runtime store or proxy singleton.
- Public `DraftProxy` meta APIs pass their owned store and synthetic identity
  explicitly. GraphQL request processing installs the instance runtime context
  before entering domain handlers.
- Domain handlers must receive instance-owned runtime state through the proxy
  request/runtime path. Do not introduce `AsyncLocalStorage` or process-wide
  mutable runtime singletons to bridge store or synthetic identity access.

## Admin API domain model

The normalized object graph should expand across Shopify Admin API domains as
local lifecycle fidelity is implemented. The current graph includes product
entities and several other domain-specific state buckets, and new domains
should be added without making any one domain special in the core engine.

Current product-domain entities include:

- Product
- ProductVariant
- ProductOption
- Metafield
- Media (even if partial)
- Collection entities plus product-scoped membership rows (lightly modeled at first)

Product-domain metafields are normalized as owner-scoped records for product, product variant, and collection owner IDs. Broader `HasMetafields` owner families still need domain-specific evidence and storage decisions before being added to shared `metafieldsSet` / `metafieldsDelete` staging.

Current customer-domain state deliberately stays narrower than the product model, but it is still normalized:

- `CustomerRecord` carries scalar/detail fields plus `taxExemptions` as a separate list from the boolean `taxExempt`
- customer-owned metafields live in a customer-scoped `customerMetafields` bucket instead of reusing product-domain metafield storage or broadening shared `metafieldsSet` owner support without separate customer-domain evidence
- staged `customerUpdate(input.metafields)` computes against the effective customer metafield set and replaces the staged customer-owned set, so downstream `customer.metafield(...)` and `customer.metafields(...)` reads stay consistent
- staged `customerMerge` updates the normalized resulting customer row, marks the source customer deleted, records the source-to-result customer id redirect in `mergedCustomerIds`, and records the observed merge job/result shape in `customerMergeRequests`
- the privacy-domain `dataSaleOptOut` mutation stores its downstream effect as `CustomerRecord.dataSaleOptOut`, keeping the mutation under privacy coverage while preserving customer read-after-write serialization

Localization state is also normalized in dedicated state buckets: available locales, shop locales, and translations live separately, while `TranslatableResource` rows are currently derived from the effective product and product-metafield graph. Locale and translation endpoint-specific boundaries are documented in `docs/endpoints/localization.md`.

Current B2B company-domain state is normalized and supports local lifecycle staging:

- `B2BCompanyRecord`, `B2BCompanyContactRecord`, `B2BCompanyContactRoleRecord`,
  and `B2BCompanyLocationRecord` store captured scalar fields plus normalized
  company-to-contact/location/role IDs
- snapshot reads support company catalog/count/detail roots and singular
  contact/role/location lookups, including empty/null behavior
- staged company, contact, location, role-assignment, address, staff-assignment,
  and tax-setting mutations write only to the in-memory staged state, preserve
  original raw mutation requests for commit replay, and update downstream
  company/contact/location/role reads without runtime Shopify writes
- B2B email delivery side effects remain unsupported because local staging
  cannot faithfully emulate outbound Shopify email delivery

Marketing-domain state keeps activity/event records and engagement metrics together but separate:

- external marketing activity lifecycle mutations stage normalized `MarketingActivity` and nested `MarketingEvent` records
- `marketingEngagementCreate` stages metric records keyed by the observed target and `occurredOn`, preserving duplicate same-day replacement behavior without inventing an engagement read root
- immediate activity/event aggregate reads stay faithful to captured Shopify behavior; for HAR-214 activity-level engagement writes did not materialize into `MarketingActivity.adSpend` on immediate downstream reads, so the local engagement records are visible through meta state/logs rather than fabricated aggregate attribution

Bulk Operations state is normalized as a shared job foundation rather than as product-, discount-, or metaobject-specific bulk behavior:

- `BulkOperation` jobs live in base/staged state with status, type, timestamps, counters, result URL fields, partial-data URL fields, error code, and original query text
- snapshot reads for `bulkOperation`, `bulkOperations`, and deprecated `currentBulkOperation` resolve from effective local state and return Shopify-like null/empty structures when no job exists
- `bulkOperations` uses the shared connection helpers for cursor windowing, `nodes`/`edges`, and selected `pageInfo`, while keeping BulkOperation-specific sort/filter decisions in the endpoint module
- `bulkOperationCancel` mutates only staged jobs, records the local cancel attempt in the mutation log, and returns captured userErrors for unknown or terminal operations without proxying supported cancel attempts upstream
- `bulkOperationRunQuery` stages supported product/product-variant query exports locally by parsing the submitted bulk query, validating documented bulk-query boundaries, writing a completed staged query job, and serving generated JSONL from an in-memory result URL
- `bulkOperationRunMutation` stages mutation imports locally when the staged upload content was uploaded through the proxy and the inner mutation root has a local bulk-import executor backed by an implemented Admin API mutation handler; unsupported inner roots create failed local jobs instead of runtime Shopify writes

## Mutation handling strategy

Mutation handling should eventually have four steps:

1. **parse** raw GraphQL document + variables
2. **interpret** into a normalized domain command, e.g. `ProductCreateCommand`
3. **apply** command against state store
4. **serialize** a Shopify-like response and userErrors array

This allows:

- deterministic testing
- commit replay from original raw mutation documents
- future conformance instrumentation per command type

Current route dispatch preserves the same supported/unsupported split through shared staged-log helpers. Supported dispatchers append the original raw request body and interpreted metadata before returning synthesized responses, while unknown or unimplemented mutations fall through to the route-level passthrough logger and upstream request path.

## Response overlay strategy

In live-hybrid mode:

1. fetch upstream Shopify read response
2. normalize the relevant subgraph
3. apply staged overlay
4. serialize back into GraphQL response shape

This design is preferred over blind JSON patching because it preserves domain-level consistency. However, early iterations may patch only specific product queries until the normalized serializer matures.

## Snapshot strategy

The server should accept both:

- raw recorded GraphQL fixture bundles
- normalized state snapshots

At startup, raw fixture bundles should be compiled into normalized state where possible.

Current implementation note:

- legacy `createApp()` reads `config.snapshotPath` eagerly when it is set
- the current legacy supported on-disk format is a normalized snapshot JSON file containing `baseState` plus optional product search connection baselines and customer catalog/search connection baselines
- legacy normalized snapshot JSON is parsed through Zod schemas at the file boundary; the same schemas derive the runtime snapshot TypeScript types
- loading that file seeds the in-memory base state before the server handles requests
- `POST /__meta/reset` restores that startup snapshot baseline, including captured connection cursor/pageInfo baselines, rather than wiping snapshot mode back to an empty store
- the Gleam port already preserves the state-dump envelope and restores the mutation log / synthetic identity slices; full normalized snapshot loading expands as the remaining domain slices are ported

Snapshot misses should return the same kind of empty/null structure Shopify returns when the backing store has no matching data.

## Meta API

Recommended endpoints:

- `GET /__meta`
- `POST /__meta/reset`
- `POST /__meta/commit`
- `GET /__meta/log`
- `GET /__meta/state`
- `GET /__meta/config`
- `GET /__meta/health`

Current implementation notes:

- legacy `GET /__meta` serves a small operator web UI backed by the existing meta API and in-memory store; the Gleam core has not yet reimplemented this UI route
- `GET /__meta/config` returns the active `port`, `shopifyAdminOrigin`, `readMode`, and `snapshotPath`
- `GET /__meta/state` returns cloned `baseState` / `stagedState` buckets for debug inspection; in the Gleam port this includes only slices that have been ported into the Gleam store serializer
- mutation-log entries retain the original GraphQL route path as well as the raw request body, so commit replay can preserve the original versioned Admin API endpoint and GraphQL request fields such as `operationName`
- Gleam `POST /__meta/commit` is synchronous on Erlang; JavaScript callers use `process_request_async` or the JS shim's async `processRequest` because upstream replay uses `fetch`

Commit response should include:

- ordered attempts
- success/failure per mutation
- stop index on first failure
- upstream response bodies or errors

## Fidelity rules

For supported operations, the proxy should aim to preserve:

- GraphQL field presence/absence
- connection structure (`edges`, `nodes`, `pageInfo`)
- nullability
- userErrors shape and common field paths
- ID stability for staged resources
- timestamp stability for staged resources
- consistent downstream query visibility

## Safety trade-off

Unsupported mutations proxy through by explicit product decision. This is dangerous in tests because it can create real side effects. The system should therefore expose clear indicators that a mutation was proxied instead of staged.

Operation registry entries must not encode permanent passthrough as an intended posture. A registered mutation should either be locally staged once supported or remain unimplemented until a local model exists; upstream passthrough is the unknown/unsupported escape hatch, not a target state for known write roots.

## Endpoint group docs

High-level runtime architecture belongs in this file. Endpoint-specific behavior, coverage boundaries, quirks, conformance capture details, and fixture-backed implementation notes belong under `docs/endpoints/<group>.md`.

Fully implemented endpoint groups should have enough detail for future agents to understand supported roots, local staging behavior, read-after-write expectations, and validation entry points without crowding this architecture overview.

## Conformance framework design

The conformance suite should include:

1. **recorders**
   - run named scenarios against real Shopify
   - save raw requests/responses
2. **state compilers**
   - convert recorded fixtures into normalized snapshots
3. **proxy parity tests**
   - replay scenarios against proxy with no live writes
   - compare payloads and downstream read behavior
4. **coverage registry**
   - map every query/mutation to implementation and parity status

Current proxy parity execution is intentionally contract-gated. A captured scenario with a proxy request is not executable until its parity spec declares strict JSON comparison targets and expected differences; captured specs must declare a non-planned comparison mode at the schema boundary. If a capture cannot yet run as proxy-vs-capture evidence, keep the gap in Linear/workpad notes rather than committing a passive scenario file. Within a declared comparison target, missing fields, extra fields, null/empty mismatches, array shape drift, changed `userErrors`, and selected-field changes fail by default. Declared expected differences are also checked in the other direction: if an expected difference no longer appears in the proxy-vs-Shopify comparison, the scenario fails until the stale expectation is removed. Expected differences are a last resort after modeling, hydration, or fixture-seeding options have been exhausted; they should not be used just to make parity tests pass. Opaque Shopify connection cursors are an accepted difference because clients cannot rely on their internal encoding. Multi-step fixture modes are reserved for flows backed by committed runtime tests when the generic parity runner cannot yet chain the fixture directly; they are not a parking place for passive captures.

Conformance registry JSON, parity specs, parity request variables, and conformance fixture JSON are validated with Zod when read by the registry/parity helpers. Types for operation registry entries, parity specs, proxy request specs, and blocker details are derived from those schemas instead of maintained as separate hand-written TypeScript interfaces.
