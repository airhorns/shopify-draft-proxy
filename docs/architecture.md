# Architecture

## Overview

`shopify-draft-proxy` is an embeddable Shopify Admin GraphQL draft proxy. The runtime is Rust, centered on `DraftProxy` in `src/proxy.rs` plus domain-specific modules under `src/proxy/`, commit replay in `src/proxy/commit.rs`, GraphQL parsing helpers in `src/graphql.rs`, operation metadata in `src/operation_registry.rs`, reusable upstream transport in `src/upstream.rs`, and the launchable HTTP bridge in `src/bin/shopify-draft-proxy-server.rs`.

The TypeScript package under `js/` is intentionally thin: it starts and owns a Rust HTTP runtime process, forwards public API requests to that process, and exposes the stable JavaScript surface for application tests.

## Read execution modes

1. **live-hybrid**
   - unknown or passthrough reads are sent upstream to Shopify
   - supported local domains can overlay staged local effects
   - request path, headers, and body are preserved for upstream transport
2. **snapshot**
   - reads are resolved from local snapshot/base state plus staged state
   - absent data should behave like Shopify behaves when no matching backend data exists
3. **passthrough**
   - reads are forwarded directly with no local overlay
   - useful as a debugging baseline

## Mutation execution paths

1. **supported mutation**
   - do not send to Shopify immediately
   - interpret the mutation into a domain command
   - apply the command to local staged state
   - synthesize a Shopify-like response
   - append a replay-ready entry to the mutation log
2. **unsupported mutation**
   - proxy through to Shopify unchanged when `unsupportedMutationMode` is `passthrough`
   - reject with a 400 GraphQL error envelope before upstream transport when `unsupportedMutationMode` is `reject`
   - remain visible in logs/observability when proxied

`POST /__meta/commit` is the explicit write-through boundary. It replays pending staged mutations upstream in original log order using the original raw GraphQL input and the commit request's auth headers. The response keeps the compatibility summary fields (`ok`, `committed`, and `failed`) and also includes per-attempt replay details plus `stopIndex` when replay stops on a transport or GraphQL error.

## High-level request flow

```text
App/test harness
  ├─ JavaScript createDraftProxy shim (optional)
  │    └─ spawned Rust HTTP server
  └─ Rust DraftProxy instance
       ├─ HTTP/meta route classifier
       │    ├─ health/config/log/state/reset/dump/restore
       │    └─ commit replay
       └─ Admin GraphQL route
            ├─ parse document/root fields/arguments/selections
            ├─ classify root through operation registry
            ├─ supported read -> local state + overlay serializer
            ├─ supported mutation -> local stage + synthesized payload + log
            └─ unsupported/unknown -> passthrough or reject according to mode
```

`DraftProxy` is instance-owned state, not a singleton. A proxy owns its normalized `Store`, mutation log, registry, synthetic ID counters, and injectable upstream/commit transports. Runtime base/staged resource data belongs under the Store rather than as loose `DraftProxy` fields. Do not introduce global mutable proxy state.

## Primary Rust modules

### `src/proxy.rs`

- owns `DraftProxy`, `Config`, `ReadMode`, the normalized Store, synthetic identity allocation, registry metadata, and injectable transports
- declares the runtime's domain submodules while keeping proxy state instance-owned instead of global

### `src/proxy/core.rs`, `src/proxy/routing.rs`, `src/proxy/dispatch.rs`

- expose `process_request(...)` as the central route boundary
- implement meta routes: health, config, log, state, reset, dump, and restore
- keep Shopify-like Admin GraphQL route classification and request-body parsing separate from domain handlers
- preserve `with_upstream_transport(...)` and `with_commit_transport(...)` test seams so behavior stays deterministic

### `src/proxy/commit.rs`

- owns `POST /__meta/commit` replay behavior
- replays staged mutations in original log order using each entry's preserved raw GraphQL body and path
- forwards the commit request's auth headers through the commit transport
- maps synthetic Shopify GIDs from successful upstream responses to authoritative GIDs before replaying later bodies
- stops on the first transport or GraphQL error, records the stopped index, and updates staged log statuses to committed/failed while leaving later staged entries untouched

### `src/proxy/*.rs` domain modules

- group supported runtime behavior by commerce area, including products/saved searches, localization/markets/catalogs, marketing/webhooks/inventory, online store, metaobjects, metafields, orders/payments, discounts/gift cards, B2B/customers, and admin/shipping/app helpers
- keep local staging, overlay reads, selected-field projection, alias-safe response keys, and live-hybrid passthrough/reject behavior near the domain logic that owns it
- use shared `Store` effective-get/list/count helpers for migrated product and saved-search read-after-write behavior, with base state, staged state, order arrays, and tombstones dumped/restored consistently
- share proxy-internal helpers only within `crate::proxy`; public package surface still flows through `DraftProxy`

### `src/graphql.rs`

