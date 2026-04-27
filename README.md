# shopify-draft-proxy

`shopify-draft-proxy` is a high-fidelity Shopify Admin GraphQL digital twin for
test environments.

Point an app at this proxy instead of Shopify. Reads can still come from the
real shop, but supported mutations are staged in local in-memory state. The
proxy returns Shopify-like mutation payloads and makes later reads behave as if
the writes already happened, while the real store stays unchanged until an
explicit `__meta/commit`.

This is not a generic mock server. The goal is to model Shopify domain behavior
closely enough that app tests can exercise realistic write/read flows without
normal test runs mutating a dev store.

## What it does

- Preserves Shopify-like versioned Admin GraphQL routes:
  `/admin/api/:version/graphql.json`.
- Forwards existing Shopify auth headers unchanged when it needs to call
  upstream Shopify.
- Stages supported mutations locally and records the original raw GraphQL body
  for later commit replay.
- Overlays staged local effects onto downstream reads.
- Proxies unsupported mutations to Shopify as an escape hatch and records that
  fact in logs/observability.
- Exposes a meta API for health, configuration, staged-state inspection, reset,
  and commit.
- Uses conformance captures and parity tests to keep behavior grounded in real
  Shopify responses.

Products are the first deep fidelity target. Other Admin GraphQL areas are
covered incrementally as they gain local models, fixtures, and tests.

## Quick start

Prerequisites:

- Node 22 or newer
- Corepack
- A Shopify dev store plus an app Admin API access token

Install dependencies:

```sh
corepack pnpm install
```

Set runtime configuration:

```sh
export SHOPIFY_ADMIN_ORIGIN="https://your-store.myshopify.com"
export SHOPIFY_ACCESS_TOKEN="shpat_or_shpca_from_your_app_install"
export SHOPIFY_DRAFT_PROXY_READ_MODE="live-hybrid"
export API_VERSION="2025-01"
export PROXY="http://localhost:3000"
```

Start the proxy:

```sh
SHOPIFY_ADMIN_ORIGIN="$SHOPIFY_ADMIN_ORIGIN" \
SHOPIFY_DRAFT_PROXY_READ_MODE="$SHOPIFY_DRAFT_PROXY_READ_MODE" \
PORT=3000 \
corepack pnpm dev
```

Check that it is running:

```sh
curl -sS "$PROXY/__meta/health"
curl -sS "$PROXY/__meta/config"
```

## Runtime modes

`SHOPIFY_DRAFT_PROXY_READ_MODE` controls how reads are answered.

- `live-hybrid` is the default. Reads go to Shopify, then staged local effects
  are overlaid when the proxy has a supported local model.
- `snapshot` answers supported reads from a normalized snapshot plus staged
  state. Set `SHOPIFY_DRAFT_PROXY_SNAPSHOT_PATH` to a snapshot JSON file.
- `passthrough` forwards reads without an overlay. This is useful as a
  debugging baseline.

Supported mutations are still staged locally instead of being sent to Shopify
during normal runtime. `POST /__meta/commit` is the explicit exception: it
replays pending staged mutations upstream in their original order.

## Example request flows

These examples use product operations because product fidelity is the primary
target.

### 1. Read through the proxy

The app sends the same Admin GraphQL request it would normally send to Shopify,
but to the proxy host:

```sh
curl -sS "$PROXY/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data '{"query":"query { products(first: 3) { nodes { id title handle status } } }"}'
```

In `live-hybrid` mode, if there is no relevant staged state, this should match
the upstream Shopify response for the same app token.

### 2. Stage a supported write locally

Send a supported mutation to the same GraphQL route:

```sh
curl -sS "$PROXY/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data '{"query":"mutation { productCreate(product: { title: \"Draft Proxy Hat\", status: DRAFT }) { product { id title handle status createdAt updatedAt } userErrors { field message } } }"}' \
  | tee /tmp/proxy-product-create.json
```

The response is synthesized by the proxy. The returned product ID is stable for
the current proxy session and the raw mutation body is appended to the local
mutation log.

Inspect the log:

```sh
curl -sS "$PROXY/__meta/log"
```

The new entry should have `status: "staged"` and `operationName:
"productCreate"`.

### 3. Read your staged write back

Extract the staged product ID:

```sh
export STAGED_PRODUCT_ID="$(
  node -e "const fs = require('node:fs'); const body = JSON.parse(fs.readFileSync('/tmp/proxy-product-create.json', 'utf8')); console.log(body.data.productCreate.product.id);"
)"
```

Read the product through the proxy:

```sh
curl -sS "$PROXY/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data "{\"query\":\"query { product(id: \\\"$STAGED_PRODUCT_ID\\\") { id title handle status createdAt updatedAt } }\"}"
```

Expected result: the proxy returns the staged product even though Shopify has
not been mutated.

### 4. Compare with direct Shopify

Call Shopify directly with the staged synthetic ID:

```sh
curl -sS "$SHOPIFY_ADMIN_ORIGIN/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data "{\"query\":\"query { product(id: \\\"$STAGED_PRODUCT_ID\\\") { id title handle status } }\"}"
```

