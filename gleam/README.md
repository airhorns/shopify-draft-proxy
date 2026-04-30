# shopify_draft_proxy

A Shopify Admin GraphQL digital twin / draft proxy. Implemented in [Gleam](https://gleam.run)
and compiled to both Erlang (BEAM) and JavaScript so it can be embedded from
Elixir/Erlang services and from Node/TypeScript without duplicating domain logic.

This directory holds the in-progress port from the legacy TypeScript
implementation in `../src`. It shares parity specs (`../config/parity-specs`)
and recorded Shopify fixtures (`../fixtures/conformance`) with the legacy
implementation. See `../GLEAM_PORT_INTENT.md` for the why and the non-goals,
and `../docs/architecture.md` for the runtime design.

> **Status:** Port in progress. The substrate (request routing, mutation log,
> snapshot/restore) is wired end-to-end; per-domain coverage is partial. The
> public API documented below is what the Gleam package will ship — gaps are
> called out inline as `TODO`.

## Public API

The package's entry point is `shopify_draft_proxy/proxy/draft_proxy`. Every
target consumes the same surface; only the syntax for calling into it
differs.

### Types

- `Request(method, path, headers, body)` — HTTP-shaped input. `headers` is a
  `Dict(String, String)`; `body` is the raw request body string (typically a
  JSON-encoded `{"query": "...", "variables": {...}}` for GraphQL).
- `Response(status, body, headers)` — HTTP-shaped output. `body` is a
  `gleam/json` tree; encode it with `json.to_string` before writing to the
  wire.
- `Config(read_mode, port, shopify_admin_origin, snapshot_path)` — sanitised
  runtime config. Mirrors what the legacy TS proxy exposes via
  `GET /__meta/config`.
- `ReadMode` — one of `Snapshot`, `LiveHybrid`, `Live`.
- `DraftProxy` — opaque-ish state record. Threaded through every request
  call so callers can advance the staged-mutation log.

### Functions

- `new() -> DraftProxy` — fresh proxy with `default_config()`.
- `default_config() -> Config` — defaults matching the TS test suite
  (`Snapshot` read mode, port 4000, `https://shopify.com` admin origin).
- `with_config(Config) -> DraftProxy` — fresh proxy with a custom config.
- `with_registry(DraftProxy, List(RegistryEntry)) -> DraftProxy` — attach a
  parsed operation registry so dispatch routes by capability instead of the
  hardcoded predicates. Optional; without a registry the proxy falls back to
  the legacy domain predicates.
- `registry_entry_has_local_dispatch(RegistryEntry) -> Bool` — report whether
  a TS registry entry is both marked implemented and accepted by a currently
  ported Gleam root predicate. This is intentionally narrower than capability
  classification so unported TS roots are not advertised as local support.
- `process_request(DraftProxy, Request) -> #(Response, DraftProxy)` — handle
  one request and return the response paired with the next proxy state. The
  TS class mutates itself in place; the Gleam port returns both halves so
  callers can thread state forward explicitly.
- `config_summary(Config) -> String` — small `read_mode@port` debug string.

Routes handled today: `GET /__meta/health`, `GET /__meta/config`,
`GET /__meta/log`, `GET /__meta/state`, `POST /__meta/reset`, and
`POST /admin/api/:version/graphql.json` for the events, delivery_settings,
saved_searches, webhooks, apps, functions, gift_cards, and segments
domains. Anything else returns 404.

> TODO: `POST /__meta/commit`, `GET /__bulk_operations/:id/result.jsonl`, and
> the staged-uploads routes — required to fully replace the TS proxy. See
> `../GLEAM_PORT_INTENT.md` "Substrate acceptance criteria".

## Using from Gleam

> TODO: installation. The package is not yet on Hex; once it is, depend on
> `shopify_draft_proxy = ">= 0.1 and < 1.0"` in your `gleam.toml`.

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
  // => {"ok":true,"message":"shopify-draft-proxy is running"}
}
```

To handle a GraphQL request, point `path` at
`/admin/api/:version/graphql.json` and put the JSON body (with `query` and
optional `variables`) in `body`. The returned `Response` is the GraphQL
envelope `{"data": ...}` — encode it with `json.to_string` before sending it
on the wire. Thread the second element of the tuple back into the next
`process_request` call to keep the staged mutation log advancing.

To use a non-default config:

```gleam
let proxy =
  draft_proxy.with_config(draft_proxy.Config(
    read_mode: draft_proxy.LiveHybrid,
    port: 4000,
    shopify_admin_origin: "https://my-shop.myshopify.com",
    snapshot_path: option.None,
  ))
