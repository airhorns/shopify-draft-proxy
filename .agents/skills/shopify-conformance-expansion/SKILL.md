---
name: shopify-conformance-expansion
description: Extend Shopify Admin GraphQL conformance coverage in shopify-draft-proxy. Use when adding or updating conformance fixtures, parity specs, parity request GraphQL files, recorder scripts, or documentation for Shopify API fidelity scenarios. Also covers the live-store setup and credential plumbing required to run conformance against a real Shopify dev store.
---

# Shopify Conformance Expansion

## Core Rule

Treat conformance as the fidelity source of truth. Do not guess Shopify behavior when a safe live capture or existing fixture can answer it. Preserve the project goal: broad, high-fidelity Shopify Admin GraphQL draft proxy behavior, not a generic mock server.

## Why a live target exists

The proxy can be implemented without a live Shopify target, but **high-fidelity behavior cannot be validated from guesswork alone**. A real dev store plus Admin API credentials is needed to:

- capture true query/mutation response shapes
- verify `userErrors` semantics
- compare null/empty behavior
- study list ordering, handles, timestamps, and derived fields
- build replayable parity fixtures

Unsupported mutations in the proxy may still passthrough today. Conformance targets must therefore be treated as **safe-to-mutate** stores until passthrough behavior is made safer and more explicit.

If a behavior is surprising or underspecified, do not guess forever — add a conformance scenario against real Shopify and record what actually happens.

## Required Workflow

1. Read `AGENTS.md`, `docs/original-intent.md`, and `docs/architecture.md`.
2. Read `docs/hard-and-weird-notes.md` only when making or changing a Shopify fidelity assumption.
3. Use Linear for operation/project tracking; do not recreate `docs/shopify-admin-worklist.md`.
4. Identify the root operation and whether it is already implemented in `config/operation-registry.json`.
5. Add or update runtime tests for proxy behavior before relying on parity evidence.
6. Decide the evidence path before recording: every new live fixture, recorder,
   or parity spec must have a same-change consumer that runs as strict proxy
   parity or a documented runtime-test-backed replay path.
7. Add or update exactly one captured, working parity spec for each scenario under `config/parity-specs/`.
8. Add proxy replay files under `config/parity-requests/` only when a captured scenario can be replayed locally as working evidence.
9. Prefer full-response strict parity targets over `selectedPaths` allowlists.
   When stable fields differ, model the behavior or expand the fixture/request
   first; use `expectedDifferences` as a narrow denylist only for unavoidable
   opaque or intentionally divergent fields, and document each reason.
10. Add live captures under `fixtures/conformance/<store-domain>/<api-version>/` only when credentials and store safety allow it.
    If the store lacks required objects, create/update/activate/delete realistic
    setup data in the capture script or setup flow and clean it up afterward
    instead of falling back to validation-only evidence.
11. Make sure every root operation in the parity spec's `operationNames` exists in `config/operation-registry.json`.
12. Add new helper scripts as TypeScript and run them with `tsx` or an equivalent TypeScript runner.
13. Do not add new planned-only or blocked-only parity specs, and do not add parity request files as TODO placeholders for future captures. Ticket-specific acceptance text asking for scaffold files does not override this rule. If a scenario cannot be captured and replayed as working evidence in the current task, document the gap in Linear/workpad notes instead of adding repository scenario files.
14. If the task specifically requires recording or re-recording live conformance
    evidence and valid Shopify credentials cannot be restored with the
    documented auth/probe paths, stop before implementation handoff: update the
    Linear workpad with the blocker, move the issue to Human Review, and do
    **not** commit, push, or open a PR.

## Live-store setup

### What you need

- A Shopify dev/test store that is safe to mutate during conformance runs.
- A Shopify app install (or custom app) with a valid Admin API access token.
- Stable configuration in a local `.env` or shell session. Canonical variable names are in `.env.example`:
  - `SHOPIFY_CONFORMANCE_STORE_DOMAIN`
  - `SHOPIFY_CONFORMANCE_ADMIN_ORIGIN`
  - `SHOPIFY_CONFORMANCE_API_VERSION`
  - `SHOPIFY_CONFORMANCE_APP_HANDLE` (optional but useful)
  - `SHOPIFY_CONFORMANCE_APP_ID` (optional; lets local inventory-adjust replay mirror `inventoryAdjustmentGroup.app.id`)
  - `SHOPIFY_CONFORMANCE_APP_API_KEY` (optional; lets local inventory-adjust replay mirror `inventoryAdjustmentGroup.app.apiKey` when `SHOPIFY_API_KEY` is not set)

The server loads `.env` automatically via `dotenv`, so a local `.env` is enough for normal `pnpm dev` / `pnpm start` workflows.

### Credential locations

The live conformance access token is no longer stored in the repo `.env`. The canonical credential lives at:

```text
~/.shopify-draft-proxy/conformance-admin-auth.json
```

The linked Shopify app has a checked-in repo copy at:

```text
shopify-conformance-app/hermes-conformance-products/
```

