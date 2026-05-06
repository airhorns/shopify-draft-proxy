# AGENTS.md

Guidance for future AI coding agents working in `shopify-draft-proxy`.

## Primary mission

Preserve the intent in `docs/original-intent.md`.

This project is a **Shopify Admin GraphQL digital twin / draft proxy**, not a generic mock server. The goal is to let tests stage realistic Shopify writes locally and observe downstream reads as if those writes happened, without mutating the real store during normal supported mutation handling.

## Non-negotiables

1. **Admin API breadth with domain depth**
   - The proxy should grow toward high-fidelity emulation across the Shopify
     Admin GraphQL API, not remain product-centric.
   - Do not claim a root is supported until its local lifecycle behavior and
     downstream read-after-write effects are modeled for that domain.

2. **Domain fidelity over hacks**
   - Prefer modeling Shopify domain behavior over brittle response patching.
   - If uncertain, add a conformance test against a real dev store.

3. **Do not send supported mutations to Shopify at runtime**
   - Supported mutations must stage locally.
   - Unsupported mutations may proxy through, but must be visible in logs/observability.
   - This runtime rule does not prohibit intentional Shopify writes in other
     phases. `__meta/commit` is expected to replay the original staged
     mutations to Shopify, and live conformance work may deliberately mutate
     disposable test shops to record faithful fixtures.
   - Do not register operations as permanent passthrough capabilities. A
     registered operation is a commitment to model it locally before it is
     considered supported; passthrough is only the unknown/unsupported escape
     hatch, not an intended execution posture for a known operation.
   - Do not mark branch-only, validation-only, or otherwise partial mutation
     handling as implemented operation support. Half support is not enough:
     a mutation root reaches supported status only when the local model can
     emulate the operation's supported lifecycle behavior and downstream
     read-after-write effects without runtime Shopify writes. Captured
     validation guardrails are useful evidence, but they must be documented as
     guardrails rather than presented as full local emulation.

4. **Keep original raw mutations for commit**
   - Commit should replay original mutations in original order.

5. **Match Shopify's empty/no-data behavior**
   - In snapshot mode and local reads, prefer the same null/empty structures Shopify returns when the backend lacks data.

6. **Docs must stay current**
   - Update `docs/architecture.md` if runtime architecture changes.
   - Put endpoint-specific quirks, field behavior, coverage notes, and
     conformance minutia in `docs/endpoints/<group>.md`, not in the high-level
     architecture doc.
   - Update `docs/original-intent.md` only if the project goal truly changes.

7. **Keep ticket identifiers out of executable artifacts**
   - Do not embed Linear issue identifiers in code, GraphQL operation names,
     capture scripts, fixture data, or generated resources.
   - Markdown docs may mention issue identifiers when they are necessary for
     specific historical context, but generally avoid them unless describing a
     particular change made at a particular time.

## Development rules

- The runtime is **Gleam**, under `src/shopify_draft_proxy/`,
  compiling to both Erlang/BEAM and JavaScript. Build/recording/registry
  scripts under `scripts/` are TypeScript (`tsx`).
- Keep runtime state in memory unless the project scope explicitly expands.
- `DraftProxy` is a value, not a stateful service. Each request returns
  the next proxy alongside the response (`process_request(proxy,
request) -> #(Response, DraftProxy)`). Embedders own the value;
  there is no ambient runtime context, no process-wide singleton.
- Domain handlers receive the instance-owned store and synthetic
  identity through the proxy value passed into them. Do not introduce
  any mechanism — JS module globals, BEAM `persistent_term`, JS
  `AsyncLocalStorage`, etc. — that would let a handler reach store or
  synthetic identity without going through the proxy value.
- Preserve Shopify-like versioned routes.
- Forward auth headers unchanged to upstream Shopify.
- Expose and test the meta API.
- Add tests for every supported operation.
- Prefer conformance fixtures over hand-wavy comments about expected behavior.
- Conformance/test shops are disposable fidelity targets. When a ticket needs
  real Shopify evidence, agents may modify those test shops deeply as needed
  to create, update, activate, delete, and clean up realistic fixtures; do not
  avoid necessary live setup just because it changes test-shop data.
