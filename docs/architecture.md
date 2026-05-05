# Architecture

## Overview

`shopify-draft-proxy` is an embeddable Shopify Admin GraphQL draft proxy.
The runtime is implemented in Gleam under `src/shopify_draft_proxy/`
and compiles to both Erlang/BEAM and JavaScript so it can be embedded in
either ecosystem. The JavaScript target also exposes a Node `http` adapter over
the Gleam core. It supports three read execution modes and two mutation
execution paths.

### Read execution modes

1. **live-hybrid**
   - read requests are sent to Shopify
   - returned payloads are patched with staged local effects
2. **snapshot**
   - reads are resolved from a startup snapshot plus staged state
   - absent data should behave like Shopify behaves when no matching backend data exists
3. **passthrough**
   - reads are forwarded directly with no overlay
   - useful as a debugging baseline

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

Unsupported mutation handling is configurable. The default
`unsupportedMutationMode` is `passthrough`, which preserves the historical
live-hybrid escape hatch. When set to `reject`, the dispatcher returns a 400
GraphQL error envelope for unsupported mutation roots before any upstream
transport call. Supported mutation roots still stage locally in both modes.

## High-level request flow

```text
App/test harness -> DraftProxy value -> operation classifier
                                   ├─ Query path
                                   │   ├─ live Shopify read (optional, via cassette in tests)
                                   │   ├─ normalized overlay engine
                                   │   └─ GraphQL response serializer
                                   ├─ Mutation path
                                   │   ├─ supported? yes -> local stage + synthesized response
                                   │   └─ supported? no  -> passthrough or 400 reject
                                   └─ Meta API path
                                       ├─ reset/log/state/config/health
                                       └─ commit replay

Gleam JS HTTP adapter -> DraftProxy value
```

`DraftProxy` is a value, not a singleton. Each request returns a new
`DraftProxy` alongside the response (`process_request(proxy, request) ->
#(Response, DraftProxy)`), threading state explicitly. There is no
ambient runtime context — embedders own the value and decide how to
persist it.

## Primary modules (Gleam runtime)

The runtime tree lives at `src/shopify_draft_proxy/`.

### `proxy/draft_proxy.gleam`

- public embeddable API: `new`, `with_config`, `with_upstream_transport`,
  `process_request`, `dispatch_graphql`, `dump_state`, `restore_state`
- the `DraftProxy` record carries the active config, synthetic identity
  registry, store, operation registry, and an optional
  `upstream_transport` used to inject a cassette in parity tests
- routes parsed GraphQL operations through `route_query` / `route_mutation`
  and falls through to `dispatch_passthrough` for unimplemented or
  force-passthrough roots in `LiveHybrid` mode unless reject-unsupported mode
  is enabled for mutation roots

### `js/src/app.ts`

- build a JavaScript-target Node `http` adapter over the Gleam-backed
  `DraftProxy` shim
- parse incoming request bodies, preserve inbound headers, and route HTTP
  requests through the same `processRequest(...)` surface as embeddable JS
  callers
- expose `callback()` and `listen(...)` helpers so launch scripts can serve
  `/admin/api/:version/graphql.json`, `/__meta/health`, `/__meta/config`,
  `/__meta/log`, `/__meta/state`, `/__meta/reset`, `/__meta/commit`,
  staged-upload targets under `/staged-uploads/...`, and generated bulk
  operation JSONL artifacts under `/__meta/bulk-operations/...` without Koa

### `proxy/proxy_state.gleam`

- defines `DraftProxy`, `Config`, and `ReadMode` (`Live | LiveHybrid |
Snapshot`)
- holds the `with_*` builders that any caller composes to configure a
  proxy value

### `proxy/upstream_query.gleam`

- single chokepoint operation handlers use to call upstream Shopify
- `fetch_sync(origin, transport, headers, operation_name, query,
variables)` returns a parsed `JsonValue` AST
- in production, falls through to `upstream_client.send_sync` (Erlang)
  or fails on JS until an async helper lands; in parity tests, a
  recorded cassette is installed via `with_upstream_transport`

### `proxy/<domain>.gleam` (one per Admin API area)

- `customers`, `products`, `orders`, `discounts`, `markets`,
  `metafields`, `metafield_definitions`, `metaobjects`, `b2b`,
  `marketing`, `shipping_fulfillments`, `gift_cards`, `online_store`,
  `localization`, `apps`, `admin_platform`, `bulk_operations`,
  `functions`, `payments`, `media`, `delivery_settings`, `events`
- each module owns its query payload builders, mutation interpreters,
  state mutators, and Node serializers for its resource area
