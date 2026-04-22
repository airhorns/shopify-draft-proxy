# Product publication conformance blocker

## What succeeded

Attempted to capture live conformance for the staged product publication family (`productPublish`, `productUnpublish`).

- safe `productPublish` / `productUnpublish` mutation payloads now capture successfully
- Shopify accepted a real publication target id during the successful safe mutation captures: `gid://shopify/Publication/82090459369`
- `productPublish` can therefore be promoted to covered parity for the successful live payload slice (`product { id }` plus `userErrors`)
- `productUnpublish` remains tracked by its own captured minimal payload slice until that operation is promoted separately

## Remaining field-level blocker

- aggregate publication fields remain blocked for this app
- publication aggregate reads still fail for this app even after the safe mutation succeeds
- current conformance credential family: `shpca`
- header mode: raw `X-Shopify-Access-Token` (raw-x-shopify-access-token)
- the active conformance credential is a Shopify user access token (`shpca_...`) sent as raw `X-Shopify-Access-Token` on this host

- publish mutation aggregate slice → Your app doesn't have a publication for this shop.
- post-publish downstream read → Your app doesn't have a publication for this shop.
- unpublish mutation aggregate slice → Your app doesn't have a publication for this shop.
- post-unpublish downstream read → Your app doesn't have a publication for this shop.

## Why the blocker remains explicit

The configured conformance app still does not have its own publication on this shop, so asking Shopify to resolve aggregate publication fields on `product` still returns `Your app doesn't have a publication for this shop.` on this host.

- current channel_config extension: `conformance-publication-target` @ `/tmp/shopify-conformance-app/hermes-conformance-products/extensions/conformance-publication-target/shopify.extension.toml`
- current channel_config create_legacy_channel_on_app_install = `true`
  Keep that blocker attached to the parity specs so future runs can distinguish the now-captured root mutation parity from the still-blocked aggregate publication field slice.

## Recommended next step

Install or configure the conformance app so it has a real publication on `very-big-test-store.myshopify.com`, then rerun `corepack pnpm conformance:capture-product-publications` to refresh the fixtures with successful aggregate publication field payloads and downstream reads.
If the channel config changed recently, do not assume deploy alone backfills a publication on the existing store install — reinstallation or explicit channel/publication setup may still be required.

## Evidence refresh commands

- `corepack pnpm conformance:probe`
- `corepack pnpm conformance:capture-product-publications`
- `corepack pnpm exec shopify app deploy --allow-updates`