- Do not limit conformance captures to validation-only branches solely because
  the success path has Shopify side effects. If the store and credentials are
  suitable, capture the success path with explicit setup and cleanup evidence.
- When a conformance scenario needs store state that does not already exist,
  create it deliberately in the recorder or setup path, then clean it up. Do
  not downgrade to capture-only or validation-only evidence just because setup
  requires extra Admin API calls or realistic test-shop mutations.
- Do not add tests that only reassert self-evident properties of checked-in
  metadata, such as exact fields in registry or parity-spec JSON files. Test
  executable behavior, schema validation, discovery semantics, or comparison
  contracts instead.
- For coverage-map-only registry work, do not add tests whose only signal is
  that specific roots exist in the operation registry, have a
  particular `domain`, `implemented: false`, empty `runtimeTests`, or also
  appear in the checked-in root introspection fixture. Those assertions are
  just restating the edited JSON. Use existing schema/conformance discovery
  checks unless the change introduces executable behavior or a real discovery
  contract.
- Do not add new planned-only or blocked-only parity scenarios as a way to
  reserve future coverage. Add checked-in parity specs only when they are backed
  by a captured interaction and can run as working evidence; otherwise keep the
  gap in Linear/workpad notes instead of repository scenario files.
  Ticket-specific requests for scaffold files do not override this rule; for
  coverage-map-only work, update the operation registry and the Linear/workpad
  notes without adding parity spec or parity request placeholders.
- Do not add capture-only parity specs as a shortcut for expensive local
  implementation. Capture-only specs should be rare and limited to cases where
  proxy implementation is hard-blocked or close to impossible with the current
  harness. If implementation work is merely large, defer it in Linear instead:
  find or create the follow-up issue(s), keep them in the appropriate workflow
  state, and mention those issue identifiers in the capture-only spec
  description/notes if a capture-only spec is truly justified.
- Captured scenarios checked into `config/parity-specs` must be executable
  evidence by schema and inventory validation: either a proxy request with
  strict comparison targets that runs in the Gleam parity runner (`pnpm
gleam:test`), or an explicitly runtime-test-backed fixture mode for
  multi-step flows the generic parity runner cannot yet replay.
- Do not add live recordings, checked-in conformance fixtures, or capture
  scripts without adding an explicit conformance spec and executable test path
  that uses the recording. Recording-only changes are not acceptable evidence,
  even when the fixture was captured from a real store.
- Do not manually author conformance parity fixtures or expected payloads as
  Shopify fidelity evidence. If a parity spec claims captured Shopify behavior,
  its fixture must come from a live capture script or from an existing recorded
  Shopify interaction, and the capture path must be registered in the aggregate
  conformance capture index when a new script is added. Local-runtime fixtures
  may prove proxy-only mechanics, but they are not a substitute for real
  Shopify evidence when the claim is about Shopify's validation, lifecycle, or
  read-after-write behavior.
- Conformance parity scenarios are discovered by convention from
  `config/parity-specs/*.json` and executed by the Gleam parity runner
  (`test/parity_test.gleam`, surfaced through `pnpm gleam:test` on
  both JS and Erlang targets). Each scenario runs the proxy in
  LiveHybrid mode against a recorded `upstreamCalls` cassette in the
  capture file (cassette-playback model — see `docs/parity-runner.md`).
  Do not add per-scenario test files that re-run one scenario — the
  iterator already covers it. Encode scenario-specific expectations in
  the parity spec.
- For parity comparisons, prefer comparing the whole selected resource payload
  and carving out explicit volatile paths such as IDs, timestamps, cursors, and
  throttle metadata. Do not build confidence by allowlisting only the scalar
  fields the current implementation already matches.
- Treat conformance `expectedDifferences` as a last resort after the
  proxy's operation handlers have been adjusted to compute the right
  response (including, if needed, a narrow `proxy/upstream_query.fetch`
  call to read the existing record from upstream). Do not add
  `expectedDifferences` merely to make parity tests pass. Opaque Shopify
  connection cursors are an acceptable expected difference because
  clients must not depend on their internal encoding. **Do not pre-seed
  parity runner state** — `base_state` seeding, `proxyRequest.localSetups`,
  artificial `localRuntimeCases`, and `seedX` keys in capture files are
  banned. A parity scenario must earn state through its replayed requests
  or cassette-backed upstream reads.
