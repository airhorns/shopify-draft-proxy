# shopify-draft-proxy

A high-fidelity **Shopify Admin GraphQL draft proxy** for testing Shopify apps.

The proxy sits between an app and Shopify. By default it proxies reads through unchanged. When the app performs supported mutations, the proxy stages those mutations locally, returns a Shopify-like response, and serves subsequent reads as if the changes happened — **without touching the real store** until an explicit commit.

## Core idea

This project is not a generic mock server. It is a **digital twin** / **draft layer** for Shopify Admin GraphQL with these goals:

- preserve the app's existing HTTP + auth behavior
- emulate Shopify as faithfully as practical
- allow tests to create/edit/delete resources without real side effects
- make staged IDs/timestamps stable for the whole runtime session
- expose a meta API for reset, inspect, and commit
- measure fidelity continuously via conformance tests against a real Shopify dev store

## Current scope

- Koa server in TypeScript
- Shopify Admin GraphQL only
- global in-memory state
- strict TypeScript
- pnpm-based workflow
- products domain first

## Key docs

- `docs/simple-demo-guide.md` — copy-pasteable local demo for read/write staging, reset, and commit
- `docs/original-intent.md` — project intent and non-goals; preserve this vision
- `docs/architecture.md` — system design and execution model
- `docs/implementation-plan.md` — milestone plan and ordered build steps
- `docs/hard-and-weird-notes.md` — fidelity traps and oddities discovered during implementation
- `.agents/skills/shopify-conformance-expansion/SKILL.md` — how to connect a real Shopify dev store/app for parity testing and extend conformance coverage

## Status

This repository is bootstrapped with the intended architecture and docs. Runtime behavior is currently scaffolded, not yet a full implementation.
