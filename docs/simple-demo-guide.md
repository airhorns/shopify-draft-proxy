# Simple Local Demo Guide

This guide demonstrates the proxy against a real app install without mutating
the Shopify store during normal supported mutation handling. It uses supported
product mutations because products are the first high-fidelity domain for this
project.

The flow shows:

- reads going through the proxy to Shopify
- supported writes staged locally by the proxy
- read-after-write returning staged local data
- direct Shopify reads showing that staged writes did not touch the real store
- choosing either `reset` to discard staged work or `commit` to replay it
  upstream

## Prerequisites

- Node 22 or newer
- `corepack` enabled
- an installed app with an Admin API access token for a dev store
- `curl`

Install dependencies once:

```sh
corepack pnpm install
```

Set shell variables for your dev store and app install:

```sh
export SHOPIFY_ADMIN_ORIGIN="https://your-store.myshopify.com"
export SHOPIFY_ACCESS_TOKEN="shpat_or_shpca_from_your_app_install"
export API_VERSION="2026-04"
export PROXY="http://localhost:3000"
```

The token is sent by the demo client exactly as an app would send it. The proxy
forwards auth headers unchanged when it needs to read from or commit to Shopify.

## 1. Start the proxy

Run the proxy in live-hybrid mode so reads can come from Shopify while supported
writes are staged locally:

```sh
SHOPIFY_ADMIN_ORIGIN="$SHOPIFY_ADMIN_ORIGIN" \
SHOPIFY_DRAFT_PROXY_READ_MODE=live-hybrid \
PORT=3000 \
corepack pnpm dev
```

Leave that process running. In another shell, confirm the meta API is available:

```sh
curl -sS "$PROXY/__meta/health"
curl -sS "$PROXY/__meta/config"
```

## 2. Make an upstream read through the proxy

This request goes to the proxy endpoint an app should use instead of calling
Shopify directly:

```sh
curl -sS "$PROXY/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data '{"query":"query { products(first: 3) { nodes { id title handle status } } }"}' \
  | tee /tmp/proxy-products-before.json
```

With no staged product changes yet, the live-hybrid proxy should return the same
product list Shopify returns for the app install.

## 3. Stage a supported product write locally

Create a unique product title for the demo:

```sh
export PRODUCT_TITLE="Draft Proxy Demo $(date +%s)"
```

Send a supported `productCreate` mutation to the proxy:

```sh
curl -sS "$PROXY/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data "{\"query\":\"mutation { productCreate(product: { title: \\\"$PRODUCT_TITLE\\\", status: DRAFT }) { product { id title handle status createdAt updatedAt } userErrors { field message } } }\"}" \
  | tee /tmp/proxy-product-create.json
```

Save the staged product ID returned by the proxy:

```sh
export STAGED_PRODUCT_ID="$(
  node -e "const fs = require('node:fs'); const body = JSON.parse(fs.readFileSync('/tmp/proxy-product-create.json', 'utf8')); console.log(body.data.productCreate.product.id);"
)"
echo "$STAGED_PRODUCT_ID"
```

The mutation response is synthesized by the proxy, and the original raw mutation
is retained in the local mutation log for a later explicit commit.

Inspect the staged mutation log:

```sh
curl -sS "$PROXY/__meta/log" | tee /tmp/proxy-log-after-create.json
```

The log entry should have `status: "staged"` and `operationName:
"productCreate"`.

## 4. Read the staged product back through the proxy

Read the staged product by ID through the proxy:

```sh
curl -sS "$PROXY/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data "{\"query\":\"query { product(id: \\\"$STAGED_PRODUCT_ID\\\") { id title handle status createdAt updatedAt } }\"}" \
  | tee /tmp/proxy-product-read-after-create.json
```

Expected result: the proxy returns the staged product. The app can read what it
just wrote even though Shopify has not been mutated.

## 5. Prove the staged write did not touch Shopify

Call Shopify directly with the same app install token, bypassing the proxy:

```sh
curl -sS "$SHOPIFY_ADMIN_ORIGIN/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data "{\"query\":\"query { product(id: \\\"$STAGED_PRODUCT_ID\\\") { id title handle status } }\"}" \
  | tee /tmp/shopify-direct-after-staged-create.json
```

