# Hermes conformance Shopify app

This is the checked-in copy of the Shopify app used for local conformance auth and publication-target setup.

The app includes minimal no-op Function extensions used for Admin GraphQL conformance evidence:

- `conformance-payment-customization`
- `conformance-validation`
- `conformance-cart-transform`
- `conformance-cart-transform-secondary`
- `conformance-fulfillment-constraint`

These extensions exist so live `shopifyFunctions` reads can expose released Function metadata and app ownership fields. They should not be used as runtime behavior fixtures for checkout validation, cart transforms, payment customization, or tax callbacks.

## Local setup

1. `cd shopify-conformance-app/hermes-conformance-products`
2. `corepack pnpm install`
3. Use the host-managed app environment referenced by
   `SHOPIFY_CONFORMANCE_APP_ENV_PATH`; do not copy secrets into the workspace or
   run `shopify app env pull` as normal bootstrap.
4. Use the repo auth helpers from the main repo root, or run Shopify CLI directly from this app directory.

The fulfillment-constraint capture requires the released
`conformance-fulfillment-constraint` Function plus
`read_fulfillment_constraint_rules` and
`write_fulfillment_constraint_rules`. Build or deploy from a Symphony workspace
with `CARGO_TARGET_DIR=target` so Shopify CLI finds each extension's WASM at the
path declared in its extension configuration.

Secrets are intentionally not committed.
