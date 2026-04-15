# Shopify conformance auth blocker

## Summary

Live Shopify conformance is currently blocked on this host because the mirrored Shopify CLI bearer-token grant is unrecoverable non-interactively.

## Evidence

- `corepack pnpm conformance:probe` failed with:
  - `401`
  - `[API] Service is not valid for authentication`
- Non-interactive refresh against `https://accounts.shopify.com/oauth/token` using the stored access token, refresh token, and Shopify CLI client id `fbdb2649-e327-4907-8f67-908d24cfd7e3` returned:
  - `400 invalid_grant`
  - `The provided access grant is invalid, expired, or revoked...`
- The repo `.env` and `~/.config/shopify-cli-kit-nodejs/config.json` both still point at the expired token pair.

## Impact

- `corepack pnpm conformance:probe` cannot authenticate
- `corepack pnpm conformance:capture-products` is blocked until auth is repaired
- Live introspection/capture for new Shopify behavior is unavailable in unattended runs

## Recommended next step

Prefer switching conformance to a dedicated dev-store Admin API token for `very-big-test-store.myshopify.com` instead of mirroring a rotating Shopify CLI account bearer token.

If a human chooses to repair CLI auth instead, they must re-authenticate Shopify CLI and persist the fresh token pair into both:

- `~/.config/shopify-cli-kit-nodejs/config.json`
- `/home/airhorns/code/shopify-draft-proxy/.env`
