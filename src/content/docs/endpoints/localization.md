---
title: 'Localization'
description: 'Coverage notes and fidelity boundaries for Localization.'
---

This endpoint group covers Shopify Admin GraphQL locale, shop-locale,
translatable-resource, and translation roots.

## Current support and limitations

### Supported roots

The current Rust operation registry does not mark any localization root as fully
implemented. Registry presence is a local-model commitment only; it is not a
claim that the whole localization lifecycle is supported for arbitrary
documents.

The registry-only read roots are:

- `availableLocales`
- `shopLocales`
- `translatableResource`
- `translatableResources`
- `translatableResourcesByIds`

The registry-only mutation roots are:

- `shopLocaleEnable`
- `shopLocaleUpdate`
- `shopLocaleDisable`
- `translationsRegister`
- `translationsRemove`

### Local behavior

The Rust runtime has a store-backed locale-catalog slice plus scenario-backed
translation branches for ported parity requests and runtime tests.
`availableLocales` and `shopLocales` project from the proxy store's baseline
locale state plus staged shop-locale rows; plain catalog reads are not selected
by document name. The runtime stages selected `shopLocaleEnable`,
`shopLocaleUpdate`, and `shopLocaleDisable` requests locally, and exposes
downstream `shopLocales` read-after-write behavior for the staged locale rows.
Supported staged shop-locale mutations append replay-ready mutation-log entries
with the original raw GraphQL request.

The staged shop-locale slice rejects primary-locale mutation attempts,
unsupported locale codes, duplicate enables, missing locales whenever
`shopLocaleUpdate` supplies `published`, enables beyond Shopify's captured
20-language shop limit, and disables for non-enabled locales with captured
Shopify-like `userErrors`.
Those shop-locale mutation payloads expose Shopify's plain `UserError`
shape: `field` and `message` are selectable, while selecting `code` is rejected
as a top-level GraphQL `undefinedField` validation error.
Market-web-presence IDs are filtered to known local, staged, captured, or
upstream-hydrated WebPresence IDs, and accepted rows project selected
`marketWebPresences`, `defaultLocale`, and locale scalar fields. In LiveHybrid
mode, localization mutation preflight hydrates referenced `Market` and
`MarketWebPresence` target IDs before local shop-locale and market-scoped
translation validation.
For `shopLocaleUpdate`, the primary-locale guard applies when the input supplies
a non-null `published` value, whether `true` or `false`; primary-locale updates
that only supply `marketWebPresenceIds` remain accepted by this slice.
The baseline shop locale includes the captured primary English row, and staged
enable/update/disable effects are merged with that baseline for subsequent
`shopLocales` reads.

`translationsRegister` and `translationsRemove` are locally modeled for the
ported product, collection, product-metafield, and market-scoped translation
scenarios. For Product resource IDs, the local slice validates existence against
known localization Product resources plus normalized product state before
applying translation-specific validation; unknown Product GIDs return
`RESOURCE_NOT_FOUND` with `field: ["resourceId"]` and `translations: null`. The
slice also validates enabled non-primary locale requirements, modeled Product
translatable-key membership (`INVALID_KEY_FOR_MODEL`), market-scoped values that
match an existing shop-level translation for the same resource/key/locale
(`FAILS_RESOURCE_VALIDATION` / `Value cannot match original content`), digest
mismatches, non-blank values, the 100-key mutation limit, market scope, and
selected handle-normalization branches. Successful translations are staged in
local translation state so
subsequent `translatableResource.translations(...)` reads observe the staged or
removed rows. Staged `Translation` rows include a synthetic DateTime-shaped
`updatedAt` value in the `translationsRegister` echo and in downstream
`translatableResource(...).translations` reads; re-registering an existing row
refreshes that timestamp. `translationsRemove` removes every requested
translation-key/locale/market combination that exists in staged state. An empty
`translationKeys` list matches no rows and returns Shopify's no-op
`translations: null, userErrors: []` payload.
For `translationsRegister` rows that violate multiple rules, captured Shopify
behavior validates locale and market gates before translation-record value and
digest validation; the market-scoped value-matches-base-translation check runs
before digest validation, so the local first `userErrors` entry follows that
precedence. Captured Shopify behavior accepts a market-scoped value matching the
source content when no shop-level translation exists for that locale/key.
Market-scoped `translationsRegister` checks market existence from store state or
upstream hydration and limits market-customizable translation keys to modeled
resource/key pairs. Modeled Product keys include `title`, `body_html`, and
`product_type`; modeled Collection keys include `title` and `body_html`.
Unmodeled market-custom resources such as `PackingSlipTemplate.body` return
`RESOURCE_NOT_MARKET_CUSTOMIZABLE`. `translationsRemove` with an unknown
market ID follows the captured Shopify no-op shape without staging removals.

For modeled Product resources, `translatableResource`,
`translatableResources`, and `translatableResourcesByIds` project
`translatableContent` from the effective Product record instead of from a
placeholder. Product content includes title, handle, product type, and populated
body HTML / SEO source fields, with Shopify key names such as `body_html`,
`product_type`, `meta_title`, and `meta_description`; each entry carries the
shop primary locale, the modeled `LocalizableContentType`, the source value, and
`sha256(value)` as the digest. The digest emitted by these reads is accepted by
the local `translationsRegister(translatableContentDigest:)` guard for the same
resource and key. Modeled Collection resources use the same source-backed
projection for title, handle, body HTML, and SEO fields that exist in local
state.

Collection translation lifecycle and market-scoped translation read support
remain fixture-backed. Product and product-metafield translation behavior has
runtime coverage for guardrails that the generic parity replay cannot isolate
cleanly.

### Boundaries

- Localization roots are handled locally for the ported request families and are
  marked `implemented` in the operation registry (i.e. they answer locally rather
  than 501), but that flag is not a broad-support claim: documents outside the
  ported request families fall through to passthrough and must not be treated as
  supported local behavior.
- `TranslatableResource` support is limited to product, collection, and
  product-metafield evidence. Product existence checks use localization baseline
  resources and normalized product state; collection and product-metafield
  translation scenarios remain fixture-backed. Other resource families return
  null/empty results and do not emit synthetic `title` content until local
  lifecycle behavior is modeled.
- Validation-only localization specs prove guardrail payloads and no-stage
  behavior for those inputs only. They do not make unmodeled translation or
  resource families generally supported.
- Digest sanitization for complex HTML remains bounded by checked-in capture
  evidence and runtime guardrails; the proxy does not claim complete Shopify
  sanitizer fidelity.
- Supported mutation slices stage locally and do not update Shopify at runtime;
  unmatched unsupported mutation documents follow the configured unsupported
  path.

### Evidence

- Registry status: `src/operation_registry.rs`
- Runtime coverage: `tests/graphql_routes.rs`, including store-backed
  `availableLocales` / `shopLocales` catalog reads without ported document-name
  markers
- Product translatable-content parity:
  `config/parity-specs/localization/localization-translatable-content-product.json`
- Shop-locale plain `UserError` parity:
  `config/parity-specs/localization/localization-shop-locale-usererror-no-code.json`
- Parity specs: `config/parity-specs/localization/*.json`
- Market-scoped translation parity:
  `config/parity-specs/localization/localization-translations-market-scoped.json`
- Fixtures: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/localization/*.json`

### Validation

- `corepack pnpm lint`
- `corepack pnpm parity -- --spec config/parity-specs/localization/localization-translatable-content-product.json`
- `corepack pnpm parity -- localization-translations-market-scoped`
- `corepack pnpm rust:test`