- domains decide on a per-operation basis whether to reach upstream
  via `upstream_query.fetch_sync` (see `docs/parity-runner.md`)

### `proxy/operation_registry.gleam` + `operation_registry_data.gleam`

- vendored, pre-compiled mapping from operation name → capability
  (execution kind, dispatch metadata, type, etc.)
- the checked-in Gleam data module is the operation-registry source of truth;
  TypeScript conformance tooling reads it through
  `scripts/support/operation-registry.ts`

### `proxy/commit.gleam`

- replays the staged mutation log against real Shopify on
  `__meta/commit`
- `run_commit_sync(store, origin, headers, send)` is parameterised on
  the transport so tests inject a fake `send`

### `state/store.gleam` + `state/types.gleam`

- normalized in-memory object graph: products, variants, options,
  metafields, customers, orders, discounts, markets, b2b companies,
  marketing activities, locales/translations, bulk operations, etc.
- two-layer overlay: `base_state` (snapshot or hydrated from upstream)
  - `staged_state` (local mutation effects). Effective reads merge the
    two; commit drains staged.

### `state/synthetic_identity.gleam`

- mints stable proxy-internal GIDs, timestamps, handles, temp IDs
- per-instance cursor so two `DraftProxy` values produce independent
  identity streams

### `state/serialization.gleam`

- versioned dump/restore of the entire `DraftProxy` value as JSON
- envelope is forward-compatible: unknown extension fields are ignored
  by current readers

### `graphql/`

- `lexer.gleam`, `parser.gleam`, `ast.gleam`, `parse_operation.gleam`,
  `root_field.gleam` — a hand-written GraphQL document parser tuned
  for the operations Shopify Admin actually serves
- avoids the dependency footprint and allocator costs of the canonical
  GraphQL grammar; supports the subset the proxy needs

### `search_query_parser.gleam`

- parses Shopify-style Admin search query strings into shared typed
  terms and expression trees
- domain modules use it for `query:` parsing instead of maintaining
  separate per-resource parsers

### `proxy/graphql_helpers.gleam`

- shared cursor windowing, `nodes`/`edges` serialization, `pageInfo`
  emission used by every connection root in the runtime
- domain modules pass sort/filter decisions in; the helper handles the
  connection envelope

### `shopify/upstream_client.gleam`

- HTTP client for upstream Shopify
- `Transport` type alias used by both `commit` (mutation replay) and
  `upstream_query` (per-operation reads)

### Build scripts (TypeScript)

The runtime is Gleam, but the build/recording/registry tooling is
still TypeScript under `scripts/`:

- `scripts/parity-record.mts` — boots the Gleam JS build in
  `LiveHybrid` against real Shopify and records `upstreamCalls`
  cassettes into capture files
- `scripts/conformance-capture-index.ts` — discovers conformance
  capture scripts and runs them
- `scripts/shopify-conformance-auth.mts` — OAuth bootstrap for
  conformance/recording credentials

The legacy TypeScript proxy runtime has been removed. Root `src/` is now the
Gleam runtime tree; remaining TypeScript lives outside root `src/` and is
limited to the JavaScript interop shim plus conformance, capture, registry, and
report tooling.

### `test/parity/runner.gleam` + `test/parity_test.gleam`

Parity runner. Discovers every spec under `config/parity-specs/**` and
runs each one through `draft_proxy.process_request` on both Erlang and
JavaScript targets via `pnpm gleam:test`.

- runs the proxy in `LiveHybrid` mode with a recorded cassette
  (`upstreamCalls` array on the capture file) installed via
  `draft_proxy.with_upstream_transport`. Operation handlers may call
  `proxy/upstream_query.fetch_*` to reach upstream; the cassette
  serves those calls deterministically
- treats a missing or malformed `upstreamCalls` cassette as a hard
  parity failure; every checked-in spec is expected to run
- compares captured Shopify payload slices to proxy payload slices with
  strict JSON semantics; nondeterministic values are tolerated only via
  explicit path-scoped rules (`expectedDifferences`) in the spec
- `expectedDifferences` is a last resort, not a fixture-seeding
  shortcut; the runner does **not** pre-seed `base_state` from
  captured responses
- see `docs/parity-runner.md` for the cassette schema, the two
  per-operation upstream-access patterns, the empty-snapshot variant,
  and the coverage repair playbook

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

Implementation:

- `draft_proxy.new()` returns a fresh `DraftProxy` value with an empty
  store, empty mutation log, default config, and a fresh synthetic
  identity cursor. `with_config(...)`, `with_upstream_transport(...)`,
  and the other `with_*` builders compose configuration onto the
  value.
