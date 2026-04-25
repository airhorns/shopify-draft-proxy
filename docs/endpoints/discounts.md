# Discounts Endpoint Group

The discounts group has catalog-first local read support. Keep discount-specific capture, access-scope, and compatibility notes here instead of in `docs/architecture.md`.

## Implemented roots

Overlay reads:

- `discountNodes`
- `discountNodesCount`
- `discountNode`
- `codeDiscountNode`
- `codeDiscountNodeByCode`
- `automaticDiscountNode`

## Unsupported roots still tracked by the registry

- `codeDiscountNodes`
- `automaticDiscountNodes`
- `automaticDiscounts`

## Behavior notes

- `discountNodes` and `discountNodesCount` are served locally in snapshot mode from normalized `DiscountRecord` state.
- Singular detail roots are served locally in snapshot mode from the same normalized `DiscountRecord` state:
  - `discountNode(id:)` returns either code or automatic discount records by node ID.
  - `codeDiscountNode(id:)` returns locally known code discounts by ID.
  - `codeDiscountNodeByCode(code:)` indexes locally known redeem codes.
  - `automaticDiscountNode(id:)` returns locally known automatic discounts by ID.
- Supported local catalog behavior includes `first` / `last`, `before` / `after`, `reverse`, `sortKey`, count `limit`, and captured query filters such as `status`, `combines_with`, `discount_class`, `type` / `discount_type`, date fields, `times_used`, and `app_id`.
- Supported detail serialization covers common native code/automatic fields captured from Shopify 2026-04: title, status, summary, starts/ends timestamps, created/updated timestamps, usage counts, discount classes, `combinesWith`, redeem-code connections, all-buyer context, `customerGets`, minimum subtotal/quantity requirements, metafields, and events.
- Snapshot misses for singular roots return `null`. Known detail records with no metafields/events return Shopify-like empty connections and null singular metafields.
- Product, variant, collection, customer, and segment links are represented as normalized IDs on discount item/context selections; serializers expose selected `id` fields and hydrate simple titles/display names when the linked resource is already present in local state.
- App-managed discount objects (`DiscountCodeApp`, `DiscountAutomaticApp`) preserve `__typename` and common scalar fields that are present in normalized state. App-only fields such as `appDiscountType`, `errorHistory`, and app `discountId` are intentionally returned as `null` with `UNSUPPORTED_APP_DISCOUNT_FIELD` errors so local reads do not invent app-function data.
- Discount query parsing uses the shared Shopify-style search parser.
- Local connection cursors use the proxy's synthetic `cursor:<gid>` form; parity specs document Shopify's opaque cursor values as non-contractual.
- `codeDiscountNodes` and `automaticDiscountNodes` remain known registry entries but are not promoted to locally implemented support until their node-specific shapes have captured fixtures.
- Deprecated `automaticDiscounts` remains unsupported rather than mapped to `automaticDiscountNodes`; unknown/unsupported reads continue through the existing passthrough path outside snapshot-only parity execution.
- Discount mutation lifecycle support is not implemented yet, but the store exposes staged discount records so later locally staged discount mutations can appear in catalog/count reads without upstream writes.
- `scripts/capture-discount-conformance.ts` probes the live conformance app Admin access scopes through `currentAppInstallation.accessScopes`.
- The capture script records `read_discounts` and `write_discounts` availability before attempting discount catalog captures.
- The capture script also creates temporary native `DiscountCodeBasic` and `DiscountAutomaticBasic` records, captures singular detail payloads, and deletes those temporary records immediately after capture.
- Tokens must come through `scripts/shopify-conformance-auth.mts`; repo `.env` files must not contain Admin access tokens.
- Discount capture fails before discount reads or writes when either required discount scope is missing.
- Discount capture files use the `discount-*` conformance naming convention only after scope checks pass.

## Validation anchors

- Discount reads: `tests/integration/discount-query-shapes.test.ts`
- Conformance fixtures and requests: `config/parity-specs/discount*.json` and matching files under `config/parity-requests/`; singular detail fixtures are `discount-code-basic-detail-read.json` and `discount-automatic-basic-detail-read.json` under the 2026-04 conformance fixture directory.
- Registry/coverage tests: `tests/unit/operation-registry.test.ts`, `tests/unit/graphql-operation-coverage.test.ts`
- Capture helper tests: `tests/unit/discount-conformance-lib.test.ts`
