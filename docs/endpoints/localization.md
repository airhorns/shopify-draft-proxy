# Localization

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

The Rust runtime has a scenario-backed localization slice for ported parity
requests and runtime tests. It serializes a baseline available-locale and
shop-locale catalog, stages selected `shopLocaleEnable`,
`shopLocaleUpdate`, and `shopLocaleDisable` requests locally, and exposes
downstream `shopLocales` read-after-write behavior for the staged locale rows.
Supported staged shop-locale mutations append replay-ready mutation-log entries
with the original raw GraphQL request.

The staged shop-locale slice rejects primary-locale mutation attempts,
unsupported locale codes, duplicate enables, missing locales for published
updates, and disables for non-enabled locales with captured Shopify-like
`userErrors`. Market-web-presence IDs are filtered to known local or captured
WebPresence IDs, and accepted rows project selected
`marketWebPresences`, `defaultLocale`, and locale scalar fields.

`translationsRegister` and `translationsRemove` are locally modeled for the
ported product, collection, product-metafield, and market-scoped translation
scenarios. The local slice validates unknown resources, enabled non-primary
locale requirements, translatable keys, digest mismatches, non-blank values,
the 100-key mutation limit, market scope, and selected handle-normalization
branches. Successful translations are staged in local translation state so
subsequent `translatableResource.translations(...)` reads observe the staged or
removed rows.

Collection translation lifecycle support is fixture-backed. Product and
product-metafield translation behavior has runtime coverage for guardrails that
the generic parity replay cannot isolate cleanly.

### Boundaries

- Localization roots are not marked implemented in the operation registry.
  Unsupported documents outside the ported request families should not be
  treated as broad local support.
- `TranslatableResource` support is limited to product, collection, and
  product-metafield evidence. Other resource families return null/empty results
  or remain unsupported until local lifecycle behavior is modeled.
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

- Registry status: `config/operation-registry.json`
- Runtime coverage: `tests/graphql_routes.rs`
- Parity specs: `config/parity-specs/localization/*.json`
- Fixtures: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/localization/*.json`

### Validation

- `corepack pnpm lint`
- `corepack pnpm rust:test`
