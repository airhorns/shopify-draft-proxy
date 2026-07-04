---
title: 'Saved Searches and URL Redirects'
description: 'Coverage notes and fidelity boundaries for Saved Searches and URL Redirects.'
---

The saved-searches group is scoped to Shopify Admin `SavedSearch` records for products, collections, orders, draft orders, files, and discount saved-search roots. URL redirect mutation/import roots are tracked here only as explicit unsupported coverage; the supported URL redirect read overlay is documented with online-store content.

## Current support and limitations

### Local saved-search support

- `savedSearchCreate`, `savedSearchUpdate`, and `savedSearchDelete` stage locally for supported resource types and retain the original raw GraphQL mutation in the mutation log for commit replay.
- Supported create `resourceType` values are `PRODUCT`, `COLLECTION`, `ORDER`, `DRAFT_ORDER`, `FILE`, `PRICE_RULE`, and `DISCOUNT_REDEEM_CODE`. `CUSTOMER` is read-root only for hydrated historical records: Shopify 2025-01 and 2026-04 return `savedSearchCreate.userErrors[{ field: null, message: "Customer saved searches have been deprecated. Use Segmentation API instead." }]`, and the local create branch mirrors that deprecation.
- Local reads are served for:
  - `productSavedSearches`
  - `collectionSavedSearches`
  - `customerSavedSearches`
  - `orderSavedSearches`
  - `draftOrderSavedSearches`
  - `fileSavedSearches`
  - `codeDiscountSavedSearches`
  - `automaticDiscountSavedSearches`
  - `discountRedeemCodeSavedSearches`
- Empty saved-search roots return a non-null connection with empty `nodes`/`edges` and false/null `pageInfo`, matching the captured no-data shape. Current captured conformance evidence only proves built-in defaults for `orderSavedSearches` and `draftOrderSavedSearches`; `productSavedSearches`, `collectionSavedSearches`, `customerSavedSearches`, `fileSavedSearches`, and discount saved-search roots stay empty unless base, upstream, or staged state contains records for those resource types. LiveHybrid media reads that select `fileSavedSearches` hydrate upstream FILE saved-search rows before rendering the local connection. The proxy does not fabricate non-order defaults without captured evidence for the current store/API shape. ORDER and DRAFT_ORDER defaults use stable synthetic local IDs rather than captured per-shop GIDs, but otherwise behave like persisted Shopify rows for local `savedSearchUpdate` and `savedSearchDelete`: updates stage an override visible through the saved-search connection, and deletes tombstone the synthetic default ID so the static default does not reappear on later reads.
- Staged saved searches are routed by `resourceType`: `PRODUCT`, `COLLECTION`, `ORDER`, `DRAFT_ORDER`, `FILE`, and `DISCOUNT_REDEEM_CODE` appear only under their matching saved-search root. `PRICE_RULE` is the current discount saved-search resource type observed by the Admin schema, so the local model exposes those staged searches through both `codeDiscountSavedSearches` and `automaticDiscountSavedSearches` until deeper discount subtype evidence proves a stricter split.
- `savedSearchDelete` projects selected payload `shop` fields from the effective shop state, including restored or hydrated shop identity, rather than from a helper-owned default shop record.
- `savedSearchCreate` and `savedSearchUpdate` enforce Shopify's case-sensitive saved-search name uniqueness across the effective local list: static defaults, hydrated base state, and staged records. Shopify scopes the backing model by shop, type, and subtype; the proxy does not currently model subtype, so this approximation keys uniqueness by `resourceType` plus exact `name`.
- Reserved saved-search names are rejected with the same `field: ["input", "name"]` / `Name has already been taken` userError when the raw supplied `name` case-insensitively equals the per-resource reserved label. Surrounding whitespace is not stripped for this comparison, so whitespace-distinct names such as `" All products"` stage like ordinary names.
- Saved-search query strings are parsed into `searchTerms` plus `filters { key value }` records. Free-text terms remain `searchTerms`; supported top-level filter projections are:
  - Plain field terms `field:value` become `filters[{ key: "field", value: "value" }]`.
  - Top-level negated field terms such as `-vendor:Archived` become `filters[{ key: "vendor_not", value: "Archived" }]`.
  - Range comparators `field:<value` / `field:<=value` become `filters[{ key: "field_max", value: "value" }]`, and `field:>value` / `field:>=value` become `filters[{ key: "field_min", value: "value" }]`.
  - Bounded ranges are represented as two range tokens, for example `inventory_total:>2 inventory_total:<10`, and project to both `_min` and `_max` filters.
  - Exists syntax `field:*` becomes `filters[{ key: "field", value: "true" }]`.
  - The all-records query `*` becomes `filters[{ key: "default", value: "true" }]` with empty `searchTerms`; this generated `default` filter is accepted before resource-specific allowlists apply.
  - Negated range terms use Shopify's normalized opposite bound in stored reads: `-inventory_total:<3` projects to `filters[{ key: "inventory_total_min", value: "3" }]` and downstream `query: "inventory_total:>=3"`.
    Captured grouped/boolean query grammar keeps the grouped expression in `searchTerms` and normalizes quoted field values to double quotes. Range, exists, and extracted negated range tokens are removed from `searchTerms`.
