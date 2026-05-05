# shopify_draft_proxy

`shopify_draft_proxy` is a Shopify Admin GraphQL digital twin / draft proxy
implemented in Gleam. It compiles to JavaScript and Erlang so Node,
TypeScript, Elixir, and Erlang test suites can embed the same domain model.

The proxy stages supported Shopify Admin GraphQL mutations in isolated
in-memory state, preserves the original raw mutations for commit replay, and
answers downstream reads as if Shopify had materialized the staged writes. It is
not a generic GraphQL mock server.

## Package API

The core entry point is:

```gleam
import shopify_draft_proxy/proxy/draft_proxy
```

Core types:

- `Request(method, path, headers, body)` accepts an HTTP-shaped request.
  `headers` is a `Dict(String, String)` and `body` is the raw request body
  string.
- `Response(status, body, headers)` returns an HTTP-shaped response. `body` is a
  `gleam/json` tree.
- `Config(read_mode, port, shopify_admin_origin, snapshot_path)` is the
  sanitized runtime configuration surfaced through `GET /__meta/config`.
- `ReadMode` is `Snapshot`, `LiveHybrid`, or `Live`; the JavaScript shim maps
  the live-only debugging posture to the public string value `passthrough`.
- `DraftProxy` is the instance-owned runtime state value.

Core functions:

- `new() -> DraftProxy`
- `default_config() -> Config`
- `with_config(Config) -> DraftProxy`
- `with_registry(DraftProxy, List(RegistryEntry)) -> DraftProxy`
- `with_default_registry(DraftProxy) -> DraftProxy`
- `with_upstream_transport(DraftProxy, Transport) -> DraftProxy`
- `registry_entry_has_local_dispatch(RegistryEntry) -> Bool`
- `process_request(DraftProxy, Request) -> #(Response, DraftProxy)`
- `process_request_async(DraftProxy, Request) -> Promise(#(Response, DraftProxy))`
  on JavaScript
- `process_graphql_request(DraftProxy, String, GraphQLRequestOptions) -> #(Response, DraftProxy)`
- `get_config_snapshot(DraftProxy) -> Json`
- `get_log_snapshot(DraftProxy) -> Json`
- `get_state_snapshot(DraftProxy) -> Json`
- `reset(DraftProxy) -> DraftProxy`
- `dump_state(DraftProxy, created_at: String) -> Json`
- `dump_state_now(DraftProxy) -> Json`
- `restore_state(DraftProxy, dump_json: String) -> Result(DraftProxy, StateDumpError)`
- `commit(DraftProxy, headers)` on each target, synchronous on Erlang and
  promise-returning on JavaScript

Use `process_request_async` on JavaScript when `POST /__meta/commit` or
live-hybrid upstream passthrough may need to await `fetch`.

## Installation

For repository development:

```sh
corepack pnpm install
gleam deps download
```

For Gleam projects before publication, use this package directory as a path
dependency from a sibling checkout or worktree. After publication:

```toml
# gleam.toml
[dependencies]
shopify_draft_proxy = ">= 0.1.0 and < 1.0.0"
```

For Node and TypeScript consumers, the npm package name remains
`shopify-draft-proxy`. The published package will bundle the Gleam-emitted ESM
and TypeScript declarations behind the stable JavaScript shim.

For Elixir consumers before Hex publication, build an Erlang shipment locally:

```sh
gleam export erlang-shipment
cd elixir_smoke
mix test
```

After Hex publication:

```elixir
defp deps do
  [
    {:shopify_draft_proxy, "~> 0.1"}
  ]
end
```

## Supported Routes

The runtime recognizes:

| Route                                                  | Status                                                                                                                                         |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST /admin/api/:version/graphql.json`                | Routes query and mutation roots for supported domains.                                                                                         |
| `GET /__meta/health`                                   | Returns liveness JSON.                                                                                                                         |
| `GET /__meta/config`                                   | Returns sanitized runtime config.                                                                                                              |
| `GET /__meta/log`                                      | Returns staged/proxied/committed mutation-log entries in replay order.                                                                         |
| `GET /__meta/state`                                    | Returns the current base/staged state buckets.                                                                                                 |
| `POST /__meta/reset`                                   | Clears staged state, logs, and synthetic identity counters.                                                                                    |
| `POST /__meta/commit`                                  | Replays staged mutations upstream in log order. Erlang can run this synchronously; JavaScript callers use `process_request_async` or `commit`. |
| `POST` / `PUT /staged-uploads/:target/:filename`       | Stores staged upload bodies in the instance-owned proxy value for local bulk mutation imports.                                                 |
| `GET /__meta/bulk-operations/:encoded_id/result.jsonl` | Serves generated local bulk operation result JSONL from instance-owned state.                                                                  |

The remaining unsupported HTTP artifact surfaces are:

- `GET /__meta` operator UI
- staged-upload byte download/serving

## Runtime Modes

`Snapshot` answers locally from snapshot and staged state for supported domains
and returns Shopify-like empty/null structures when the local store lacks data.

`LiveHybrid` answers supported domains locally and forwards unknown or
unimplemented reads upstream when the registry and runtime mode allow it. On
JavaScript, forwarding is async and requires `process_request_async` or the JS
shim's async `processRequest`.

`Live` is reserved for live-only debugging. It must not be used as a way to
mark known mutation roots as supported; supported mutations stage locally in
every mode.

Commit replay is the only normal path that intentionally sends staged raw
mutation bodies upstream.

## State Threading

`process_request` returns the response and the next proxy state:

```gleam
let #(response, next_proxy) = draft_proxy.process_request(proxy, request)
```

Callers must keep `next_proxy` and pass it to subsequent requests. This is how
the mutation log, staged state, snapshot baseline, and synthetic ID/timestamp
registry advance without module-level mutable runtime state.

The JavaScript shim wraps this in a mutable class to preserve
`createDraftProxy(...).processRequest(...)` ergonomics. Elixir and direct Gleam
callers should thread the returned proxy value explicitly.

## Using From Gleam

```gleam
import gleam/dict
import gleam/io
import gleam/json
import shopify_draft_proxy/proxy/draft_proxy