- The proxy is a value, not a stateful service. Every request returns
  the next proxy alongside the response: `process_request(proxy,
request) -> #(Response, DraftProxy)`. Embedders own the value and
  decide whether to thread it through their request loop, store it in
  an Erlang `gen_server`, or hold it in a JS variable.
- `dump_state(proxy, created_at)` serializes the entire value (store,
  mutation log, snapshot/reset baselines, synthetic identity cursor,
  runtime caches) as a versioned JSON envelope.
  `restore_state(proxy, dump_json)` rehydrates one. The store portion
  is serialized as a full own-field state map rather than a
  hand-maintained subset, so newly added buckets are persisted
  automatically. Unknown envelope metadata is ignored on read so
  future writers can extend the format without breaking current
  readers.
- Domain handlers receive the instance-owned store and synthetic
  identity through the proxy value passed into them. There is no
  ambient runtime context, no process-wide singleton, and no
  equivalent of `AsyncLocalStorage`.
- The Gleam JS HTTP adapter owns one `DraftProxy` value per app instance and
  replaces it with the returned next value after each request. The adapter does
  not use a process-wide runtime store or proxy singleton.

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

`dispatch_graphql` in `proxy/draft_proxy.gleam` preserves the
supported/unsupported split. Supported domain handlers append the
original raw request body and interpreted metadata to the mutation log
before returning a synthesized response. Unknown or unimplemented
mutations fall through to `dispatch_passthrough`, which forwards
verbatim to the upstream transport.

## Response overlay strategy

In live-hybrid mode:

1. fetch upstream Shopify read response
2. normalize the relevant subgraph
3. apply staged overlay
4. serialize back into GraphQL response shape

This design is preferred over blind JSON patching because it preserves domain-level consistency. However, early iterations may patch only specific product queries until the normalized serializer matures.

## Snapshot strategy

A `DraftProxy` value can be seeded from a normalized snapshot rather
than booted empty. The supported on-disk format is the JSON envelope
produced by `dump_state(...)`: `base_state` buckets, mutation log,
synthetic identity cursor, optional connection baselines (product
search, customer catalog/search). `restore_state(proxy, dump_json)`
loads it into the value before the first request.

`POST /__meta/reset` restores the startup snapshot baseline, including
captured connection cursor/pageInfo baselines — it does not wipe
snapshot mode back to an empty store.

Snapshot misses return the same kind of empty/null structure Shopify
returns when the backing store has no matching data.

## Meta API

Recommended endpoints:

- `GET /__meta`
- `POST /__meta/reset`
- `POST /__meta/commit`
- `GET /__meta/log`
- `GET /__meta/state`
- `GET /__meta/config`
- `GET /__meta/health`

Implementation notes:

- `GET /__meta/config` returns the active `port`, `shopifyAdminOrigin`,
  `readMode`, and `snapshotPath`
- `GET /__meta/state` returns cloned `base_state` / `staged_state`
  buckets for debug inspection, including runtime-only object graph
  maps such as staged orders, draft orders, and calculated orders that
  are not part of the normalized snapshot file schema
- mutation-log entries retain the original GraphQL route path as well
  as the raw request body, so `commit.run_commit_sync` can replay the
  original versioned Admin API endpoint and GraphQL request fields
  such as `operationName`

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

Parity execution is contract-gated. A captured scenario is not
executable until its parity spec declares strict JSON comparison
targets and any expected differences; captured specs must declare a
non-planned comparison mode at the schema boundary. If a capture
cannot yet run as proxy-vs-capture evidence, keep the gap in
Linear/workpad notes rather than committing a passive scenario file.

Within a declared comparison target, missing fields, extra fields,
null/empty mismatches, array shape drift, changed `userErrors`, and
selected-field changes fail by default. Declared expected differences
are also checked in the other direction: if an expected difference no
longer appears in the proxy-vs-Shopify comparison, the scenario fails
until the stale expectation is removed. Expected differences are a
last resort after the operation handler has been adjusted to compute
the right response (including, where needed, a narrow
`upstream_query.fetch_sync` call); they are not a shortcut for making
parity tests pass. Opaque Shopify connection cursors are an accepted
difference because clients cannot rely on their internal encoding.

The cassette-playback model is detailed in `docs/parity-runner.md`.

Spec / capture / registry JSON is decoded by typed Gleam decoders at
the runner boundary (`test/parity/spec.gleam`) and by Zod
schemas in the TypeScript build/recording scripts. Parity specs and captures
remain under `config/parity-specs/**` and `fixtures/conformance/**`; the
operation registry source of truth is
`src/shopify_draft_proxy/proxy/operation_registry_data.gleam`.
