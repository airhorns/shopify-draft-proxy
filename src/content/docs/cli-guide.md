---
title: CLI Guide
description: Repository command reference for building, running, validating, and recording.
---

There is no standalone product CLI binary yet. Day-to-day command-line usage is through root `package.json` scripts and Gleam commands.

Use `corepack pnpm ...` in unattended or CI-like environments.

## Docs Site

```sh
corepack pnpm docs:dev
corepack pnpm docs:build
corepack pnpm docs:preview
```

`docs:dev` starts the local Starlight development server. `docs:build` runs Astro's content/config check and builds the static docs site. `docs:preview` serves the built docs locally.

## Local Service

```sh
corepack pnpm dev
corepack pnpm build
corepack pnpm start
```

`dev` builds the Gleam JavaScript target and starts the TypeScript watch server under `js/`. `build` compiles the Gleam JavaScript target and the TypeScript shim. `start` runs `js/dist/server.js`, so run `build` first.

## Core Validation

```sh
corepack pnpm lint
corepack pnpm typecheck
corepack pnpm test
```

`lint` runs oxlint, oxfmt, and Gleam format checks. `typecheck` builds the JS shim and runs the root TypeScript check. `test` runs the targeted Vitest integration and unit suite configured for the repository.

## Gleam Runtime

```sh
corepack pnpm gleam:build
corepack pnpm gleam:test
corepack pnpm gleam:test:js
corepack pnpm gleam:test:erlang
corepack pnpm gleam:format
corepack pnpm gleam:format:check
```

The runtime is authored in Gleam under `src/shopify_draft_proxy/` and is expected to compile to both JavaScript and Erlang.

## JavaScript and Elixir Smokes

```sh
corepack pnpm gleam:smoke:js
corepack pnpm elixir:smoke
```

The JavaScript smoke validates the package shim. The Elixir smoke exports an Erlang shipment and runs the checked-in wrapper tests under `elixir_smoke/`.

## Parity and Conformance

```sh
corepack pnpm parity:run
corepack pnpm parity -- <scenario-id>
corepack pnpm conformance:check
corepack pnpm conformance:status
corepack pnpm conformance:capture
```

Parity scenarios in `config/parity-specs/**` replay captured Shopify interactions through the Gleam parity runner. Conformance scripts and captures are used to collect new real-Shopify evidence when a domain needs behavior proof.

Live capture commands require valid conformance credentials from `scripts/shopify-conformance-auth.mts`; do not read Shopify conformance tokens directly from a repository `.env` file.

## Credential Utilities

```sh
corepack pnpm conformance:probe
corepack pnpm conformance:auth-link
corepack pnpm conformance:exchange-auth -- '<full callback url>'
corepack pnpm conformance:refresh-auth
```

The canonical credential file is `~/.shopify-draft-proxy/conformance-admin-auth.json`. Probe before recording if the effective store is surprising.
