# Saved Searches and URL Redirects

HAR-312 adds the first local saved-search model. This is scoped to Shopify Admin `SavedSearch` records for products, collections, orders, draft orders, files, and discount saved-search roots.

The saved-search runtime is now owned by the Gleam port. The legacy TypeScript saved-search domain handler has been removed after the Gleam implementation passed the local staging, query grammar, and resource-root parity scenarios on both targets.

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
- Empty saved-search roots return a non-null connection with empty `nodes`/`edges` and false/null `pageInfo`, matching the captured no-data shape. `orderSavedSearches` and `draftOrderSavedSearches` are captured exceptions: Shopify returns default order and draft-order saved searches even when no merchant-created saved searches are present, and the local saved-search domain preserves those records.
- Staged saved searches are routed by `resourceType`: `PRODUCT`, `COLLECTION`, `ORDER`, `DRAFT_ORDER`, `FILE`, and `DISCOUNT_REDEEM_CODE` appear only under their matching saved-search root. `PRICE_RULE` is the current discount saved-search resource type observed by the Admin schema, so the local model exposes those staged searches through both `codeDiscountSavedSearches` and `automaticDiscountSavedSearches` until deeper discount subtype evidence proves a stricter split.
- Saved-search query strings are parsed into `searchTerms` plus `filters { key value }` records. Simple top-level field terms of the form `key:value` become filters, while free-text terms remain `searchTerms`. Captured grouped/boolean query grammar keeps the grouped expression in `searchTerms`, normalizes quoted field values to double quotes, and extracts top-level negated field terms such as `-vendor:Archived` as `filters[{ key: "vendor_not", value: "Archived" }]`.
- Mutation payloads preserve the submitted `query` ordering, while downstream connection reads expose the normalized stored query. The captured `savedSearchUpdate` validation branch also keeps valid query changes visible in the payload when an overlong name is rejected.
- Query parsing uses the shared Admin search-query term helpers for token interpretation, with saved-search-specific stored-query projection layered on top. The local model now covers the captured quoted/grouped `OR` expression and top-level negated-filter normalization from HAR-458; broader resource-specific execution semantics remain out of scope until dedicated fixtures prove them.
- Shopify 2026-04 resource-root evidence showed `query:` arguments are rejected on most saved-search connection roots,
  so executable parity for resource roots uses valid first-only reads. Runtime tests still cover local query filtering as
  a compatibility surface for hydrated or staged records, but version-specific GraphQL argument validation is not
  modeled in this endpoint.

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
- `config/parity-specs/saved-searches/saved-search-local-staging.json` replays the create, downstream read, overlong-name update validation, and missing update/delete branches through the generic parity runner with strict JSON targets. Expected differences are limited to deterministic local IDs and opaque connection cursors.
- The HAR-402 `2026-04` resource-root capture confirmed `PRODUCT`, `COLLECTION`, `ORDER`, `DRAFT_ORDER`, `FILE`, and `DISCOUNT_REDEEM_CODE` create/read/delete behavior, the customer-create deprecation userError, default order/draft-order saved-search records, and the fact that most saved-search connection roots reject `query:` arguments in that API version.
- `config/parity-specs/saved-searches/saved-search-resource-roots.json` replays that HAR-402 capture through the generic parity runner with strict JSON targets for create payloads, downstream resource-root reads, cleanup deletes, and post-delete reads. Expected differences are limited to deterministic local IDs/legacy IDs.
- `tests/integration/saved-search-flow.test.ts` additionally verifies resource-specific read-after-write routing for every locally supported saved-search create `resourceType`, including the explicit current `PRICE_RULE` behavior shared by code and automatic discount saved-search roots.
- HAR-458 added `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/saved-searches/saved-search-query-grammar.json` and `config/parity-specs/saved-searches/saved-search-query-grammar.json` for quoted/grouped saved-search grammar. The executable parity target proves Shopify's mutation-payload preservation, downstream stored-query normalization, `searchTerms` shape, and negated-filter extraction for a grouped `OR` expression.
