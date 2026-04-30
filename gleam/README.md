# shopify_draft_proxy

`shopify_draft_proxy` is the Gleam implementation of the
`shopify-draft-proxy` Shopify Admin GraphQL digital twin. It compiles to both
JavaScript and Erlang so the same domain model can be embedded from
Node/TypeScript and from Elixir/BEAM services.

This directory is the active port from the temporary legacy TypeScript runtime
in `../src`. Parity specs in `../config/parity-specs` and conformance fixtures
in `../fixtures/conformance` remain shared evidence. See
`../GLEAM_PORT_INTENT.md` for the port contract and `../GLEAM_PORT_LOG.md` for
the newest landed passes.

## Status

The Gleam core routes HTTP-shaped requests, owns instance state explicitly, and
has partial but growing domain coverage. It is the implementation direction for
the repository. The legacy TypeScript runtime remains only until the Gleam port
reaches parity and is promoted to the repository root.

The package is not published to npm or Hex yet. Local source and smoke-test
usage are documented below so JS and Elixir consumers can validate the current
embedder surfaces before release packaging is cut over.

## Public API

The main entry point is:

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
- `ReadMode` is `Snapshot`, `LiveHybrid`, or `Live`.
- `DraftProxy` is the instance-owned runtime state value.

Core functions:

- `new() -> DraftProxy`
- `default_config() -> Config`
- `with_config(Config) -> DraftProxy`
- `with_registry(DraftProxy, List(RegistryEntry)) -> DraftProxy`
- `with_default_registry(DraftProxy) -> DraftProxy`
- `process_request(DraftProxy, Request) -> #(Response, DraftProxy)`
- `process_graphql_request(DraftProxy, String, GraphQLRequestOptions) -> #(Response, DraftProxy)`
- `get_config_snapshot(DraftProxy) -> Json`
- `get_log_snapshot(DraftProxy) -> Json`
- `get_state_snapshot(DraftProxy) -> Json`
- `reset(DraftProxy) -> DraftProxy`
- `dump_state(DraftProxy, created_at: String) -> Json`
- `dump_state_now(DraftProxy) -> Json`
- `restore_state(DraftProxy, dump_json: String) -> Result(DraftProxy, StateDumpError)`
- `commit(DraftProxy, headers)` on each target, synchronous on Erlang and
  Promise-returning on JavaScript

On JavaScript, use `process_request_async` when `POST /__meta/commit` or
live-hybrid upstream passthrough may need to await `fetch`.

## Installation

For repository development:

```sh
corepack pnpm install
cd gleam
gleam deps download
```

For Gleam projects before publication, use this directory as a path dependency
from a sibling checkout or worktree. After publication, the dependency will be:

```toml
# gleam.toml
[dependencies]
shopify_draft_proxy = ">= 0.1.0 and < 1.0.0"
```

For Node/TypeScript consumers, the final npm package remains
`shopify-draft-proxy`. Until release cutover, the checked-in JS shim lives in
`gleam/js/` and is exercised by the root `gleam:smoke:js` script.

For Elixir consumers before Hex publication, build an Erlang shipment locally:

```sh
cd gleam
gleam export erlang-shipment
cd elixir_smoke
mix test
```

After Hex publication, use the normal dependency shape:

```elixir
defp deps do
  [
    {:shopify_draft_proxy, "~> 0.1"}
  ]
end
```

## Supported Routes

The Gleam core currently recognizes:

