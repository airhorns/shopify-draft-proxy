# AGENTS.md

Guidance for future AI coding agents working in `shopify-draft-proxy`.

## Primary mission

Preserve the intent in `docs/original-intent.md`.

This project is a **Shopify Admin GraphQL digital twin / draft proxy**, not a generic mock server. The goal is to let tests stage realistic Shopify writes locally and observe downstream reads as if those writes happened, without mutating the real store during normal supported mutation handling.

## Non-negotiables

1. **Products first, but deep**
   - Start with products and their directly related sub-resources.
   - Do not chase broad coverage before product fidelity is credible.

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
   - Update `docs/original-intent.md` only if the product goal truly changes.

## Development rules

- Use strict TypeScript.
- Keep runtime state in memory unless the project scope explicitly expands.
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
- Do not add tests that only reassert self-evident properties of checked-in
  metadata, such as exact fields in registry or parity-spec JSON files. Test
  executable behavior, schema validation, discovery semantics, or comparison
  contracts instead.
- For coverage-map-only registry work, do not add tests whose only signal is
  that specific roots exist in `config/operation-registry.json`, have a
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
  strict comparison targets that runs in the `conformance:parity` script, or
  an explicitly runtime-test-backed fixture mode for multi-step flows the
  generic parity runner cannot yet replay.
- Do not add live recordings, checked-in conformance fixtures, or capture
  scripts without adding an explicit conformance spec and executable test path
  that uses the recording. Recording-only changes are not acceptable evidence,
  even when the fixture was captured from a real store.
- Conformance parity scenarios are discovered by convention from `config/parity-specs/*.json` and executed by the single vitest suite at `tests/unit/conformance-parity-scenarios.test.ts` (also exposed as `pnpm conformance:parity`). Do not add per-scenario `it(...)` blocks that re-run one scenario — the iterator already covers it. Encode scenario-specific expectations in the parity spec.
- Treat conformance `expectedDifferences` as a last resort after modeling or
  fixture seeding has been exhausted; do not add them merely to make parity
  tests pass. Opaque Shopify connection cursors are an acceptable expected
  difference because clients must not depend on their internal encoding.
- Repo scripts must be TypeScript files executed with `tsx` or similar, not
  `.mjs` files. Do not add `.mjs` files anywhere in this repository.
- Relative TypeScript import specifiers must use the emitted JavaScript
  extension that TypeScript expects for NodeNext output (`.js` for `.ts`, `.mjs`
  for `.mts`, `.cjs` for `.cts`). Do not import local modules with source
  extensions such as `.ts`, `.mts`, or `.cts`; `pnpm lint` enforces this with
  oxlint's `import/extensions` rule.
- In unattended or CI-like workspaces, prefer `corepack pnpm ...` for package
  scripts. Bare `pnpm` may not be on `PATH` even though the repo is configured
  for pnpm through Corepack.
- Search implementations must use the shared helpers in
  `src/search-query-parser.ts` for Shopify Admin `query:` parsing, execution,
  AST traversal, term-list guards, and primitive term matching. Endpoint modules
  should provide only the domain-specific positive term matcher and documented
  Shopify quirks; do not add new resource-local query parsers or duplicated
  `matches*QueryNode` traversal helpers.
- Connection implementations must use the shared helpers in
  `src/proxy/graphql-helpers.ts` for cursor windowing, `nodes`/`edges`
  serialization, and selected `pageInfo` fields. Keep resource-specific sorting,
  filtering, cursor derivation, and node projection in the owning resource
  module, then pass those decisions into `paginateConnectionItems(...)` and
  `serializeConnection(...)` instead of rebuilding connection loops locally.

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
