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
implemented: their canonical registry entries are answered without runtime
Shopify writes. Registry implementation is still narrower than support for
arbitrary Markets documents.

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

The Rust runtime has scenario-backed Markets slices for parity requests and
runtime tests. These slices stage local state only for the request families
recognized by the Rust dispatcher and should not be treated as broad registry
support.

Market lifecycle slices cover `marketCreate`, `marketUpdate`, and downstream
`market(id:)` reads for staged records. The local model stages selected scalar,
status/enabled, region, price-inclusion, currency-settings, catalog, and web
presence relations; rejects captured status/enabled mismatches, incompatible
price inclusions, invalid or unsupported country regions, duplicate region
codes, invalid names, duplicate names, and generated-handle collisions; and
retains original raw mutations for commit replay on successful staging.
Staged country-region nodes are stored as `MarketRegionCountry` records with a
stable synthesized `id`, deterministic ISO country `name`, `code`, and
`__typename`, so mutation payloads and downstream `market` / `markets` overlay
reads expose the same region-node shape.
Local `markets`, `webPresences`, `market.catalogs`,
`market.webPresences`, `Catalog.markets`, and `MarketWebPresence.markets`
projections use the shared connection helpers for selected `nodes`, `edges`,
stable ID cursors, selected `pageInfo`, and `first` / `last` / `after` /
`before` cursor windows. Local `markets(query:, sortKey:, reverse:)` applies
supported query filtering before sort, reverse, and cursor windowing. The local
query slice supports free-text matching plus `id:`, `name:`, `handle:`,
`status:`, `type:`, and `enabled:` terms; unrecognized keyed filters are
treated as unsupported terms and return an empty staged connection rather than
broadly matching every staged market. Supported deterministic sort keys are
`ID`, `NAME`, `HANDLE`, `STATUS`, and `TYPE`, with unknown sort keys falling
back to ID order.
Unsupported country-region validation is driven by a generated Shopify-derived
Markets set captured from live `CountryCode` enum probes; the 2026-04 evidence
in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/market-create-unsupported-country-region.json`
probed 245 `CountryCode` enum values and rejects `AN`, `BV`, `CU`, `HM`,
`IR`, `KP`, and `SY` before staging.
Captured `marketCreate` name validation rejects blank names with `BLANK` then
`TOO_SHORT`, rejects one-character names with `TOO_SHORT`, and treats name
uniqueness as case-insensitive before handle generation.

Staged `currencySettings.baseCurrency.currencyCode` preserves the requested
enum value unchanged. When `currencySettings` is present without an explicit
`baseCurrency`, `marketCreate` and `marketUpdate` default the base currency to
the observed shop currency rather than assuming a fixed store currency.
`currencyName` is projected from a local ISO-4217 display-name table for known
codes, including the currencies observed in checked-in Markets conformance
fixtures. If a future Shopify enum value is not yet mapped, the runtime returns
`Unknown Currency` instead of echoing the ISO code as a misleading display name.
Base-currency input uses Shopify-style `CurrencyCode` variable coercion: public
enum values such as `XAF` stage locally, while non-enum values such as `ZZZ`
return top-level `INVALID_VARIABLE` before resolver execution.

Catalog slices cover `catalogCreate`, `catalogUpdate`, `catalogContextUpdate`,
`catalogDelete`, and downstream `catalog` / `catalogs` reads for staged market,
company-location, and country catalog records. They validate missing contexts,
unknown catalog/market/company-location/price-list/publication IDs, context
driver mismatches, duplicate or taken relations where captured, remove-only
context updates, and delete-detach behavior for linked price lists.
`catalogContextUpdate` reads `marketIds`, `companyLocationIds`, legacy
`locationIds`, and `countryCodes` from add/remove inputs and applies the local
`(existing - remove) + add` diff to the catalog's context dimension. Captured
Admin API 2026-04 behavior returns `MARKET_NOT_FOUND` at
`["input", "context", "marketIds", <index>]` when `catalogCreate` references an
unknown context market ID, `COMPANY_LOCATION_NOT_FOUND` at indexed
`companyLocationIds` paths for missing company locations, and
`CONTEXT_DRIVER_MISMATCH` when a context input does not match the catalog's
driver type. Catalog relation IDs must resolve from staged
price-list/publication state, or from read-only upstream hydration in
non-snapshot modes; hardcoded relation IDs are not treated as owned records.
After a local catalog write, the Markets overlay serves `catalogsCount(type:
MARKET)` from staged catalog state with `EXACT` precision instead of returning
null or falling back to cold-only upstream data.
`catalogs` and `catalogsCount` share the same staged catalog working set for
market, company-location, and country catalogs. Catalog connection reads honor
`type`, captured-safe `query` terms for bare text plus `id:`, `title:`,
`status:`, and `type:`, default `sortKey: ID`, `sortKey: TITLE`, `reverse`,
cursor windows through `first`, `last`, `after`, and `before`, and return
selected `nodes`, `edges { cursor node }`, and computed `pageInfo`.
`catalogsCount(limit:)` applies Shopify-style `EXACT` / `AT_LEAST` precision to
the same filtered list.

Price-list and quantity-pricing slices stage selected price list records,
fixed-price rows, quantity rules, and quantity price breaks for captured
product and variant IDs. Downstream `priceList` / `priceLists` reads expose the
staged rows in the checked-in scenarios. Local `priceLists` reads apply standard
connection windows and computed `pageInfo` over staged price lists.
`PriceList.prices` applies read-time connection windows, `originType`, and the
captured fixed-price ID search filters `variant_id:` and `product_id:`. Other
`PriceList.prices(query:)` terms intentionally return an empty local connection
instead of guessing Shopify's broader search grammar. Validation covers name,
currency, parent adjustment, `catalogId` existence/taken checks, unknown
resource, duplicate fixed-price, missing fixed-price, fixed-price `price` /
`compareAtPrice` currency mismatches, fixed-price missing-variant short-circuit
behavior, product-level fixed-price, no-op, quantity-rule, and price-limit
branches represented by parity specs. Captured Admin API 2026-04 behavior
returns `CATALOG_DOES_NOT_EXIST` or
`CATALOG_TAKEN` at `["input", "catalogId"]` for price-list catalog relation
validation. When `priceListCreate` has both a catalog relation error and
another invalid field such as a duplicate name or invalid parent adjustment, the
catalog error is returned first. `priceListUpdate` returns `priceList: null` for
those catalog validation failures while leaving the staged price list unchanged.
`quantityPricingByVariantUpdate`, `quantityRulesAdd`, and `quantityRulesDelete`
validate price-list IDs against staged or hydrated price-list records instead of
accepting arbitrary IDs. Quantity-pricing add-side currency validation compares
the submitted money currency to the referenced price list's actual currency, and
quantity-pricing / quantity-rules variant validation uses observed base/staged
ProductVariant and fixed-price variant state when variant state is available;
unknown IDs return the Shopify-like variant user error instead of being treated
as successfully updated or deleted.

Web-presence slices stage create/update/delete behavior for the captured
subfolder, default-locale, alternate-locale, root-URL, duplicate-language,
primary-domain-delete, and relation scenarios. Documents that co-select
price-list roots with `webPresenceCreate`, `webPresenceUpdate`,
`webPresenceDelete`, or `quantityRulesDelete` use the same staged stores and
payload validation as the standalone local paths. Market-localization slices
stage and remove localized content for captured localizable resources, including
unknown-resource, too-many-key, digest, market key, and no-op removal branches.
`marketLocalizableResource` resolves only resource IDs observed in staged
market-localizable resource state or staged market-scoped translations; unknown
IDs return `null`.

`marketsResolvedValues` and market/catalog/price-list reads have fixture-backed
empty, fallback, and buyer-country behavior where captured. Resolved value
`currencyCode` uses the observed shop currency. For `priceInclusivity`, taxes
come from the matching staged market's tax price-inclusion setting when the
buyer country resolves to that market, then from observed shop tax-inclusion
flags when no market-specific setting applies. Duties stay false unless an
observed base shop state explicitly provides a duty-inclusion flag; public
Admin GraphQL 2026-04 accepted `INCLUDE_DUTIES_IN_PRICE` on a Market record but
still resolved `marketsResolvedValues.priceInclusivity.dutiesIncluded` as
false for the captured buyer signal. Unsupported catalog, price-list, B2B/app
catalog, contextual pricing, and richer resolved-value derivations are not
synthesized beyond the checked-in evidence.

### Boundaries

- Implemented Markets roots are local-runtime slices, not broad support for the
  whole Markets domain. Unsupported root shapes must still fall through the
  configured unsupported path and stay visible in logs/observability.
- Catalog membership and price-list semantics outside the modeled
  market-catalog, company-location catalog, country catalog, and
  fixed-price/quantity-pricing slices remain unsupported.
- Catalog search predicates outside bare text, `id:`, `title:`, `status:`, and
  `type:` remain unsupported and are treated as no-match filters locally.
- Captured Admin API 2026-04 parity for non-market `catalogContextUpdate`
  covers `companyLocationIds`; country-code and legacy `locationIds` context
  updates are runtime-test-backed local behavior because those input fields are
  not exposed by the captured 2026-04 `CatalogContextInput` schema.
- Validation-only Markets specs prove guardrail payloads and no-stage behavior
  for those inputs only. They do not make the corresponding mutation roots
  generally supported.
- Relative price-list adjustments, market priority conflicts, B2B/app catalog
  contextual pricing, and variants without modeled fixed-price rows remain
  explicit fidelity gaps.
- Deprecated `marketWebPresenceCreate`, `marketWebPresenceUpdate`, and
  `marketWebPresenceDelete` aliases are not marked implemented without their
  own payload, cleanup, and validation evidence.
- Unsupported mutation documents outside the modeled local slices follow the
  configured unsupported path and must remain visible in logs/observability.
