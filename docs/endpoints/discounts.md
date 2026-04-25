# Discounts Endpoint Group

The discounts group is tracked in the operation registry but has no implemented local runtime roots yet. Keep discount-specific capture and access-scope notes here instead of in `docs/architecture.md`.

## Implemented roots

None.

## Behavior notes

- `scripts/capture-discount-conformance.ts` probes the live conformance app Admin access scopes through `currentAppInstallation.accessScopes`.
- The capture script records `read_discounts` and `write_discounts` availability before attempting discount catalog captures.
- Tokens must come through `scripts/shopify-conformance-auth.mts`; repo `.env` files must not contain Admin access tokens.
- Discount capture fails before discount reads or writes when either required discount scope is missing.
- Discount capture files use the `discount-*` conformance naming convention only after scope checks pass.

## Validation anchors

- Registry/coverage tests: `tests/unit/operation-registry.test.ts`, `tests/unit/graphql-operation-coverage.test.ts`
- Capture helper tests: `tests/unit/discount-conformance-lib.test.ts`
