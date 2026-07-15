# Storefront API

The Storefront API surface covers `/api/<version>/graphql.json` requests. The proxy currently supports a narrow read-only 2026-04 slice for store context roots that can be hydrated from authenticated Storefront reads and, for a few matching fields, shared Admin-observed store state.

## Current support and limitations

### Supported roots

Read roots:

- `shop`
- `localization`
- `locations`
- `paymentSettings`
- `publicApiVersions`

Mutation roots remain unsupported for local Storefront execution. In snapshot mode they return the Storefront snapshot mutation rejection response; in live-hybrid mode unimplemented Storefront roots continue through the Storefront passthrough path and are logged as Storefront traffic.

### Local behavior

The first-slice roots dispatch locally only for Storefront API version `2026-04`. Dispatch is keyed by the Storefront surface plus parsed root fields, so Admin roots with the same names stay isolated from Storefront handling. Selection aliases, fragments, built-in directives, GraphQL validation, and the selected API version are preserved by the Storefront route before local projection runs.

Live-hybrid reads hydrate missing first-slice base state through explicit Storefront upstream calls, then answer the caller from the instance-owned store. The hydrated state includes Storefront shop fields, context-keyed localization, payment settings, locations with captured cursors, and public API versions. Snapshot reads do not invent shop, localization, payment, location, market, or API-version values; empty state returns null objects or empty connections/lists according to the local no-data boundary.

`@inContext(country:, language:)` is parsed into a reusable Storefront request context. The current context model stores country and language values and leaves room for later buyer, company, and location context without adding a separate dispatcher.

`shop` projects selected Storefront fields from captured Storefront shop state when available. It may reuse Admin-observed `shop`, `primaryDomain`, shop policy, money-format, and payment-setting fields when those shapes line up. It does not fabricate policy handles, domains, brand assets, or payment account values when neither Storefront nor Admin state has supplied them.

`localization` is context-keyed. Default context and `@inContext(country:, language:)` reads hydrate separate records so later Storefront calls can observe the same country, language, and market selection without another upstream request.

`locations` projects a Storefront connection from captured Storefront locations plus locally staged or Admin-observed active, non-fulfillment-service locations. Captured Storefront cursors are retained. Locally observed Admin locations use deterministic ID cursors when no Storefront cursor has been captured. `first`, `after`, `last`, `before`, `reverse`, and representative Storefront sort keys are handled through the shared connection helpers.

`paymentSettings` uses captured Storefront payment settings first. When only Admin shop state is available, it projects the overlapping currency, presentment currency, country, and digital wallet fields and leaves Storefront-only payment fields null or absent according to the caller selection.

`publicApiVersions` returns captured Storefront API version records only after Storefront hydration. Snapshot mode returns an empty list rather than deriving API versions from checked-in schema metadata.

### Boundaries

This is not support for every field on `Shop`, `Localization`, `Location`, `PaymentSettings`, or related nested types. Fields outside the selected and hydrated boundary return null/empty values when no shared store state has supplied them. Storefront product, cart, customer, checkout, metaobject, content, and mutation domains remain outside this slice unless another endpoint document names them explicitly.
