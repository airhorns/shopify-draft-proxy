# Payments

This endpoint group tracks Admin GraphQL payment-area roots whose behavior is sensitive because they can expose payment settings, payment methods, payment status, or Shopify Functions-backed checkout configuration.

## Supported roots

- `paymentCustomization(id:)`
- `paymentCustomizations(...)`

The supported slice is read-only and snapshot/local. Lifecycle mutation roots such as `paymentCustomizationCreate`, `paymentCustomizationUpdate`, `paymentCustomizationDelete`, and `paymentCustomizationActivation` remain unsupported until they have a disposable Shopify Function/payment customization fixture and a local staging model that does not mutate checkout behavior at runtime.

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

## Access Scopes And Capture Notes

HAR-219 recorded that the refreshed 2026-04-25 conformance app can safely read payment customization empty/null behavior with `read_payment_customizations`, and HAR-223 captured that current empty/null slice in `payment-customization-empty-read`. Non-empty detail, Function ownership, and error-history behavior should be promoted into fixtures/parity specs only after real interactions exist and the comparison contract is ready.

Do not add planned-only parity specs for payment roots. Keep unsupported payment-area reads and writes as registry/workpad gaps until captured evidence can back local behavior.

## Validation

- `tests/integration/payment-customization-query-shapes.test.ts`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
