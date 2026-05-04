# GLEAM_PORT_INTENT.md

This file is the source of truth for _why_ the Gleam port exists, _what_ it
must preserve from the legacy TypeScript implementation, and _how we know it
is succeeding_. It is read-first context for any agent (human or otherwise)
joining the port mid-flight.

It deliberately does not enumerate steps; the phase plan lives in conversation
history and the per-domain skill (`.agents/skills/gleam-port/SKILL.md`, written
in Phase 6). This file outlives any specific phase.

The running narrative of what has actually been ported, what was learned, and
what is now blocked or unblocked lives in `GLEAM_PORT_LOG.md`. New passes
append entries there, not here.

## Why port

`shopify-draft-proxy` is a high-fidelity Shopify Admin GraphQL digital twin.
Today it ships as a TypeScript-only embeddable library (and a Koa server in
front of it). Two near-term consumers cannot use it as-is:

1. **Elixir / Erlang services on the BEAM**, which need an in-process,
   reusable digital twin to stage Shopify writes during their own tests.
2. **Browser- and edge-resident JS** that wants the same embeddable surface
   without a Node round-trip.

A pure Gleam core, compiled to both Erlang and JavaScript, satisfies both
without duplicating domain logic. Gleam is chosen because:

- It compiles natively to both targets we need.
- It is statically typed and ML-flavoured, which fits the proxy's
  command/state-machine shape.
- The existing TypeScript implementation translates to Gleam reasonably
  cleanly: most of `src/proxy/*` is pure functions over normalized records,
  not Node-specific glue.

## What the port must preserve

The port is a _re-implementation_, not a refactor. Everything in this list is
non-negotiable; if a Gleam design choice would break one of them, the design
choice is wrong.