- Before handing off a fidelity PR, check the recent rejected-review lessons:
  - If behavior is claimed to match Shopify, include executable parity
    evidence (a checked-in parity spec running through `pnpm gleam:test`)
    unless the work is explicitly limited to non-behavioral docs or
    registry bookkeeping. Integration/unit tests alone prove local code
    paths, not Shopify fidelity.
  - Do not turn implementation gaps into endpoint documentation as a substitute
    for fixing or proving behavior. Endpoint docs may explain a supported
    boundary after the behavior/evidence is in place; unresolved gaps belong in
    Linear/workpad notes or a linked follow-up issue.
  - Do not edit a parity replay request away from the recorded Shopify request
    shape unless you also re-record or add a live capture proving that shape.
    Variables, root inputs, and selected fields in parity requests must remain
    faithful to their capture.
  - When review finds one missing serializer field, error code, Node resolver,
    state dump bucket, or similar family-level issue, audit and fix the sibling
    code paths rather than patching only the reviewed example.
  - New capture scripts must be reachable through the aggregate conformance
    capture index/runner. Do not add resource-specific package scripts or docs
    that imply each scenario needs its own top-level runner.
  - If the branch adds no meaningful behavior, evidence, or docs beyond
    current `main`, close/cancel it instead of trying to preserve a no-op PR.
- Build/recording scripts must be TypeScript (`.ts` / `.mts`) executed
  with `tsx` or similar. Do not add `.mjs` files anywhere in this
  repository.
- Relative TypeScript import specifiers must use the emitted JavaScript
  extension that TypeScript expects for NodeNext output (`.js` for `.ts`,
  `.mjs` for `.mts`, `.cjs` for `.cts`). Do not import local modules with
  source extensions such as `.ts`, `.mts`, or `.cts`; `pnpm lint` enforces
  this with oxlint's `import/extensions` rule.
- Do not add files to linter/formatter ignore lists just because formatting
  changes test fixtures or parity requests. Format the files, then fix the
  affected tests, captures, specs, or code so the formatted files remain
  checked by the normal tooling.
- In unattended or CI-like workspaces, prefer `corepack pnpm ...` for
  package scripts. Bare `pnpm` may not be on `PATH` even though the repo
  is configured for pnpm through Corepack.
- Before adding a resource-local parser, serializer, scalar reader,
  projection helper, or metafield/search/connection utility, read
  `docs/helpers.md` and search for an existing shared helper. If a new
  shared helper is genuinely needed, add it to the shared module and
  document it in `docs/helpers.md` in the same change.
- Search implementations must use
  `src/shopify_draft_proxy/search_query_parser.gleam` for Shopify
  Admin `query:` parsing, execution, AST traversal, term-list guards,
  and primitive term matching. Endpoint modules provide only the
  domain-specific positive term matcher and documented Shopify quirks;
  do not add resource-local query parsers or duplicated traversal
  helpers.
- Connection implementations must use
  `src/shopify_draft_proxy/proxy/graphql_helpers.gleam` for
  cursor windowing, `nodes`/`edges` serialization, and selected
  `pageInfo` fields. Keep resource-specific sorting, filtering, cursor
  derivation, and node projection in the owning resource module, then
  pass those decisions into the shared connection helpers instead of
  rebuilding connection loops locally.

## GitHub repository

- The canonical GitHub repository is `airhorns/shopify-draft-proxy`.
- Open pull requests against `airhorns/shopify-draft-proxy`; do not target personal forks.
- If a workspace remote points at a personal fork, retarget it to `git@github.com:airhorns/shopify-draft-proxy.git` before pushing or creating a PR.
- If `gh pr edit` fails with GitHub's Projects classic / `projectCards`
  deprecation path, do not treat that as a GitHub blocker. Use narrower
  `gh api` calls for the specific metadata update, such as REST label updates or
  GraphQL `updatePullRequest`, then verify the PR state with `gh pr view`.
- GitHub's reactions API supports a fixed reaction set and may not accept the
  workflow's green-circle done marker on review comments. When `🟢` cannot be
  applied as a reaction, remove the `👀` reaction, add a concise handled reply
  containing `🟢`, and resolve the review thread when the requested change is
  fully addressed.

## Shopify conformance auth rule

