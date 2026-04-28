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

Product translatable content currently includes `title`, `handle`, optional `body_html`, optional `product_type`, and optional SEO `meta_title` / `meta_description` keys when the normalized product has default SEO values. Product SEO keys are modeled as fields on the product translatable resource, not as standalone metafield resources. Content digests use Shopify's observed SHA-256 value digest behavior for those scalar values.

Product metafield translatable content is limited to product-owned metafields and exposes the Shopify `value` key. Broader metafield owners and special SEO-like metafield behavior remain out of scope until separately captured.

### Mutation behavior

`shopLocaleEnable`, `shopLocaleUpdate`, and `shopLocaleDisable` stage only local shop-locale state. They do not update Shopify at runtime. The supported slice covers enabling available locales, toggling `published`, replacing local `marketWebPresenceIds`, and disabling non-primary locales.

`translationsRegister` stages translations for locally known product and product-metafield resources after validating resource existence, enabled shop locale, translatable key, non-blank value, digest match, and unsupported market-specific input. Locale validation uses enabled `shopLocales`, not the broader `availableLocales` catalog. Digest mismatch is checked only after the requested key resolves to translatable content, so invalid-key errors are not polluted by stale-digest errors. `translationsRemove` removes matching local/base translations and returns the removed translation payloads. Subsequent `translatableResource.translations(locale:)` reads observe these staged changes.

Market-specific `TranslationInput.marketId` / `translationsRemove(marketIds:)` for this generic localization root remains explicit unsupported behavior in the local model. Market-specific metafield localization is covered separately by the Markets `marketLocalizations*` roots.

## Historical and developer notes

### Conformance evidence

Live Admin GraphQL 2026-04 evidence was captured against `harry-test-heelo.myshopify.com` in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/localization/localization-locale-translation-fixture.json`.

The capture includes:

- root introspection for all HAR-314 roots
- locale and product translatable-resource reads
- unknown resource validation for `translationsRegister` and `translationsRemove`
- a safe `fr` shop-locale enable/update/disable lifecycle with cleanup
- a product title `translationsRegister` / downstream read / `translationsRemove` / downstream empty read lifecycle with cleanup

The generic parity runner replays the captured read, unknown-resource validation, locale lifecycle, and product-title translation lifecycle through the local proxy. `tests/integration/localization-flow.test.ts` also covers local-only guardrails that are difficult to isolate in the generic fixture replay: product SEO keys, product-metafield `value` translations, enabled-locale validation, invalid keys, stale digests, and read-after-remove behavior.
