---
title: Getting Started
description: Install the repository toolchain and run the proxy locally.
---

This guide uses the repository source checkout. Release packaging is still private to this repository.

## Prerequisites

- Node 22 or newer.
- Corepack with pnpm enabled.
- Rust and Cargo for the HTTP runtime.
- A Shopify dev store and Admin API access token when running live-hybrid flows against a real shop.

The repository includes `.mise.toml` for the host toolchain. If you use Mise and direnv, run:

```sh
mise install
direnv allow
```

## Install Dependencies

```sh
corepack pnpm install
```

## Build the Runtime and JavaScript Shim

```sh
corepack pnpm build
```

The root package builds the Rust HTTP server first, then compiles the TypeScript shim under `js/`.

## Use a Language Package

The JavaScript package starts and owns the Rust HTTP service. The Ruby and Python
packages embed the same Rust runtime in-process as native extensions.

- [JavaScript Library](/api/javascript/) for TypeScript and JavaScript tests.
- [Ruby Gem](/api/ruby/) for Ruby tests and host-language transport hooks.
- [Python Library](/api/python/) for Python tests and host-language transport hooks.

## Start the Local HTTP Service

```sh
SHOPIFY_ADMIN_ORIGIN=https://your-store.myshopify.com \
PORT=4000 \
corepack pnpm dev
```

The service listens on `http://localhost:4000` by default when `PORT` is omitted. It exposes the Shopify Admin GraphQL route and the meta API routes documented in the [HTTP Service reference](/api/http-service/).

## First Health Check

```sh
curl http://localhost:4000/__meta/health
```

Expected response:

```json
{
  "ok": true,
  "message": "shopify-draft-proxy is running"
}
```

## First GraphQL Request

```sh
curl http://localhost:4000/admin/api/2025-01/graphql.json \
  -H 'content-type: application/json' \
  -H 'x-shopify-access-token: shpat_test_token' \
  --data '{"query":"{ shop { name } }"}'
```

In live-hybrid mode, reads can go upstream when the runtime does not have a local staged answer. In snapshot mode, supported reads answer from the local snapshot plus staged state and should return Shopify-like empty or null structures when data is absent.

## Choose Runtime Modes

```sh
SHOPIFY_DRAFT_PROXY_READ_MODE=snapshot
SHOPIFY_DRAFT_PROXY_UNSUPPORTED_MUTATION_MODE=reject
SHOPIFY_DRAFT_PROXY_SNAPSHOT_PATH=./fixtures/example-snapshot.json
```

Use `reject` for unsupported mutations when a test suite should fail closed instead of letting an unknown mutation reach Shopify.
