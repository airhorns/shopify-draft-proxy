# Hermes conformance Shopify app

This is the checked-in copy of the Shopify app used for local conformance auth and publication-target setup.

The app includes minimal no-op Function extensions used for Admin GraphQL conformance evidence:

- `conformance-payment-customization`
- `conformance-validation`
- `conformance-cart-transform`

These extensions exist so live `shopifyFunctions` reads can expose released Function metadata and app ownership fields. They should not be used as runtime behavior fixtures for checkout validation, cart transforms, payment customization, or tax callbacks.

## Local setup

1. `cd shopify-conformance-app/hermes-conformance-products`
2. `corepack pnpm install`
3. `shopify app env pull` or otherwise create `.env` with `SHOPIFY_API_KEY` and `SHOPIFY_API_SECRET`
4. Use the repo auth helpers from the main repo root, or run Shopify CLI directly from this app directory.

Secrets are intentionally not committed.
