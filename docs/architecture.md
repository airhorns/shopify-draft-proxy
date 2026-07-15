# Architecture

## Overview

`shopify-draft-proxy` is an embeddable Shopify Admin GraphQL draft proxy with a separately modeled Storefront API surface. The Rust runtime executes requests through real `async-graphql` schemas: Admin schemas come from complete captured SDL in `config/admin-graphql/<version>/schema.graphql`, while the Storefront 2026-04 schema is rendered from the complete captured introspection graph in `config/storefront-graphql/2026-04/schema.json`. `DraftProxy` in `src/proxy.rs` owns the store and runtime services; `src/admin_graphql.rs` owns the shared dynamic type-system builder and Admin schema inventory; `src/storefront_graphql.rs` owns the independent Storefront inventory; the two request bridges execute their respective schemas; `src/resolver_registry.rs` maps public roots to globally unique internal resolver names and execution capabilities; domain behavior remains under `src/proxy/`; and `src/proxy/commit.rs` owns replay.

The TypeScript package under `js/` is intentionally thin: it starts and owns a Rust HTTP runtime process, forwards public API requests to that process, and exposes the stable JavaScript surface for application tests.

The Python package under `python/` is also a thin embedding surface: it builds a PyO3 native extension that owns Rust `DraftProxy` instances in-process and calls the same Rust request/meta API used by the HTTP bridge.

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

The upstream transport boundary rejects any implemented mutation root whose
registry execution mode is `stage-locally` if a handler attempts to forward that
mutation before commit. Supported handlers may still issue query-only hydration
requests in LiveHybrid mode, but the caller's original write document is held
for local staging and explicit commit replay.

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
       ├─ Admin GraphQL route
       │    ├─ select the captured Admin schema from the versioned request path
       │    ├─ async-graphql parses, selects, coerces, validates, and executes the operation
       │    ├─ generic schema resolvers classify each Admin root through the instance resolver registry
       │    ├─ supported read -> store-backed domain resolver / overlay
       │    ├─ supported mutation -> local domain staging + replay log
       │    ├─ node(s) -> cross-domain node-loader registry, then read-through when needed
       │    └─ unsupported/unknown -> passthrough or reject according to mode
       └─ Storefront GraphQL route
            ├─ select the independent captured Storefront schema from the versioned request path
            ├─ async-graphql validates and executes against Storefront types only
            ├─ local roots -> `storefront*` internal resolver names -> Storefront domain callback
            ├─ supported read -> Storefront projection from shared store state
            ├─ snapshot-only unknown roots -> schema-shaped empty/null values with null propagation
            └─ live-hybrid unknown roots -> one unchanged Storefront passthrough request
