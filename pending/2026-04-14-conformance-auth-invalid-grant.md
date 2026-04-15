# Live conformance blocked: expired unrecoverable Shopify CLI grant pair

## Summary

`corepack pnpm conformance:probe` failed against `very-big-test-store.myshopify.com` with:

- HTTP `401`
- `[API] Service is not valid for authentication`

The repo `.env` conformance token and the Shopify CLI account session in `~/.config/shopify-cli-kit-nodejs/config.json` point at the same expired bearer-token grant pair.

A non-interactive refresh attempt against `https://accounts.shopify.com/oauth/token` using the stored `access_token`, `refresh_token`, and Shopify CLI client id `fbdb2649-e327-4907-8f67-908d24cfd7e3` returned:

- HTTP `400`
- `error: invalid_grant`

That means the currently persisted access/refresh pair is no longer recoverable non-interactively on this host.

## Options considered

1. **Refresh the existing Shopify CLI grant pair non-interactively**
   - Tried.
   - Blocked by `invalid_grant`.

2. **Keep retrying the same refresh request**
   - Rejected.
   - The grant pair is unrecoverable; repeated retries will not help.

3. **Switch conformance to a dedicated dev-store Admin API token**
   - Recommended.
   - Avoids dependence on short-lived Shopify CLI account bearer tokens for unattended cron work.

4. **Human repair of Shopify CLI auth**
   - Possible fallback.
   - Requires re-authenticating Shopify CLI, then persisting the new token pair into both:
     - `~/.config/shopify-cli-kit-nodejs/config.json`
     - `/home/airhorns/code/shopify-draft-proxy/.env`

## Blocked downstream work

Anything requiring fresh live Shopify conformance is blocked until auth is repaired, including:

- `corepack pnpm conformance:probe`
- `corepack pnpm conformance:capture-products`
- new live fixture capture to settle uncertain Shopify behavior

## Recommended next step

Prefer a dedicated Admin API token for `very-big-test-store.myshopify.com` in repo `.env` for unattended conformance work. If that is not available, a human should re-auth Shopify CLI and persist the new token pair to both the CLI config and repo `.env` before the next autonomous surface-expansion pass.