Auth helper scripts prefer that repo-local app copy over the legacy `/tmp/shopify-conformance-app/...` location for reading app config and `.env` secrets. Secrets are still intentionally untracked — create a local `.env` in that app directory via `shopify app env pull` or equivalent.

### Generating a fresh grant

```bash
corepack pnpm conformance:auth-link
corepack pnpm conformance:exchange-auth -- '<full callback url>'
```

### Refreshing an expiring token

The current host uses a **refreshable expiring store-auth token** persisted in the shared home-folder credential. When it expires, repair the repo with the refresh path first — do not guess or generate a brand-new auth link unnecessarily.

```bash
corepack pnpm conformance:refresh-auth
corepack pnpm conformance:probe
```

`conformance:refresh-auth` does:

- reads `refresh_token` + `client_id` from `~/.shopify-draft-proxy/conformance-admin-auth.json`
- reads `SHOPIFY_API_SECRET` from the linked app `.env`
  - first candidate when present: `shopify-conformance-app/<SHOPIFY_CONFORMANCE_APP_HANDLE>/.env`
  - default candidate: `/tmp/shopify-conformance-app/<SHOPIFY_CONFORMANCE_APP_HANDLE>/.env`
  - override with `SHOPIFY_CONFORMANCE_APP_ENV_PATH=/path/to/app/.env`
- calls `POST https://<store>/admin/oauth/access_token` with a form-encoded body (`client_id`, `client_secret`, `grant_type=refresh_token`, `refresh_token`)
- persists the returned rotated token pair back into `~/.shopify-draft-proxy/conformance-admin-auth.json`
- verifies the refreshed token immediately with a live `shop` probe

Important current-host findings:

- Shopify's newer refreshable store-auth flow may return a token shaped like `shpca_...`, not only `shpat_...`. Treat Shopify token families with the broader `^shp[a-z]+_` rule.
- Those tokens must still be sent as raw `X-Shopify-Access-Token: <token>`.
- The refresh response can rotate **both** the access token and refresh token, so persisting them atomically matters.

If `conformance:refresh-auth` fails, inspect the returned JSON before retrying. A shared-credential refresh failing with `invalid_request` / `This request requires an active refresh_token` means the saved store-auth grant is no longer refreshable — stop retrying it. Generate a new store-auth link, complete the browser approval flow, exchange the callback in one step, and only then probe again.

In unattended Linear workflow, generating a new auth link or completing browser
approval is an external human action. When a required recording/re-recording
depends on that action, record the failed probe/refresh details in the workpad,
move the issue to Human Review, and do not create a commit, push a branch, or
open a PR for partial work.

### Symphony workspace credential link

On the unattended Symphony host, older workspaces may still have a repo-local `.env` linked to the original checkout:

```text
/home/airhorns/code/shopify-draft-proxy/.env
```

New Symphony workspaces should prefer the home-folder credential file above. If a workspace still needs the original checkout `.env`, link it instead of copying secret values into the workspace:

```bash
ln -sfn /home/airhorns/code/shopify-draft-proxy/.env .env
```

Do not commit the symlink or any secret-bearing `.env` file; `.gitignore` excludes them. The link only proves where to load credentials from. `corepack pnpm conformance:probe` is still the required gate for proving the current token is valid before any live capture.

### Legacy Shopify CLI token refresh fallback

Prefer `corepack pnpm conformance:refresh-auth` and the store-auth flow above. If an older host is still using a Shopify CLI account bearer token, the refresh material lives in:

```text
~/.config/shopify-cli-kit-nodejs/config.json
```

That file stores a JSON string at `sessionStore`. Parse it, use `currentSessionId` to find `accounts.shopify.com[<currentSessionId>].identity.refreshToken`, and refresh against Shopify Accounts with a **form-encoded** request:

```text
POST https://accounts.shopify.com/oauth/token
Content-Type: application/x-www-form-urlencoded

grant_type=refresh_token
client_id=fbdb2649-e327-4907-8f67-908d24cfd7e3
refresh_token=<stored identity refresh token>
```

Do not perform a non-persisting "test" refresh. A successful response can rotate the refresh token immediately, so the first successful response must be persisted in the same step:

- update `identity.accessToken`, `identity.refreshToken`, and `identity.expiresAt`
- update any application entries that mirror the old `accessToken`
- write the updated `sessionStore` back to `~/.config/shopify-cli-kit-nodejs/config.json`
- update `SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN` in `/home/airhorns/code/shopify-draft-proxy/.env`
- update the workspace `.env` too if it is not a symlink to the original checkout file

After persisting, run `corepack pnpm conformance:probe`. If the refresh response is `invalid_grant`, the stored CLI grant is no longer recoverable non-interactively; stop retrying and switch to the store-auth flow above, a dedicated dev-store Admin API token, or a fresh Shopify CLI authentication.

For unattended ticket work, an unrecoverable `invalid_grant` or equivalent
invalid/missing live credential blocks any required recording/re-recording. Do
not continue by committing code or opening a PR unless the ticket can be fully
completed with existing fixtures/local/snapshot evidence and no required live
capture remains.

### Probe the live target before writing parity fixtures

Once the vars are present:

```bash
corepack pnpm conformance:probe
```

