# shopify-draft-proxy

`shopify-draft-proxy` is a high-fidelity Shopify Admin GraphQL digital twin for
test environments.

Point an app at this proxy instead of Shopify. Supported mutations are staged in
local in-memory state, mutation payloads are synthesized with Shopify-like
shapes, and later reads behave as if the writes happened. The real store remains
unchanged during normal supported mutation handling until an explicit
`/__meta/commit`.

This is not a generic mock server. The goal is to model Shopify domain behavior
closely enough that app tests can exercise realistic write/read flows without
normal test runs mutating a dev store.

## Implementation Status

The current implementation track is the Gleam port in `gleam/`. Gleam is the
runtime direction for the package because the same domain model can compile to
JavaScript for Node/TypeScript consumers and to Erlang for Elixir/BEAM
consumers.

The legacy TypeScript/Koa runtime in `src/` remains in the repository as a
temporary compatibility baseline while the port reaches parity. It still carries
some route coverage that the Gleam implementation is actively absorbing, but it
is not the future-facing system. Once port parity is complete, the TypeScript
runtime will be deleted and the Gleam package layout will be promoted to the
repository root.

## What It Does

- Preserves Shopify-like versioned Admin GraphQL routes:
  `/admin/api/:version/graphql.json`.
- Forwards Shopify auth headers unchanged when an upstream Shopify call is
  required.
- Stages supported mutations locally and records the original raw GraphQL body
  for later commit replay.
- Overlays staged local effects onto downstream reads for ported domains.
- Uses explicit unsupported boundaries for missing domains or missing runtime
  adapters instead of pretending partial support is complete.
- Exposes meta APIs for health, configuration, staged-state inspection, reset,
  log inspection, and commit.
- Uses conformance captures and parity tests to keep behavior grounded in real
  Shopify responses.

## Install From Source

The release packaging cutover is still in progress. For repository work, install
the root toolchain and the Gleam package dependencies:

```sh
corepack pnpm install
cd gleam
gleam deps download
```

Useful root scripts:

```sh
corepack pnpm gleam:test
corepack pnpm gleam:smoke:js
corepack pnpm elixir:smoke
corepack pnpm conformance:check
corepack pnpm conformance:parity
```

The eventual package names are:

- npm: `shopify-draft-proxy`
- Hex/Gleam: `shopify_draft_proxy`

Until the packaging cutover lands, JS callers can use the checked-in Gleam JS
shim under `gleam/js/`, and Elixir callers can use the local Erlang shipment
smoke project under `gleam/elixir_smoke/`.

## Embedding From JavaScript

The JS-facing package will keep the existing `createDraftProxy(config)` shape.
The implementation behind that shape is the Gleam-emitted ESM plus a thin
TypeScript shim.

```ts
import { createDraftProxy } from 'shopify-draft-proxy';

const proxy = createDraftProxy({
  readMode: 'snapshot',
  port: 4000,
  shopifyAdminOrigin: 'https://your-store.myshopify.com',
});

const response = await proxy.processRequest({
  method: 'POST',
  path: '/admin/api/2025-01/graphql.json',
  headers: {
    'x-shopify-access-token': 'shpat_test_token',
  },
  body: {
    query: '{ shop { name } }',
  },
});

console.log(response.status, response.body);
```

Each `DraftProxy` owns its in-memory store, mutation log, snapshot baseline, and
synthetic identity registry. The JS shim presents an imperative object API, but
the Gleam core remains explicit about state internally.

## Embedding From Elixir

The same Gleam core compiles to BEAM. Before Hex publication, the local smoke
path is:

```sh
cd gleam
gleam export erlang-shipment
cd elixir_smoke
mix test
```

Once the package is published, an Elixir application will depend on the Hex
package normally:

```elixir
defp deps do
  [
    {:shopify_draft_proxy, "~> 0.1"}
  ]
end
```

Gleam modules are callable from Elixir as Erlang modules. The current direct
interop shape is intentionally low-level:

```elixir
alias :shopify_draft_proxy@proxy@draft_proxy, as: DraftProxy

proxy = DraftProxy.new()
request = {:request, "GET", "/__meta/health", %{}, ""}

{response, next_proxy} = DraftProxy.process_request(proxy, request)
{:response, status, body, _headers} = response
```

Thread `next_proxy` into the next call to preserve staged state. A friendlier
Elixir wrapper is part of the port completion work; until then, adapter code can
call the Erlang-shaped Gleam API directly.

## State Threading

Gleam returns the next proxy value from request processing:

