---
name: gleam-runtime-validation
description: Use when running or debugging shopify-draft-proxy Gleam runtime validation, parity tests, Erlang/JavaScript target checks, host OTP setup, Thompson OTP 25/OTP 28 issues, stale BEAM artifacts, escript/gleam_json failures, or deciding whether local Erlang tooling is a real blocker.
---

# Gleam Runtime Validation

Use this skill before running or diagnosing repository validation for the
Gleam runtime in `shopify-draft-proxy`.

## Ground Rules

- The runtime is under `src/shopify_draft_proxy/` and must stay portable across
  JavaScript and Erlang/BEAM.
- Use `corepack pnpm ...` for package scripts in unattended workspaces.
- Do not treat host Erlang/OTP 25 as a human blocker. This repo pins an OTP 28
  toolchain through `.mise.toml`.
- Run both targets for runtime changes unless the ticket explicitly narrows the
  scope to docs or non-runtime bookkeeping.

## Required Reading

- `docs/original-intent.md`
- `docs/architecture.md`
- `docs/gleam-runtime.md` for public runtime API and embedding details
- `docs/GLEAM_PORT_LOG.md` only when old porting decisions or historical
  validation notes matter

## Host Toolchain Check

Check the active OTP before Erlang-target validation:

```sh
erl -eval 'erlang:display(erlang:system_info(otp_release)), halt().' -noshell
```

If `erl` is missing or reports OTP 25 on Thompson, use the checked-in mise
toolchain from the repository root:

```sh
mise trust .mise.toml
mise install
eval "$(mise activate bash)"
```

Then verify `erl` reports OTP 28 and clear stale artifacts before Erlang tests:

```sh
erl -eval 'erlang:display(erlang:system_info(otp_release)), halt().' -noshell
gleam clean
```

In unattended shells where activating mise is inconvenient, pin the existing
install for the command:

```sh
PATH=/home/airhorns/.local/share/mise/installs/erlang/28.4.2/bin:/home/airhorns/.local/share/mise/installs/gleam/1.16.0/bin:$PATH gleam clean
PATH=/home/airhorns/.local/share/mise/installs/erlang/28.4.2/bin:/home/airhorns/.local/share/mise/installs/gleam/1.16.0/bin:$PATH corepack pnpm gleam:test
```

## Validation Commands

For runtime behavior changes, prefer the repo aggregate:

```sh
corepack pnpm gleam:test
```

When isolating target-specific failures:

```sh
gleam test --target javascript
gleam test --target erlang
```

For parity/conformance changes, also use the relevant conformance skill and run
the scenario or aggregate command required by the ticket/workpad. Do not
replace required captured parity evidence with local unit tests unless a human
review explicitly grants that exception for the ticket.

## Failure Triage

- `gleam_json` reporting OTP 27+ is required: the active host Erlang is too old;
  activate or prepend the mise OTP 28 path and run `gleam clean`.
- `undef`, `escript`, or stale BEAM-module errors after switching OTP versions:
  run `gleam clean` before retrying.
- JavaScript-only or Erlang-only failures are real portability defects until a
  local code/path issue proves otherwise; do not ignore target drift.
- Docker should be a last resort for this repo. Prefer `.mise.toml` because the
  project already pins Erlang 28.4.2 and Gleam 1.16.0 for unattended hosts.

## Workpad Notes

When recording validation in Linear, include:

- active branch and short SHA
- host OTP before repair when relevant
- whether mise was activated or PATH was pinned
- `gleam clean` when stale artifacts were possible
- exact validation commands and pass/fail counts
