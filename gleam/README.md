# Gleam port of `shopify-draft-proxy`

This directory holds the in-progress Gleam port of the Shopify Admin GraphQL
draft proxy. It compiles to both Erlang (for Elixir/BEAM consumers) and
JavaScript (for Node consumers and the existing TypeScript callers).

The port shares parity specs (`../config/parity-specs`) and recorded Shopify
fixtures (`../fixtures/conformance`) with the legacy TypeScript implementation
in `../src`. See `../docs/architecture.md` for the runtime design and
`../AGENTS.md` for the project's non-negotiables.

## Development

```sh
# Install dependencies (uses Hex via the bundled Erlang/Elixir toolchain)
gleam deps download

# Run tests on both targets
gleam test --target erlang
gleam test --target javascript
```

## Layout

- `src/` — Gleam source.
- `test/` — gleeunit tests.
- `gleam.toml` — package manifest (default target: javascript).

The package will be promoted to the repository root and the legacy TypeScript
in `../src` will be removed once domain coverage reaches parity. Until then
both implementations live side-by-side and are exercised against the same
parity specs and conformance fixtures.

## Phase 0

The current contents are a smoke-only skeleton used to validate:

1. `gleam test` passes on both `--target erlang` and `--target javascript`.
2. The JavaScript target output can be imported as ESM by Node and asserted
   from a vitest test (`../tests/integration/gleam-interop.test.ts`).
3. The Erlang target output can be loaded by an Elixir mix project and
   asserted from `mix test` (`./elixir_smoke/`).

Real domain code lands in Phase 2; see the project plan in conversation
history for the phase outline.