- Before local validation and staging, `savedSearchCreate` and `savedSearchUpdate` rewrite metafield query tokens of the form `metafields.$app...<dot><key>` using the same app-namespace preparation Shopify applies to saved-search input. Live 2025-01 evidence for the conformance app showed bare `metafields.$app.tier:gold` is stored by Shopify as `metafields.app--<api_client_id>.tier:gold`; the local proxy uses the request's `x-shopify-draft-proxy-api-client-id` header when present, and falls back to the deterministic synthetic namespace `app--shopify-draft-proxy-local-app` when no effective API client identity is available. This rewrite feeds the mutation payload, staged `SavedSearchRecord.query`, parsed `searchTerms`, and derived `filters` so downstream saved-search reads expose the resolved namespace instead of the submitted `$app` shorthand.
- `savedSearchCreate` and `savedSearchUpdate` run a best-effort query validation pass before staging. The current reserved-filter table covers `ORDER` saved searches with `reference_location_id`, which Shopify rejects as `Search terms is invalid, 'reference_location_id' is a reserved filter name`. The current compatibility table covers `PRODUCT` saved searches where `collection_id` is mutually exclusive with `tag`, `published_status`, and `error_feedback`, returning one deduped `Query has incompatible filters: ...` userError that lists every conflicting field. `collection_id` by itself remains valid.
- Resolver-level saved-search `userErrors` use the base Shopify `UserError` shape with only `field` and `message` when those fields are selected. An empty string `name` returns `field: ["input", "name"]` / `Name can't be blank` and does not short-circuit query validation, so blank-name and invalid-query errors can aggregate in one payload. On update, the blank-name branch only runs when `name` is explicitly supplied; omitting `name` preserves the existing saved-search name.
- Top-level saved-search filter tokens are validated against per-resource allowlists before staging. Resolved `metafields.<namespace>.<key>` filters are accepted as Shopify custom-data search filters and are not rejected by the fixed resource allowlists. The current local allowlist coverage is:
  - `PRODUCT`: `collection_id`, `created_at`, `error_feedback`, `handle`, `id`, `inventory_total`, `product_type`, `published_at`, `published_status`, `sku`, `status`, `tag`, `title`, `updated_at`, `vendor`
  - `COLLECTION`: `collection_type`, `handle`, `id`, `product_id`, `product_publication_status`, `publishable_status`, `published_at`, `published_status`, `title`, `updated_at`
  - `ORDER`: `channel_id`, `created_at`, `customer_id`, `email`, `financial_status`, `fulfillment_status`, `id`, `location_id`, `name`, `processed_at`, `sales_channel`, `status`, `tag`, `test`, `updated_at`
  - `DRAFT_ORDER`: `created_at`, `customer_id`, `email`, `id`, `name`, `status`, `tag`, `updated_at`
  - `FILE`: `created_at`, `filename`, `id`, `media_type`, `original_source`, `status`, `updated_at`
  - `DISCOUNT_REDEEM_CODE`: `code`, `created_at`, `discount_id`, `id`, `status`, `updated_at`
