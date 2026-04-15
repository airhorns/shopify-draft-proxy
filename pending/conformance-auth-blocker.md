# Conformance auth blocker

- Probe failure: `401` with `[API] Service is not valid for authentication`
- Refresh result: Shopify Accounts returned `400 invalid_grant` for the stored CLI access/refresh token pair
- Current impact: both `~/.config/shopify-cli-kit-nodejs/config.json` and repo `.env` still point at the expired token pair
- Recommended next step: switch conformance to a dedicated dev-store Admin API token for `very-big-test-store.myshopify.com`, or have a human re-auth Shopify CLI and persist the new token pair into both locations before the next unattended conformance run
