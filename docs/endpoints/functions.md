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

Function-backed behavior is modeled as metadata/state only. The proxy records the Function handle or ID attached to validations and cart transforms, preserves or hydrates local `ShopifyFunction` metadata rows for downstream reads, and updates the relevant detail/catalog roots after staged writes.

The runtime does not execute external Shopify Function code, invoke Function WASM, run checkout/cart transform behavior, or call tax calculation callbacks. `taxAppConfigure` stores readiness metadata only.

Supported mutation roots stage locally and append the original raw GraphQL request body to the mutation log for ordered `__meta/commit` replay. Runtime requests for these implemented roots must not proxy to Shopify.

When a validation or cart transform references a `ShopifyFunction` already present in local state, the proxy preserves that Function's captured metadata, including `description`, `appKey`, and selected `app` fields. This is ownership evidence for Admin GraphQL metadata reads only. The proxy does not verify that the inbound token belongs to the same installed app or check Partner Dashboard extension release state.

`validationCreate` resolves the referenced Function from local state or cassette-backed hydration before staging. It rejects missing or multiple identifiers, unresolved references, and known non-validation API types without staging a `Validation` or inventing a `ShopifyFunction` row. Omitted or explicit `null` `title` input falls back to the resolved Function-derived title used by Shopify's create resolver, while an explicit empty string is preserved. Omitted `enable` / `enabled` input defaults to `false`, activation is capped at 25 active validations across effective state, and validation-owned metafields are persisted for downstream reads.

`cartTransformCreate` resolves the referenced Function from local state or cassette-backed hydration before staging, then persists cart-transform metadata locally. It also persists valid `metafields` inputs onto the staged cart transform in input order. Missing metafield values and malformed JSON values return `INVALID_METAFIELDS` userErrors with `field: ["metafields", index, "value"]` and do not stage a cart transform. Live Shopify Admin `2026-04` accepted an empty metafield namespace during exploratory capture, so the proxy does not reject that branch for cart transforms.

For known non-cart-transform `ShopifyFunction` references, Shopify distinguishes the two identifier branches even though the message copy is the same: the `functionId` branch returns `FUNCTION_NOT_FOUND` with `field: ["functionId"]`, while the `functionHandle` branch returns `FUNCTION_DOES_NOT_IMPLEMENT` with `field: ["functionHandle"]`. Both branches leave `cartTransforms` unchanged.

HAR-416 added live Shopify evidence for the conformance app's released Function catalog rows. On Admin API `2026-04`, `ShopifyFunction.id` is returned as a raw Function string ID, `apiType` is returned as lowercase strings such as `cart_checkout_validation` and `cart_transform`, and app ownership is exposed through `appKey` plus selected `app` fields. The local model preserves those exact values when they are seeded from conformance evidence; it does not normalize them to synthetic GIDs or enum-like uppercase values.

Local validation guardrails currently cover missing/multiple Function identifiers, unknown or API-mismatched `validationCreate` Function references, unknown validation/cart-transform update or delete IDs, `validationUpdate` attempts to pass non-input Function rebinding fields, and activation beyond the 25-active-validation cap. These branches return GraphQL errors or `userErrors` locally and still avoid runtime Shopify writes.

`validationCreate` and `validationUpdate` persist validation-owned `metafields` inputs into the staged `Validation` record. Subsequent `validation`, `validations`, and mutation payload reads expose those rows through `metafields(...)` connection selections, including namespace, key, type, value, timestamps, owner type, and compare digest when selected. `validation.metafield(namespace:, key:)` resolves locally persisted rows.

`cartTransformCreate` persists cart-transform-owned `metafields` inputs into the staged `CartTransform` record. Subsequent mutation payload and `cartTransforms` reads expose those rows through `metafields(...)` connection selections and `cartTransform.metafield(namespace:, key:)`, including namespace, key, type, value, timestamps, `compareDigest`, and Shopify's captured owner type `CARTTRANSFORM` when selected.

### Boundaries

- Live store authorization and app ownership checks are not reproduced locally. Tests should use this domain for draft proxy metadata behavior, not to validate app-extension deployment, released Function availability beyond captured/hydrated Function metadata, or tax app installation authority.
- Function execution outcomes remain out of scope. A future conformance-backed increment should capture checkout/cart/tax runtime side effects separately if the proxy ever needs to model them.

### HAR-455 fidelity review notes

Admin GraphQL 2026-04 Function metadata docs keep validation and cart-transform configuration centered on Function references plus metadata such as `blockOnFailure` and optional metafields. The proxy models those Admin metadata rows and downstream catalog/detail reads only; except for `validationCreate` rejecting a known non-validation `ShopifyFunction.apiType`, it does not validate extension release state, cross-app ownership, or Partner Dashboard deployment authority.

`shopifyFunctions` remains metadata-only evidence. It can prove that Function identity, handle, API type, and app ownership fields are preserved from seeded/captured metadata, but it does not prove that the corresponding Function code can run in checkout, cart transforms, or tax callbacks.

`taxAppConfigure` is intentionally stored as local readiness metadata. Shopify tax-app authority, tax calculation callbacks, and real tax-service readiness are external side effects that the current proxy cannot faithfully emulate without a suitably authorized disposable tax app.

## Historical and developer notes

### Shape evidence

