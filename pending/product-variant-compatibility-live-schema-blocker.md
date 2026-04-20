# Product variant compatibility live schema blocker

## What failed

Attempted to probe the legacy single-variant compatibility roots against the live Shopify Admin GraphQL schema used by the conformance store.

## Evidence

- store: `very-big-test-store.myshopify.com`
- api version: `2025-01`
- dedicated probe command: `corepack pnpm conformance:probe-product-variant-compatibility-roots`
- live schema rejected all three compatibility roots before any write-path parity capture could run:
- `productVariantCreate`
  > Field 'productVariantCreate' doesn't exist on type 'Mutation'
- `productVariantUpdate`
  > Field 'productVariantUpdate' doesn't exist on type 'Mutation'
- `productVariantDelete`
  > Field 'productVariantDelete' doesn't exist on type 'Mutation'

## Why this blocks closure

The repo still implements `productVariantCreate`, `productVariantUpdate`, and `productVariantDelete` as compatibility roots, but the current 2025-01 live schema on the conformance store does not expose those mutation fields. Without direct live roots, this family cannot be promoted from declared-gap to covered via first-party mutation capture on the current store/api-version pair.

## What was completed anyway

1. added a dedicated live-schema probe command for the single-variant compatibility family instead of leaving the blocker as an inferred sentence in generated docs
2. refreshed a durable blocker note from the current host token and store so future runs can verify the schema drift explicitly
3. preserved the adjacent live-supported bulk variant family as the real parity baseline (`productVariantsBulkCreate`, `productVariantsBulkUpdate`, `productVariantsBulkDelete`) rather than faking direct coverage for missing roots

## Recommended next step

If Shopify reintroduces these compatibility roots on a future API version/store, rerun `corepack pnpm conformance:probe-product-variant-compatibility-roots` and then capture direct live parity. Otherwise keep treating this family as a compatibility-only declared gap while the bulk variant family remains the covered live path.
