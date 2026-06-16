# AGENTS.md

Guidance for future AI coding agents working in `shopify-draft-proxy`.

## Primary mission

Preserve the intent in `docs/original-intent.md`.

This project is a **Shopify Admin GraphQL digital twin / draft proxy**, not a generic mock server. The goal is to let tests stage realistic Shopify writes locally and observe downstream reads as if those writes happened, without mutating the real store during normal supported mutation handling.

## Current runtime

- The runtime is **Rust**, centered on `src/proxy.rs`, `src/graphql.rs`, `src/operation_registry.rs`, and `src/bin/shopify-draft-proxy-server.rs`.
- The TypeScript package surface in `js/src/` is a thin embeddable shim around the Rust HTTP runtime.
- Operation registry metadata lives in `src/operation_registry.rs`; TypeScript tooling reads it through the Rust `operation-registry-json` exporter so there is no second checked-in registry source.
- No Gleam source or build steps remain. Do not reintroduce Gleam runtime code, BEAM/Elixir smoke tests, or Gleam build requirements.

## Non-negotiables

1. **Admin API breadth with domain depth**
   - The proxy should grow toward high-fidelity emulation across the Shopify Admin GraphQL API, not remain product-centric.
   - Do not claim a root is supported until its local lifecycle behavior and downstream read-after-write effects are modeled for that domain.

2. **Domain fidelity over hacks**
   - Prefer modeling Shopify domain behavior over brittle response patching.
   - If uncertain, add a conformance test against a real dev store.
   - **Never return canned responses keyed to a conformance document.** Runtime handlers must compute every response from the proxy store model (`self.store.staged`). It is cheating to inspect the incoming GraphQL document — e.g. `query.contains("SomeScenarioName")`, an `is_*_document(...)` predicate, or a `*_fixture_data(...)` helper — and reply with a hardcoded literal or `include_str!`/baked JSON payload that satisfies one specific conformance scenario without modeling the operation. A handler that ignores store state and returns a fixed payload passes a single check while implementing nothing. Such request-sniffing-plus-canned-reply code is temporary scaffolding to be deleted, not an acceptable implementation; do not add more of it, and convert existing instances to real store-backed handling.

3. **Do not send supported mutations to Shopify at runtime**
   - Supported mutations must stage locally.
   - Unsupported mutations may proxy through, but must be visible in logs/observability.
   - `POST /__meta/commit` is expected to replay the original staged mutations to Shopify, and live conformance work may deliberately mutate disposable test shops to record faithful fixtures.
   - Do not register operations as permanent passthrough capabilities. A registered operation is a commitment to model it locally before it is considered supported.
   - The operation registry's `implemented` flag means only that the proxy handles the operation locally instead of 501-ing — it spans every locally-handled root field, including document-gated special-case handlers, and is decoupled from capability routing (the uniform table dispatch keys on `LOCAL_DISPATCH_ROOTS`, not on the flag). `implemented` is **not** a fidelity claim: a canned handler (see rule 2) is `implemented` but unsupported, and exists only until it is converted to real store-backed handling.
   - "Supported" is the higher bar and is tracked separately by declared `runtimeTests` plus captured conformance coverage. A mutation root reaches supported status only when the local model can emulate the operation's lifecycle behavior and downstream read-after-write effects from the store, without runtime Shopify writes and without branch-only, validation-only, or canned responses.

4. **Keep original raw mutations for commit**
   - Commit should replay original mutations in original order.

5. **Match Shopify's empty/no-data behavior**
   - In snapshot mode and local reads, prefer the same null/empty structures Shopify returns when the backend lacks data.

6. **Docs must stay current**
   - Update `docs/architecture.md` if runtime architecture changes.
   - Put endpoint-specific quirks, field behavior, coverage notes, and conformance minutia in `src/content/docs/endpoints/<group>.md`, not in the high-level architecture doc.
   - Update `docs/original-intent.md` only if the project goal truly changes.

7. **Keep ticket identifiers out of executable artifacts**
   - Do not embed Linear issue identifiers in code, GraphQL operation names, capture scripts, fixture data, or generated resources.
   - Markdown docs may mention issue identifiers when they are necessary for specific historical context.

## Development rules

- Keep runtime state instance-owned. `DraftProxy` owns its store, registry, synthetic identity, mutation log, and injectable transports; do not introduce global mutable proxy state.
- Preserve Shopify-like versioned routes.
- Forward auth headers unchanged to upstream Shopify.
- Expose and test the meta API.
- Add tests for every supported operation.
- Prefer conformance fixtures over hand-wavy comments about expected behavior.
- Conformance/test shops are disposable fidelity targets. When a task needs real Shopify evidence, agents may modify those test shops deeply as needed to create, update, activate, delete, and clean up realistic fixtures.
- Do not add tests that only reassert self-evident properties of checked-in metadata. Test executable behavior, schema validation, discovery semantics, or comparison contracts instead.
- Do not add planned-only, blocked-only, or capture-only parity scenarios as a shortcut for implementation. Checked-in parity specs must be backed by captured interactions and executable evidence.
- Do not hand-author or synthetically generate checked-in conformance fixture data, parity fixtures, or expected payloads as Shopify fidelity evidence.
- Do not prove parity by loading or patching internal proxy state directly. Parity setup must run through the same public request surface an agent would use: GraphQL mutations/queries, uploads, or documented meta APIs only when the scenario is specifically about those meta APIs. Internal state dumps/restores, setup-state JSON, hidden runner hooks, or fixtures that seed `baseState`/`stagedState` are test shortcuts, not Shopify fidelity evidence.
- Protected parity evidence must not drift without explicit user approval:
  - `config/parity-specs/**`
  - `config/parity-requests/**`
  - `fixtures/conformance/**`