- Root availability is captured in `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`.
- Runtime local-staging evidence is recorded in `fixtures/conformance/local-runtime/2026-04/functions/functions-metadata-flow.json`.
- App/owner metadata evidence is recorded in `fixtures/conformance/local-runtime/2026-04/functions/functions-owner-metadata-flow.json` and the `functions-owner-metadata-local-staging` parity spec; the scenario seeds known `ShopifyFunction` records and verifies validation/cart-transform lifecycle reads preserve captured `appKey` and `app` selections instead of inventing or dropping owner metadata.
- HAR-698 validation create evidence is recorded in `fixtures/conformance/local-runtime/2026-04/functions/functions-validation-create-validation.json` and enforced by `config/parity-specs/functions/functions-validation-create-validation.json`; the scenario verifies omitted `enable` defaults to `false`, the 26th active validation returns `MAX_VALIDATIONS_ACTIVATED`, non-validation Function API references return `FUNCTION_DOES_NOT_IMPLEMENT`, validation metafields persist, and a downstream `validations` connection observes the staged rows.
- `validationCreate` userError shape evidence is recorded in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/functions-validation-create-error-shape.json` and enforced by `config/parity-specs/functions/functions-validation-create-error-shape.json`; live Shopify returns `NOT_FOUND` with `field: ["validation", "functionId"]` for unknown Function ids, prefixes Function API mismatches and missing identifiers with `validation`, and returns `field: ["validation"]` for multiple Function identifiers.
- `validationCreate` title fallback evidence is recorded in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/validation-create-title-fallback-parity.json` and enforced by `config/parity-specs/functions/validation-create-title-fallback-parity.json`; live Shopify persists the same Function-derived title for omitted and explicit `null` title input, preserves an explicit empty string, and returns the persisted title through both `validation(id:)` and `validations(first:)` reads.
- `cartTransformCreate` API-mismatch evidence is recorded in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/functions-cart-transform-create-api-mismatch-by-identifier.json` and enforced by `config/parity-specs/functions/functions-cart-transform-create-api-mismatch-by-identifier.json`; live Shopify returns `FUNCTION_NOT_FOUND` for the `functionId` branch, `FUNCTION_DOES_NOT_IMPLEMENT` for the `functionHandle` branch, and an empty downstream `cartTransforms` connection after both failed mutations.
- `cartTransformCreate` metafield evidence is recorded in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/functions-cart-transform-create-metafields.json` and enforced by `config/parity-specs/functions/functions-cart-transform-create-metafields.json`; live Shopify returns `INVALID_METAFIELDS` for missing metafield value and malformed JSON value branches, persists valid cart-transform metafields in input order, resolves singular `metafield(namespace:, key:)`, and exposes the same rows through downstream `cartTransforms` reads.
- HAR-703 validation update shape evidence is recorded in `fixtures/conformance/local-runtime/2026-04/functions/functions-validation-update-shape.json` and enforced by `config/parity-specs/functions/functions-validation-update-shape.json`; the scenario verifies `validationUpdate` rejects Function rebinding input shape, enforces the 25-active-validation cap, persists validation metafields, and returns those metafields through a downstream `validations` connection.
- HAR-777 max-cap evidence is recorded in `fixtures/conformance/local-runtime/2026-04/functions/functions-validation-max-cap.json` and enforced by `config/parity-specs/functions/functions-validation-max-cap.json`; the scenario stages 25 active validations locally, verifies the next `validationCreate(enable: true)` and `validationUpdate(enable: true)` return `MAX_VALIDATIONS_ACTIVATED` with `field: []`, and keeps the supported mutations local-only.
- Live app ownership evidence is recorded in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/functions-live-owner-metadata-read.json` and enforced by `config/parity-specs/functions/functions-live-owner-metadata-read.json`. The fixture was captured after deploying the repo-local conformance app with released `conformance-validation` and `conformance-cart-transform` Function extensions, and the parity request verifies `shopifyFunction` / `shopifyFunctions` reads preserve `appKey` and selected `app` fields without Function execution.
- HAR-416 mutation probes against the live store now reach validation/cart-transform resolver userErrors. The fixture records wrong Function API type, unknown/unowned Function handle, invalid metafield, and duplicate cart-transform registration branches. Shopify allowed duplicate `validationCreate` calls for the same validation Function on this shop, so that branch is recorded as live success evidence plus cleanup rather than a duplicate userError. True cross-app references still require a second installed app; the fixture records unknown/unowned handles as the reachable unattended authority boundary. `taxAppConfigure` remains blocked by tax-calculation-app authority even with the refreshed grant, and the access-denied payload is preserved as blocker evidence.
- Shopify Admin docs for the current API describe `validationCreate` / `validationUpdate` inputs as Function-handle based validation metadata with `enable`, `blockOnFailure`, `metafields`, and `title`.
- Shopify Admin docs for `cartTransformCreate` expose direct `functionId` / `functionHandle`, `blockOnFailure`, and optional metafield inputs.
- Shopify Admin docs for `taxAppConfigure` expose a `ready: Boolean!` mutation returning `taxAppConfiguration` and `userErrors`.

### Follow-up gaps

- Capture true cross-app Function reference behavior only after a second installed conformance app exposes released validation/cart-transform Functions in the same shop.
- Capture tax app readiness userErrors only with a grant/app that has `write_taxes` and is authorized as a tax calculations app; until then, keep local `taxAppConfigure` as metadata-only readiness staging.
- Keep checkout cart transform execution, validation execution, and tax calculation callbacks out of this metadata endpoint group until separate runtime side-effect evidence exists.

### Validation

- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
