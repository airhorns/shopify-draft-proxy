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

In LiveHybrid mode, cold localization reads use the cassette-backed upstream
read as the authoritative locale and source-content slice, then hydrate
available locales, shop locales, and product source-content markers into base
state. Follow-up localization mutations still stage locally and never replay to
Shopify at runtime; the hydrated source markers only let local
`translationsRegister` / `translationsRemove` validate the captured product
resource and digest.

### Mutation behavior

`shopLocaleEnable`, `shopLocaleUpdate`, and `shopLocaleDisable` stage only local shop-locale state. They do not update Shopify at runtime. The supported slice covers enabling available locales, toggling `published`, replacing local `marketWebPresenceIds`, and disabling non-primary locales. `shopLocaleEnable` always stages the enabled locale as unpublished, including when a stale local record previously had `published: true`; publishing remains a `shopLocaleUpdate` concern. `ShopLocale.marketWebPresences` reads and mutation payloads project from the staged market web presence IDs and preserve selected `id`, `__typename`, and `defaultLocale` fields. Attempts to enable the primary locale, unpublish the primary locale, or disable the primary locale return a `CAN_NOT_MUTATE_PRIMARY_LOCALE` shop-locale user error, and failed disable payloads return `locale: null`. Updating or disabling a locale that is not enabled returns `SHOP_LOCALE_DOES_NOT_EXIST`. Disabling a locale also removes locally staged/base translations for that locale, matching Shopify's documented destructive locale-delete behavior.

The local `ShopLocaleError` serializer emits SCREAMING_SNAKE `code` values when a client selects `userErrors.code`. The current public Admin GraphQL 2026-04 schema exposes these shop-locale payload fields as plain `UserError` with only `field` and `message`, so the live parity fixture compares the public field/message shape while unit tests cover the proxy's local `code` selection behavior.

`translationsRegister` stages translations for locally known product and product-metafield resources after validating resource existence, enabled shop locale, translatable key, non-blank value, digest match, and Shopify's 100-key mutation limit. Locale validation uses enabled `shopLocales`, not the broader `availableLocales` catalog. Empty `translations` inputs no-op with `translations: []` and no user errors. Blank values return a `FAILS_RESOURCE_VALIDATION` `TranslationUserError` with `translations: []`; more than 100 inputs return `TOO_MANY_KEYS_FOR_RESOURCE` on `field: ["resourceId"]` with `translations: null`. Digest mismatch is checked only after the requested key resolves to translatable content, so invalid-key errors are not polluted by stale-digest errors. Mixed valid/invalid rows persist and return the successfully staged translations alongside indexed `userErrors`. `TranslationInput.marketId` is accepted and stored as part of the translation key so downstream `translatableResource.translations(locale:, marketId:)` reads return the market-scoped value and serialize captured `Translation.market` fields when selected.

`translationsRemove` removes matching local/base translations and returns the removed translation payloads when at least one row was removed. Empty `translationKeys` or `locales` inputs no-op with `translations: null` and no synthetic blank-field user error. When `marketIds` is supplied, removal is scoped by `(resourceId, key, locale, marketId)`; when it is omitted or empty, the unscoped translation branch is removed. Subsequent `translatableResource.translations(locale:, marketId:)` reads observe these staged changes.

## Historical and developer notes

### Conformance evidence

Live Admin GraphQL 2026-04 evidence was captured against `harry-test-heelo.myshopify.com` in:

- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/localization/localization-locale-translation-fixture.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/localization/localization-disable-clears-translations.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/localization/localization-shop-locale-primary-guards.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/localization/localization-translations-error-codes.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/localization/localization-translations-market-scoped.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/localization/localization-payload-shapes.json`

The capture includes:

- root introspection for all HAR-314 roots
- locale and product translatable-resource reads
- unknown resource validation for `translationsRegister` and `translationsRemove`
- a safe `fr` shop-locale enable/update/disable lifecycle with cleanup
- primary-locale validation for `shopLocaleEnable`, primary-unpublish validation for `shopLocaleUpdate`, primary-locale validation for `shopLocaleDisable`, and missing-locale validation for `shopLocaleUpdate`
- `TranslationErrorCode` parity for `translationsRegister` empty-list no-op, blank-value `FAILS_RESOURCE_VALIDATION`, 101-key `TOO_MANY_KEYS_FOR_RESOURCE`, and `translationsRemove` empty-locales no-op behavior
- a product title `translationsRegister` / downstream read / `translationsRemove` / downstream empty read lifecycle with cleanup
- a market-scoped product title `translationsRegister` / downstream `translatableResource.translations(locale:, marketId:)` read / `translationsRemove(marketIds:)` / downstream empty read lifecycle with cleanup
- a product title `translationsRegister` / `shopLocaleDisable` / downstream empty read lifecycle proving Shopify removes translations for the disabled locale
- `ShopLocale.marketWebPresences` payload and downstream read projection, failed primary-locale disable payloads with `locale: null`, and mixed `translationsRegister` partial-success payloads

The generic parity runner replays the captured read, unknown-resource validation, locale lifecycle, shop-locale primary guard validation, translation error-code validation, product-title translation lifecycle, locale-disable translation cleanup lifecycle, and HAR-711 payload-shape scenario through the local proxy. The Gleam parity runner also covers local-only guardrails that are difficult to isolate in the generic fixture replay: product SEO keys, product-metafield `value` translations, enabled-locale validation, invalid keys, stale digests, read-after-remove behavior, and the local no-upstream execution path for locale-disable translation cleanup.

### HAR-449 gap review

The current localization model intentionally preserves dedicated `availableLocales`, `shopLocales`, and `translations` state buckets. The reviewed Shopify docs and public examples reinforce those as separate locale lifecycle, translatable-resource, and owner-scoped translation concepts rather than evidence for a shared abstraction.

High-risk paths now have executable evidence through the captured localization parity fixture plus integration coverage for guardrails that the generic fixture replay does not isolate. Remaining known boundaries are unsupported resource families beyond product/product-metafield `TranslatableResource` rows and Shopify-specific digest sanitizer edge cases for complex HTML. Those stay documented as unsupported or capture-driven future work rather than claimed full localization coverage.