- parses GraphQL documents with `graphql-parser`
- extracts operation type, operation name/path, source locations, and top-level root fields without routing by alias or raw query text
- preserves raw root-field argument sources separately from resolved values so validators can distinguish omitted arguments, literal nulls, bound variables, and unbound variables
- resolves root-field arguments from literals, enums, lists, input objects, and variables for existing callers that need the compatibility view
- extracts selection sets and nested selection paths while preserving response aliases and expanding supported inline/named fragments

### `src/operation_registry.rs`

- typed registry of operation capability metadata
- classifies implemented roots by domain and execution kind
- keeps unimplemented/unknown roots explicit so metadata alone does not imply runtime support

### `src/upstream.rs`

- owns the reusable HTTPS-capable upstream Admin transport used by the Rust HTTP bridge
- builds preserved-method, preserved-path, preserved-body requests for live-hybrid passthrough, local-domain upstream reads, and commit replay
- forwards Shopify Admin auth headers unchanged while dropping hop-by-hop and computed transport headers such as `host`, `content-length`, and `connection`
- returns proxy `Response` values so `DraftProxy` can keep its injectable upstream and commit transport seams for deterministic tests

### `src/bin/shopify-draft-proxy-server.rs`

- thin Rust HTTP server used by `pnpm dev`, `pnpm start`, and the TypeScript public API shim
- reads environment configuration such as `PORT`, `READ_MODE`, `UNSUPPORTED_MUTATION_MODE`, `SHOPIFY_ADMIN_ORIGIN`, and snapshot/bulk-file settings
- adapts inbound HTTP requests into `DraftProxy::process_request(...)`
- handles adapter-only surfaces such as staged uploads and bulk-operation artifact serving
- installs the real reusable upstream client for live-hybrid passthrough and commit replay

## TypeScript package surface

### `js/src/runtime.ts`

- implements `createDraftProxy(config)` by spawning the Rust server on an isolated port
- exposes `processRequest`, `getLog`, `getState`, `dumpState`, `restoreState`, and `dispose`
- owns child-process cleanup so tests do not leak Rust server processes

### `js/src/index.ts`, `js/src/types.ts`

- expose the public TypeScript API and schema names for the Rust-backed runtime shim

The TypeScript package is not a second proxy implementation. New runtime behavior belongs in Rust, with TypeScript only adapting public package ergonomics or test harnesses.

## Conformance and parity tooling

- Protected parity evidence lives under:
  - `config/parity-specs/**`
  - `config/parity-requests/**`
  - `fixtures/conformance/**`
- Those paths must be registered in the conformance capture index when they drift from `origin/main`.
- `scripts/check-protected-evidence-invariants.ts` compares protected evidence against `origin/main` and rejects unregistered changes.
- `scripts/conformance-capture-index.ts`, `scripts/conformance-check.ts`, and `scripts/conformance-status-report.ts` maintain capture metadata and status reporting.
- `config/operation-registry.json` is the TypeScript tooling snapshot of operation metadata. The executable Rust registry remains in `src/operation_registry.rs`.

## State model

The runtime should use normalized state rather than raw GraphQL blobs.

`DraftProxy` owns a typed Rust `Store` for runtime resource state. Products and saved searches use normalized records with shared effective-read helpers, while other staged domain data also lives under `Store::staged` so reset, dump/restore plumbing, and future normalization work have one ownership boundary.

The normalized product and saved-search portions currently include:

- `BaseState` for snapshot, fixture, or restored upstream state
- `StagedState` for local inserts and updates
- ordered ID arrays for deterministic effective lists and dump/restore round trips
- tombstone sets for staged deletes

Core state categories:

- base state learned from snapshots, fixtures, or upstream reads
- staged Store state for local inserts/updates/deletes and other local domain effects not yet committed
- ordered mutation log entries containing original request path, raw query, variables, capability metadata, resource IDs, and status
- synthetic identity counters scoped to a `DraftProxy` instance

Effective reads merge base state and staged state through shared Store helpers, respecting staged deletes and Shopify-like null/empty behavior. Commit drains staged log entries only after successful upstream replay.

## Public route contract

The Rust HTTP bridge serves:

- `POST /admin/api/:version/graphql.json`
- `GET /__meta/health`
- `GET /__meta/config`
- `GET /__meta/log`
- `GET /__meta/state`
- `POST /__meta/reset`
- `POST /__meta/dump`
- `POST /__meta/restore`
- `POST /__meta/commit`
- `POST` / `PUT /staged-uploads/:target/:filename`
- `GET /__meta/bulk-operations/:encoded_id/result.jsonl`

Keep Shopify-like versioned Admin API paths even when tests use local/snapshot mode.

## Development rules

- Route GraphQL behavior by actual root fields, not operation names.
- Preserve aliases in response keys for every root that can be selected with an alias.
- Keep unsupported passthrough explicit in logs and docs.
- Do not register an operation as implemented until its local lifecycle and downstream read-after-write effects are modeled.
- Prefer conformance fixtures over guessed Shopify semantics.
- Add tests before behavior changes and run the full Rust-port verification loop before pushing.
