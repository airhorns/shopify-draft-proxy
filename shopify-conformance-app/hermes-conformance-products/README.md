# Hermes conformance Shopify app

This is the checked-in copy of the Shopify app used for local conformance auth and publication-target setup.

## Local setup

1. `cd shopify-conformance-app/hermes-conformance-products`
2. `corepack pnpm install`
3. `shopify app env pull` or otherwise create `.env` with `SHOPIFY_API_KEY` and `SHOPIFY_API_SECRET`
4. Use the repo auth helpers from the main repo root, or run Shopify CLI directly from this app directory.

Secrets are intentionally not committed.