1. **The proxy's domain rules are unchanged.** Every guarantee in
   `AGENTS.md` (don't send supported mutations to Shopify at runtime, keep
   raw mutations for commit, match Shopify's empty/no-data behaviour, etc.)
   applies to the Gleam port verbatim.
2. **Parity specs and conformance fixtures are owned by the parity
   runner.** `config/parity-specs/**` and `fixtures/conformance/**` may
   be amended — each capture file gains an `upstreamCalls` cassette and
   legacy `seedX` keys are being removed — but the comparison contract
   (what gets compared and how) is preserved. The cassette-playback
   parity model is documented in `docs/parity-runner.md`.
3. **The public embeddable shape is preserved.** Existing TypeScript callers
   continue to import `createDraftProxy(config)`, call
   `processRequest({ method, path, headers, body })`, and receive
   `{ status, body, headers }` with the same JSON content. The Gleam
   implementation is delivered to JS callers as ESM with a thin TS shim that
   re-exports the same names and types.
4. **The Koa-mounted HTTP server keeps working** at the same routes
   (`/admin/api/:version/graphql.json`, `/__meta/...`) with the same response
   shapes. The HTTP adapter may move to Gleam (`mist` on BEAM, a small Node
   `http` shim on JS), but the route surface does not change.
5. **Mutation-log envelope, `__meta/state` shape, and snapshot file format
   stay stable** so existing fixture tooling keeps working across the port.
6. **No regressions in fidelity for already-supported domains.** A domain's
   parity specs must keep passing; the proxy's read-after-write behaviour for
   supported mutations must continue to match Shopify.

## Non-goals

To keep the port tractable, these are _not_ part of the port:

- Rewriting the conformance capture scripts (`scripts/capture-*.mts`). They
  produce fixture JSON that both implementations consume; they do not run at
  proxy runtime.
- Rewriting `shopify-conformance-app/`.
- Changing the GraphQL surface, error semantics, or `userErrors` shapes that
  the TypeScript proxy currently emits.
- Retiring the TypeScript runtime domain-by-domain during incremental port
  passes. The original TypeScript implementation and TypeScript tests remain in
  place until the final all-port cutover has verified 100% parity across
  domains, integration coverage, CI, packaging, and docs.
- A reimplementation in Rust, Roc, or any other language. Gleam is the
  decision.

## How we know we are succeeding

The port has _two_ dimensions of progress: **substrate** (the runtime that
all domains share) and **domain coverage** (per-domain endpoint groups).
Both have explicit acceptance bars.

### Substrate acceptance criteria

The substrate is "done" when, on both the Erlang and JavaScript targets:

- [ ] A `DraftProxy` value can be constructed from `AppConfig` with the same
      validation behaviour as the TypeScript `createDraftProxy`.
- [ ] `process_request` accepts a `{method, path, headers, body}` request and
      returns `{status, body, headers}` matching the TypeScript proxy's HTTP
      shape for `/admin/api/:version/graphql.json`, `/__meta/health`,
      `/__meta/config`, `/__meta/log`, `/__meta/state`, `/__meta/reset`,
      and `/__meta/commit`.
- [ ] The mutation log envelope round-trips through `dump_state` /
      `restore_state` and matches the on-disk shape produced by the
      TypeScript `DraftProxyStateDump` (schema string, version, fields).
- [ ] The synthetic identity registry produces stable IDs across a session
      and resets on `reset()`.
- [ ] The normalized snapshot loader accepts existing snapshot JSON files
      from `fixtures/snapshots/` without modification.
- [ ] The GraphQL parser (ported from `graphql-js`) accepts every operation
      document referenced by `config/parity-requests/**` without error and
      classifies operation type and root fields identically to the
      TypeScript `parse-operation.ts` output for those documents.
- [ ] The parity runner (gleeunit-driven) loads every spec under
      `config/parity-specs/**`, recognises the comparison-mode contract, and
      treats every runner error or comparison mismatch as a hard failure.
- [ ] All three interop boundaries are green: `gleam test --target erlang`,
      `gleam test --target javascript`, the Node ESM smoke
      (`tests/integration/gleam-interop.test.ts`), and the Elixir mix smoke
      (`gleam/elixir_smoke/`).

### Per-domain acceptance criteria

A domain (e.g. `events`, `saved-searches`, `products`) is "ported" when:

- [ ] Every parity spec under `config/parity-specs/<domain>/` that runs
      against the TypeScript proxy also runs against the Gleam proxy and
      passes, with **byte-identical** comparison semantics (same expected
      differences, same strict-JSON behaviour).
- [ ] The corresponding integration tests in `tests/integration/<domain>-*`
      have been ported to gleeunit and pass on both targets. Test names and
      assertions remain semantically equivalent; gleeunit idioms replace
      vitest idioms but coverage of behaviour does not shrink.
- [ ] The original TypeScript implementation and TypeScript tests for that
      domain remain intact while the broader port is still incremental. Per-
      domain parity proves the Gleam surface is ready; it does not authorize
      deleting `src/proxy/<domain>.ts`, TypeScript state/store slices,
      dispatcher wiring, TypeScript integration tests, or TypeScript
      conformance/parity runner coverage before final all-port cutover.
- [ ] The interop smoke tests still pass — i.e. nothing in the Gleam port
      has broken JS or BEAM consumers' ability to load and call the package.
- [ ] A "porting note" entry is added to `.agents/skills/gleam-port/SKILL.md`
      capturing anything non-obvious that future ports of similar domains
      should know.

### Whole-port acceptance criteria

The port is "complete" when:

- [ ] All endpoint groups currently registered in
      `config/operation-registry.json` with `implemented: true` are ported
      and their TypeScript implementations deleted.
- [ ] `src/` contains no proxy domain code. `gleam/` is promoted to the repo
      root (renamed back to `src/`/`test/` conventions).
- [ ] The Koa server is replaced by the Gleam HTTP adapter on both targets,
      with no behavioural change to consumers.
- [ ] A real Elixir consumer can `mix deps.get` the Gleam package from Hex
      (or a path dep) and exercise a productCreate → product read →
      `__meta/commit` lifecycle.
- [ ] A real Node consumer can `pnpm add shopify-draft-proxy` and exercise
      the same lifecycle through the JS shim.
- [ ] The TypeScript-only conformance capture scripts continue to work
      against the Gleam proxy as their staging target if so configured.

## Working principles for agents driving the port

- **Parity specs are the oracle.** When the TypeScript and Gleam
  implementations disagree, run the relevant parity spec; whichever matches
  the Shopify capture wins. Do not "improve" Shopify's behaviour.
- **Both implementations live until final all-port cutover.** A
  half-ported domain in Gleam is a hazard if the TypeScript version is
  already deleted. Keep the original TypeScript implementation and tests
  intact until the final all-port cutover proves 100% parity across the whole
  repository.
- **Type-driven, not opportunistic.** When porting a domain, port the
  state types first, then the read paths, then the mutations, then commit
  behaviour. Do not interleave domains.
- **Diff against Shopify, not against TypeScript.** The TypeScript
  implementation has bugs. The recorded fixtures and parity specs do not.
  When ambiguity arises, look at the capture, not at `src/proxy/...`.
- **Avoid clever Gleam.** Gleam's expressiveness can hide GraphQL
  null-vs-absent and array-vs-edges-vs-nodes pitfalls behind nice-looking
  pattern matches. When in doubt, write the dumb explicit version.
- **Preserve mutation log byte-shape.** Anything serialised into the
  mutation log must round-trip identically; this is what enables commit
  replay against real Shopify.
- **One target green is not enough.** Every change must be exercised on
  both `--target erlang` and `--target javascript`; behaviour drift between
  the two is the most expensive bug class to find later.

## Open architectural decisions deferred until they bind

These are real choices the port will make, but each can defer until the
relevant domain forces it. Do not pre-design.

- **Logging.** Pino is TS-only. The Gleam port likely emits structured logs
  via a small per-target adapter (`logger.gleam` with FFI to Pino on JS and
  to `:logger` on BEAM). Decide once a domain needs structured logging.
- **HTTP server.** Koa cannot run on BEAM. Replace with `mist` on Erlang and
  a thin Node `http` adapter on JavaScript. Decide once domain coverage is
  large enough to justify cutting Koa.
- **Upstream HTTP client.** `fetch` exists natively on both targets via the
  shape Gleam already abstracts (`gleam_http`). The exact client choice can
  be picked when the first live-hybrid domain is ported, not before.
- **Search query parser** (`src/search-query-parser.ts`). Port lazily — only
  when a domain that uses it is ported.
- **Zod equivalents.** `gleam/dynamic` decoders cover the same role; how
  much shared decoder infrastructure to extract is decided after the third
  domain is ported, not before.
