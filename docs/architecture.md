# Architecture

## Overview

`shopify-draft-proxy` is a Koa-based reverse proxy for Shopify Admin GraphQL. It supports three read execution modes and two mutation execution paths.

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

## High-level request flow

```text
App -> Koa server -> operation classifier
                     ├─ Query path
                     │   ├─ live Shopify read (optional)
                     │   ├─ normalized overlay engine
                     │   └─ GraphQL response serializer
                     └─ Mutation path
                         ├─ supported? yes -> local stage + synthesized response
                         └─ supported? no  -> passthrough to Shopify
```

## Primary modules

### `src/config.ts`

- parse environment/configuration
- select runtime mode
- hold Shopify upstream URL/version settings

### `src/app.ts`

- build Koa app
- register body parser, request logging, meta routes, proxy routes

### `src/logger.ts`

- create the shared pino structured logger for runtime proxy logs
- use `pino-pretty` single-line output for local development
- provide child loggers for server and proxy modules
- keep unsupported mutation passthrough visible through structured warning logs

### `src/server.ts`

- start HTTP server

### `src/graphql/`

- parse GraphQL documents
- identify operation type and operation name
- eventually map known operations to capability records

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

### `src/shopify/`

- upstream HTTP client
- request serialization
- commit executor
- conformance helpers later

### `src/meta/`

- reset, commit, state, log endpoints

### `src/testing/`

- scenario fixtures
- recorder/replayer helpers
- parity comparators

### `scripts/conformance-scenario-registry.ts`

- discovers standard conformance scenarios from `config/parity-specs/*.json`
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

## Product-first domain model

Initial normalized entities should include at least:

- Product
- ProductVariant
- ProductOption
- Metafield
- Media (even if partial)
- Collection entities plus product-scoped membership rows (lightly modeled at first)

The architecture should be open to later domains without making products special in the core engine.

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

- `createApp()` now reads `config.snapshotPath` eagerly when it is set
- the current supported on-disk format is a normalized snapshot JSON file containing `baseState` plus optional customer catalog/search connection baselines
- normalized snapshot JSON is parsed through Zod schemas at the file boundary; the same schemas derive the runtime snapshot TypeScript types
- loading that file seeds the in-memory base state before the server handles requests
- `POST /__meta/reset` restores that startup snapshot baseline rather than wiping snapshot mode back to an empty store

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

- `GET /__meta` serves a small operator web UI backed by the existing meta API and in-memory store; it renders the current mutation log/state and exposes reset/commit controls without adding separate persistent UI state
- `GET /__meta/config` returns the active `port`, `shopifyAdminOrigin`, `readMode`, and `snapshotPath`
- mutation-log entries retain the original GraphQL route path as well as the raw document + variables, so commit replay can preserve the original versioned Admin API endpoint
- `POST /__meta/commit` replays pending `staged` / `proxied` mutations against upstream Shopify in original log order using the caller-provided `X-Shopify-Access-Token`
- commit replay persists per-entry `committed` / `failed` statuses back into the in-memory log and stops at the first upstream transport or GraphQL failure

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

Current proxy parity execution is intentionally contract-gated. A captured scenario with a proxy request is not executed until its parity spec declares strict JSON comparison targets and expected differences. Within a declared comparison target, missing fields, extra fields, null/empty mismatches, array shape drift, changed `userErrors`, and selected-field changes fail by default. Declared expected differences are also checked in the other direction: if an expected difference no longer appears in the proxy-vs-Shopify comparison, the scenario fails until the stale expectation is removed. The first promoted comparison is `product-create-live-parity`, which compares mutation `data` and immediate downstream product read `data`; Shopify cost/throttle `extensions` remain outside that first explicit contract until the proxy models cost metadata.

Conformance registry JSON, parity specs, parity request variables, and conformance fixture JSON are validated with Zod when read by the registry/parity helpers. Types for operation registry entries, parity specs, proxy request specs, and blocker details are derived from those schemas instead of maintained as separate hand-written TypeScript interfaces.
