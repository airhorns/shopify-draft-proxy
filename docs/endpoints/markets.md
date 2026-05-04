# Markets Endpoint Group

The markets group has local slices for captured Shopify Markets reads and stage-local lifecycle mutations. Keep Markets-specific capture details, coverage boundaries, and field behavior here instead of in `docs/architecture.md`.

## Current support and limitations

### Implemented roots

Overlay reads:

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

Stage-local mutations:

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

### Unsupported roots still tracked by the registry

- None for the schema-current Markets roots currently covered by the local registry slice. Deprecated `marketWebPresence*` aliases remain outside local support.

### Behavior notes

- Captured Markets reads hydrate normalized Market, Catalog, and PriceList records keyed by ID, with captured cursor and order metadata preserved for connection responses.
- In LiveHybrid parity replay, cold Markets reads use Pattern 2 upstream cassette hydration rather than passthrough. When no local Markets state exists for the requested root, the proxy fetches the captured upstream response, hydrates the local Markets/Product slices from that response, and returns the upstream payload verbatim; later reads stay local so staged read-after-write effects are not bypassed.
- Snapshot `market(id:)` and `markets(...)` reads resolve from the normalized Market bucket. The local serializer preserves selected-field behavior, unknown-id `null`, empty connections, `nodes`, `edges`, `pageInfo`, `first`, `last`, `before`, `after`, `reverse`, sort keys, root `type` and `status`, and captured-safe `query` filters such as `name`, `id`, `market_type`, and `market_condition_types`.
- Snapshot `catalog(id:)`, `catalogs(...)`, and `catalogsCount(...)` resolve from captured Catalog records. The current modeled slice covers MarketCatalog fields `id`, `title`, `status`, `markets`, `marketsCount`, `priceList`, `publication`, and `operations`, plus pagination, `type: MARKET`, `query` filters for `id`, `title`, `status`, and `market_id`, and count `limit` precision.
- Snapshot `priceList(id:)` and `priceLists(...)` resolve from captured and staged PriceList records. The current modeled slice covers `id`, `name`, `currency`, `fixedPricesCount`, `parent.adjustment`, nullable `catalog`, and `prices` for captured relative rows plus locally staged fixed price rows linked to product variants. `PriceList.prices(query: "variant_id:<numeric-id>")`, `product_id:<numeric-id>`, `originType`, and local connection pagination are modeled only for hydrated or staged rows.
- Nested Market connection fields such as `conditions.regions`, `catalogs`, and `webPresences` are projected from captured nested payloads with local connection windowing. Captured connection `pageInfo` is preserved when the stored slice is replayed as-is, which matters for truncated price-list prices.
- `marketsResolvedValues(buyerSignal: { countryCode })` is schema-confirmed against Admin GraphQL 2026-04 through the Markets baseline fixture. Captured payloads are replayed by buyer country when available, and snapshot/local reads can resolve supported buyer-country signals from effective staged or hydrated `Market` state.
- Local resolved values currently model country-region market matching for active markets, resolved country/market currency, `priceInclusivity`, resolved `MarketCatalog` connections, and resolved `MarketWebPresence` connections. Connection projection uses the shared pagination/serialization helpers through the normal Markets projection path, including selected `nodes`, `edges`, and `pageInfo`.
- Top-level `webPresences`, nested `Market.webPresences`, and `MarketsResolvedValues.webPresences` hydrate normalized `MarketWebPresence` records from captured payloads and apply local connection windowing. Generic `node(id:)` / `nodes(ids:)` resolves effective `MarketWebPresence` GIDs through the same Markets projection path, so captured and staged web-presence records keep selected-field behavior aligned with the root reads.
- `BuyerSignalInput.countryCode` is treated as a Shopify `CountryCode` enum value. Snapshot/local handling returns a Shopify-like `INVALID_VARIABLE` error for unsupported enum values such as `AQ` instead of silently falling back to fake buyer-context data.
- If no effective market matches a supported country and no captured resolved-values payload exists for that buyer signal, local `marketsResolvedValues` returns country-currency defaults where known, false tax/duties inclusivity, and empty catalog/web-presence connections. Live probes against the current conformance shop resolve unmatched countries through the shop primary market/web presence, so a true no-market/no-web-presence live capture remains shop-configuration blocked with the current credential.
- Unsupported catalog/price-list branches remain explicit null/empty projections when no captured data exists. Catalog membership mutations and unsupported price-list derivations outside the modeled fixed-price/quantity-pricing slice are not faked.
- Supported lifecycle mutations are staged locally and are not sent upstream during normal runtime handling. The mutation log keeps the original raw request body and route path so commit can replay the exact mutation order later.
- In LiveHybrid parity replay, supported Markets mutations that depend on existing Shopify state run a narrow preflight hydrate before local validation/staging. Current preflight coverage hydrates captured price-list, product/variant, product-metafield, and web-presence baselines for `priceListFixedPricesByProductUpdate`, `quantityPricingByVariantUpdate`, `quantityRulesAdd`, `quantityRulesDelete`, `webPresenceCreate`, `marketLocalizationsRegister`, and `marketLocalizationsRemove`; the original supported mutation still stages locally and is not sent upstream at runtime.
- `marketCreate` generates stable synthetic `Market` IDs, handles, status/enabled values, timestamps, conditions, currency settings, price inclusions, catalog references, and web presence references from the input.
- `marketUpdate` resolves staged or captured markets by ID, preserves existing fields when inputs omit them, and stages merged changes for downstream `market` and `markets` reads.
- `marketDelete` marks the market deleted in staged state. Deleted staged/captured markets return `null` from `market(id:)` and are removed from `markets(...)` connections while the deleted ID remains visible in meta state.
- `catalogCreate` stages MarketCatalog records only. It generates stable synthetic `MarketCatalog` IDs, title/status values, timestamps, empty operations, optional linked publication and price-list references, and a market context connection from existing Market IDs. It does not invent markets, price lists, publications, product memberships, or publication state.
- `catalogUpdate` resolves staged or captured MarketCatalog records by ID, preserves omitted fields, supports title/status changes, and can replace the market context when `input.context.marketIds` is provided.
- `catalogContextUpdate` adds and removes existing Market IDs from a staged/captured MarketCatalog context. Resulting `catalog`, `catalogs`, `catalogsCount`, `Market.catalogs`, and `MarketCatalog.markets` reads use the effective staged catalog state.
- `catalogDelete` marks the MarketCatalog deleted in staged state. Deleted staged/captured catalogs return `null` from `catalog(id:)`, disappear from `catalogs(...)`, `catalogsCount(...)`, and nested `Market.catalogs`, while the deleted ID remains visible in meta state.
- Catalog mutation validation is intentionally conservative. Captured parity currently covers blank `catalogCreate` titles and unknown IDs for `catalogUpdate`, `catalogContextUpdate`, and `catalogDelete`; unsupported non-market catalog contexts return local userErrors instead of claiming full B2B or app catalog support.
- `priceListCreate`, `priceListUpdate`, and `priceListDelete` stage local PriceList lifecycle records. Create/update support the schema-current slice needed by downstream reads: `name`, `currency`, `parent.adjustment`, optional `catalogId`, stable synthetic IDs, timestamps, empty `quantityRules`, and read-after-write visibility through `priceList`, `priceLists`, linked MarketCatalog reads, meta state, and the mutation log.
- Currency changes on `priceListUpdate` clear locally staged fixed-price rows and reset `fixedPricesCount` to `0`, matching Shopify's destructive currency-change behavior without inventing replacement prices.
- `priceListFixedPricesAdd`, `priceListFixedPricesUpdate`, and `priceListFixedPricesDelete` stage fixed price rows for product variants already present in normalized product state. The schema-current 2026-04 `priceListFixedPricesByProductUpdate` shape stages product-level `pricesToAdd` and `pricesToDeleteByProductIds`, then materializes those product fixed prices as fixed rows for the product's locally known variants. Fixed rows are exposed through `PriceList.prices(originType: FIXED)`, `fixedPricesCount`, meta state, and mutation log entries; relative price rows from captures are preserved unless the currency changes.
- Locally staged `PriceListPrice.price` and `QuantityPriceBreak.price` amounts follow Shopify `MoneyV2.amount` decimal normalization by trimming redundant trailing zeroes while preserving one fractional digit, such as `17.00` -> `17.0`.
- `quantityPricingByVariantUpdate`, `quantityRulesAdd`, and `quantityRulesDelete` stage PriceList quantity pricing locally for product variants already present in normalized product state. The modeled slice covers fixed price upserts/deletes, fixed `QuantityRule` rows, generated `QuantityPriceBreak` rows, captured unknown price-list and unknown-variant validation branches, and read-after-write visibility through `PriceList.quantityRules` and `PriceList.prices(originType: FIXED).quantityPriceBreaks`. Live evidence with setup and cleanup is checked in at `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/markets/quantity-pricing-rules-parity.json`, and `config/parity-specs/markets/quantity-pricing-rules-local-staging.json` now replays that capture through strict `captured-vs-proxy-request` parity targets for the quantity pricing mutation, downstream `priceList` read, cleanup delete payload, and validation branches.
- Price-list validation is intentionally conservative until broader live write fixtures exist. Local branches cover blank/duplicate names, full Money::Currency-style ISO code validation, required `priceListCreate` currency/parent inputs, parent adjustment type validation, unknown price-list/catalog/product/variant IDs, duplicate fixed-price additions, deleting/updating missing fixed prices, schema-current product-level fixed-price target errors, and the captured quantity pricing/rule unknown ID branches. Admin GraphQL 2026-04 no longer accepts variant IDs on `priceListFixedPricesByProductUpdate`; product-level success, cleanup, downstream `PriceList.prices(originType: FIXED)` reads, unknown-product errors, and unknown-price-list errors are captured in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/price-list-fixed-prices-by-product-update-parity.json` and replayed by `config/parity-specs/markets/price-list-fixed-prices-by-product-update.json`. Permission/access-scope failures and richer B2B catalog/contextual product-variant pricing payloads remain documented gaps.
- Product and variant contextual pricing fields are derived for the staged MarketCatalog fixed-price slice when a country buyer context resolves to an active staged/hydrated region market, a linked market catalog, and an effective price list with fixed rows for known product variants. The local derivation covers `Product.contextualPricing(context: { country })` `fixedQuantityRulesCount`, `priceRange`, `minVariantPricing`, `maxVariantPricing`, and `ProductVariant.contextualPricing(context: { country })` `price`, nullable `compareAtPrice` / `unitPrice`, `quantityRule`, and `quantityPriceBreaks` from staged `priceListFixedPrices*` and `quantityPricingByVariantUpdate` state. HAR-412 captured Admin GraphQL 2026-04 live evidence for these selected fields while a disposable product-level fixed price was active on the Mexico Markets price list, then cleaned up the fixed price and captured the reverted contextual price; local parity still replays that hydrated capture through `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-contextual-pricing-price-list-parity.json` and `config/parity-specs/products/product-contextual-pricing-price-list-read.json`. Relative price-list adjustments, market priority conflicts, B2B/app catalog contextual pricing, and product variants without modeled fixed price rows remain explicit gaps.
- `webPresenceCreate` and `webPresenceUpdate` stage local `MarketWebPresence` records using the schema-current 2026-04 input slice: `domainId`, `defaultLocale`, `alternateLocales`, and `subfolderSuffix`. The local model enforces domain/subfolder mutual exclusion, basic locale format and uniqueness checks, letters-only subfolder suffix validation, duplicate domain/subfolder checks against effective state, and update-by-ID existence checks.
- Web presence root URL derivation is local-only. Subfolder presences derive locale URLs from the captured shop/web-presence domain when available and fall back to the proxy's synthetic shop URL; domain-ID-only presences synthesize a stable domain object because `WebPresenceCreateInput` does not carry a host.
- Current-schema `webPresenceCreate` does not take a market ID. Market association remains modeled through market-side web presence references such as `marketUpdate` inputs that add web presence IDs. When a staged market references a modeled web presence, downstream top-level `webPresences`, nested `Market.webPresences`, `MarketsResolvedValues.webPresences` with a captured baseline, meta state, and the mutation log expose the local-only change.
- `webPresenceDelete` stages a local tombstone without deleting Shopify data at runtime. Successful deletes return `deletedId`, remove the presence from top-level `webPresences`, nested `Market.webPresences`, and modeled `MarketsResolvedValues.webPresences`, and expose the deleted ID in meta state. Unknown IDs and already-deleted IDs return `WEB_PRESENCE_NOT_FOUND`, matching the captured Shopify branch.
- Staged `marketCreate` / `marketUpdate` changes affect `marketsResolvedValues` and fixed-price contextual product reads only for modeled active region-country matches. Staged `catalogCreate`, `catalogUpdate`, and `catalogContextUpdate` affect resolved catalogs and fixed-price contextual product reads through modeled market context links. Staged `webPresenceCreate` / `webPresenceUpdate` affect resolved web presences through modeled market-side references. Broader buyer-signal branches, market priority rules, B2B/app catalog resolution, relative price-list adjustments, and unsupported price-list derivations remain explicit gaps until captured evidence supports them.
- Captured validation parity currently covers safe no-side-effect branches for blank `marketCreate` names and unknown IDs for `marketUpdate`/`marketDelete`. Additional success-path conformance should use disposable market setup/cleanup before touching shared buyer-facing market configuration.
- Admin GraphQL 2026-04 currently exposes market-localizable resource filtering for `METAFIELD` and `METAOBJECT`, not direct `PRODUCT` or `COLLECTION` resource types. The local proxy supports product-metafield `MarketLocalizableResource` identities first, but it does not assume every product metafield has market-localizable content.
- `marketLocalizableResource(resourceId:)` resolves supported product metafield IDs from effective local state and returns `null` for unknown IDs. Default ad hoc product metafields return a `MarketLocalizableResource` with empty `marketLocalizableContent` and empty `marketLocalizations`, matching the HAR-448 disposable-product capture. `marketLocalizableResources(resourceType: METAFIELD, ...)` and `marketLocalizableResourcesByIds(...)` preserve Shopify-like connection shape, cursor pagination, selected fields, unknown-id omission, and empty/no-data responses. `METAOBJECT` currently returns an empty local slice until metaobject state exists.
- `marketLocalizationsRegister` stages market-specific values locally only when a product metafield record is explicitly seeded with market-localizable content, such as fixture-backed `key`, `value`, and `digest` entries. It validates resource ID, market ID, key support, blank values, digest equality against `marketLocalizableContent.digest`, and empty input arrays, returning `TranslationUserError`-shaped `field`, `message`, and `code` values without proxying supported calls upstream.
- Re-registering the same resource/key/market combination updates the staged localization value and timestamp. `marketLocalizationsRemove` removes staged values for requested keys and markets and returns the removed localization payloads when seeded content exists. For default ad hoc product metafields with no market-localizable content, Shopify returns `marketLocalizations: null` and no user errors on remove for `key: value`; the local proxy preserves that branch. Downstream `marketLocalizableResource(...).marketLocalizations(marketId:)` reads observe staged register/remove changes only for seeded market-localizable content; product reads do not invent unsupported localized product fields.
- Current live evidence for these roots was captured against `harry-test-heelo.myshopify.com` on Admin GraphQL 2026-04. The empty read capture proves `read_translations` access for the read roots; no-side-effect unknown-resource mutation captures prove `write_translations` access and `RESOURCE_NOT_FOUND` semantics. HAR-448 adds a disposable product-metafield capture proving default product metafields expose empty `marketLocalizableContent`, reject `marketLocalizationsRegister` `key: value` with `INVALID_KEY_FOR_MODEL`, and return a null removal payload without user errors. Successful live localization writes remain an explicit gap until Shopify evidence identifies a disposable resource shape with non-empty market-localizable content.
- `webPresenceDelete` success, unknown-ID, already-deleted, and downstream top-level `webPresences` read-after-delete behavior are captured in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/market-web-presence-delete-parity.json` and replayed by `config/parity-specs/markets/web-presence-delete-local-staging.json`. HAR-448 adds `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/market-web-presence-lifecycle-parity.json` and `config/parity-specs/markets/web-presence-lifecycle-local-staging.json` for create/update/delete lifecycle payloads and downstream reads. That capture confirms Shopify subfolder `rootUrls.url` values include a trailing slash, which the local serializer now preserves. Deprecated `marketWebPresenceCreate` / `marketWebPresenceUpdate` / `marketWebPresenceDelete` aliases remain visible in Shopify docs, but this repo does not mark them implemented without fixture-backed behavior for payload shape, association cleanup, and validation errors.

## Historical and developer notes

### Validation anchors

- Runtime reads: `tests/integration/markets-query-shapes.test.ts`
- Runtime lifecycle staging: `tests/integration/markets-lifecycle-flow.test.ts`
- Runtime market localization staging: `tests/integration/markets-localization-flow.test.ts`
- Conformance parity: `tests/unit/conformance-parity-scenarios.test.ts`
- Conformance fixtures and requests: `config/parity-specs/markets/market*.json`, `config/parity-specs/markets/markets*.json`, `config/parity-specs/markets/catalog*.json`, `config/parity-specs/markets/price-list*.json`, and matching files under `config/parity-requests/markets/`
