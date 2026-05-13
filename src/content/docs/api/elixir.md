---
title: Elixir Library
description: BEAM and Elixir API reference for the Gleam runtime wrapper.
---

The runtime compiles to Erlang/BEAM from the same Gleam source used by JavaScript. The checked-in `elixir_smoke` wrapper demonstrates the intended Elixir-facing shape before package publication.

## Repository Smoke Path

```sh
gleam export erlang-shipment
cd elixir_smoke
mix test
```

From the repository root, the same smoke path is wrapped by:

```sh
corepack pnpm elixir:smoke
```

## Future Package Dependency

After publication, Elixir applications will depend on the Hex package normally:

```elixir
defp deps do
  [
    {:shopify_draft_proxy, "~> 0.1"}
  ]
end
```

## Create a Proxy

```elixir
proxy =
  ShopifyDraftProxy.with_config(
    read_mode: :snapshot,
    unsupported_mutation_mode: :reject,
    port: 4000,
    shopify_admin_origin: "https://your-store.myshopify.com"
  )
```

`ShopifyDraftProxy.new/0` creates a proxy with the default registry. `with_config/1` accepts read mode, unsupported mutation mode, port, Shopify Admin origin, optional snapshot path, and the bulk operation upload limit.

## GraphQL Requests

```elixir
response =
  ShopifyDraftProxy.graphql(
    proxy,
    "{ shop { name } }",
    api_version: "2025-01",
    headers: %{"x-shopify-access-token" => "shpat_test_token"}
  )

%ShopifyDraftProxy.Response{
  status: 200,
  body: body,
  proxy: next_proxy
} = response
```

The response body is a JSON string so the application can decode it with its preferred JSON library.

Always thread `next_proxy` into the next request. The BEAM wrapper keeps the Gleam `DraftProxy` value opaque and returns the next value explicitly so staged state, logs, and synthetic IDs stay isolated per proxy instance.

## HTTP-Shaped Requests

```elixir
%ShopifyDraftProxy.Response{proxy: next_proxy} =
  ShopifyDraftProxy.request(
    proxy,
    "GET",
    "/__meta/health",
    "",
    %{}
  )
```

Use `request/5` for exact route tests and `graphql/3` for Admin GraphQL request construction.

## Meta Helpers

```elixir
ShopifyDraftProxy.config(proxy)
ShopifyDraftProxy.log(proxy)
ShopifyDraftProxy.state(proxy)
ShopifyDraftProxy.reset(proxy)
ShopifyDraftProxy.commit(proxy, %{"x-shopify-access-token" => "shpat_real_token"})
```

`dump_state/2` returns a JSON state dump string. `restore_state/2` returns `{:ok, proxy}` or `{:error, reason}`.

## Commit Transport Injection

`commit_with/4` is the smoke-wrapper path for testing commit replay with an injected transport. It lets tests assert the staged raw mutations are replayed without requiring a live Shopify write.
