# GLEAM_PORT_INTENT.md

This file is the source of truth for *why* the Gleam port exists, *what* it
must preserve from the legacy TypeScript implementation, and *how we know it
is succeeding*. It is read-first context for any agent (human or otherwise)
joining the port mid-flight.

It deliberately does not enumerate steps; the phase plan lives in conversation
history and the per-domain skill (`.agents/skills/gleam-port/SKILL.md`, written
in Phase 6). This file outlives any specific phase.

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

The port is a *re-implementation*, not a refactor. Everything in this list is
non-negotiable; if a Gleam design choice would break one of them, the design
choice is wrong.

1. **The proxy's domain rules are unchanged.** Every guarantee in
   `AGENTS.md` (don't send supported mutations to Shopify at runtime, keep
   raw mutations for commit, match Shopify's empty/no-data behaviour, etc.)
   applies to the Gleam port verbatim.
2. **Parity specs and conformance fixtures stay byte-identical.**
   `config/parity-specs/**` and `fixtures/conformance/**` are not rewritten.
   Only the runner that consumes them is reimplemented.
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

To keep the port tractable, these are *not* part of the port:

- Rewriting the conformance capture scripts (`scripts/capture-*.mts`). They
  produce fixture JSON that both implementations consume; they do not run at
  proxy runtime.
- Rewriting `shopify-conformance-app/`.
- Changing the GraphQL surface, error semantics, or `userErrors` shapes that
  the TypeScript proxy currently emits.
- Maintaining feature parity between the two implementations *after* a domain
  is fully ported. Once a domain reaches parity in Gleam, the TypeScript
  version is removed; the Gleam version is then the authority.
- A reimplementation in Rust, Roc, or any other language. Gleam is the
  decision.

## How we know we are succeeding

The port has *two* dimensions of progress: **substrate** (the runtime that
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
      reports the same "skipped vs runnable vs failing" partition as the
      TypeScript runner does for the corresponding domains.
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
- [ ] The TypeScript implementation of that domain (`src/proxy/<domain>.ts`,
      its slice of `src/state/types.ts`, its slice of `src/state/store.ts`,
      its dispatcher entry in `src/proxy/routes.ts`) is **deleted**. Both
      implementations are not maintained side-by-side after parity.
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
- **Both implementations live until a domain is fully ported.** A
  half-ported domain in Gleam is a hazard if the TypeScript version is
  already deleted. Delete only after parity.
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

## Spike findings (2026-04-28)

A first viability spike has run end-to-end through Gleam: HTTP-shaped
request → JSON body parse → custom GraphQL parser → operation summary
→ events-domain dispatcher → empty-connection serializer → JSON
response. 98 gleeunit tests pass on both `--target erlang` and
`--target javascript`. The port is concrete enough now to surface real
strengths and risks rather than speculate.

### What is ported and working

| Module                            | LOC  | TS counterpart                  |
| --------------------------------- | ---- | ------------------------------- |
| `graphql/source` + `location`     | ~80  | `language/source`, `location`   |
| `graphql/token_kind` + `token`    | ~70  | `language/tokenKind`, `tokenKind`|
| `graphql/character_classes`       | ~60  | `language/characterClasses`     |
| `graphql/lexer`                   | ~530 | `language/lexer`                |
| `graphql/ast`                     | ~140 | `language/ast` (executable subset) |
| `graphql/parser`                  | ~720 | `language/parser`               |
| `graphql/parse_operation`         | ~100 | `graphql/parse-operation`       |
| `graphql/root_field`              | ~200 | `graphql/root-field`            |
| `state/synthetic_identity` + FFI  | ~180 | `state/synthetic-identity`      |
| `proxy/graphql_helpers` (slice)   | ~110 | `proxy/graphql-helpers` (15%)   |
| `proxy/events`                    | ~80  | `proxy/events`                  |
| `proxy/draft_proxy` (skeleton)    | ~190 | `proxy-instance` + `proxy/routes` (skeleton) |

Roughly **2.5K LOC of Gleam** replacing roughly the same TS surface,
with FFI proven on both targets via the ISO timestamp helpers.

### Strengths

- **Sum types + exhaustive matching catch GraphQL shape bugs at
  compile time.** Adding a new `Selection` variant (e.g.
  `InlineFragment`) makes every consumer fail to compile until it
  decides what to do — exactly the property the proxy needs to keep
  null-vs-absent handling honest.
- **`Result`-threaded parsing replaces graphql-js's mutable lexer
  cleanly.** The recursive descent reads as well as the TS original;
  the immutable state threading didn't add meaningful boilerplate
  beyond `use … <- result.try(…)`.
- **Cross-target parity is real.** Every test passes on both BEAM and
  JS, including FFI-bound timestamp formatting. The platform-specific
  cost was small (one `.erl` + one `.mjs` file, ~10 lines each).
- **Public API translates 1:1.** `process_request(request) ->
  (response, proxy)` mirrors the TS `processRequest`, with the
  registry threaded explicitly to preserve immutability — no design
  compromise required.

### Risks and open questions

- **Store + types is the long pole.** `src/state/store.ts` is 5800
  lines with 449+ methods; `src/state/types.ts` is 2800 lines of
  resource record definitions. This is the single biggest porting
  cost and was deliberately deferred in the spike. It will dominate
  the calendar; the events handler skipped the store entirely because
  events are read-only and always empty in the proxy. Most other
  domains will not have that escape hatch.
- **Deep generic helpers like `serializeConnection<T>` need a different
  shape in Gleam.** The TS version takes callbacks (`serializeNode`,
  `getCursorValue`) and is reused across every connection-shaped
  field. In Gleam, parametric polymorphism handles this, but the
  number of arguments grows quickly; the spike sidestepped by
  specializing for the empty-items case. For real domains we'll need
  a more carefully designed connection helper, possibly with a
  configuration record instead of positional callbacks.
- **Mutable-API ergonomics.** Threading the proxy through every call
  is correct but verbose. The right pattern long-term is probably a
  `gleam_otp` actor that owns the registry + store, with handlers
  that send messages — but that's only worth introducing when there's
  enough state to justify it. For now the explicit threading is fine
  and matches Gleam idioms.
- **No date/time stdlib.** ISO 8601 formatting requires FFI; this is
  per-target boilerplate that scales linearly with the number of
  date/time operations. Manageable, but a friction point.
- **Block strings, descriptions, schema definitions deliberately
  omitted from the parser.** Operation documents in
  `config/parity-requests/**` don't use them — but if any future
  Shopify client introduces block string arguments the parser will
  need extending. Documented as a known gap in `lexer.gleam` /
  `parser.gleam`.

### Recommendation

Continue the port. The substrate is sound; the GraphQL parser is the
hardest subjective port (4 of the 12 substrate modules) and it landed
without surprises. The next bottleneck is mechanical: porting
`state/types.ts` resource records and the corresponding slices of
`state/store.ts`, one domain at a time. Start with `delivery-settings`
or `saved-searches` — both are small and have minimal store coupling
— before tackling `customers` or `products`.

---

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
