# Markets Endpoint Group

The markets group has local slices for captured Shopify Markets reads and stage-local lifecycle mutations. Keep Markets-specific capture details, coverage boundaries, and field behavior here instead of in `docs/architecture.md`.

## Implemented roots

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
- `webPresenceCreate`
- `webPresenceUpdate`
- `marketLocalizationsRegister`
- `marketLocalizationsRemove`

## Unsupported roots still tracked by the registry

- `webPresenceDelete`

## Behavior notes

- Captured Markets reads hydrate normalized Market, Catalog, and PriceList records keyed by ID, with captured cursor and order metadata preserved for connection responses.
- Snapshot `market(id:)` and `markets(...)` reads resolve from the normalized Market bucket. The local serializer preserves selected-field behavior, unknown-id `null`, empty connections, `nodes`, `edges`, `pageInfo`, `first`, `last`, `before`, `after`, `reverse`, sort keys, root `type` and `status`, and captured-safe `query` filters such as `name`, `id`, `market_type`, and `market_condition_types`.
- Snapshot `catalog(id:)`, `catalogs(...)`, and `catalogsCount(...)` resolve from captured Catalog records. The current modeled slice covers MarketCatalog fields `id`, `title`, `status`, `markets`, `marketsCount`, `priceList`, `publication`, and `operations`, plus pagination, `type: MARKET`, `query` filters for `id`, `title`, `status`, and `market_id`, and count `limit` precision.
- Snapshot `priceList(id:)` and `priceLists(...)` resolve from captured PriceList records. The current modeled slice covers `id`, `name`, `currency`, `fixedPricesCount`, `parent.adjustment`, nullable `catalog`, and `prices` for captured relative/fixed price rows linked to product variants. `PriceList.prices(query: "variant_id:<numeric-id>")`, `product_id:<numeric-id>`, `originType`, and local connection pagination are modeled only for hydrated rows.
- Nested Market connection fields such as `conditions.regions`, `catalogs`, and `webPresences` are projected from captured nested payloads with local connection windowing. Captured connection `pageInfo` is preserved when the stored slice is replayed as-is, which matters for truncated price-list prices.
- Top-level `webPresences`, nested `Market.webPresences`, and `MarketsResolvedValues.webPresences` hydrate normalized `MarketWebPresence` records from captured payloads and apply local connection windowing.
- Unsupported catalog/price-list branches remain explicit null/empty projections when no captured data exists. Quantity rules, catalog membership mutations, and staged price-list fixed-price mutations are not faked.
- Supported lifecycle mutations are staged locally and are not sent upstream during normal runtime handling. The mutation log keeps the original raw request body and route path so commit can replay the exact mutation order later.
- `marketCreate` generates stable synthetic `Market` IDs, handles, status/enabled values, timestamps, conditions, currency settings, price inclusions, catalog references, and web presence references from the input.
- `marketUpdate` resolves staged or captured markets by ID, preserves existing fields when inputs omit them, and stages merged changes for downstream `market` and `markets` reads.
- `marketDelete` marks the market deleted in staged state. Deleted staged/captured markets return `null` from `market(id:)` and are removed from `markets(...)` connections while the deleted ID remains visible in meta state.
- `webPresenceCreate` and `webPresenceUpdate` stage local `MarketWebPresence` records using the schema-current 2026-04 input slice: `domainId`, `defaultLocale`, `alternateLocales`, and `subfolderSuffix`. The local model enforces domain/subfolder mutual exclusion, basic locale format and uniqueness checks, duplicate domain/subfolder checks against effective state, and update-by-ID existence checks.
- Web presence root URL derivation is local-only. Subfolder presences derive locale URLs from the captured shop/web-presence domain when available and fall back to the proxy's synthetic shop URL; domain-ID-only presences synthesize a stable domain object because `WebPresenceCreateInput` does not carry a host.
- Current-schema `webPresenceCreate` does not take a market ID. Market association remains modeled through market-side web presence references such as `marketUpdate` inputs that add web presence IDs. When a staged market references a modeled web presence, downstream top-level `webPresences`, nested `Market.webPresences`, `MarketsResolvedValues.webPresences` with a captured baseline, meta state, and the mutation log expose the local-only change.
- Captured validation parity currently covers safe no-side-effect branches for blank `marketCreate` names and unknown IDs for `marketUpdate`/`marketDelete`. Additional success-path conformance should use disposable market setup/cleanup before touching shared buyer-facing market configuration.
- Admin GraphQL 2026-04 currently exposes market-localizable resource filtering for `METAFIELD` and `METAOBJECT`, not direct `PRODUCT` or `COLLECTION` resource types. The local proxy supports the product-adjacent `METAFIELD` slice first by projecting product metafields as `MarketLocalizableResource` records with one localizable content entry: key `value`, current metafield value, and the metafield `compareDigest`.
- `marketLocalizableResource(resourceId:)` resolves supported product metafield IDs from effective local state and returns `null` for unknown IDs. `marketLocalizableResources(resourceType: METAFIELD, ...)` and `marketLocalizableResourcesByIds(...)` preserve Shopify-like connection shape, cursor pagination, selected fields, unknown-id omission, and empty/no-data responses. `METAOBJECT` currently returns an empty local slice until metaobject state exists.
- `marketLocalizationsRegister` stages market-specific values locally for product metafield resources only. It validates resource ID, market ID, key support, blank values, digest equality against `marketLocalizableContent.digest`, and empty input arrays, returning `TranslationUserError`-shaped `field`, `message`, and `code` values without proxying supported calls upstream.
- Re-registering the same resource/key/market combination updates the staged localization value and timestamp. `marketLocalizationsRemove` removes staged values for requested keys and markets and returns the removed localization payloads. Downstream `marketLocalizableResource(...).marketLocalizations(marketId:)` reads observe staged register/remove changes; product reads do not invent unsupported localized product fields.
- Current live evidence for these roots was captured against `harry-test-heelo.myshopify.com` on Admin GraphQL 2026-04. The empty read capture proves `read_translations` access for the read roots; no-side-effect unknown-resource mutation captures prove `write_translations` access and `RESOURCE_NOT_FOUND` semantics. Successful live localization writes remain intentionally avoided until a disposable setup/cleanup story exists for buyer-facing localized values.
- `webPresenceDelete` is schema-current in 2026-04, and deprecated `marketWebPresenceCreate` / `marketWebPresenceUpdate` / `marketWebPresenceDelete` aliases remain visible in Shopify docs, but this repo does not mark them implemented without fixture-backed behavior for payload shape, association cleanup, and validation errors.

## Validation anchors

- Runtime reads: `tests/integration/markets-query-shapes.test.ts`
- Runtime lifecycle staging: `tests/integration/markets-lifecycle-flow.test.ts`
- Runtime market localization staging: `tests/integration/markets-localization-flow.test.ts`
- Conformance parity: `tests/unit/conformance-parity-scenarios.test.ts`
- Conformance fixtures and requests: `config/parity-specs/market*.json`, `config/parity-specs/markets*.json`, `config/parity-specs/price-list*.json`, and matching files under `config/parity-requests/`
