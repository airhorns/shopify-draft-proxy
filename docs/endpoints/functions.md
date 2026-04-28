# Functions

This endpoint group covers Shopify Function-backed Admin metadata roots for validations, cart transforms, Shopify Function catalog reads, and tax app readiness.

## Current support and limitations

### Supported roots

Queries:

- `validation(id:)`
- `validations(...)`
- `cartTransforms(...)`
- `shopifyFunction(id:)`
- `shopifyFunctions(...)`

Mutations:

- `validationCreate`
- `validationUpdate`
- `validationDelete`
- `cartTransformCreate`
- `cartTransformDelete`
- `taxAppConfigure`

### Local behavior

Function-backed behavior is modeled as metadata/state only. The proxy records the Function handle or ID attached to validations and cart transforms, creates local `ShopifyFunction` metadata rows for downstream reads, and updates the relevant detail/catalog roots after staged writes.

The runtime does not execute external Shopify Function code, invoke Function WASM, run checkout/cart transform behavior, or call tax calculation callbacks. `taxAppConfigure` stores readiness metadata only.

Supported mutation roots stage locally and append the original raw GraphQL request body to the mutation log for ordered `__meta/commit` replay. Runtime requests for these implemented roots must not proxy to Shopify.

### Boundaries

- Metafield inputs are not expanded into full owner-scoped metafield records for this first Functions increment; selected `metafield` returns `null` and selected `metafields` returns an empty connection.
- Live store authorization and app ownership checks are not reproduced locally. Tests should use this domain for draft proxy metadata behavior, not to validate app-extension deployment or tax app eligibility.
- Function execution outcomes remain out of scope. A future conformance-backed increment should capture checkout/cart/tax runtime side effects separately if the proxy ever needs to model them.

## Historical and developer notes

### Shape evidence

- Root availability is captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-graphql-root-operation-introspection.json`.
- Runtime local-staging evidence is recorded in `fixtures/conformance/local-runtime/2026-04/functions-metadata-flow.json` and enforced by `tests/integration/functions-flow.test.ts`.
- Shopify Admin docs for the current API describe `validationCreate` / `validationUpdate` inputs as Function-handle based validation metadata with `enable`, `blockOnFailure`, `metafields`, and `title`.
- Shopify Admin docs for `cartTransformCreate` expose direct `functionId` / `functionHandle`, `blockOnFailure`, and optional metafield inputs.
- Shopify Admin docs for `taxAppConfigure` expose a `ready: Boolean!` mutation returning `taxAppConfiguration` and `userErrors`.

### Validation

- `tests/integration/functions-flow.test.ts`
- `tests/unit/capabilities.test.ts`
- `tests/unit/capabilities-anonymous.test.ts`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