```

## Using from Elixir

The Gleam package compiles to BEAM and is publishable to Hex, so Elixir
consumes it as an ordinary mix dependency. Gleam modules become Erlang
modules with `@`-separated path segments, so `shopify_draft_proxy/proxy/draft_proxy`
is callable as `:shopify_draft_proxy@proxy@draft_proxy`.

> TODO: installation. Once published:
>
> ```elixir
> # mix.exs
> defp deps do
>   [
>     {:shopify_draft_proxy, "~> 0.1"}
>   ]
> end
> ```
>
> Until then, the canonical way to consume the package locally is the
> `gleam export erlang-shipment` artefact loaded by the smoke project in
> `./elixir_smoke/` — see [Building a release artefact](#building-a-release-artefact).

### Calling conventions

- Gleam custom types compile to Erlang records, which on the wire are
  positional tuples tagged with the constructor name. `Config(...)` becomes
  `{:config, read_mode, port, origin, snapshot_path}`.
- Gleam variants (no payload) are bare atoms. `Snapshot` ⇒ `:snapshot`,
  `LiveHybrid` ⇒ `:live_hybrid`, `Live` ⇒ `:live`.
- `Option(a)` is `{:some, a} | :none` — there is no Elixir-style `nil`.
- `Result(a, b)` is `{:ok, a} | {:error, b}` — works directly with `with`/`case`.
- Strings are UTF-8 binaries on both sides ✓.
- Dicts (`gleam/dict`) are Erlang maps under the hood ✓.

### Example

```elixir
defmodule MyApp.DraftProxyDemo do
  @moduledoc """
  Minimal end-to-end use of the Gleam-compiled draft proxy from Elixir.
  """

  alias :shopify_draft_proxy@proxy@draft_proxy, as: DraftProxy

  def health_check do
    proxy = DraftProxy.new()

    request =
      {:request, "GET", "/__meta/health", %{}, ""}

    {response, _next_proxy} = DraftProxy.process_request(proxy, request)
    {:response, status, body, _headers} = response

    {status, body}
  end

  def graphql_request(query, variables \\ %{}) do
    proxy = DraftProxy.new()

    body = Jason.encode!(%{"query" => query, "variables" => variables})

    request =
      {:request, "POST", "/admin/api/2025-01/graphql.json", %{}, body}

    {response, next_proxy} = DraftProxy.process_request(proxy, request)
    {:response, status, json_tree, _headers} = response

    # `json_tree` is a `gleam/json` value — convert with `gleam@json:to_string/1`.
    {status, :gleam@json.to_string(json_tree), next_proxy}
  end
end
```

A custom config:

```elixir
config =
  {:config,
   :live_hybrid,                           # read_mode
   4000,                                   # port
   "https://my-shop.myshopify.com",        # shopify_admin_origin
   :none                                   # snapshot_path :: Option(String)
  }

