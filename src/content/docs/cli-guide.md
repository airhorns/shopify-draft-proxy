---
title: CLI Guide
description: Repository command reference for building, running, validating, and recording.
---

There is no standalone product CLI binary yet. Day-to-day command-line usage is through root `package.json` scripts, Cargo, and TypeScript helper scripts.

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

`dev` and `start` run `cargo run --bin shopify-draft-proxy-server --quiet`. `build` compiles the Rust HTTP server and then builds the TypeScript shim under `js/`.

## Core Validation

```sh
corepack pnpm lint
corepack pnpm typecheck
corepack pnpm test
```

`lint` runs oxlint and oxfmt. `typecheck` builds the JS shim and runs the root TypeScript check. `test` runs the targeted Vitest integration and unit suite configured for the repository.

## Rust Runtime

```sh
corepack pnpm rust:fmt
corepack pnpm rust:test
corepack pnpm rust:check
corepack pnpm rust:clippy
```

The runtime is authored in Rust under `src/`. The server entry point is `src/bin/shopify-draft-proxy-server.rs`, and core request handling lives in `src/proxy.rs`, `src/graphql.rs`, and `src/operation_registry.rs`.

## JavaScript Shim

```sh
corepack pnpm --dir js build
```

The JavaScript package under `js/` is a thin embeddable shim around the Rust HTTP runtime. Build it directly when you are changing package types or process-launch behavior.

## Parity and Conformance

```sh
corepack pnpm parity:run
corepack pnpm parity -- <scenario-id>
corepack pnpm conformance:check
corepack pnpm conformance:status
corepack pnpm conformance:capture
```

Parity scenarios in `config/parity-specs/**` replay captured Shopify interactions through the parity runner. Conformance scripts and captures are used to collect new real-Shopify evidence when a domain needs behavior proof.

Live capture commands require valid conformance credentials from `scripts/shopify-conformance-auth.mts`; do not read Shopify conformance tokens directly from a repository `.env` file.

## Credential Utilities

```sh
corepack pnpm conformance:probe
corepack pnpm conformance:auth-link
corepack pnpm conformance:exchange-auth -- '<full callback url>'
corepack pnpm conformance:refresh-auth
```

The canonical credential file is `~/.shopify-draft-proxy/conformance-admin-auth.json`. Probe before recording if the effective store is surprising.
