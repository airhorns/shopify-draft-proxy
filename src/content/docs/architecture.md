---
title: Architecture
description: How the draft proxy routes requests, stages state, and preserves Shopify-like behavior.
---

The runtime authority is Gleam under `src/shopify_draft_proxy/`. It compiles to JavaScript and Erlang so JavaScript, TypeScript, Elixir, and Erlang test suites use the same domain model.

## Request Flow

```text
App or test harness
  -> DraftProxy value
  -> operation classifier
    -> query path
      -> optional upstream Shopify read
      -> normalized staged-state overlay
      -> GraphQL response serializer
    -> mutation path
      -> supported root: local domain command + staged state + synthesized payload
      -> unsupported root: passthrough or reject
    -> meta path
      -> health/config/log/state/reset/commit
```

The JavaScript HTTP adapter wraps the same `process_request` runtime surface used by embedders.

## DraftProxy Is a Value

The core API returns the response and the next proxy value:

```gleam
let #(response, next_proxy) = draft_proxy.process_request(proxy, request)
```

There is no process-wide singleton, ambient context, or module-global store. Domain handlers receive the instance-owned store and synthetic identity through the proxy value passed into them.

JavaScript wraps this in a mutable class for convenience. Elixir and direct Gleam callers thread the next proxy value explicitly.

## Read Modes

| Mode          | Behavior                                                                                                                       |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `snapshot`    | Resolve supported reads from startup snapshot plus staged state. Missing data should match Shopify's null or empty structures. |
| `live-hybrid` | Forward reads upstream when local state has nothing to add, then overlay staged effects for supported domains.                 |
| `passthrough` | Live-only debugging baseline with no overlay for reads.                                                                        |

`live-hybrid` lets fidelity expand one operation at a time without pretending every Admin API domain is complete.

## Mutation Paths

Supported mutations are parsed into domain commands, applied to local staged state, and returned with Shopify-like payloads and user errors. They are not sent to Shopify during normal runtime.

Unsupported mutations use the configured escape hatch:

- `passthrough` forwards the request upstream and records that fact.
- `reject` returns a 400 GraphQL error envelope before any upstream mutation call.

`POST /__meta/commit` intentionally replays staged raw mutations upstream in original order.

## State Model

The store is normalized into base and staged buckets:

- `baseState` contains snapshot-derived entities or records learned from safe upstream reads.
- `stagedState` contains local inserts, updates, deletes, derived indexes, and generated artifacts.
- `mutationLog` preserves the original raw GraphQL mutation requests plus interpreted command metadata.
- `syntheticIdentityRegistry` mints stable local IDs, timestamps, handles, and cursors.

Effective reads merge base and staged state. Commit drains staged work by replaying the original raw mutations.

## Domain Modules

Each Admin API area owns its query builders, mutation interpreters, serializers, and state mutators. Large domains may split implementation under subdirectories while keeping a stable public entry point.

Endpoint-specific quirks and coverage notes live in `docs/endpoints/`. High-level runtime shape lives in `docs/architecture.md`.

## Operation Registry

The generated operation registry maps operation names to capability metadata. A registered supported operation is a commitment to local lifecycle behavior and downstream read-after-write effects. Branch-only or validation-only handling is documented as a guardrail, not full support.
