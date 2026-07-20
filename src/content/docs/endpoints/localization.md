---
title: 'Localization'
description: 'Coverage notes and fidelity boundaries for Localization.'
---

This endpoint group covers Shopify Admin GraphQL locale, shop-locale,
translatable-resource, and translation roots.

## Current support and limitations

### Supported roots

The operation registry routes these read roots to the local Rust model and
declares executable runtime coverage for them:

- `availableLocales`
- `shopLocales`
- `translatableResource`
- `translatableResources`
- `translatableResourcesByIds`

These mutations also route locally and have executable runtime coverage:

- `shopLocaleEnable`
- `shopLocaleUpdate`
- `shopLocaleDisable`
- `translationsRegister`
- `translationsRemove`

Support is scoped to the locale and Product/Collection translation behavior
described below. Registry implementation means the roots do not 501 or write to
Shopify during ordinary handling; it is not a claim of complete coverage for
every `TranslatableResourceType`.

### Local behavior

The Rust runtime has store-backed locale catalogs, canonical translatable
resource observations, and staged locale/translation overlays.
`availableLocales` and `shopLocales` project from the proxy store's baseline or
argument-scoped LiveHybrid observations plus staged shop-locale rows; plain
catalog reads are not selected by document name. Completeness is tracked per
root and normalized arguments/selection, so observing `availableLocales`, one
`shopLocales(published:)` filter, or one resource never suppresses an unrelated
cold read. The runtime stages `shopLocaleEnable`,
`shopLocaleUpdate`, and `shopLocaleDisable` requests locally, and exposes
downstream `shopLocales` read-after-write behavior for the staged locale rows.
Only effective staged changes append the original raw GraphQL request for
ordered commit replay; validation failures and successful no-stage results do
not enter the mutation log.

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
mode, locale mutation preflight uses one query-only batch for the authoritative
locale catalog and only referenced, still-unknown `MarketWebPresence` IDs before
local validation. Translation mutation preflight similarly batches the exact
resource, locale, translation scope, and unknown Market prerequisites. These
preflights never forward the mutation to Shopify.
For `shopLocaleUpdate`, the primary-locale guard uses the primary row in the
proxy's baseline plus staged shop-locale state and applies when the input
supplies a non-null `published` value, whether `true` or `false`;
primary-locale updates that only supply `marketWebPresenceIds` remain accepted
by this slice. Market-web-presence default-locale projections fall back to the
same resolved primary locale when no staged web-presence record carries a more
specific default. The default snapshot baseline includes a captured primary
English row, and staged enable/update/disable effects are merged with that
baseline for subsequent `shopLocales` reads.

`translationsRegister` and `translationsRemove` are locally modeled for the
product, collection, product-metafield, and market-scoped translation scenarios.
For Product resource IDs, the local slice validates existence against
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
`updatedAt` value from the proxy's mutation clock in the `translationsRegister`
echo and in downstream `translatableResource(...).translations` reads;
re-registering an existing row refreshes that timestamp. Validation failures
that do not stage rows leave existing staged timestamps, allocators, and replay
log unchanged. Partially successful registration appends exactly one replay
entry when at least one row stages.
`translationsRemove` removes every requested
translation-key/locale/market combination that exists in the authoritative
observed baseline or staged state, and records tombstones so removed baseline
rows stay absent. An empty
`translationKeys` list matches no rows and returns Shopify's no-op
`translations: null, userErrors: []` payload without entering replay.
For `handle` translations, normalization lowercases ASCII alphanumerics,
collapses separators to single dashes, trims edge dashes, and uses a
deterministic `localized-<hash>` fallback derived from the submitted value when
normalization would otherwise be empty. The fallback avoids reusing a
fixture-derived handle for unrelated non-ASCII or punctuation-only values.
For `translationsRegister` rows that violate multiple rules, captured Shopify
behavior validates locale and market gates before translation-record value and
digest validation, and market existence wins before locale enablement or
primary-locale validation when a row violates both. The market-scoped
value-matches-base-translation check runs before digest validation, so the local
first `userErrors` entry follows that precedence. Captured Shopify behavior
accepts a market-scoped value matching the source content when no shop-level
translation exists for that locale/key.
Market-scoped `translationsRegister` checks market existence from store state or
upstream hydration and limits market-customizable translation keys to modeled
resource/key pairs. Modeled Product keys include `title`, `body_html`, and
`product_type`; modeled Collection keys include `title` and `body_html`.
Unmodeled market-custom resources such as `PackingSlipTemplate.body` return
`RESOURCE_NOT_MARKET_CUSTOMIZABLE`. Missing Product or Collection IDs, and
resource types the proxy cannot resolve locally such as absent `Menu` IDs,
return `RESOURCE_NOT_FOUND` on `translationsRegister` and `translationsRemove`
before staging any translation rows. `translationsRemove` with an unknown market
ID follows the captured Shopify no-op shape without staging removals.

