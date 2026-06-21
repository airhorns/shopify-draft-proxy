---
title: 'Markets Endpoint Group'
description: 'Coverage notes and fidelity boundaries for Markets Endpoint Group.'
---

This endpoint group covers Shopify Admin GraphQL Markets roots for markets,
catalogs, price lists, quantity pricing, web presences, resolved buyer values,
and market-localized content.

## Current support and limitations

### Implemented local roots

The Rust operation registry marks the Markets roots below as locally
implemented: they route through `LOCAL_DISPATCH_ROOTS` and are answered without
runtime Shopify writes. Registry implementation is still narrower than support
for arbitrary Markets documents.

Implemented read roots:

- `market`
- `markets`
- `catalog`
- `catalogs`
- `catalogsCount`
- `priceList`
- `priceLists`
- `webPresences`
- `marketsResolvedValues`
- `marketLocalizableResource`
- `marketLocalizableResources`

Implemented mutation roots:

- `marketCreate`
- `marketUpdate`
- `catalogCreate`
- `catalogUpdate`
- `catalogContextUpdate`
- `catalogDelete`
- `priceListCreate`
- `priceListUpdate`
- `priceListDelete`
- `priceListFixedPricesAdd`
- `priceListFixedPricesUpdate`
- `priceListFixedPricesDelete`
- `priceListFixedPricesByProductUpdate`
- `quantityPricingByVariantUpdate`
- `quantityRulesAdd`
- `quantityRulesDelete`
- `webPresenceCreate`
- `webPresenceUpdate`
- `webPresenceDelete`
- `marketLocalizationsRegister`
- `marketLocalizationsRemove`

Other Markets roots remain registry-known but unsupported until the proxy has
local lifecycle/read models and executable evidence for those roots.

### Local behavior

The Rust runtime has scenario-backed Markets slices for ported parity requests
and runtime tests. These slices stage local state only for the request families
recognized by the Rust dispatcher and should not be treated as broad registry
support.

Market lifecycle slices cover `marketCreate`, `marketUpdate`, and downstream
`market(id:)` reads for staged records. The local model stages selected scalar,
status/enabled, region, price-inclusion, currency-settings, catalog, and web
presence relations; rejects captured status/enabled mismatches, incompatible
price inclusions, invalid or unsupported country regions, duplicate region
codes, invalid names, duplicate names, and generated-handle collisions; and
retains original raw mutations for commit replay on successful staging.
Unsupported country-region validation is driven by a generated Shopify-derived
Markets set captured from live `CountryCode` enum probes; the 2026-04 evidence
rejects `AN`, `BV`, `CU`, `HM`, `IR`, `KP`, and `SY` before staging.
Captured `marketCreate` name validation rejects blank names with `BLANK` then
`TOO_SHORT`, rejects one-character names with `TOO_SHORT`, and treats name
uniqueness as case-insensitive before handle generation.

Staged `currencySettings.baseCurrency.currencyCode` preserves the requested
enum value unchanged. `currencyName` is projected from a local ISO-4217
display-name table for known codes, including the currencies observed in
checked-in Markets conformance fixtures. If a future Shopify enum value is not
yet mapped, the runtime returns `Unknown Currency` instead of echoing the ISO
code as a misleading display name.

Catalog slices cover `catalogCreate`, `catalogUpdate`, `catalogContextUpdate`,
`catalogDelete`, and downstream `catalog` / `catalogs` reads for market-backed
catalog records. They validate missing contexts, unknown catalog/market/price
list/publication IDs, duplicate or taken relations where captured, remove-only
context updates, and delete-detach behavior for linked price lists.

Price-list and quantity-pricing slices stage selected price list records,
fixed-price rows, quantity rules, and quantity price breaks for captured
product and variant IDs. Downstream `priceList` / `priceLists` reads expose the
staged rows in the checked-in scenarios. Validation covers name, currency,
parent adjustment, `catalogId` existence/taken checks, unknown resource,
duplicate fixed-price, missing fixed-price, product-level fixed-price, no-op,
quantity-rule, and price-limit branches represented by parity specs. Captured
Admin API 2026-04 behavior returns `CATALOG_DOES_NOT_EXIST` or
`CATALOG_TAKEN` at `["input", "catalogId"]` for price-list catalog relation
validation, and `priceListUpdate` returns `priceList: null` for those catalog
validation failures while leaving the staged price list unchanged.

Web-presence slices stage create/update/delete behavior for the captured
subfolder, default-locale, alternate-locale, root-URL, duplicate-language,
primary-domain-delete, and relation scenarios. Market-localization slices stage
and remove localized content for captured localizable resources, including
unknown-resource, too-many-key, digest, market key, and no-op removal branches.

`marketsResolvedValues` and market/catalog/price-list reads have fixture-backed
empty, fallback, and buyer-country behavior where captured. Unsupported
catalog, price-list, B2B/app catalog, contextual pricing, and resolved-value
derivations are not synthesized beyond the checked-in evidence.

### Boundaries

- Implemented Markets roots are local-runtime slices, not broad support for the
  whole Markets domain. Unsupported root shapes must still fall through the
  configured unsupported path and stay visible in logs/observability.
- Catalog membership and price-list semantics outside the modeled
  market-catalog and fixed-price/quantity-pricing slices remain unsupported.
- Validation-only Markets specs prove guardrail payloads and no-stage behavior
  for those inputs only. They do not make the corresponding mutation roots
  generally supported.
- Relative price-list adjustments, market priority conflicts, B2B/app catalog
  contextual pricing, and variants without modeled fixed-price rows remain
  explicit fidelity gaps.
- Deprecated `marketWebPresenceCreate`, `marketWebPresenceUpdate`, and
  `marketWebPresenceDelete` aliases are not marked implemented without their
  own payload, cleanup, and validation evidence.
- Unsupported mutation documents outside the ported local slices follow the
  configured unsupported path and must remain visible in logs/observability.

### Evidence

- Registry status: `src/operation_registry.rs`
- Runtime coverage: `tests/graphql_routes.rs`
- Unsupported market country-region parity: `config/parity-specs/markets/market-create-unsupported-country-region.json`
- Unsupported market country-region capture: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/market-create-unsupported-country-region.json`
- Markets parity specs: `config/parity-specs/markets/*.json`
- Related product contextual-pricing parity: `config/parity-specs/products/product-contextual-pricing-price-list-read.json`
- Related B2B quantity-rules parity: `config/parity-specs/b2b/quantity-rules-extended-validation.json`
- Markets fixtures: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/markets/*.json` and `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/*.json`

### Validation

- `corepack pnpm parity -- market-create-unsupported-country-region`
- `corepack pnpm lint`
- `corepack pnpm rust:test`
