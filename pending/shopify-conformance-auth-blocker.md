# Shopify conformance auth blocker

- Probe failure: `401` with `[API] Service is not valid for authentication`
- Refresh attempt against `https://accounts.shopify.com/oauth/token` returned `400 invalid_grant`
- Both `~/.config/shopify-cli-kit-nodejs/config.json` and repo `.env` point at the expired Shopify CLI token pair

## Recommended next step

Switch conformance to a dedicated dev-store Admin API token for `very-big-test-store.myshopify.com`, or have a human re-authenticate Shopify CLI and persist the new token pair into both the CLI config and repo `.env`.
