# Discounts Endpoint Group

The discounts group has catalog-first local read support. Keep discount-specific capture, access-scope, and compatibility notes here instead of in `docs/architecture.md`.

## Implemented roots

Overlay reads:

- `discountNodes`
- `discountNodesCount`

## Unsupported roots still tracked by the registry

- `codeDiscountNodes`
- `automaticDiscountNodes`
- `automaticDiscounts`

## Behavior notes

- `discountNodes` and `discountNodesCount` are served locally in snapshot mode from normalized `DiscountRecord` state.
- Supported local catalog behavior includes `first` / `last`, `before` / `after`, `reverse`, `sortKey`, count `limit`, and captured query filters such as `status`, `combines_with`, `discount_class`, `type` / `discount_type`, date fields, `times_used`, and `app_id`.
- Discount query parsing uses the shared Shopify-style search parser.
- Local connection cursors use the proxy's synthetic `cursor:<gid>` form; parity specs document Shopify's opaque cursor values as non-contractual.
- `codeDiscountNodes` and `automaticDiscountNodes` remain known registry entries but are not promoted to locally implemented support until their node-specific shapes have captured fixtures.
- Deprecated `automaticDiscounts` remains unsupported rather than mapped to `automaticDiscountNodes`; unknown/unsupported reads continue through the existing passthrough path outside snapshot-only parity execution.
- Discount mutation lifecycle support is not implemented yet, but the store exposes staged discount records so later locally staged discount mutations can appear in catalog/count reads without upstream writes.
- `scripts/capture-discount-conformance.ts` probes the live conformance app Admin access scopes through `currentAppInstallation.accessScopes`.
- The capture script records `read_discounts` and `write_discounts` availability before attempting discount catalog captures.
- Tokens must come through `scripts/shopify-conformance-auth.mts`; repo `.env` files must not contain Admin access tokens.
- Discount capture fails before discount reads or writes when either required discount scope is missing.
- Discount capture files use the `discount-*` conformance naming convention only after scope checks pass.

## Validation anchors

- Discount reads: `tests/integration/discount-query-shapes.test.ts`
- Conformance fixtures and requests: `config/parity-specs/discount*.json` and matching files under `config/parity-requests/`
- Registry/coverage tests: `tests/unit/operation-registry.test.ts`, `tests/unit/graphql-operation-coverage.test.ts`
- Capture helper tests: `tests/unit/discount-conformance-lib.test.ts`