```

The engine owns operation selection, aliases, fragments, built-in directives,
variable/default coercion, selection projection, abstract-type checks, and null
propagation. Root resolvers are request-scoped and execute serially against the
same instance-owned proxy. Each invocation receives the engine-coerced root
arguments and the original selected operation remains available to domain
handlers. A local resolver receives `RootResolverContext`, whose mode can only
be `OverlayRead` or `StageLocally`; passthrough is decided before domain code is
entered. The runtime does not synthesize and reparse an internal one-root
document. Single-root serialization exists only at the upstream transport
boundary when one root genuinely must be forwarded. A few intentionally grouped
read paths retain the complete operation to share hydration work. Multi-root
mutations containing both local and passthrough roots are rejected before
execution because splitting would change atomicity and could leak a supported
write upstream. A fully passthrough document is forwarded once, not once per
selected root.

## GraphQL schema and resolver boundaries

- Each Admin route version has its own full captured SDL. `config/admin-graphql/manifest.json` declares the executable inventory and default (`2026-07`); `AdminApiVersion` selects and lazily caches one immutable schema for `2025-01`, `2025-10`, `2026-01`, `2026-04`, or `2026-07`. Fields and input types that differ by version are therefore enforced by the requested route's actual schema.
- Storefront keeps an independent version inventory and schema cache because Admin and Storefront intentionally reuse names such as `shop` with different types and semantics. `StorefrontApiVersion` currently executes the captured 2026-04 type graph. The accepted 2025-01 route remains an explicit legacy passthrough/no-data compatibility boundary until a complete schema capture exists for that version; it never substitutes the Admin schema or the Storefront 2026-04 schema.
- The shared schema builder registers objects, interfaces, unions, enums, scalars, input objects, arguments, defaults, descriptions, and deprecations dynamically. This avoids maintaining thousands of handwritten Rust GraphQL wrapper types while still using a real GraphQL executor. Storefront's captured introspection JSON is deterministically rendered to SDL once before entering the same builder.
- Captured custom scalars have explicit codecs. URL, RFC 3339 DateTime, decimal/money, integer, JSON, and string-like scalar families are validated deliberately, and schema construction fails when a new capture introduces an unknown scalar instead of silently treating it as arbitrary JSON.
- Root fields share one generic resolver bridge. Domain code continues to model Shopify behavior and store effects directly; it does not need a second resolver-shaped copy of every function. Complex lifecycle behavior can remain in rich domain methods, while ordinary output fields are read from the returned typed JSON object by the generic schema resolver.
- Returning a JSON object is not permission to return arbitrary shape. For every selected nested field, the executable schema validates its type and the generic object resolver reports an explicit `Local resolver did not implement Type.field` execution error when the domain result omits that field. The engine then applies GraphQL null propagation.
- `ResolverRegistry` is owned by each `DraftProxy` and derives executable callbacks from implemented operation-registry entries. Admin registrations keep their public root names (`shop`, `products`); Storefront registrations receive globally unique internal names (`storefrontShop`, `storefrontProducts`). Surface-aware lookup performs that translation and also verifies the operation type and public root before returning a callback. Duplicate internal names fail registry construction, so same-named API roots cannot collide. There is no second checked-in local-routing inventory to synchronize. Every implemented capability domain has one distinct domain-owned callback, and structural tests prevent domains from collapsing back into a shared compatibility handler or crossing API surfaces.
- A domain callback is the only GraphQL-shaped entry point for that domain. It routes the root to existing store-backed lifecycle methods directly; ordinary fields do not acquire a second one-line resolver/service copy.
- Storefront's `@inContext` directive is interpreted from the original operation by the Storefront domain. Because the dynamic engine cannot register executable custom directives, only the engine-facing copy removes `@inContext` and variables used exclusively by it; all other directives and variable uses remain under normal schema validation.
- `node(id:)` and `nodes(ids:)` use one type-to-loader inventory in `src/node_resolver_inventory.rs`; each entry carries its executable loader rather than a second loader enum/switch. Loaders return explicit `Found`, `KnownMissing`, `NeedsHydration`, or `UnsupportedType` states. Live-hybrid sends one upstream request for a mixed cold batch, merges staged values and tombstones over the response before caching it, and preserves input ordering/null placeholders. Snapshot mode never hydrates upstream.

`DraftProxy` is instance-owned state, not a singleton. A proxy owns its normalized `Store`, mutation log, registry, synthetic ID counters, injectable runtime clock, and injectable upstream/commit transports. Runtime base/staged resource data belongs under the Store rather than as loose `DraftProxy` fields. Do not introduce global mutable proxy state.

## Primary Rust modules

### `src/proxy.rs`

- owns `DraftProxy`, `Config`, `ReadMode`, the normalized Store, synthetic identity allocation, registry metadata, runtime clock, and injectable transports
- declares the runtime's domain submodules while keeping proxy state instance-owned instead of global

### `src/proxy/core.rs`, `src/proxy/routing.rs`, `src/proxy/graphql_runtime.rs`, `src/proxy/storefront_graphql_runtime.rs`, `src/proxy/graphql_error_compat.rs`

- expose `process_request(...)` as the central route boundary
- implement meta routes: health, config, log, state, reset, dump, and restore
- keep Shopify-like Admin GraphQL route classification and request-body parsing separate from domain handlers
- execute Admin and Storefront requests through independent route-versioned `async-graphql` schemas
- isolate Shopify-specific parse/validation/coercion envelope translation in `graphql_error_compat.rs`; domain resolvers do not own top-level GraphQL error formatting
- provide request-scoped Admin and Storefront root executors that serialize access to the instance-owned proxy, reuse grouped reads where necessary, and invoke domain-owned callbacks with one local-only context
- preserve original multi-root mutation documents in the replay log while preventing mixed local/passthrough writes
- wrap upstream transports with the stage-locally mutation guard while leaving query hydration and commit replay on their explicit transport paths
- preserve `with_clock(...)`, `with_upstream_transport(...)`, and `with_commit_transport(...)` test seams so behavior stays deterministic

### `src/admin_graphql.rs`

- owns the shared captured-schema builder and parses each Admin SDL into a complete dynamic `async-graphql` schema
- caches one executable schema per supported Admin API version
- registers an explicit codec for every captured custom scalar and fails schema construction for unknown scalar names
- exposes schema-registry metadata used by compatibility error formatting and bulk-query planning, replacing the former partial mutation/output schema models
- provides generic root and nested object resolvers, including explicit missing-field errors and abstract-type resolution from `__typename` or unambiguous schema metadata

### `src/storefront_graphql.rs`, `src/proxy/storefront_graphql_runtime.rs`, `src/proxy/storefront.rs`

- render the captured Storefront introspection graph into the shared dynamic schema builder and cache it independently from Admin
- select the executable Storefront schema by route version without falling back to a different version or API surface
- bridge engine root invocations to surface-qualified Storefront registrations, or forward one unchanged request when the complete operation is passthrough-only
- preserve the original operation for Storefront context interpretation while validating an engine-only copy without `@inContext`
- own Storefront hydration, context-keyed state, local projections, and schema-enforced snapshot no-data/null behavior

### `src/resolver_registry.rs`, `src/node_resolver_inventory.rs`, `src/proxy/node_registry.rs`

- derive instance-owned root capabilities from the operation registry instead of maintaining a parallel root-handler inventory
- attach executable root callbacks and node-loader function pointers directly to those inventories, avoiding name-to-enum-to-match indirection
- make locally resolvable `Node` implementors and their loader behavior auditable through one exported inventory
- keep generic `node` / `nodes` execution, mixed local/upstream merging, observation, and type loaders together in `node_registry.rs`
- route lookups to domain-owned store readers, including Shopify's exceptional `Market/Region/...` identity shape and one-batch live-hybrid hydration

### `src/proxy/commit.rs`

- owns `POST /__meta/commit` replay behavior
- replays staged mutations in original log order using each entry's preserved raw GraphQL body and path
- forwards the commit request's auth headers through the commit transport
- maps synthetic Shopify GIDs from successful upstream responses to authoritative GIDs before replaying later bodies
- stops on the first transport or GraphQL error, records the stopped index, and updates staged log statuses to committed/failed while leaving later staged entries untouched

### `src/proxy/*.rs` domain modules

- group supported runtime behavior by commerce area, including products/saved searches, localization/markets/catalogs, marketing/webhooks/inventory, online store, metaobjects, metafields, orders/payments/fulfillment, discounts/gift cards, B2B/customers, and admin/shipping/app helpers
- own each area's root resolver beside those domain methods; `graphql_runtime.rs` contains no compatibility-domain switch
- keep local staging, overlay reads, selected-field projection, alias-safe response keys, and live-hybrid passthrough/reject behavior near the domain logic that owns it
- use shared `Store` effective-get/list/count helpers for migrated product and saved-search read-after-write behavior, with base state, staged state, order arrays, and tombstones dumped/restored consistently
- use the shared staged-connection query helpers for staged resource lists that need Shopify-like search filtering, sort-key mapping, `reverse`, cursor windows, and filtered counts; resource modules supply predicate and sort adapters while `connection.rs` owns the order of operations
- share proxy-internal helpers only within `crate::proxy`; public package surface still flows through `DraftProxy`

### `src/graphql.rs`

- retains a compatibility document view for domain handlers and the isolated Shopify error adapter; it is no longer the GraphQL executor
- extracts operation type, operation name/path, source locations, and top-level root fields without routing by alias or raw query text
- preserves raw root-field argument sources separately from resolved values so validators can distinguish omitted arguments, literal nulls, bound variables, and unbound variables
- resolves root-field arguments from literals, enums, lists, input objects, and variables for existing callers that need the compatibility view
- extracts selection sets and nested selection paths while preserving response aliases, evaluating standard `@skip` / `@include` directives, and expanding supported inline/named fragments

### `src/operation_registry.rs`

- typed registry of operation capability metadata
- classifies roots by domain and execution kind
- the `implemented` flag marks roots the proxy handles locally (instead of 501-ing). Canonical implemented registry entries are the local-routing inventory. It is a "we answer this locally" fact, not a fidelity claim
- capability routing resolves a non-passthrough capability only when the root field matches an implemented registry entry's canonical name; anything else falls through to passthrough
- keeps passthrough/unknown roots explicit so non-implemented metadata does not imply runtime support
- exposes one registry source used by `ResolverRegistry`, runtime gates, and tests so executable handlers and exported metadata stay auditable together

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

## Python package surface

### `python/`

- builds a maturin/PyO3 native extension from `python/Cargo.toml`
- depends on the main Rust library crate and stores a separate Rust `DraftProxy` inside each Python `DraftProxy` object
- exposes `process_request`, `process_graphql_request`, `get_config`, `get_log`, `get_state`, `dump_state`, `restore_state`, and `reset`
- uses the Rust `POST /__meta/dump` and `POST /__meta/restore` route behavior for serialization so Python dump/restore stays aligned with the canonical state schema

The Python package is not a second proxy implementation and does not spawn the Rust HTTP server. It embeds the same Rust runtime in-process for Python tests.

## Conformance and parity tooling

- Protected parity evidence lives under:
  - `config/parity-specs/**`
  - `config/parity-requests/**`
  - `fixtures/conformance/**`
- Those paths must be registered in the conformance capture index when they drift from `origin/main`.
- `scripts/check-protected-evidence-invariants.ts` compares protected evidence against `origin/main` and rejects unregistered changes.
- `scripts/conformance-capture-index.ts`, `scripts/conformance-check.ts`, and `scripts/conformance-status-report.ts` maintain capture metadata and status reporting.
- `src/operation_registry.rs` is the executable source of truth for operation metadata. TypeScript tooling loads the same metadata through the Rust `operation-registry-json` exporter instead of maintaining a second checked-in JSON registry.
- Complete version-scoped Admin GraphQL SDL lives at `config/admin-graphql/<api-version>/schema.graphql`, with the shared inventory/default in `config/admin-graphql/manifest.json`. `scripts/capture-admin-graphql-schema.mts` records and normalizes SDL from Shopify introspection, and both runtime execution and schema-aware tooling consume those sources.
- Parity requests use the API version declared by their live capture (or its fixture path) unless a target explicitly overrides `apiVersion`; an unsupported declared/path version is an error rather than a silent schema substitution. Captures with no version evidence use the manifest default (`2026-07`).
- Full parity can emit a machine-readable result document. Main-branch CI publishes it as a baseline; pull requests reject new failing specs, new failing targets inside known-red specs, and missing baseline scenarios while still reporting known failures and fixes.

## State model

The runtime should use normalized state rather than raw GraphQL blobs.

`DraftProxy` owns a typed Rust `Store` for runtime resource state. Products and saved searches use normalized records with shared effective-read helpers, while other staged domain data also lives under `Store::staged` so reset, dump/restore plumbing, and future normalization work have one ownership boundary. Order and gift-card LiveHybrid hydration also stores known base records and related baseline/configuration data in `BaseState` so supported local mutations can overlay real upstream reads without runtime Shopify writes.

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
- `POST /api/:version/graphql.json` for accepted Storefront API versions, including passthrough and explicitly supported local Storefront reads
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

Keep Shopify-like versioned Admin and Storefront API paths even when tests use
local/snapshot mode. Admin and Storefront supported-version policies are split
so one surface can move without implying support for the other. Storefront
GraphQL traffic stays on the Storefront route, uses supported local Storefront
read projections where modeled, forwards unsupported or cold live-hybrid reads
through the upstream transport with Storefront headers preserved, and does not
enter Admin local dispatch or staged commit replay. In snapshot mode unsupported
Storefront query roots return schema-shaped no-data responses, while Storefront
mutations reject explicitly until a local Storefront lifecycle model exists.

## Development rules

- Route GraphQL behavior by actual root fields, not operation names.
- Never compute a response by sniffing the GraphQL document name (`query.contains("ScenarioName")`, `is_*_document`, `*_fixture_data`) and returning a hardcoded/`include_str!` payload. Runtime handlers must derive responses from the store model; canned scenario-keyed replies are cheating and must not be reintroduced.
- Preserve aliases in response keys for every root that can be selected with an alias.
- Keep unsupported passthrough explicit in logs and docs.
- Marking a root `implemented` only states that the proxy answers it locally; do not call an operation **supported** until its local lifecycle and downstream read-after-write effects are modeled from the store (tracked by runtime tests and conformance coverage).
- Prefer conformance fixtures over guessed Shopify semantics.
- Add tests before behavior changes and run the full verification loop before pushing.
