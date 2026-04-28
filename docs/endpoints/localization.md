# Localization

HAR-314 adds the first local localization slice for Admin GraphQL locale and translation roots.

## Current support and limitations

### Implemented roots

- `availableLocales`
- `shopLocales`
- `translatableResource`
- `translatableResources`
- `translatableResourcesByIds`
- `shopLocaleEnable`
- `shopLocaleUpdate`
- `shopLocaleDisable`
- `translationsRegister`
- `translationsRemove`

### Local model

Locale state is normalized in memory as available locale records, shop locale records, and owner-scoped translation records keyed by resource, locale, optional market, and translation key.

The implemented translatable-resource owner slice is intentionally narrow:

- `PRODUCT` resources are derived from locally known `ProductRecord` rows.
- `METAFIELD` resources are derived from product-owned metafields.
- Unsupported `TranslatableResourceType` branches return an empty connection.
- Unknown singular resource IDs return `null` for reads and `RESOURCE_NOT_FOUND` `TranslationUserError` payloads for translation mutations.

Product translatable content currently includes `title`, `handle`, optional `body_html`, and optional `product_type`. Content digests use Shopify's observed SHA-256 value digest behavior for those scalar values.

### Mutation behavior

`shopLocaleEnable`, `shopLocaleUpdate`, and `shopLocaleDisable` stage only local shop-locale state. They do not update Shopify at runtime. The supported slice covers enabling available locales, toggling `published`, replacing local `marketWebPresenceIds`, and disabling non-primary locales.

`translationsRegister` stages translations for locally known product and product-metafield resources after validating resource existence, enabled locale, translatable key, non-blank value, digest match, and unsupported market-specific input. `translationsRemove` removes matching local/base translations and returns the removed translation payloads. Subsequent `translatableResource.translations(locale:)` reads observe these staged changes.

Market-specific `TranslationInput.marketId` / `translationsRemove(marketIds:)` for this generic localization root remains explicit unsupported behavior in the local model. Market-specific metafield localization is covered separately by the Markets `marketLocalizations*` roots.

## Historical and developer notes

### Conformance evidence

Live Admin GraphQL 2026-04 evidence was captured against `harry-test-heelo.myshopify.com` in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/localization-locale-translation-fixture.json`.

The capture includes:

- root introspection for all HAR-314 roots
- locale and product translatable-resource reads
- unknown resource validation for `translationsRegister` and `translationsRemove`
- a safe `fr` shop-locale enable/update/disable lifecycle with cleanup
- a product title `translationsRegister` / downstream read / `translationsRemove` / downstream empty read lifecycle with cleanup

The generic parity runner does not yet replay this multi-step locale and translation lifecycle directly. The fixture is therefore enforced by `tests/integration/localization-flow.test.ts` and registered as a `captured-fixture` parity spec.
