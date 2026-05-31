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
