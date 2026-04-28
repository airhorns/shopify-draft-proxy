# Saved Searches and URL Redirects

HAR-312 adds the first local saved-search model. This is scoped to Shopify Admin `SavedSearch` records for products, collections, customers, orders, draft orders, files, and discount saved-search roots.

## Current support and limitations

### Local saved-search support

- `savedSearchCreate`, `savedSearchUpdate`, and `savedSearchDelete` stage locally for supported resource types and retain the original raw GraphQL mutation in the mutation log for commit replay.
- Supported `resourceType` values are `PRODUCT`, `COLLECTION`, `CUSTOMER`, `ORDER`, `DRAFT_ORDER`, `FILE`, `PRICE_RULE`, and `DISCOUNT_REDEEM_CODE`.
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
- Empty saved-search roots return a non-null connection with empty `nodes`/`edges` and false/null `pageInfo`, matching the captured no-data shape. `draftOrderSavedSearches` is the captured exception: Shopify returns five default draft-order saved searches even when no merchant-created saved searches are present, and the local saved-search domain preserves those records.
- Saved-search query strings are parsed into simple `searchTerms` plus `filters { key value }` records by splitting field terms of the form `key:value` from free text. The local stored `query` is normalized as search terms followed by filters, which matches the captured connection-read ordering for the product saved-search slice.
- Mutation payloads preserve the submitted `query` ordering, while downstream connection reads expose the normalized stored query. The captured `savedSearchUpdate` validation branch also keeps valid query changes visible in the payload when an overlong name is rejected.

### URL redirect blockers

URL redirect roots are intentionally registered as unimplemented coverage, not supported local behavior.

- `urlRedirectSavedSearches` and `urlRedirectsCount` returned access denied requiring `read_online_store_navigation`.
- `urlRedirects` returned access denied under the current credential.
- `urlRedirectCreate` and `urlRedirectImportCreate` returned access denied requiring `write_online_store_navigation`.
- `urlRedirectImportCreate` / `urlRedirectImportSubmit` also need CSV preview and async job evidence before local support can be claimed.

Do not mark URL redirect create/update/delete/import/bulk-delete roots as implemented until success-path fixtures capture validation, path/target normalization, search/count/pageInfo behavior, job shapes, and downstream read-after-write effects.

## Historical and developer notes

### Captured evidence

The current conformance credential was valid for `harry-test-heelo.myshopify.com` / Admin GraphQL `2025-01`.

- Schema introspection confirmed `SavedSearchConnection` roots and `SavedSearch` fields: `id`, `legacyResourceId`, `name`, `query`, `resourceType`, `searchTerms`, and `filters { key value }`.
- `savedSearchCreate(resourceType: PRODUCT)` returned a SavedSearch payload with empty `userErrors`.
- A downstream `productSavedSearches(first:, reverse:)` read returned that saved search with cursor-bearing `pageInfo`.
- Missing `savedSearchUpdate` and `savedSearchDelete` returned `userErrors[{ field: ["input", "id"], message: "Saved Search does not exist" }]`.
- Updating a saved search with a name longer than 40 characters returned `userErrors[{ field: ["input", "name"], message: "Name is too long (maximum is 40 characters)" }]` while keeping the existing name in the payload.
- `config/parity-specs/saved-search-local-staging.json` replays the create, downstream read, overlong-name update validation, and missing update/delete branches through the generic parity runner with strict JSON targets. Expected differences are limited to deterministic local IDs and opaque connection cursors.