- Unknown top-level filters for covered resource types return `Query is invalid, '<field>' is not a valid filter`. Create failures return `savedSearch: null` with `field: ["input", "query"]`; update failures return the captured non-null payload echo without staging effective state and also use `field: ["input", "query"]`. Reserved-filter update errors remain distinct: they use `field: ["input", "searchTerms"]` because Shopify records that validation on `search_terms`. Resource types without an allowlist continue to fall through to existing validation instead of producing guessed false positives.
- Failed query validation on create returns `savedSearch: null`. Captured 2025-01 update behavior differs: Shopify returns a non-null `savedSearchUpdate.savedSearch` echo containing the submitted invalid query plus `userErrors`, but the proxy keeps that failed update out of effective staged state.
- Mutation payloads preserve the submitted `query` ordering, while downstream connection reads expose the normalized stored query. The captured `savedSearchUpdate` validation branch also keeps valid query changes visible in the payload when an overlong name is rejected.
- Missing/null required input object fields are rejected before local staging with Shopify-style top-level GraphQL coercion errors. Captured 2025-01 evidence covers inline omissions for `name`/`query`/`resourceType` on `savedSearchCreate`, `id` on `savedSearchUpdate`, and `id` on `savedSearchDelete`, plus variable-supplied `SavedSearchCreateInput` omissions for `resourceType`/`name` and `SavedSearchDeleteInput.id` missing or null. Inline omissions use `missingRequiredInputObjectAttribute`; variable omissions/nulls use top-level `INVALID_VARIABLE`. These payloads contain `errors` and no resolver `userErrors`.
- An explicitly supplied empty saved-search `query: ""` is valid on create and update. The local model stages the empty string instead of returning a blank-query userError.
- Query parsing uses the shared Admin search-query term helpers for token interpretation, with saved-search-specific stored-query projection layered on top. The local model now covers the captured quoted/grouped `OR` expression, top-level negated-filter normalization, range and exists filter projection, plus the reserved-filter and compatibility guardrails. The shared term parser is total, so Shopify parser-invalid branches that depend on per-resource filter typing, such as numeric/date comparator validation, remain TODOs for a follow-up capture instead of guessed local behavior.
- Shopify 2026-04 resource-root evidence showed `query:` arguments are rejected on most saved-search connection roots,
  so executable parity for resource roots uses valid first-only reads. Runtime tests still cover local query filtering as
  a compatibility surface for hydrated or staged records, but version-specific GraphQL argument validation is not
  modeled in this endpoint.

### Registry-only URL redirect roots

URL redirect mutation/import roots are intentionally registered as unimplemented coverage, not supported local behavior. The read roots `urlRedirect` and `urlRedirects` have a narrow local overlay for redirect rows staged by metaobject handle updates, documented in `/endpoints/online-store/`; that does not imply support for URL redirect mutation lifecycle roots.

- `urlRedirectSavedSearches` and `urlRedirectsCount` returned access denied requiring `read_online_store_navigation` in the saved-search blocker capture.
- `urlRedirectCreate` and `urlRedirectImportCreate` returned access denied requiring `write_online_store_navigation`.
- `urlRedirectImportCreate` / `urlRedirectImportSubmit` also need CSV preview and async job evidence before local support can be claimed.

Do not mark URL redirect create/update/delete/import/bulk-delete roots as implemented until success-path fixtures capture validation, path/target normalization, search/count/pageInfo behavior, job shapes, and downstream read-after-write effects.
