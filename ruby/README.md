# Shopify Draft Proxy Ruby Gem

This folder contains the Ruby package surface for the Rust-backed Shopify Draft Proxy runtime.

The gem is intentionally thin: it starts the Rust HTTP server and talks to its Shopify-like routes. It does not reimplement GraphQL routing, staging, commit replay, or Shopify domain behavior in Ruby.

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

## Runtime Selection

By default the gem runs:

```bash
cargo run --bin shopify-draft-proxy-server --quiet
```

from the repository root. For packaged or CI usage, build the Rust server first and point the gem at the binary:

```bash
cargo build --bin shopify-draft-proxy-server
SHOPIFY_DRAFT_PROXY_SERVER_BIN=../target/debug/shopify-draft-proxy-server \
  ruby -Ilib:test test/shopify_draft_proxy_smoke_test.rb
```

## Smoke Tests

From the repository root:

```bash
corepack pnpm ruby:smoke
```

The smoke runner builds the Rust server, then runs the Ruby Minitest suite. If local Ruby is unavailable, it falls back to Docker with the built server binary mounted into the container.
