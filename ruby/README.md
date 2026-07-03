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

## Storage adapters (persisting state)

By default a proxy holds its staged state in process memory and loses it when
the process exits. A **storage adapter** lets you persist that state somewhere
you control — a file, Redis, a database row, S3 — so it survives restarts or is
shared between processes.

An adapter is any object responding to two methods:

```ruby
# load        → a state dump Hash (as returned by #dump_state) or nil
# save(dump)  → persist the given dump Hash
```

On construction the proxy calls `#load`; a non-nil result rehydrates the proxy
(and takes precedence over any `state:` seed). After that, the proxy calls
`#save` whenever a request changed persistable state:

```ruby
proxy = ShopifyDraftProxy.create(
  shopify_admin_origin: "https://example.myshopify.com",
  storage: ShopifyDraftProxy::Storage::File.new("tmp/proxy-state.json"),
)

# ... run mutations; each change is persisted automatically ...
proxy.dispose
```

`ShopifyDraftProxy::Storage::File` is a batteries-included adapter (and the
reference implementation of the contract). A custom backend is just as small:

```ruby
class RedisStorage
  def initialize(redis, key)
    @redis = redis
    @key = key
  end

  def load
    raw = @redis.get(@key)
    raw && JSON.parse(raw)
  end

  def save(dump)
    @redis.set(@key, JSON.generate(dump))
  end
end

proxy = ShopifyDraftProxy.create(
  shopify_admin_origin: "https://example.myshopify.com",
  storage: RedisStorage.new(redis, "shop:#{worker_id}"),
)
```

### When state is persisted

`persist:` controls the save cadence:

- `:each_mutation` (default) — the proxy saves automatically after every request
  that changed state, plus after `commit` and `reset`. Pure reads never save.
  You can still call `persist!` at any time to force a write.
- `:manual` — the proxy never saves on its own; call `proxy.persist!` when you
  want to flush the current state. Fastest, but you own every write.

```ruby
proxy = ShopifyDraftProxy.create(
  shopify_admin_origin: "https://example.myshopify.com",
  storage: my_storage,
  persist: :manual,
)

run_a_batch_of_mutations(proxy)
proxy.persist! # one write for the whole batch
```

Each adapter is expected to be the single writer of its stored dump. The
intended concurrency model is *many isolated stores* — one key per test or
worker — rather than many writers sharing one key (whole-state saves are
last-write-wins).

### Threading

A `DraftProxy` instance is **not** safe to share across threads. Each call
borrows the underlying Rust store mutably for the whole request — including the
outbound transport IO performed during a `commit` — so a second thread entering
the *same instance* concurrently will panic with a Rust `BorrowMutError`. Give
each thread/worker its own proxy (and its own storage key); that is the same
"many isolated stores" model described above.

### If a save fails

Under `:each_mutation`, persistence is **fail-loud**: if `storage.save` raises
(e.g. the backend is unreachable), the error propagates out of the request call.
The in-memory mutation has already applied, but the cached version token is *not*
advanced, so the next successful save writes the full current state and the
skipped write self-heals. A raised save therefore means "this mutation applied
but was not persisted" — rescue it if your backend can be flaky, or use
`persist: :manual` to control exactly when writes happen.

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
