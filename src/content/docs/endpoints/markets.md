---
title: 'Markets Endpoint Group'
description: 'Coverage notes and fidelity boundaries for Markets Endpoint Group.'
---

This endpoint group covers Shopify Admin GraphQL Markets roots for markets,
catalogs, price lists, quantity pricing, web presences, resolved buyer values,
and market-localized content.

## Current support and limitations

### Supported roots

The current Rust operation registry does not mark any Markets root as fully
implemented. Registry presence is a local-model commitment only; it is not a
claim that the whole Markets domain is supported for arbitrary documents.

The registry-only read roots are:

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
- `marketLocalizableResourcesByIds`

The registry-only mutation roots are:

- `marketCreate`
- `marketUpdate`
- `marketDelete`
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
Captured `marketCreate` name validation rejects blank names with `BLANK` then
`TOO_SHORT`, rejects one-character names with `TOO_SHORT`, and treats name
uniqueness as case-insensitive before handle generation.

Catalog slices cover `catalogCreate`, `catalogUpdate`, `catalogContextUpdate`,
`catalogDelete`, and downstream `catalog` / `catalogs` reads for market-backed
catalog records. They validate missing contexts, unknown catalog/market/price
list/publication IDs, duplicate or taken relations where captured, remove-only
context updates, and delete-detach behavior for linked price lists.

Price-list and quantity-pricing slices stage selected price list records,
fixed-price rows, quantity rules, and quantity price breaks for captured
product and variant IDs. Downstream `priceList` / `priceLists` reads expose the
staged rows in the checked-in scenarios. Validation covers name, currency,
parent adjustment, unknown resource, duplicate fixed-price, missing fixed-price,
product-level fixed-price, no-op, quantity-rule, and price-limit branches
represented by parity specs.

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

- Markets roots remain `implemented: false` in the current operation registry.
  Scenario-backed local staging is limited to the ported request documents and
  runtime tests.
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
- Markets parity specs: `config/parity-specs/markets/*.json`
- Related product contextual-pricing parity: `config/parity-specs/products/product-contextual-pricing-price-list-read.json`
- Related B2B quantity-rules parity: `config/parity-specs/b2b/quantity-rules-extended-validation.json`
- Markets fixtures: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/markets/*.json` and `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/*.json`

### Validation

- `corepack pnpm lint`
- `corepack pnpm rust:test`
