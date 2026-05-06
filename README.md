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

## What It Does

- Preserves Shopify-like versioned Admin GraphQL routes:
  `/admin/api/:version/graphql.json`.
- Forwards Shopify auth headers unchanged when an upstream Shopify call is
  required.
- Stages supported mutations locally and records the original raw GraphQL body
  for later commit replay.
- Overlays staged local effects onto downstream reads for supported domains.
- Proxies unsupported mutations to Shopify as an escape hatch by default and
  records that fact in logs/observability. Set
  `unsupportedMutationMode: 'reject'` when tests must fail closed instead of
  letting unsupported mutations reach Shopify.
- Exposes meta APIs for health, configuration, staged-state inspection, reset,
  log inspection, and commit.
- Uses conformance captures and parity tests to keep behavior grounded in real
  Shopify responses.

The runtime is implemented in Gleam and compiled to JavaScript and Erlang from
the same domain model. JavaScript and TypeScript consumers use the
`shopify-draft-proxy` npm package, and then Elixir and other BEAM consumers use the
`shopify_draft_proxy` mix package.

## Install From Source

Release packaging is still private to this repository. For repository work,
install the root toolchain and package dependencies:

```sh
corepack pnpm install
gleam deps download
```

Prerequisites:

- Node 22 or newer
- Corepack
- Erlang/OTP 28 and Gleam 1.16.0 for host Gleam test runs
- A Shopify dev store plus an app Admin API access token for live-hybrid
  runtime or live conformance work

The repository includes a `.mise.toml` that pins the Gleam host toolchain. If
you use Mise, run `mise install` from the repository root and the checked-in
`.envrc` will activate those tools automatically when `direnv` is enabled.

Useful root scripts:

```sh
corepack pnpm gleam:test
corepack pnpm gleam:smoke:js
corepack pnpm elixir:smoke
corepack pnpm conformance:check
corepack pnpm parity:run
```

The package names are:

- npm: `shopify-draft-proxy`
- Hex/Gleam: `shopify_draft_proxy`

## Embedding From JavaScript

JavaScript callers use `createDraftProxy(config)` and HTTP-shaped request
objects. The package keeps this surface stable while the implementation behind
it is the Gleam-emitted ESM plus a thin TypeScript shim.

```ts
import { createDraftProxy } from 'shopify-draft-proxy';

const proxy = createDraftProxy({
  readMode: 'snapshot',
  unsupportedMutationMode: 'passthrough',
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
the core runtime still advances state explicitly after each request.

The JavaScript package also exports `createApp(config, proxy?)`, which builds a
Node `http` adapter over a `DraftProxy` instance for route-level tests.

## Embedding From Elixir

The same Gleam core compiles to BEAM. Before Hex publication, the repository
smoke path is:

```sh
gleam export erlang-shipment
cd elixir_smoke
mix test
```

After publication, an Elixir application will depend on the Hex package
normally:

```elixir
defp deps do
  [
    {:shopify_draft_proxy, "~> 0.1"}
  ]
end
```

The checked-in Elixir wrapper keeps the Gleam proxy value opaque and returns
the next proxy state with each response:

```elixir
proxy = ShopifyDraftProxy.new()

%ShopifyDraftProxy.Response{status: 200, body: body, proxy: next_proxy} =
  ShopifyDraftProxy.graphql(proxy, "{ shop { name } }")

{:ok, decoded} = Jason.decode(body)
```

Thread `next_proxy` into the next call to preserve staged state. Adapter-level
code can also call the compiled Gleam modules directly through their Erlang
module names, but application tests should use the wrapper.

## State Threading

The core request API returns the response and the next proxy value:

```gleam
let #(response, next_proxy) = draft_proxy.process_request(proxy, request)
```

That is deliberate. Runtime state is owned by a `DraftProxy` value rather than
by process-wide mutable state, and callers must keep the returned value for the
next request. This is how staged resources, mutation logs, snapshots, and
synthetic IDs stay isolated per embedded proxy instance.

## Runtime Modes

`snapshot` answers supported reads from local snapshot and staged state. Absent
data should match Shopify's null/empty behavior rather than inventing records.

`live-hybrid` sends unknown or unimplemented reads upstream and overlays staged
local effects for supported domains. JavaScript upstream work is async, so
callers should use the async JS API when live upstream fetches or commit replay
may be needed.

`passthrough` is the live-only debugging posture exposed to JavaScript callers.
It is not support for known mutation roots; supported mutations still stage
locally, and unknown/unsupported passthrough must remain visible in
observability.

`unsupportedMutationMode` controls unsupported mutation roots in `live-hybrid`.
It defaults to `passthrough`, preserving the escape hatch that forwards the
request upstream and records a proxied log entry. Set it to `reject` to return a
400 GraphQL error envelope before any upstream call when the mutation root is
not locally supported. Supported local mutations still stage locally in either
mode.

`POST /__meta/commit` is the explicit exception to local-only supported
mutation handling: it replays pending staged mutations upstream in original
order.

## Supported Routes

The package routes:

- `POST /admin/api/:version/graphql.json`
- `GET /__meta/health`
- `GET /__meta/config`
- `GET /__meta/log`
- `GET /__meta/state`
- `POST /__meta/reset`
- `POST /__meta/commit`
- `POST` / `PUT /staged-uploads/:target/:filename`
- `GET /__meta/bulk-operations/:encoded_id/result.jsonl`

`POST /__meta/commit` replays staged mutations in log order. On JavaScript it
uses async upstream fetches; on Erlang it can run synchronously when a transport
is supplied.

The remaining intentionally unsupported HTTP boundaries are:

- `GET /__meta` operator UI
- staged-upload byte download/serving

Those routes are artifact-serving surfaces, not permission to weaken domain
fidelity for GraphQL roots.

## Current Domain Coverage

Coverage is domain-specific. A root is not considered supported until the local
lifecycle and downstream read-after-write behavior are modeled for that domain.
Validation-only or branch-only handling is documented as a guardrail, not full
support.

Current Gleam domain work covers the generated port plan across products,
customers, orders, B2B, bulk operations, webhooks, saved searches, events,
gift cards, segments, localization, metaobjects, metafields, markets, media,
discounts, apps/functions, payments, privacy, online store, store properties,
shipping/fulfillment surfaces, and Admin Platform utilities. Endpoint-specific
coverage notes live under `docs/endpoints/`.

## Conformance testing

We prove that the proxy correctly emulates Shopify by taking extensive recordings of Shopify's real behavior and then making sure that the proxy acts the same as Shopify, with the exception of doing the real-world side effects and some non-deterministic things like id and timestamp generation.

See the conformance specs at `config/parity-specs`.