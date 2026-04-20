---
name: shopify-conformance-expansion
description: Extend Shopify Admin GraphQL conformance coverage in shopify-draft-proxy. Use when adding or updating conformance fixtures, parity specs, parity request GraphQL files, recorder scripts, or documentation for Shopify API fidelity scenarios.
---

# Shopify Conformance Expansion

## Core Rule

Treat conformance as the fidelity source of truth. Do not guess Shopify behavior when a safe live capture or existing fixture can answer it. Preserve the project goal: product-first Shopify Admin GraphQL draft proxy behavior, not a generic mock server.

## Required Workflow

1. Read `AGENTS.md`, `docs/original-intent.md`, and `docs/architecture.md`.
2. Read `docs/hard-and-weird-notes.md` only when making or changing a Shopify fidelity assumption.
3. Use Linear for operation/project tracking; do not recreate `docs/shopify-admin-worklist.md`.
4. Identify the root operation and whether it is already implemented in `config/operation-registry.json`.
5. Add or update runtime tests for proxy behavior before relying on parity evidence.
6. Add or update exactly one parity spec for each scenario under `config/parity-specs/`.
7. Add proxy replay files under `config/parity-requests/` when the scenario can be replayed locally.
8. Add live captures under `fixtures/conformance/<store-domain>/<api-version>/` only when credentials and store safety allow it.
9. Make sure every root operation in the parity spec's `operationNames` exists in `config/operation-registry.json`.
10. Add new helper scripts as TypeScript and run them with `tsx` or an equivalent TypeScript runner.

## Scenario Convention

Do not add a central scenario manifest. Standard scenarios are discovered by convention from `config/parity-specs/*.json`.

Each parity spec must carry the scenario metadata:

- `scenarioId`: stable id for this parity scenario.
- `operationNames`: root Shopify operations covered by the scenario.
- `scenarioStatus`: `planned` until live capture exists, then `captured`.
- `assertionKinds`: what confidence the scenario builds, such as `payload-shape`, `user-errors-parity`, or `downstream-read-parity`.
- `liveCaptureFiles`: empty for planned scenarios, fixture paths for captured scenarios.
- `proxyRequest`: `documentPath` and `variablesPath` when replay through the proxy is scaffolded; `null` when capture-only or not ready.
- `comparison`: strict JSON comparison contract for captured scenarios that are ready to execute.
- `notes`: concise fidelity findings, blockers, or promotion criteria.

Explicit scenario override config is only for unusual cases that cannot fit this parity spec shape. Avoid it for normal expansion.

## Confidence Ladder

Use the strongest feasible evidence:

1. Runtime tests prove local staging/overlay behavior.
2. Planned parity specs make unsupported live capture gaps explicit.
3. Proxy request files make local replay deterministic.
4. Captured live fixtures settle Shopify payload shape, nullability, ordering, timestamps, and user errors.
5. `conformance:check` runs the repo's Vitest structural checks for discovered scenarios.
6. `conformance:parity` reports replay readiness and executes strict comparison scenarios.

## Validation

Always run:

```bash
corepack pnpm conformance:check
corepack pnpm conformance:parity
corepack pnpm typecheck
```

Run targeted Vitest files for the changed operation or conformance wiring. Run `corepack pnpm test` before handoff when the change is broader than one isolated fixture/spec.

## Safety

Never run live mutation capture against a store unless the repo docs and credentials indicate it is safe to mutate. Supported runtime mutations must remain staged locally; conformance capture is a separate live-dev-store workflow.