This performs a minimal Admin GraphQL `shop` query against the configured store and fails fast if the domain/origin/token combination is wrong. Internally it resolves the token via `getValidConformanceAccessToken(...)`, which probes the stored token, refreshes it when possible, and reports a clear error when the home-folder credential is missing or dead.

If `conformance:probe` fails and the current task needs fresh live recordings,
the failure is not a warning to ignore. Treat it as a required-auth blocker:
preserve the failing command/error in the Linear workpad, explain why existing
fixtures/local replay are insufficient for the requested acceptance criteria,
move the issue to Human Review, and stop without commit/push/PR.

### Capture product-domain fixtures from the live store

```bash
corepack pnpm conformance:capture -- --run products
corepack pnpm conformance:capture -- --run product-mutations
corepack pnpm conformance:capture -- --run product-state-mutations
corepack pnpm conformance:capture -- --run product-option-mutations
corepack pnpm conformance:capture -- --run collections
corepack pnpm conformance:capture -- --run collection-mutations
```

The package file intentionally does not expose one shortcut per recorder. Use
the central runner above, inspect it with `corepack pnpm conformance:capture`,
or execute a recorder directly:

```bash
corepack pnpm exec tsx ./scripts/capture-product-mutation-conformance.mts
```

This writes live Admin GraphQL captures under:

```text
fixtures/conformance/<store-domain>/<api-version>/
```

The current capture set records:

- catalog page / cursor sample
- detailed product shape
- variant matrix shape
- search / count behavior
- variant-backed product search samples (`sku:` and, when available in the dev store, `barcode:`)
- top-level `collection(id:)` detail with nested `products` connection fields
- top-level `collections` catalog pagination with nested `products` slices

## Scenario Convention

Do not add a central scenario manifest. Standard scenarios are discovered by convention from `config/parity-specs/*.json`.

Each parity spec must carry the scenario metadata:

- `scenarioId`: stable id for this parity scenario.
- `operationNames`: root Shopify operations covered by the scenario.
- `scenarioStatus`: `captured`; new planned-only or blocked-only specs are not acceptable.
- `assertionKinds`: what confidence the scenario builds, such as `payload-shape`, `user-errors-parity`, or `downstream-read-parity`.
- `liveCaptureFiles`: fixture paths for captured scenarios.
- `proxyRequest`: `documentPath` and `variablesPath` when a captured scenario can be replayed through the proxy as working evidence. New scenarios should not set this to `null` unless proxy replay is genuinely hard-blocked by the current harness and the blocker is documented in the spec notes and Linear workpad.
- `comparison`: strict JSON comparison contract for captured scenarios that are ready to execute.
- `notes`: concise fidelity findings, blockers, or promotion criteria.

Every operation named by a discovered parity spec must exist in `config/operation-registry.json`, and every implemented operation in the registry must be named by at least one discovered parity spec. The scenario-to-operation mapping lives in each parity spec's `operationNames` field; the registry stays focused on runtime capability classification and runtime-test files.

Explicit scenario override config is only for unusual cases that cannot fit this parity spec shape. Avoid it for normal expansion.

### Do not add explicit per-scenario parity tests

`tests/unit/conformance-parity-scenarios.test.ts` is the single convention-driven vitest suite that discovers every parity spec, filters to `ready-for-comparison`, and runs `executeParityScenario` against each. Adding or promoting a parity spec is enough to get CI coverage — do not write a new `it(...)` block that runs the same scenario explicitly. If you need richer per-scenario assertions (e.g. specific comparison target names), encode them in the parity spec itself; the runner validates them from the spec.

Scenarios become `ready-for-comparison` only after they declare both a proxy request and a strict JSON comparison contract. Valid high-assurance scenarios must compare explicit targets and list every allowed difference as a path-scoped rule with a reason. Use matchers for legitimate nondeterminism such as Shopify IDs, timestamps, and throttle metadata. An `ignore: true` rule means the proxy has not reached parity for that path; it must also set `regrettable: true` and should only be used for hard temporary gaps that will be fixed later. Do not use `expectedDifferences`, `ignore`, or a narrow target list merely to make an incomplete implementation pass.

When a scenario compares a resource selected by the proxy request, prefer a target at the whole selected resource object, such as `$.mutation.response.data.productCreate.product` or `$.downstreamRead.response.data.order.fulfillments[0]`, instead of allowlisting individual scalar fields. Add path-scoped exceptions for volatile fields such as generated IDs, cursors, and timestamps only when the comparator proves they differ. This keeps new selected fields covered by default and makes exceptions a denylist rather than an allowlist.

## Confidence Ladder

Use the strongest feasible evidence:

1. Runtime tests prove local staging/overlay behavior.
2. Captured live fixtures settle Shopify payload shape, nullability, ordering, timestamps, and user errors.
3. Proxy request files make local replay deterministic.
4. `conformance:check` runs the repo's Vitest structural checks for discovered scenarios.
5. `conformance:parity` runs the convention-driven vitest suite at `tests/unit/conformance-parity-scenarios.test.ts`, which iterates every discovered parity spec and executes strict comparisons for `ready-for-comparison` scenarios.

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
