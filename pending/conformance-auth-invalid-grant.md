# Conformance auth blocker: expired Shopify CLI bearer token pair

## Summary

Live conformance for `very-big-test-store.myshopify.com` is currently blocked on this host.

## Evidence

- `corepack pnpm conformance:probe` returned:
  - HTTP `401`
  - payload: `[API] Service is not valid for authentication`
- The repo `.env` and `~/.config/shopify-cli-kit-nodejs/config.json` both point at the same expired Shopify CLI bearer-token grant pair.
- A non-interactive refresh attempt against `https://accounts.shopify.com/oauth/token` using the stored access token, refresh token, and Shopify CLI client id `fbdb2649-e327-4907-8f67-908d24cfd7e3` returned:
  - HTTP `400`
  - OAuth error `invalid_grant`

## Why this is blocked

An `invalid_grant` refresh result means the persisted access/refresh pair is no longer recoverable non-interactively. Per the host-specific conformance workflow, retrying the same refresh request will not repair the session.

## Recommended next step

Switch conformance to a dedicated dev-store Admin API token for `very-big-test-store.myshopify.com` instead of the rotating Shopify CLI account bearer token.

If a human chooses to repair the CLI-auth path instead, they need to:

1. re-authenticate Shopify CLI interactively
2. persist the fresh token pair into both:
   - `~/.config/shopify-cli-kit-nodejs/config.json`
   - `/home/airhorns/code/shopify-draft-proxy/.env`
3. rerun:
   - `corepack pnpm conformance:probe`
   - `corepack pnpm conformance:capture-products`