| Route                                   | Status                                                                                                                                              |
| --------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST /admin/api/:version/graphql.json` | Routes query and mutation roots for ported domains.                                                                                                 |
| `GET /__meta/health`                    | Returns liveness JSON.                                                                                                                              |
| `GET /__meta/config`                    | Returns sanitized runtime config.                                                                                                                   |
| `GET /__meta/log`                       | Returns staged/proxied/committed mutation-log entries in replay order.                                                                              |
| `GET /__meta/state`                     | Returns the currently ported base/staged state buckets.                                                                                             |
| `POST /__meta/reset`                    | Clears staged state, logs, and synthetic identity counters.                                                                                         |
| `POST /__meta/commit`                   | Replays staged mutations upstream in log order. Erlang can run this synchronously; JavaScript callers must use `process_request_async` or `commit`. |

The Gleam core does not yet serve the legacy operator UI at `GET /__meta`.
The JS HTTP server adapter, staged-upload byte route, and bulk operation result
route are remaining cutover work. Until those land, the legacy TypeScript/Koa
server remains the compatibility bridge for those HTTP-only surfaces.

## Runtime Modes

`Snapshot` answers locally from snapshot/staged state for ported domains and
returns Shopify-like empty/null structures when the local store lacks data.

`LiveHybrid` answers ported domains locally and forwards unknown or
unimplemented operations upstream when the registry marks them as passthrough.
On JavaScript, forwarding is async and therefore requires `process_request_async`
or the JS shim's async `processRequest`.

`Live` is reserved for the live-only debugging posture that replaces the legacy
TypeScript `passthrough` mode during the final server cutover. Do not rely on it
as a complete server mode yet.

Supported mutations are staged locally in every mode. Commit replay is the only
normal path that intentionally sends those staged raw mutation bodies upstream.

## State Threading

`process_request` returns the response and the next proxy state:

```gleam
let #(response, next_proxy) = draft_proxy.process_request(proxy, request)
```

Callers must keep `next_proxy` and pass it to subsequent requests. This is how
the mutation log, staged state, and synthetic ID/timestamp registry advance
without process-wide mutable state.

The JavaScript shim wraps this in a mutable class to preserve the existing
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

The compatibility surface is the existing `createDraftProxy(config)` API. The
checked-in shim at `gleam/js/src/index.ts` translates JS-shaped config and
requests into Gleam records and unwraps Gleam responses back to JS objects.

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

The JS shim currently exports `createDraftProxy`, `DraftProxy`,
`DRAFT_PROXY_STATE_DUMP_SCHEMA`, `DraftProxyCommitError`, and the public config,
request, response, state, log, and commit result types. `createApp` and
`loadConfig` deliberately throw clear not-implemented errors until the Gleam JS
HTTP server adapter and config loader are complete.

Interop notes:

- Gleam records become JS objects/classes in emitted ESM; the shim hides those
  details from public callers.
- `Dict` is converted from/to ordinary JS objects at the boundary.
- `Option` is collapsed to optional or nullable JS values by the shim.
- `processRequest` is async so it can cover JS `fetch` for commit replay and
  live-hybrid passthrough.

## Using From Elixir

Gleam modules compile to Erlang module names with `@` separators. Direct calls
therefore use `:shopify_draft_proxy@proxy@draft_proxy`.

```elixir
alias :shopify_draft_proxy@proxy@draft_proxy, as: DraftProxy

proxy = DraftProxy.new()
request = {:request, "GET", "/__meta/health", %{}, ""}

{response, next_proxy} = DraftProxy.process_request(proxy, request)
{:response, status, body, _headers} = response
```

Calling conventions:

- Gleam records compile to tagged tuples such as
  `{:config, read_mode, port, origin, snapshot_path}`.
- No-payload variants compile to atoms such as `:snapshot` and
  `:live_hybrid`.
- `Option(a)` is `{:some, value}` or `:none`.
- `Result(a, b)` is `{:ok, value}` or `{:error, reason}`.
- `Dict` is an Erlang map.

Example custom config:

```elixir
config =
  {:config,
   :live_hybrid,
   4000,
   "https://my-shop.myshopify.com",
   :none}

proxy = DraftProxy.with_config(config)
```

The direct tuple shape is acceptable for adapter code. A thin Elixir wrapper
module that hides the tuple details behind Elixir structs/functions is tracked
as remaining port work.

## Conformance

The Gleam port does not rewrite parity specs or fixture bytes. It consumes the
same repository evidence as the legacy runtime and must eventually run every
scenario that the TypeScript parity runner runs.

Useful checks from the repository root:

```sh
corepack pnpm conformance:check
corepack pnpm conformance:parity
```

Useful checks from this directory:

```sh
gleam test --target erlang
gleam test --target javascript
gleam format --check
```

Live conformance capture remains a repository-level workflow driven by the
TypeScript capture scripts. Those scripts intentionally use shared fixture
formats so the captured evidence can validate the Gleam proxy as it reaches
parity.

## Remaining Unsupported Boundaries

- The package is not yet published to npm or Hex.
- `createApp` and `loadConfig` are not implemented in the Gleam JS shim.
- The JS HTTP server adapter is still separate from the Gleam core.
- `GET /__meta` operator UI is still legacy TypeScript/Koa only.
- Staged-upload byte serving and bulk operation result-file serving are not yet
  served by the Gleam JS HTTP adapter.
- Direct Elixir calls use Erlang-shaped tuples until the Elixir wrapper lands.
- Some endpoint domains and Relay `node`/`nodes` serializers remain partial,
  as tracked by the generated HAR-475 child issues.
- `Live` read mode is reserved and should not be treated as complete
  passthrough behavior.

## Development

```sh
# from gleam/
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

After the port is complete, this package will be promoted to the repository
root and the legacy TypeScript runtime under `../src` will be deleted.
