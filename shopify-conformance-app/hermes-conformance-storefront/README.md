# Storefront conformance app

`hermes-conformance-storefront` is a **separate public sales-channel app** used only to provision and exercise a Storefront API token for `shopify-draft-proxy` live-conformance recording. It intentionally sits alongside, rather than replaces, `hermes-conformance-products`, which retains the project's Admin API credential and broad Admin scopes.

The sales-channel declaration is the `channel_config` extension under `extensions/conformance-storefront-channel/`. Its release is required before Shopify permits the app to provision Storefront access tokens through `storefrontAccessTokenCreate`.

## Local-only credentials

Shopify CLI populates `.env` with this app's API secret; it is gitignored. Runtime credentials are also gitignored and owner-readable only:

- `~/.shopify-draft-proxy/conformance-storefront-admin-auth.json` — refreshable Admin credential for this sales-channel app.
- `~/.shopify-draft-proxy/conformance-storefront-auth.json` — provisioned Storefront API token.

## Setup and validation

From the repository root:

```bash
corepack pnpm conformance:storefront-auth-link
# Approve the printed URL on the disposable conformance store, then:
corepack pnpm conformance:exchange-storefront-auth -- '<full localhost callback URL>'
corepack pnpm conformance:grant-storefront-token hermes-conformance-storefront-recording
corepack pnpm conformance:probe-storefront
```

The probe makes a real Storefront API request selecting `products.nodes { id title tags }`, so it validates the persisted token and its product-listing/product-tag grants without committing a fixture. Real Storefront parity evidence must still be captured through a registered capture script, never synthesized from the proxy.