Expected result: Shopify should not know about that staged local ID. This is the
normal safety property: supported mutations do not touch the real store at
runtime.

### 5. Discard staged work

Reset drops staged state, generated IDs, caches, and logs. In snapshot mode it
restores the startup snapshot baseline.

```sh
curl -sS -X POST "$PROXY/__meta/reset"
curl -sS "$PROXY/__meta/log"
```

### 6. Commit staged work intentionally

Only call commit when you want the staged mutations to run against Shopify.

```sh
curl -sS -X POST "$PROXY/__meta/commit" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN"
```

Commit replays pending `staged` log entries to Shopify in original order, using
the original GraphQL route path and request body. It stops on the first failed
upstream attempt and returns a report with each attempt, upstream status/body or
transport error, and `stopIndex` when applicable.

### 7. Unsupported mutation escape hatch

If a mutation is not supported by a local model, the proxy forwards it to
Shopify unchanged. That can create real side effects. Unsupported mutation
passthrough is visible in structured logs and in `GET /__meta/log` with
`status: "proxied"` so tests and operators can detect it.

## Meta API

The meta API runs on the same Koa server as the proxy.

### `GET /__meta`

Returns a small operator web UI for inspecting the current mutation log and
state, and for triggering reset or commit actions.

### `GET /__meta/health`

Returns a simple liveness payload:

```json
{
  "ok": true,
  "message": "shopify-draft-proxy is running"
}
```

### `GET /__meta/config`

Returns runtime configuration visible to the proxy:

```json
{
  "runtime": {
    "readMode": "live-hybrid"
  },
  "proxy": {
    "port": 3000,
    "shopifyAdminOrigin": "https://your-store.myshopify.com"
  },
  "snapshot": {
    "enabled": false,
    "path": null
  }
}
```

### `GET /__meta/log`

Returns ordered mutation-log entries:

```json
{
  "entries": [
    {
      "id": "gid://shopify/MutationLogEntry/...",
      "operationName": "productCreate",
      "path": "/admin/api/2025-01/graphql.json",
      "status": "staged"
    }
  ]
}
```

Entries retain the original GraphQL query, variables, request body, interpreted
capability metadata, and status. Supported local writes use `staged`;
unsupported passthrough writes use `proxied`; commit updates entries to
`committed` or `failed`.

### `GET /__meta/state`

Returns a debug snapshot of the in-memory object graph:

- `baseState` for snapshot-derived or hydrated upstream data
- `stagedState` for local inserts, updates, deletes, and derived indexes

This endpoint is for tests and operators. It is not a Shopify API surface.

### `POST /__meta/reset`

Resets runtime state:

- discards staged state
- clears mutation logs and generated IDs
- restores the initial normalized snapshot baseline when one was loaded

Example:

```sh
curl -sS -X POST "$PROXY/__meta/reset"
```

### `POST /__meta/commit`

Replays pending staged mutations to Shopify:

- uses the original route path and request body
- sends attempts in original log order
- maps committed synthetic IDs to authoritative Shopify IDs when possible
- stops at the first HTTP, GraphQL, or transport failure
- returns `ok`, `stopIndex`, and an `attempts` array

Example:

```sh
curl -sS -X POST "$PROXY/__meta/commit" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN"
```

## Configuration

Runtime environment variables:

- `SHOPIFY_ADMIN_ORIGIN`: required, for example
  `https://your-store.myshopify.com`
- `SHOPIFY_DRAFT_PROXY_READ_MODE`: optional, one of `live-hybrid`, `snapshot`,
  or `passthrough`; defaults to `live-hybrid`
- `SHOPIFY_DRAFT_PROXY_SNAPSHOT_PATH`: optional normalized snapshot JSON path
- `PORT`: optional; defaults to `3000`

Conformance credentials are intentionally separate from normal runtime config.
Live conformance auth is stored outside the repo at
`~/.shopify-draft-proxy/conformance-admin-auth.json` and accessed through the
repo conformance scripts.

## Development

Common commands:

```sh
corepack pnpm dev
corepack pnpm typecheck
corepack pnpm test
corepack pnpm lint
corepack pnpm conformance:check
corepack pnpm conformance:parity
```

Important docs:

- `docs/original-intent.md`: project intent, non-goals, and fidelity standard
- `docs/architecture.md`: request flow, state model, runtime modes, and meta API
- `docs/simple-demo-guide.md`: longer copy-pasteable product staging demo
- `docs/helpers.md`: shared helper APIs to use before adding new utilities
- `docs/hard-and-weird-notes.md`: captured Shopify quirks and fidelity traps
- `docs/endpoints/`: endpoint-specific behavior and coverage notes

## Current posture

The proxy is intentionally coverage-driven. A mutation root is considered
supported only when the local model can emulate its supported lifecycle behavior
and downstream read-after-write effects without normal runtime Shopify writes.
Validation-only or branch-only handling is documented as guardrail coverage, not
as full operation support.
