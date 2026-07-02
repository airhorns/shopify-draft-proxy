---
title: Shopify Draft Proxy
description: A local Shopify Admin GraphQL digital twin for realistic app tests.
---

`shopify-draft-proxy` lets app and integration tests talk to a local Shopify Admin GraphQL proxy instead of mutating a real store during normal supported mutation handling.

The proxy is not a generic mock server. It keeps a local in-memory model of supported Shopify domains, stages supported mutations, records the original raw mutation bodies for later commit replay, and makes later reads behave as if Shopify had accepted those staged writes.

## What It Provides

- Versioned Shopify Admin GraphQL routes such as `/admin/api/2025-01/graphql.json`.
- Supported mutations staged in local state instead of sent to Shopify at runtime.
- Read-after-write overlays for modeled domains.
- Live-hybrid, snapshot, and passthrough-oriented read modes.
- Meta endpoints for health, config, logs, state inspection, reset, and commit.
- JavaScript, Ruby, Python, and local HTTP service entry points over the Rust runtime.

## Runtime Rule

Supported mutations should not be forwarded to Shopify during normal proxy runtime. They are interpreted locally, written to staged state, and returned with Shopify-like response shapes. Unsupported mutations may use the passthrough escape hatch, but that path must remain visible in observability.

`POST /__meta/commit` is the explicit exception: it replays the staged raw mutations upstream in original order when a test intentionally chooses to commit.

## Main Paths

- [Getting Started](/getting-started/) walks through installing and making the first local request.
- [JavaScript Library](/api/javascript/) covers the TypeScript-facing package shim.
- [Ruby Gem](/api/ruby/) covers the native Ruby embedding surface.
- [Python Library](/api/python/) covers the native Python embedding surface.
- [HTTP Service](/api/http-service/) lists the local service routes.
- [Endpoint Reference](/endpoints/products/) exposes the current domain coverage notes.
- [CLI Guide](/cli-guide/) documents the repository commands used to build, run, and validate the proxy.
- [Architecture](/architecture/) explains the request flow and state model.
- [Robustness](/robustness/) explains the test and conformance strategy.
