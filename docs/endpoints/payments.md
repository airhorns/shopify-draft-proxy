# Payments

This endpoint group tracks Admin GraphQL payment-area roots whose behavior is sensitive because they can expose payment settings, payment methods, payment status, or Shopify Functions-backed checkout configuration.

## Supported roots

- `paymentCustomization(id:)`
- `paymentCustomizations(...)`
- `paymentCustomizationCreate`
- `paymentCustomizationUpdate`
- `paymentCustomizationDelete`
- `paymentCustomizationActivation`
- `paymentTermsTemplates(paymentTermsType:)`

Payment customization writes are local-only once supported. They must not invoke Shopify Functions or mutate checkout payment behavior at runtime; commit replay keeps the original raw mutation for an explicit later commit.

## Payment customizations

Payment customization records live in normalized state as `PaymentCustomizationRecord` rows keyed by Admin GID. Snapshot-mode reads return:

- `paymentCustomization(id:)`: the modeled record when present, otherwise `null`
- `paymentCustomizations(...)`: a non-null connection with `nodes`, `edges`, and selected `pageInfo`; an empty normalized graph returns empty arrays and false/null pageInfo values

Catalog reads support local cursor pagination through `first`, `last`, `after`, and `before`, plus `reverse`. Search query support is intentionally limited to captured-safe filters:

- `enabled:true|false`
- `function_id:<gid-or-tail>`
- `id:<gid-or-tail>`
- default/title text matching over captured `title`

Local cursors use the proxy's synthetic `cursor:<gid>` form. Shopify's opaque cursor encoding is not a contract clients should depend on.

Selected scalar detail fields currently include `id`, `legacyResourceId`, `title`, `enabled`, and `functionId`. `shopifyFunction` and `errorHistory` are replayed from captured normalized JSON only; when those slices are absent the serializer returns `null` rather than inventing Function ownership or failure history. Owner-scoped `metafield(namespace:, key:)` and `metafields(...)` selections serialize from the payment customization's captured metafield rows using the shared metafield serializer.

Lifecycle mutations stage against the same normalized records:

- `paymentCustomizationCreate(paymentCustomization:)` creates a synthetic local `PaymentCustomization` record with selected metafields when required `title`, `enabled`, and a Function identifier are present. The current Admin API uses `functionHandle`; deprecated `functionId` remains accepted for captured 2025-01 parity branches.
- `paymentCustomizationUpdate(id:, paymentCustomization:)` merges scalar/metafield input over an existing normalized record. Updating either Function identifier clears the other local identifier and does not invent a `shopifyFunction` object.
- `paymentCustomizationActivation(ids:, enabled:)` toggles `enabled` on existing normalized records and returns the updated ids.
- `paymentCustomizationDelete(id:)` marks a normalized record deleted so downstream detail reads return `null` and catalog reads omit it.

Captured validation branches are modeled locally for missing create fields, missing Function id `gid://shopify/ShopifyFunction/0`, unknown update/delete ids, unknown activation ids, and empty activation id lists. The latest 2026-04 docs also expose `functionHandle`, `MULTIPLE_FUNCTION_IDENTIFIERS`, `FUNCTION_NOT_FOUND`, and `INVALID_METAFIELDS`; the local model accepts Function handles, rejects the captured invalid handle sentinels, rejects requests that provide both `functionId` and `functionHandle`, and rejects structurally invalid owner metafields. The current local model does not maintain a full Shopify Function catalog; successful create/update paths preserve the provided Function identifier and return `shopifyFunction` only if that field was present in normalized state.

## Access Scopes And Capture Notes

HAR-219 recorded that the refreshed 2026-04-25 conformance app can safely read payment customization empty/null behavior with `read_payment_customizations`, and HAR-223 captured that current empty/null slice in `payment-customization-empty-read`. The same conformance credential has `write_payment_customizations`; HAR-223 captured validation/error branches for missing Function ownership and unknown ids in `payment-customization-validation`.

The current test store had no released `ShopifyFunction` nodes at capture time, so non-empty live happy-path create/update/delete/activation remains local-runtime evidence rather than a live Shopify parity contract. HAR-223 added and deployed the repo-local `conformance-payment-customization` Function extension to the conformance app, but the existing store install still returned an empty `shopifyFunctions` catalog afterward; a future live happy-path capture likely needs a refreshed app install/grant that can see the released Function. Non-empty detail, Function ownership, and error-history behavior should be promoted into fixtures/parity specs only after real interactions exist and the comparison contract is ready.

Do not add planned-only parity specs for payment roots. Keep unsupported payment-area reads and writes as registry/workpad gaps until captured evidence can back local behavior.

## Payment terms templates

`paymentTermsTemplates(paymentTermsType:)` is modeled as a read-only local catalog in snapshot and live-hybrid modes. The normalized default catalog is based on the 2025-01 `harry-test-heelo.myshopify.com` capture from 2026-04-27 and preserves Shopify's order and scalar values for:

- `Due on receipt` (`RECEIPT`, `dueInDays: null`)
- `Due on fulfillment` (`FULFILLMENT`, `dueInDays: null`)
- `Net 7`, `Net 15`, `Net 30`, `Net 45`, `Net 60`, and `Net 90` (`NET`)
- `Fixed` (`FIXED`, `dueInDays: null`)

The optional `paymentTermsType` argument filters by exact enum value. Selected template fields currently include `id`, `name`, `description`, `dueInDays`, `paymentTermsType`, `translatedName`, and `__typename`. Payment terms lifecycle mutations (`paymentTermsCreate`, `paymentTermsUpdate`, and `paymentTermsDelete`) remain unsupported and may only pass through as unknown/unsupported operations; they are not registered as local capabilities.

## Validation

- `tests/integration/payment-customization-query-shapes.test.ts`
- `tests/integration/payment-terms-query-shapes.test.ts`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