Expected result: Shopify returns `{"data":{"product":null}}` for the staged
synthetic product ID. That is the safety property: supported mutations are local
until an explicit commit.

## 6. Stage an update and read it back

Rename the staged product through the proxy:

```sh
export UPDATED_PRODUCT_TITLE="$PRODUCT_TITLE Updated"

curl -sS "$PROXY/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data "{\"query\":\"mutation { productUpdate(product: { id: \\\"$STAGED_PRODUCT_ID\\\", title: \\\"$UPDATED_PRODUCT_TITLE\\\" }) { product { id title handle status updatedAt } userErrors { field message } } }\"}" \
  | tee /tmp/proxy-product-update.json
```

Read it again through the proxy:

```sh
curl -sS "$PROXY/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data "{\"query\":\"query { product(id: \\\"$STAGED_PRODUCT_ID\\\") { id title handle status updatedAt } }\"}" \
  | tee /tmp/proxy-product-read-after-update.json
```

Expected result: the proxy returns the updated title from staged local state.

Shopify should still not have that staged product:

```sh
curl -sS "$SHOPIFY_ADMIN_ORIGIN/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data "{\"query\":\"query { product(id: \\\"$STAGED_PRODUCT_ID\\\") { id title handle status } }\"}" \
  | tee /tmp/shopify-direct-after-staged-update.json
```

Expected result: Shopify still returns `{"data":{"product":null}}`.

## 7A. Discard the staged work

Use `reset` when you want to abandon all staged state and logs:

```sh
curl -sS -X POST "$PROXY/__meta/reset"
curl -sS "$PROXY/__meta/log"
```

Read the staged product ID through the proxy after reset:

```sh
curl -sS "$PROXY/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data "{\"query\":\"query { product(id: \\\"$STAGED_PRODUCT_ID\\\") { id title handle status } }\"}" \
  | tee /tmp/proxy-product-read-after-reset.json
```

Expected result: the proxy no longer returns the staged product. In live-hybrid
mode, the read falls back to Shopify, which should also return `product: null`
for the staged synthetic ID.

## 7B. Commit a staged create instead

Only run this section when you intentionally want to create a real product in
the dev store.

Use this as an alternative to the update/discard path above. Start from a clean
proxy session, repeat sections 3 and 4 only, and then call commit before staging
ID-dependent follow-up mutations such as `productUpdate`.

```sh
curl -sS -X POST "$PROXY/__meta/commit" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  | tee /tmp/proxy-commit.json
```

The commit endpoint replays pending `staged` mutations to Shopify in original
order and stops at the first failure. The response includes each upstream
attempt, an explicit `success` flag, and either the real Shopify response body
or a transport error.

For this simple demo, commit only the staged `productCreate`. The proxy keeps
original raw mutations for replay, so a later staged `productUpdate` that uses
the proxy's synthetic product ID is not a good commit demonstration.

Extract the real Shopify product ID returned by the committed `productCreate`:

```sh
export COMMITTED_PRODUCT_ID="$(
  node -e "const fs = require('node:fs'); const body = JSON.parse(fs.readFileSync('/tmp/proxy-commit.json', 'utf8')); console.log(body.attempts[0].upstreamBody.data.productCreate.product.id);"
)"
echo "$COMMITTED_PRODUCT_ID"
```

Now direct Shopify reads should find the committed product:

```sh
curl -sS "$SHOPIFY_ADMIN_ORIGIN/admin/api/$API_VERSION/graphql.json" \
  -H "Content-Type: application/json" \
  -H "X-Shopify-Access-Token: $SHOPIFY_ACCESS_TOKEN" \
  --data "{\"query\":\"query { product(id: \\\"$COMMITTED_PRODUCT_ID\\\") { id title handle status } }\"}" \
  | tee /tmp/shopify-direct-after-commit.json
```

Expected result: Shopify returns the committed product. This is the only part of
the demo that mutates the real store.

## Cleanup after a commit

If the committed demo product should be removed from the dev store, delete it
intentionally through Shopify or through the proxy plus another explicit commit.
Do not send cleanup mutations to the proxy unless you understand whether that
operation is supported locally or will be proxied upstream.
