# Product mutation conformance blocker

## What failed

Attempted to capture live conformance for the staged product mutation family (`productCreate`, `productUpdate`, `productDelete`).

- `productCreate`
- `productUpdate`
- `productDelete`

Live probe still works, but the first mutation capture failed immediately on Shopify Admin GraphQL:

- `ACCESS_DENIED`
- required access: `write_products` access scope. Also: The user must have a permission to create products.

Observed error excerpt:

> Access denied for productCreate field. Required access: `write_products` access scope. Also: The user must have a permission to create products.

## Why this blocks closure

Without a write-capable token, the repo cannot capture successful live mutation payload shape, userErrors behavior for safe writes, or immediate downstream read-after-write parity for `productCreate`, `productUpdate`, and `productDelete`.

## What was completed anyway

1. added a reusable live-write capture harness for the staged create/update/delete family
2. kept the rich create/update payload slice aligned with the existing parity-request scaffolds so a future write-capable token can capture the same shapes directly

## Recommended next step

Switch the repo conformance credential to a safe dev-store token with `write_products`, then rerun `corepack pnpm conformance:capture-product-mutations`.

