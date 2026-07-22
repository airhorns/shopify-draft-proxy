---
title: Ruby Gem
description: Ruby API reference for embedding the proxy runtime in-process.
---

The Ruby gem embeds the Rust `DraftProxy` runtime in the current Ruby process. It
does not start the local HTTP server and does not reimplement Shopify routing in
Ruby.

Each `ShopifyDraftProxy.create(...)` call owns an independent proxy instance with
its own staged state, mutation log, synthetic IDs, and transport hooks.

## Install

The gem name is `shopify-draft-proxy` and the require path is
`shopify_draft_proxy`.

For a released gem:

```sh
gem install shopify-draft-proxy
```

From this repository checkout, build the native extension with:

```sh
cd ruby
bundle install
bundle exec rake native:build
```

Run the Ruby smoke suite from the repository root with:

```sh
corepack pnpm ruby:smoke
```

For a local app that consumes the checkout directly, use a path dependency while
release packaging is not available from your package index:

```ruby
gem "shopify-draft-proxy", path: "../shopify-draft-proxy/ruby"
```

## Configuration and Auth

```ruby
require "shopify_draft_proxy"

proxy = ShopifyDraftProxy.create(
  read_mode: "snapshot",
  unsupported_mutation_mode: "reject",
  shopify_admin_origin: "https://your-store.myshopify.com",
)
```

Common options:

| Option                                                  | Purpose                                                                  |
| ------------------------------------------------------- | ------------------------------------------------------------------------ |
| `read_mode`                                             | `"snapshot"`, `"live-hybrid"`, or `"passthrough"`. Defaults to snapshot. |
| `unsupported_mutation_mode`                             | `"passthrough"` or `"reject"`.                                           |
| `shopify_admin_origin`                                  | Upstream Shopify origin used for live reads and commit replay.           |
| `snapshot_path`                                         | Optional snapshot file loaded into the runtime.                          |
| `state`                                                 | Optional state dump from `dump_state`.                                   |
| `transport`                                             | Optional callable for upstream reads and commit replay.                  |
| `bulk_operation_run_mutation_max_input_file_size_bytes` | Optional local staged-upload size guardrail.                             |

Headers are part of a staged operation and are preserved with its path and raw GraphQL body. Use placeholder values instead of concrete credentials when state will be dumped or inspected:

```ruby
proxy.process_graphql_request(
  { query: mutation },
  headers: {
    "X-Shopify-ServiceApp-Scope-Restricted-Token" => "{{credential}}",
    "X-Shopify-Service-Name" => "sidekick-server",
  },
)
```

A header hash passed to `commit` applies to every replay, preserving the commit-wide API:

```ruby
proxy.commit(
  headers: {
    "Authorization" => "Bearer #{ENV.fetch("SHOPIFY_ADMIN_ACCESS_TOKEN")}",
  },
)
```

The Ruby binding also accepts a callable that receives each staged operation and returns its complete final header set. This supports mixed-credential commits without persisting tokens:

```ruby
proxy.commit(
  headers: lambda do |operation|
    operation.fetch("headers").merge(
      "X-Shopify-ServiceApp-Scope-Restricted-Token" => token_for(operation),
    )
  end,
)
```

Supported mutations are staged locally during normal runtime. `commit` is the explicit write-through boundary and replays the original staged operations in order.

## Quickstart

This example runs fully in snapshot mode and does not require a live Shopify
token.

```ruby
require "json"
require "shopify_draft_proxy"

begin
  proxy = ShopifyDraftProxy.create(
    read_mode: "snapshot",
    unsupported_mutation_mode: "reject",
    shopify_admin_origin: "https://example.myshopify.com",
  )

  create = proxy.process_graphql_request(
    {
      query: <<~GRAPHQL,
        mutation {
          savedSearchCreate(input: { name: "Promo orders", query: "tag:promo", resourceType: ORDER }) {
            savedSearch { id name query resourceType }
            userErrors { field message }
          }
        }
      GRAPHQL
    },
  )

  raise create.body.to_json unless create.status == 200

  read = proxy.process_graphql_request(
    {
      query: '{ orderSavedSearches(query: "Promo") { nodes { id name } } }',
    },
  )

  puts read.body.fetch("data").fetch("orderSavedSearches").fetch("nodes")
  puts proxy.get_log.fetch("entries").length
ensure
  proxy&.dispose
end
```

`process_request` is available when a test needs exact route behavior:

```ruby
health = proxy.process_request(method: "GET", path: "/__meta/health")
puts health.status
puts health.body
```

## Transport Hooks

Ruby performs the proxy's outbound HTTP through a transport callable. The default
transport uses `Net::HTTP`, and custom transports can add tracing, VCR/WebMock
integration, retries, or a shared connection pool.

```ruby
transport = lambda do |request|
  # request => { "method", "url", "headers", "body" }
  ShopifyDraftProxy::Transports::DEFAULT.call(request)
end

proxy = ShopifyDraftProxy.create(
  shopify_admin_origin: "https://example.myshopify.com",
  transport: transport,
)
```

The transport must return `{ "status", "headers", "body" }`.

## Limitations

- The Ruby gem follows the same operation coverage as the Rust runtime. Check the
  endpoint reference for supported lifecycle behavior before relying on a root.
- `origin` returns `nil` and `dispose` is a no-op because the Ruby package does
  not spawn an HTTP server process.
- Unsupported mutations may still passthrough when
  `unsupported_mutation_mode: "passthrough"` is configured.
- The repository source is the authoritative package reference when a registry
  release is not available yet.

## References

- [Ruby package source](https://github.com/airhorns/shopify-draft-proxy/tree/main/ruby)
- [Ruby gemspec](https://github.com/airhorns/shopify-draft-proxy/blob/main/ruby/shopify-draft-proxy.gemspec)
- [Ruby README](https://github.com/airhorns/shopify-draft-proxy/blob/main/ruby/README.md)
- [Ruby smoke tests](https://github.com/airhorns/shopify-draft-proxy/tree/main/ruby/test)