pub fn main() {
  let proxy = draft_proxy.new()

  let request =
    draft_proxy.Request(
      method: "GET",
      path: "/__meta/health",
      headers: dict.new(),
      body: "",
    )

  let #(response, _next_proxy) = draft_proxy.process_request(proxy, request)
  io.println(json.to_string(response.body))
}
```

GraphQL requests use the versioned Shopify Admin path and a JSON body string:

```gleam
let request =
  draft_proxy.Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: dict.new(),
    body: "{\"query\":\"{ shop { name } }\"}",
  )
```

Custom configuration:

```gleam
import gleam/option

let proxy =
  draft_proxy.with_config(draft_proxy.Config(
    read_mode: draft_proxy.LiveHybrid,
    port: 4000,
    shopify_admin_origin: "https://my-shop.myshopify.com",
    snapshot_path: option.None,
  ))
```

## Using From JavaScript / TypeScript

The JavaScript package exports the stable `createDraftProxy(config)` API. The
shim translates JS-shaped config and requests into Gleam records and unwraps
Gleam responses back to JS objects.

```ts
import { createDraftProxy } from 'shopify-draft-proxy';

const proxy = createDraftProxy({
  readMode: 'snapshot',
  port: 4000,
  shopifyAdminOrigin: 'https://my-shop.myshopify.com',
});

const response = await proxy.processRequest({
  method: 'POST',
  path: '/admin/api/2025-01/graphql.json',
  headers: { 'x-shopify-access-token': 'shpat_test_token' },
  body: { query: '{ shop { name } }' },
});

console.log(response.status, response.body);
```

The JS shim exports `createDraftProxy`, `DraftProxy`, `createApp`,
`DraftProxyHttpApp`, `loadConfig`, `DRAFT_PROXY_STATE_DUMP_SCHEMA`,
`DraftProxyCommitError`, and the public config, request, response, state, log,
and commit result types.

`createApp(config, proxy?)` constructs a Node `http` adapter over a
Gleam-backed `DraftProxy` instance. The adapter exposes `callback()` and
`listen(...)`; `listen(...)` returns the underlying Node `Server`.

`loadConfig(env?)` reads the runtime environment:

- `SHOPIFY_ADMIN_ORIGIN` is required
- `PORT` defaults to `3000`
- `SHOPIFY_DRAFT_PROXY_READ_MODE` defaults to `live-hybrid`
- `SHOPIFY_DRAFT_PROXY_SNAPSHOT_PATH` enables snapshot loading

Interop notes:

- Gleam records become JS objects/classes in emitted ESM; the shim hides those
  details from public callers.
- `Dict` values are converted from and to ordinary JS objects at the boundary.
- `Option` values become optional or nullable JS values.
- `processRequest` is async so it can cover JS `fetch` for commit replay and
  live-hybrid passthrough.

To launch the JavaScript-target HTTP adapter during package work:

```sh
SHOPIFY_ADMIN_ORIGIN=https://your-store.myshopify.com corepack pnpm dev

corepack pnpm build
SHOPIFY_ADMIN_ORIGIN=https://your-store.myshopify.com corepack pnpm start
```

## Using From Elixir

Elixir application code should use the checked-in `ShopifyDraftProxy` wrapper
when consuming the Erlang shipment.

```elixir
proxy = ShopifyDraftProxy.new()

%ShopifyDraftProxy.Response{status: 200, body: body, proxy: next_proxy} =
  ShopifyDraftProxy.graphql(proxy, "{ shop { name } }")

{:ok, decoded} = Jason.decode(body)
```

Custom config:

```elixir
proxy =
  ShopifyDraftProxy.with_config(
    read_mode: :live_hybrid,
    port: 4000,
    shopify_admin_origin: "https://my-shop.myshopify.com"
  )
