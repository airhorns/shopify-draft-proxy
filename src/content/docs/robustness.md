---
title: Robustness
description: How the project proves Shopify fidelity and avoids mock-server drift.
---

The proxy is robust only when local behavior is tied back to real Shopify behavior. The project uses layered validation so a supported operation proves both local mechanics and Shopify-like fidelity.

## Fidelity Standard

Supported behavior should preserve:

- Shopify GraphQL response shapes, nullability, and common error semantics.
- Stable synthetic IDs, timestamps, handles, and cursors inside a proxy session.
- User error paths and codes for supported branches.
- Read-after-write behavior after staged local mutations.
- Original raw mutation bodies for commit replay.

A root is not considered supported just because it appears in the registry or has a validation branch. The local model must emulate the supported lifecycle and downstream reads without runtime Shopify writes.

## Test Layers

| Layer                                                   | What it proves                                                                      |
| ------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| Rust tests under `src/` and `tests/`                    | Domain handlers, state transitions, serializers, parsers, and helper contracts.     |
| JavaScript integration tests under `tests/integration/` | The TypeScript shim, Node HTTP adapter, launch scripts, and JS package surface.     |
| Parity specs under `config/parity-specs/`               | Proxy responses compared against captured Shopify behavior.                         |
| Conformance capture scripts under `scripts/`            | Repeatable live Shopify evidence collection for tricky or newly supported behavior. |

## Parity Runner

Parity scenarios replay recorded interactions through the Rust-backed proxy runtime and compare selected proxy payloads with captured Shopify payloads.

The runner uses a cassette-playback model. Captures may include `upstreamCalls` for safe reads that the proxy performs while serving a local operation. Mutations in parity still stage locally; parity tests do not write to Shopify.

Scenarios must earn state through replayed requests or cassette-backed upstream reads. Pre-seeding parity runner state is not allowed because it can hide missing lifecycle behavior.

## Conformance Captures

When Shopify behavior is uncertain, add a live conformance capture against a disposable test shop. Capture scripts should create required setup data deliberately, record the interaction, and clean up when needed.

Checked-in captures are evidence only when they are connected to an executable spec or test path. Recording-only changes are not enough.

## Validation Commands

```sh
corepack pnpm lint
corepack pnpm typecheck
corepack pnpm test
corepack pnpm rust:test
corepack pnpm parity:run
```

Use targeted commands while developing, then run the checks that match the blast radius before handing off. Docs-only changes should at least build the docs site and run the repository lint path affected by package, Markdown, and config edits.

## Why This Matters

The project exists so test suites can exercise realistic Shopify write/read flows locally without normal supported mutations touching the real store. That only works if the proxy models domain behavior instead of patching isolated response fragments until a narrow test passes.
