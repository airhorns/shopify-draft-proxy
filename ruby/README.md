# Shopify Draft Proxy Ruby Gem

This folder contains the Ruby package surface for the Rust-backed Shopify Draft Proxy runtime.

The gem is intentionally thin: it loads a native Ruby extension powered by the Rust `DraftProxy` library and calls that runtime in-process. It does not reimplement GraphQL routing, staging, commit replay, or Shopify domain behavior in Ruby.

## Usage

```ruby
require "shopify_draft_proxy"

proxy = ShopifyDraftProxy.create(
  read_mode: "snapshot",
  shopify_admin_origin: "https://example.myshopify.com",
)

health = proxy.process_request(method: "GET", path: "/__meta/health")
puts health.body

graphql = proxy.process_graphql_request(
  {
    query: '{ orderSavedSearches(query: "Promo") { nodes { id name } } }',
  },
)
puts graphql.body
proxy.dispose
```

## Transports (outbound HTTP)

The proxy's _outbound_ HTTP — the `commit` replay and any live-hybrid
passthrough reads — runs in Ruby, not Rust. The Rust core hands a Ruby
**transport** callable a request hash and expects a response hash back:

```ruby
# request  → { "method", "url", "headers", "body" }
# response ← { "status", "headers", "body" }
```

Doing the IO in Ruby means the GVL is released during the socket wait (so other
threads keep running) and Ruby-level instrumentation — OpenTelemetry, WebMock,
VCR — observes the request like any other `Net::HTTP` call.

The default transport (`ShopifyDraftProxy::Transports::DEFAULT`) is a plain
`Net::HTTP` round-trip. Supply your own to add tracing, retries, or a pooled
connection:

```ruby
traced = lambda do |request|
  span = tracer.start_span("shopify.commit", attributes: { "http.url" => request["url"] })
  ShopifyDraftProxy::Transports::DEFAULT.call(request)
ensure
  span&.finish
end

proxy = ShopifyDraftProxy.create(
  shopify_admin_origin: "https://example.myshopify.com",
  transport: traced,
)
```

A transport is anything responding to `#call`. When omitted, the default
`Net::HTTP` transport is installed.

## Per-operation commit headers

A staged operation preserves its request headers with its path and raw GraphQL body. Use placeholder values rather than concrete credentials when state will be dumped or inspected:

```ruby
proxy.process_graphql_request(
  { query: mutation },
  headers: { "Authorization" => "Bearer {{credential}}" },
)
```

Passing a header hash to `commit` applies it to every replay. Passing a callable resolves headers separately for each staged operation:

```ruby
proxy.commit(
  headers: lambda do |operation|
    operation.fetch("headers").merge(
      "Authorization" => "Bearer #{token_for(operation)}",
    )
  end,
)
```

The callable must return the complete final header hash. It may replace the staged authentication scheme, allowing one commit to mix staff and service-app credentials without storing tokens in proxy state.

## Persisting state (dump / restore)

The gem keeps proxy state in process memory. To persist it across processes — a
file, Redis, a database row, ... — drive serialization from your own code: seed
a fresh proxy with a previously saved dump, run an operation, and save the new
dump. This is plain `dump_state` / `state:` serialization, with no runtime
cooperation required.

To persist only when state actually changed — without diffing whole dumps — use
the version token. Every response carries the current token in its
`x-sdp-state-version` header, and
`ShopifyDraftProxy::DraftProxy.state_version_of(dump)` computes that same token
for any dump Hash. Compare the seed dump's token against the response header and
save only when they differ:

```ruby
seed = load_dump_from_somewhere # a prior #dump_state, or nil

proxy = ShopifyDraftProxy.create(
  shopify_admin_origin: "https://example.myshopify.com",
  state: seed,
)
baseline =
  if seed
    ShopifyDraftProxy::DraftProxy.state_version_of(seed)
  else
    ShopifyDraftProxy::DraftProxy::PRISTINE_STATE_VERSION
  end

response = proxy.process_graphql_request({ query: mutation })

version = response.headers["x-sdp-state-version"]
save_dump_somewhere(proxy.dump_state) if version && version != baseline
proxy.dispose
```

A pure read leaves the token unchanged, so it writes nothing — you persist
exactly when staged state changed, driven entirely from your own code.

## Native Extension Build

The native extension lives in `native/` and compiles to:

```bash
ruby/lib/shopify_draft_proxy/shopify_draft_proxy_native.so
```

For local development from the repository root:

```bash
cd ruby
bundle exec rake native:build
```

Each `ShopifyDraftProxy.create(...)` call owns an independent Rust `DraftProxy` instance in the current Ruby process. `dump_state` and `restore_state` use the same Rust state dump schema as the native runtime.

The package exposes `ShopifyDraftProxy::DRAFT_PROXY_STATE_DUMP_SCHEMA` with the
current dump schema identifier so callers can validate serialized state before
restoring it.

## Smoke Tests

From the repository root:

```bash
corepack pnpm ruby:smoke
```

The smoke runner builds the native extension with Cargo, then runs the Ruby Minitest suite with Ruby from the local environment. The repository mise configuration includes Ruby for development shells and CI runs the same Minitest smoke suite in a dedicated Ruby job.