- For parity comparisons, prefer comparing the whole selected resource payload and carving out explicit volatile paths such as IDs, timestamps, cursors, and throttle metadata.
- Treat `expectedDifferences` as a last resort after the proxy's operation handlers have been adjusted to compute the right response.
- Do not change `scripts/parity-run.ts` or other parity/conformance comparison harness code to soften expectations, broaden cassette matching, ignore mismatches, or allow synthesized/canned evidence to pass. If recordings are wrong, re-record them from Shopify; if the proxy does not match recordings, fix the implementation or the captured fixture source.
- Before handing off a fidelity PR, ensure there is executable evidence for behavior claims. Integration/unit tests prove local code paths; parity/conformance evidence proves Shopify fidelity.
- When changing Shopify-fidelity behavior, map every changed validation branch, payload shape, or read-after-write effect to an explicit captured parity comparison target in the same PR. An existing scenario file only counts when the exact target already asserts that branch and the workpad records the target name; adjacent coverage or an unchanged broad scenario is not enough.
- Build/recording scripts must be TypeScript (`.ts` / `.mts`) executed with `tsx` or similar. Do not add `.mjs` files anywhere in this repository.
- Relative TypeScript import specifiers must use the emitted JavaScript extension that TypeScript expects for NodeNext output (`.js` for `.ts`, `.mjs` for `.mts`, `.cjs` for `.cts`).
- Do not add files to linter/formatter ignore lists just because formatting changes fixtures or parity requests. Format the files, then fix affected tests, captures, specs, or code.
- In unattended or CI-like workspaces, prefer `corepack pnpm ...` for package scripts.
- Before adding a resource-local parser, serializer, scalar reader, projection helper, or metafield/search/connection utility, read `docs/helpers.md` and search for an existing shared helper.
- Do not hardcode captured Shopify resource IDs in runtime implementation to make one fixture pass. Fixture/shop IDs may appear in tests, docs, parity requests, and recorded evidence, but production handlers must derive IDs from request arguments, staged/base store state, observed upstream responses, or synthetic ID allocators.

## Verification loop

Run the full Rust-port gate before pushing:

```bash
corepack pnpm conformance:fixture-invariants
corepack pnpm rust:fmt
corepack pnpm rust:clippy
corepack pnpm rust:test
git diff --check
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm conformance:check
corepack pnpm conformance:capture:check
corepack pnpm conformance:status -- --output-json .conformance/current/conformance-status-report.json --output-markdown .conformance/current/conformance-status-comment.md
corepack pnpm build
corepack pnpm test
```

Then push and watch GitHub Actions to completion.

## GitHub repository

- The canonical GitHub repository is `airhorns/shopify-draft-proxy`.
- Open pull requests against `airhorns/shopify-draft-proxy`; do not target personal forks.
- If a workspace remote points at a personal fork, retarget it to `git@github.com:airhorns/shopify-draft-proxy.git` before pushing or creating a PR.
- If `gh pr edit` fails with GitHub's Projects classic / `projectCards` deprecation path, use narrower `gh api` calls for the specific metadata update, such as REST label updates or GraphQL `updatePullRequest`, then verify the PR state with `gh pr view`.

## Shopify conformance auth rule

- Do **not** read `SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN` from repo `.env` in scripts anymore.
- All live conformance scripts must get credentials through `scripts/shopify-conformance-auth.mts`.
- The canonical credential file is `~/.shopify-draft-proxy/conformance-admin-auth.json`.
- The checked-in Shopify app copy lives at `shopify-conformance-app/hermes-conformance-products/`; helper scripts prefer that repo-local app over the legacy `/tmp/shopify-conformance-app/...` copy when it exists.
- `getValidConformanceAccessToken(...)` is the single entry point for token access. It probes the stored access token, refreshes it when possible, and throws a clear error when the stored credential is missing or unrecoverable.
- Workspace `.env` files must not be stale copies of `.env.example`; placeholder `SHOPIFY_CONFORMANCE_STORE_DOMAIN` / `SHOPIFY_CONFORMANCE_ADMIN_ORIGIN` values will make a valid home-folder token look invalid.
- If a task requires recording or re-recording conformance evidence and `getValidConformanceAccessToken(...)` / `corepack pnpm conformance:probe` cannot produce a valid live credential after the documented repair paths, do not commit code, push a branch, or open a PR. Record the blocker in the appropriate workpad and move the issue to Human Review.

## Suggested workflow

1. Read `docs/original-intent.md`.
2. Read `docs/architecture.md`.
3. Know that `docs/helpers.md` exists; read it before adding or duplicating shared proxy/helper utilities.
4. Know that `docs/hard-and-weird-notes.md` exists; search or read the relevant parts when fidelity assumptions or unusual Shopify behavior matter, and add to it when new hard/weird behavior is discovered.
5. Add/adjust tests before implementation.
6. Update docs after shipping behavior.
