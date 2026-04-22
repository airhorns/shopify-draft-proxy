# orderUpdate expanded conformance auth blocker

## What this run checked

Attempted to refresh live Shopify conformance before capturing expanded `orderUpdate` parity for:

- shipping address
- customer email-adjacent fields exposed on `OrderInput`: `email`, `phone`, `poNumber`
- tags and note
- custom attributes
- order metafields

## Current blocker

The current stored Shopify conformance credential is unusable:

- failing command: `corepack pnpm conformance:probe`
- credential entry point: `scripts/shopify-conformance-auth.mts`
- credential path: `~/.shopify-draft-proxy/conformance-admin-auth.json`
- exact error: `Stored Shopify conformance access token is invalid and refresh failed: This request requires an active refresh_token`

## Current implementation stance

The local proxy stages expanded known-order `orderUpdate` fields from current Shopify Admin GraphQL docs and keeps the existing captured validation fixtures for unknown ID and missing ID behavior. The expanded parity fixture should be captured once a fresh conformance grant is available.
