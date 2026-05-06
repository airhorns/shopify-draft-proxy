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
validation, `corepack pnpm gleam:test`, or CI.

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

- The command builds the JavaScript target before invoking
  `scripts/parity-run.ts`.
- Scenario lookup uses the spec's `scenarioId` field under
  `config/parity-specs/`.
- Failure output names the spec path and the Gleam parity assertion or runner
  error.
- Full handoff validation for runtime or conformance changes still follows the
  ticket workpad and repository guidance, usually including
  `corepack pnpm conformance:check`, `corepack pnpm gleam:test`, and
  `corepack pnpm typecheck`.