proxy = :shopify_draft_proxy@proxy@draft_proxy.with_config(config)
```

> TODO: ship a thin Elixir wrapper module (`ShopifyDraftProxy`) that hides
> the tuple shapes behind structs and `Jason.decode/1`-friendly responses.
> The current shape is fine for adapter code but is unfriendly to call from
> application code directly.

## Using from TypeScript / JavaScript

The Gleam package emits ESM as a build target, and that emitted ESM **will
become the only TypeScript implementation** once the port lands. The legacy
`../src` will be deleted; consumers will continue to import the same
`createDraftProxy(config)` / `processRequest(...)` names from a thin TS shim
that re-exports the Gleam-emitted modules with stable types.

> TODO: delete `../src/**` once Gleam domain coverage matches the legacy
> proxy. See `../GLEAM_PORT_INTENT.md` "Domain coverage acceptance criteria".

> TODO: installation. `shopify-draft-proxy` will continue to be the npm
> package name; the published tarball will bundle the Gleam-emitted ESM
> plus a `dist/index.{js,d.ts}` shim. Until the cutover, `../src` ships the
> legacy implementation under that name.

### Planned public surface

The TS shim re-exports the same names the legacy `../src/index.ts` exports
today. Notable items:

- `createDraftProxy(config?: AppConfig): DraftProxy`
- `DraftProxy#processRequest(request: DraftProxyRequest): DraftProxyHttpResponse`
- `DraftProxy#dumpState(): DraftProxyStateDump` / `restoreState(dump)`
- `DraftProxyCommitError`, `DRAFT_PROXY_STATE_DUMP_SCHEMA`
- Types: `AppConfig`, `ReadMode`, `DraftProxyRequest`,
  `DraftProxyHttpResponse`, `DraftProxyStateDump`, etc.

### Calling conventions (Gleam → JS)

- Gleam records become plain JS objects with the field names you wrote in
  the Gleam source: `Request(method:, path:, headers:, body:)` ⇒
  `{ method, path, headers, body }`. Gleam's compiler emits TypeScript
  declarations alongside the ESM (`typescript_declarations = true` in
  `gleam.toml`), so editor IntelliSense works without a hand-written `.d.ts`.
- Gleam tuples (`#(a, b)`) become JS arrays — `process_request` returns
  `[response, nextProxy]`.
- `Option(a)` is a tagged class instance; the shim collapses it to
  `T | null` for the JS-facing API.
- `Dict` is a JS `Map`; the shim accepts and returns plain objects.
- `Result(a, b)` is preserved; the shim throws on `Error(_)` for the
  imperative TS callers and exposes a `safe`-prefixed variant for callers
  that want to handle the error tuple directly.

### Example (planned shim shape)

```ts
import { createDraftProxy } from 'shopify-draft-proxy';

const proxy = createDraftProxy();

const response = proxy.processRequest({
  method: 'POST',
  path: '/admin/api/2025-01/graphql.json',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ query: '{ events(first: 1) { nodes { id } } }' }),
});

console.log(response.status, response.body);
```

> TODO: ship the TS shim (`gleam/ts/`) and wire `package.json#exports` to
> point at it. Until then, the smoke test in
> `../tests/integration/gleam-interop.test.ts` imports the raw Gleam ESM
> directly and exercises `hello()`.

## Development

```sh
# Install dependencies
gleam deps download

# Run tests on both targets
gleam test --target erlang
gleam test --target javascript
```

## Building a release artefact

For Elixir/Erlang consumers, `gleam export erlang-shipment` produces a
self-contained directory tree of `.beam` files that can be loaded into any
mix/rebar3 project without an Elixir-side compile step. The
`elixir_smoke/` project consumes that shipment to assert the package is
loadable and callable from Elixir.

```sh
# from gleam/
gleam export erlang-shipment

# then, from gleam/elixir_smoke/
mix test
```

This is the local equivalent of `mix deps.get && mix compile` against a
published Hex release; running it before `gleam publish` catches BEAM-side
regressions that the JavaScript test target would miss.

## Layout

- `src/` — Gleam source.
  - `shopify_draft_proxy.gleam` — root module (currently a phase-0 marker).
  - `shopify_draft_proxy/proxy/draft_proxy.gleam` — public entry point.
  - `shopify_draft_proxy/proxy/*.gleam` — per-domain dispatchers.
  - `shopify_draft_proxy/graphql/*.gleam` — lexer, parser, root-field walk.
  - `shopify_draft_proxy/state/*.gleam` — store, synthetic identity, types.
- `test/` — gleeunit tests, mirroring `src/`.
- `elixir_smoke/` — mix project that loads the Erlang shipment and asserts
  the package is callable from Elixir.
- `gleam.toml` — package manifest. Default target is JavaScript so
  `gleam test` exercises the runtime Node consumers will use; the Erlang
  target is run alongside in CI and via `gleam test --target erlang`.

The package will be promoted to the repository root and the legacy
TypeScript in `../src` will be deleted once domain coverage reaches parity.