```

Wrapper conventions:

- `ShopifyDraftProxy.new/0` returns an opaque `%ShopifyDraftProxy{}` value.
- `ShopifyDraftProxy.graphql/3` and meta helpers return
  `%ShopifyDraftProxy.Response{status:, body:, headers:, proxy:}`.
- `body` is a JSON string converted from the Gleam JSON tree.
- `ShopifyDraftProxy.dump_state/2` returns the state-dump JSON string.
- `ShopifyDraftProxy.restore_state/2` returns `{:ok, proxy}` or
  `{:error, reason}`.
- `ShopifyDraftProxy.commit_with/4` is the BEAM embedder seam for deterministic
  commit reports without real Shopify HTTP.

The raw Gleam module remains callable as
`:shopify_draft_proxy@proxy@draft_proxy` for adapter-level code that needs the
compiled tuple ABI.

## Conformance

The Gleam runtime consumes the repository's parity specs and recorded Shopify
fixtures without rewriting their evidence. The parity runner executes
cassette-backed LiveHybrid scenarios through an injected upstream transport so
supported mutations still stage locally while reads can use recorded Shopify
context.

Useful checks from the repository root:

```sh
corepack pnpm conformance:check
corepack pnpm parity:run
```

Useful checks from this package directory:

```sh
gleam test --target erlang
gleam test --target javascript
gleam format --check
```

Live conformance capture remains a repository-level workflow driven by the
TypeScript capture scripts. Those scripts intentionally produce shared fixture
formats that validate the Gleam runtime as coverage reaches parity.

## Development

```sh
# from the repository root/
gleam deps download
gleam test --target erlang
gleam test --target javascript
gleam format --check
```

Release-shipment smoke:

```sh
gleam export erlang-shipment
cd elixir_smoke
mix test
```

From the repository root, `corepack pnpm elixir:smoke` runs the same flow. On
hosts without native `escript` or `mix`, the script falls back to the
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container and installs Elixir
inside the disposable container before running the smoke project.

### Live end-to-end smokes

In addition to the offline interop smoke above, `elixir_smoke/` ships a
`:live`-tagged test that exercises the full live-hybrid mutation +
commit cycle against a real Shopify test store. It is the canonical
proof that the Elixir wrapper attaches the operation registry, that
`productCreate` stages locally with a synthetic GID instead of leaking
upstream, and that `/__meta/commit` replays through `:httpc` and gets
real GIDs back.

```sh
# Elixir/Erlang target:
corepack pnpm e2e:elixir-product-create-commit-smoke
# → runs scripts/elixir-e2e-product-create-commit-smoke.ts which
#   rebuilds the Erlang shipment, refreshes the conformance token, and
#   invokes `mix test --only live test/live_hybrid_e2e_test.exs`.

# JS / Node target (same scenario, in-process Node HTTP app):
corepack pnpm e2e:product-create-commit-smoke
# → runs scripts/e2e-product-create-commit-smoke.mts.
```

Both scripts pull credentials through `scripts/shopify-conformance-auth.mts`
and clean up any products they create on success **and** failure (cleanup
runs in `on_exit` / `finally`). Running both before merging committable-
mutation changes is the cross-target sanity check; running just the
Elixir one verifies the BEAM-side wrapper. The `:live` tag in
`elixir_smoke/test_helper.exs` keeps these out of plain `mix test` so
the offline smoke stays hermetic.

## Remaining Unsupported Boundaries

- The package is not yet published to npm or Hex.
- `GET /__meta` operator UI is not served by the Gleam HTTP adapter.
- Staged-upload byte download/serving is not yet served by the Gleam HTTP
  adapter.
- Direct Elixir calls below the wrapper use Erlang-shaped tuples.
- Some endpoint domains and Relay `node`/`nodes` serializers remain partial,
  as tracked by the generated port issues.
- `Live` read mode is reserved and should not be treated as complete
  passthrough behavior.

## Layout

- `src/` - Gleam source.
- `src/shopify_draft_proxy/proxy/draft_proxy.gleam` - public entry point and
  dispatcher.
- `src/shopify_draft_proxy/proxy/*.gleam` - per-domain dispatchers.
- `src/shopify_draft_proxy/graphql/*.gleam` - GraphQL lexer/parser/root-field
  helpers.
- `src/shopify_draft_proxy/state/*.gleam` - store, records, synthetic identity,
  and state dump helpers.
- `test/` - gleeunit tests.
- `test/parity/` - Gleam parity runner and comparison helpers.
- `js/` - TypeScript compatibility shim over the Gleam-emitted JavaScript.
- `elixir_smoke/` - Mix project that loads the Erlang shipment and verifies
  BEAM interop.
- `gleam.toml` - package manifest. JavaScript is the default target; Erlang is
  tested alongside it.

The legacy TypeScript runtime that previously lived under root `src` has been
deleted instead of maintained beside this implementation. TypeScript repository
tooling for capture, registry generation, JavaScript interop, and packaging
remains where it is still useful.