For modeled Product resources, `translatableResource`,
`translatableResources`, and `translatableResourcesByIds` project
`translatableContent` from the effective Product record instead of from a
placeholder. Product content includes title, handle, product type, and populated
body HTML / SEO source fields, with Shopify key names such as `body_html`,
`product_type`, `meta_title`, and `meta_description`; each entry carries the
shop primary locale, the modeled `LocalizableContentType`, the source value, and
`sha256(value)` as the digest. The digest emitted by these reads is accepted by
the local `translationsRegister(translatableContentDigest:)` guard for the same
resource and key. Digest validation compares against known source content only;
when a resource/key has not been hydrated with source content, the proxy does
not reject a row by digest prefix or other synthetic sentinel. Modeled
Collection resources use the same source-backed projection for title, handle,
body HTML, and SEO fields that exist in local state.
Unknown or omitted singular `translatableResource` IDs return `null`, and
empty `translatableResources` connections remain empty instead of fabricating a
default resource ID. `translatableResources(first:/last:/after:/before:,
reverse:)` applies `reverse` to the local resource-ID order before computing
the requested cursor window and selected `pageInfo`. In LiveHybrid mode, cold
reads forward the caller's complete document once per request and cache only
the exact completed root scope. Staged insertions, updates, and tombstones are
overlaid on the observed window. If a tombstone underfills a partial page, the
runtime issues at most one adjacent-cursor refill sized to the missing rows plus
relevant staged removals, preserving upstream opaque cursors and page
boundaries instead of hydrating the complete catalog.

`translatableResourcesByIds` forwards one batched caller document for unknown
IDs, deduplicates repeated IDs, preserves the Shopify-observed canonical order
when applying the effective local overlay, omits confirmed missing resources
with Shopify's empty-connection/null semantics, and caches proven misses. A
captured mixed Product/Collection request returned Collection then Product for
both caller input orders. The runtime retains that observed connection order
instead of sorting staged overlays by the input list. It does not issue per-ID
hydration calls.
Observed resources and translations remain separate from canonical
Product/Collection records, so a narrow localization selection cannot erase
unselected product or collection fields. Dump/restore preserves observations,
exact completeness scopes, staged rows, tombstones, counters, and mutation log;
reset clears the resettable observations and overlays while retaining the
configured snapshot baseline.

Product and Collection source-content and translation lifecycles have runtime
coverage plus captured Shopify comparisons for the documented validation and
read-after-write slices. Generic observed resources can retain captured
translatable content and translations, but their resource-specific validation
rules remain partial unless listed above.

### Boundaries

- Localization roots are handled locally and marked `implemented` in the
  operation registry. Unmodeled resource families return the documented
  null/empty/user-error boundary; registered localization mutations do not fall
  through to runtime Shopify writes.
- `TranslatableResource` support is limited to product, collection, and
  product-metafield evidence. Product existence checks use localization baseline
  resources, normalized Product/Collection state, and exact upstream
  observations. Other resource families return null/empty results and do not
  emit synthetic `title` content until local lifecycle behavior is modeled.
- Validation-only localization specs prove guardrail payloads and no-stage
  behavior for those inputs only. They do not make unmodeled translation or
  resource families generally supported.
- Digest sanitization for complex HTML remains bounded by checked-in capture
  evidence and runtime guardrails; the proxy does not claim complete Shopify
  sanitizer fidelity.
- Supported mutation slices stage locally and do not update Shopify at runtime;
  unmatched unsupported mutation documents follow the configured unsupported
  path.
