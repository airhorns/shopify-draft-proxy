---
name: parity-runner
description: Use when running or debugging targeted conformance parity scenarios, validating one checked-in parity spec, using `corepack pnpm parity -- <scenario-id>`, or deciding between targeted parity and full parity gates.
---

# Parity Runner

Use this skill when you need fast scenario-level proof for a checked-in
conformance parity spec. It documents the targeted runner only; it does not
lower the handoff bar for fidelity work.

## When to Use

- During implementation or review rework for one parity scenario.
- When a ticket or reviewer asks for `corepack pnpm parity -- <scenario-id>`.
- When debugging a single parity spec by `scenarioId` or file path.
- When checking a narrow fix before rerunning the required full gates.

Do not use targeted parity as a replacement for the ticket's required
validation, `corepack pnpm rust:test`, or CI.

## Commands

Run from the repository root. In unattended workspaces, load the pinned tool
environment first:

```sh
eval "$(mise env)" && corepack pnpm parity -- saved-search-reserved-name
```

Run by explicit spec path:

```sh
eval "$(mise env)" && corepack pnpm parity -- --spec config/parity-specs/saved-searches/saved-search-reserved-name.json
```

Print debug output for a scenario:

```sh
eval "$(mise env)" && corepack pnpm parity -- saved-search-reserved-name --debug
```

Run every discovered parity spec through the targeted wrapper when you need to
compare wrapper behavior, while still treating the full parity gates as
authoritative before handoff:

```sh
eval "$(mise env)" && corepack pnpm parity -- --all
```

## Notes

- The command runs the Rust-backed parity path exposed by the root package scripts.
- Scenario lookup uses the spec's `scenarioId` field under
  `config/parity-specs/`.
- Failure output names the spec path and the Rust parity assertion or runner
  error.
- Full handoff validation for runtime or conformance changes still follows the
  ticket workpad and repository guidance, usually including
  `corepack pnpm conformance:check`, `corepack pnpm rust:test`, and
  `corepack pnpm typecheck`.

## Setup State Guardrail

Do not repair a parity failure by adding or restoring pre-seeded proxy state.
Specs and fixtures must not introduce `proxyRequest.localSetups`, `baseState`,
`stagedState`, setup-state JSON, hidden runner hooks, or private `DraftProxy`
store patching.

When a scenario needs existing Shopify resources, use the production-like path:
model the prerequisite through public GraphQL requests, or have the operation
handler fetch the needed upstream slice and record that call in the fixture's
`upstreamCalls` cassette with `corepack pnpm parity:record <scenario-id>`.
`dumpState`, `restoreState`, and `POST /__meta/restore` are valid only for
their documented meta API/runtime-test surfaces, not as parity fixture setup
shortcuts.
