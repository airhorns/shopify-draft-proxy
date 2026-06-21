---
title: Architecture
description: How the draft proxy routes requests, stages state, and preserves Shopify-like behavior.
---

The runtime authority is Rust under `src/`. JavaScript and TypeScript callers use a thin package shim that starts the Rust HTTP runtime and forwards requests to it.

## Request Flow

```text
App or test harness
  -> DraftProxy instance
  -> operation classifier
    -> query path
      -> optional upstream Shopify read
      -> normalized staged-state overlay
      -> GraphQL response serializer
    -> mutation path
      -> supported root: local domain command + staged state + synthesized payload
      -> unsupported root: passthrough or reject
    -> meta path
      -> health/config/log/state/reset/dump/restore/commit
```

The JavaScript package wraps the HTTP surface exposed by `src/bin/shopify-draft-proxy-server.rs`.

## DraftProxy Is Instance-Owned

The Rust `DraftProxy` owns its store, operation registry, synthetic identity, mutation log, and injectable transports:

```rust
pub struct DraftProxy {
    // instance-owned runtime state
}
```

There is no process-wide singleton, ambient context, or module-global store. The JavaScript shim owns a child Rust server process per `DraftProxy` instance, preserving isolated staged state for each test session.

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

`POST /__meta/commit` intentionally replays staged raw mutations upstream in original order. Commit replay lives in `src/proxy/commit.rs`, uses the commit request's auth headers, maps successful synthetic IDs to authoritative Shopify IDs for later replay bodies, and stops on the first transport or GraphQL error while reporting per-attempt details.

## State Model

The store is normalized into base and staged buckets:

- `baseState` contains snapshot-derived entities or records learned from safe upstream reads.
- `stagedState` contains local inserts, updates, deletes, derived indexes, and generated artifacts.
- `mutationLog` preserves the original raw GraphQL mutation requests plus interpreted command metadata.
- `syntheticIdentityRegistry` mints stable local IDs, timestamps, handles, and cursors.

Effective reads merge base and staged state. Commit drains staged work only after successful upstream replay.

## Domain Modules

Each Admin API area owns its query builders, mutation interpreters, serializers, and state mutators. Large domains may split implementation under subdirectories while keeping a stable public entry point.

Endpoint-specific quirks and coverage notes live in `src/content/docs/endpoints/`. High-level runtime shape lives in `docs/architecture.md`.

## Operation Registry

The Rust operation registry maps operation names to capability metadata and exposes the local dispatch root inventory used by runtime gates and tests. TypeScript tooling reads that same metadata through the Rust `operation-registry-json` exporter instead of a second checked-in JSON registry.

The `implemented` flag marks every root the proxy answers locally instead of sending upstream. It is kept aligned with the uniform `LOCAL_DISPATCH_ROOTS` table, which is the local-routing inventory; anything without a dispatch root passes through rather than erroring. `implemented` is therefore a routing fact, not a fidelity claim.

Being **supported** is the higher bar: a commitment to local lifecycle behavior and downstream read-after-write effects, modeled from the store and proven by runtime tests plus captured conformance coverage. Branch-only or validation-only responses are documented as guardrails, not full support. Canned responses (a handler that sniffs the GraphQL document and returns a hardcoded payload) are prohibited and were removed from local dispatch.