```gleam
let #(response, next_proxy) = draft_proxy.process_request(proxy, request)
```

That is deliberate. Runtime state is owned by a `DraftProxy` value rather than
by process-wide mutable state, and callers must thread the returned proxy
forward. JS wraps this in a mutable class for compatibility with existing
TypeScript consumers; Elixir/BEAM callers should keep the returned value in
their test process or wrapper state.

## Runtime Modes

`snapshot` answers supported reads from local snapshot/staged state. Absent
data should match Shopify's null/empty behavior rather than inventing records.

`live-hybrid` sends unknown or unimplemented reads upstream and handles ported
domains locally. On JavaScript this upstream path is async, so callers must use
the async JS shim or `process_request_async` when live upstream fetches may be
needed.

The legacy TypeScript runtime names its third debugging mode `passthrough`. The
Gleam API currently exposes a `Live` variant for the same future need, but the
fully documented live-only server behavior is still part of the remaining port
work.

Supported mutations are still staged locally in every mode. `POST
/__meta/commit` is the explicit exception: it replays pending staged mutations
upstream in their original order.

## Supported Routes

The Gleam core currently routes:

- `POST /admin/api/:version/graphql.json`
- `GET /__meta/health`
- `GET /__meta/config`
- `GET /__meta/log`
- `GET /__meta/state`
- `POST /__meta/reset`
- `POST /__meta/commit`

`POST /__meta/commit` is synchronous on the Erlang target. On JavaScript,
commit replay uses `fetch`, so synchronous `process_request` returns a clear
501 message and async callers should use `process_request_async` or the JS
shim's async `processRequest`.

The legacy TypeScript/Koa server still serves the operator UI at `GET /__meta`
and the in-memory bulk operation artifact route
`GET /__bulk_operations/:id/result.jsonl`. The Gleam JS HTTP server adapter,
staged-upload byte handoff, and bulk artifact serving are tracked as remaining
cutover work.

## Current Domain Coverage

The Gleam dispatcher has partial, growing coverage for endpoint groups that have
already been implemented in the legacy runtime. Recent passes include events,
delivery settings, saved searches, webhooks, apps, functions, gift cards,
segments, metafield definitions, localization, metaobject definitions,
marketing, bulk operations, media, Admin Platform utilities, and Store
Properties shop/policy behavior.

Coverage is intentionally domain-specific. A root is not considered supported
until the local lifecycle and downstream read-after-write behavior are modeled
for that domain. Validation-only or branch-only handling is documented as a
guardrail, not full support.

## Conformance Workflow

Conformance remains the fidelity standard for the port:

1. Capture real Shopify request/response fixtures against disposable test
   shops.
2. Keep parity specs and fixture bytes stable.
3. Replay runnable scenarios against the proxy.
4. Compare strict JSON payload slices, allowing only explicit volatile paths.
5. Use failures to drive domain modeling rather than weakening scenario files.

Local checks:

```sh
corepack pnpm conformance:check
corepack pnpm conformance:parity
```

Live capture credentials are intentionally separate from normal runtime config
and are loaded through the repository conformance auth helpers, not from copied
workspace `.env` token values.

## Intentionally Unsupported Boundaries

- The legacy TypeScript runtime is still present only as a temporary parity
  baseline and compatibility bridge.
- The Gleam JS HTTP server adapter is not the primary shipped server yet.
- The friendly Elixir wrapper is not complete; direct Erlang-shaped calls are
  the current BEAM interop boundary.
- The package publishing cutover to npm and Hex is not complete.
- Some endpoint groups and Relay `node`/`nodes` resolvers remain partial until
  their generated port issues land.
- Staged-upload byte serving and bulk operation result-file serving still need
  the Gleam HTTP artifact route work.

## Important Docs

- `GLEAM_PORT_INTENT.md`: why the port exists and the non-negotiable parity bar
- `GLEAM_PORT_LOG.md`: newest-first narrative of landed Gleam port passes
- `gleam/README.md`: detailed Gleam, JS, and Elixir embedder notes
- `docs/original-intent.md`: project intent, non-goals, and fidelity standard
- `docs/architecture.md`: request flow, state model, runtime modes, and meta API
- `docs/conformance-capture.md`: indexed capture-command lookup by domain
- `docs/helpers.md`: shared helper APIs to use before adding new utilities
- `docs/hard-and-weird-notes.md`: captured Shopify quirks and fidelity traps
- `docs/endpoints/`: endpoint-specific behavior and coverage notes
