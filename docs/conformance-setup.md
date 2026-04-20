# Real Shopify Conformance Setup

This project needs a real Shopify development target to measure fidelity against actual Admin GraphQL behavior.

## Why this exists

The proxy can be implemented without a live Shopify target, but **high-fidelity behavior cannot be validated from guesswork alone**. We need a real dev store plus Admin API credentials to:

- capture true query/mutation response shapes
- verify `userErrors` semantics
- compare null/empty behavior
- study list ordering, handles, timestamps, and derived fields
- build replayable parity fixtures

## What we need

### 1. A Shopify dev/test store

A store we can safely mutate during conformance runs.

### 2. A Shopify app installation with Admin API access

For early conformance work, a single installed app or custom app with a valid Admin API access token is enough.

### 3. Stable configuration

Set these variables in a local `.env` or shell session:

- `SHOPIFY_CONFORMANCE_STORE_DOMAIN`
- `SHOPIFY_CONFORMANCE_ADMIN_ORIGIN`
- `SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN`
- `SHOPIFY_CONFORMANCE_API_VERSION`
- `SHOPIFY_CONFORMANCE_APP_HANDLE` (optional but useful)

See `.env.example` for the canonical variable names.

The server now loads `.env` automatically via `dotenv`, so a local `.env` file is enough for normal `pnpm dev` / `pnpm start` workflows.

#### Symphony workspace credential link

On the current unattended Symphony host, the durable conformance credential file lives in the original checkout:

```text
/home/airhorns/code/shopify-draft-proxy/.env
```

New Symphony workspaces should link their repo-local `.env` to that file instead of copying secret values into the workspace:

```bash
ln -sfn /home/airhorns/code/shopify-draft-proxy/.env .env
```

If a workspace already has a placeholder `.env`, replace it with the symlink before running live conformance. Do not commit the symlink or any secret-bearing `.env` file; `.gitignore` excludes them. The link only proves where to load credentials from. `corepack pnpm conformance:probe` is still the required gate for proving the current token is valid before any live capture.

#### Shopify CLI token refresh fallback

Prefer a dedicated `shpat_...` Admin API token for unattended conformance. If the host is still using a Shopify CLI account bearer token, the refresh material lives in:

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

After persisting, run `corepack pnpm conformance:probe`. If the refresh response is `invalid_grant`, the stored CLI grant is no longer recoverable non-interactively; stop retrying it and switch to a dedicated dev-store Admin API token or have a human re-authenticate Shopify CLI and persist the fresh pair into both files.

### 4. Validate structural conformance coverage before live probing

Before probing the live target, run:

```bash
pnpm conformance:check
pnpm conformance:parity
```

`conformance:check` runs the normal Vitest structural checks for the canonical operation registry, convention-discovered parity specs, captured fixture references, and proxy request files. Any implemented operation must declare runtime-test coverage and be named by at least one discovered parity spec.

There is no central conformance scenario manifest. Standard scenarios are discovered from:

```text
config/parity-specs/*.json
```

Each parity spec is the scenario metadata source and must include:

- `scenarioId`
- `operationNames`
- `scenarioStatus` (`planned` or `captured`)
- `assertionKinds`
- `liveCaptureFiles`
- `proxyRequest.documentPath` / `proxyRequest.variablesPath` when a proxy parity request exists
- `comparison` for captured scenarios that are ready to execute as strict JSON comparisons

Every operation named by a discovered parity spec must exist in `config/operation-registry.json`, and every implemented operation in the registry must be named by at least one discovered parity spec. The scenario-to-operation mapping lives in each parity spec's `operationNames` field; the registry stays focused on runtime capability classification and runtime-test files.

`conformance:parity` executes captured scenarios only after they declare both a proxy request and a strict JSON comparison contract. Valid high-assurance scenarios must compare explicit targets and list every allowed difference as a path-scoped rule with a reason. Use matchers for legitimate nondeterminism such as Shopify IDs, timestamps, and throttle metadata. An `ignore: true` rule means the proxy has not reached parity for that path; it must also set `regrettable: true` and should only be used for hard temporary gaps that will be fixed later.

### 5. Probe the live target before writing parity fixtures

Once the vars are present, run:

```bash
pnpm conformance:probe
```

This performs a minimal Admin GraphQL `shop` query against the configured store and fails fast if the domain/origin/token combination is wrong.

### 5. Capture product-domain fixtures from the live store

Run:

```bash
pnpm conformance:capture-products
pnpm conformance:capture-product-mutations
pnpm conformance:capture-product-state-mutations
pnpm conformance:capture-product-option-mutations
pnpm conformance:capture-collections
pnpm conformance:capture-collection-mutations
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

## Near-term workflow

1. Pick the product-first Shopify behavior to model and add/update runtime tests for the proxy behavior.
2. Add or update a parity spec in `config/parity-specs/`; do not edit a central scenario manifest.
3. Add a proxy request pair under `config/parity-requests/` for scenarios ready to replay through the proxy.
4. Add captured live fixtures under `fixtures/conformance/<store-domain>/<api-version>/` when credentials and store safety allow it.
5. Ensure the parity spec's `operationNames` list matches existing root operations in `config/operation-registry.json`.
6. Run `pnpm conformance:check` and `pnpm conformance:parity`.

## Important safety note

Unsupported mutations in the proxy may still passthrough today. Conformance targets must therefore be treated as **safe-to-mutate** stores until passthrough behavior is made safer and more explicit.

## Principle

If a behavior is surprising or underspecified, do not guess forever. Add a conformance scenario against real Shopify and record what actually happens.