- Do **not** read `SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN` from repo `.env` in scripts anymore.
- All live conformance scripts must get credentials through `scripts/shopify-conformance-auth.mts`.
- The canonical credential file is `~/.shopify-draft-proxy/conformance-admin-auth.json`.
- The checked-in Shopify app copy lives at `shopify-conformance-app/hermes-conformance-products/`; helper scripts prefer that repo-local app over the legacy `/tmp/shopify-conformance-app/...` copy when it exists.
- `getValidConformanceAccessToken(...)` is the single entry point for token access. It probes the stored access token, refreshes it when possible, and throws a clear error when the stored credential is missing or unrecoverable.
- Workspace `.env` files must not be stale copies of `.env.example`; placeholder
  `SHOPIFY_CONFORMANCE_STORE_DOMAIN` / `SHOPIFY_CONFORMANCE_ADMIN_ORIGIN`
  values will make a valid home-folder token look invalid. If a workspace needs
  repo-local env config, link `.env` to `/home/airhorns/code/shopify-draft-proxy/.env`
  rather than copying it.
- A stale non-placeholder `.env` can also point at the wrong test store. Before
  live capture, verify the effective store with `corepack pnpm conformance:probe`
  and inspect the resolved `SHOPIFY_CONFORMANCE_STORE_DOMAIN` /
  `SHOPIFY_CONFORMANCE_ADMIN_ORIGIN` when the probe target is surprising.
- New auth grants should be generated with `corepack pnpm conformance:auth-link`, and callback exchange should go through `corepack pnpm conformance:exchange-auth -- '<full callback url>'`.
- If a task requires recording or re-recording conformance evidence and
  `getValidConformanceAccessToken(...)` / `corepack pnpm conformance:probe`
  cannot produce a valid live credential after the documented repair paths, do
  not commit code, push a branch, or open a PR. Record the blocker in the Linear
  workpad and move the issue to Human Review.

## Working in the Gleam runtime

The proxy runtime lives under `src/shopify_draft_proxy/` with tests under
`test/`, and compiles to both Erlang/BEAM and JavaScript.

- **Read first:** `GLEAM_PORT_INTENT.md` (non-negotiables). Use
  `GLEAM_PORT_LOG.md` only as historical context when older porting decisions
  matter for the task.
- **Generic Gleam idioms** (decoders, opaque types, OTP, etc.) live in
  `.agents/skills/gleam/SKILL.md`.
- **Both targets, every change:** `gleam test --target erlang` AND
  `gleam test --target javascript`. Drift between them is the most
  expensive bug class.
- **Thompson host Erlang path:** if `erl` is missing, or if
  `erl -eval 'erlang:display(erlang:system_info(otp_release)), halt().' -noshell`
  reports OTP 25, use the checked-in mise toolchain instead of Docker. From the
  repository root, run:
  ```sh
  mise trust .mise.toml
  mise install
  eval "$(mise activate bash)"
  ```
  Then verify `erl` reports OTP 28 and run `gleam clean` before testing so
  stale OTP 25-compiled artifacts are rebuilt. In unattended shells where
  trusting mise is not appropriate, prepend the existing install directly for
  that command:
  ```sh
  PATH=/home/airhorns/.local/share/mise/installs/erlang/28.4.2/bin:$PATH gleam clean
  PATH=/home/airhorns/.local/share/mise/installs/erlang/28.4.2/bin:$PATH corepack pnpm gleam:test
  ```

The legacy TypeScript runtime has been removed. Do not add TypeScript
runtime behavior back under `src/`; new operation handling, fidelity
work, and domain expansions land in Gleam.

## Suggested workflow

1. Read `docs/original-intent.md`.
2. Read `docs/architecture.md`.
3. Know that `docs/helpers.md` exists; read it before adding or duplicating
   shared proxy/helper utilities.
4. Know that `docs/hard-and-weird-notes.md` exists; search or read the
   relevant parts when fidelity assumptions or unusual Shopify behavior matter,
   and add to it when new hard/weird behavior is discovered.
5. Check Linear for the next operation to implement.
6. Add/adjust tests before implementation.
7. Update docs after shipping behavior.

## Repo status note

Early commits may only contain scaffolding. Do not mistake scaffolding for finished behavior.
