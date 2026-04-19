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

Snapshot misses should return the same kind of empty/null structure Shopify returns when the backing store has no matching data.

## Meta API

Recommended endpoints:

- `POST /__meta/reset`
- `POST /__meta/commit`
- `GET /__meta/log`
- `GET /__meta/state`
- `GET /__meta/config`
- `GET /__meta/health`

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
