# Saved Searches and URL Redirects

HAR-312 adds the first local saved-search model. This is scoped to Shopify Admin `SavedSearch` records for products, collections, orders, draft orders, files, and discount saved-search roots.

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
- Empty saved-search roots return a non-null connection with empty `nodes`/`edges` and false/null `pageInfo`, matching the captured no-data shape. `orderSavedSearches` and `draftOrderSavedSearches` are captured exceptions: Shopify returns default order and draft-order saved searches even when no merchant-created saved searches are present, and the local saved-search domain preserves those records. Those default records behave like persisted Shopify rows for local `savedSearchUpdate` and `savedSearchDelete`: updates stage an override visible through the saved-search connection, and deletes record the default ID as deleted so the static default does not reappear on later reads.
- Staged saved searches are routed by `resourceType`: `PRODUCT`, `COLLECTION`, `ORDER`, `DRAFT_ORDER`, `FILE`, and `DISCOUNT_REDEEM_CODE` appear only under their matching saved-search root. `PRICE_RULE` is the current discount saved-search resource type observed by the Admin schema, so the local model exposes those staged searches through both `codeDiscountSavedSearches` and `automaticDiscountSavedSearches` until deeper discount subtype evidence proves a stricter split.
- `savedSearchCreate` and `savedSearchUpdate` enforce Shopify's case-sensitive saved-search name uniqueness across the effective local list: static defaults, hydrated base state, and staged records. Shopify scopes the backing model by shop, type, and subtype; the proxy does not currently model subtype, so this approximation keys uniqueness by `resourceType` plus exact `name`.
- Saved-search query strings are parsed into `searchTerms` plus `filters { key value }` records. Simple top-level field terms of the form `key:value` become filters, while free-text terms remain `searchTerms`. Captured grouped/boolean query grammar keeps the grouped expression in `searchTerms`, normalizes quoted field values to double quotes, and extracts top-level negated field terms such as `-vendor:Archived` as `filters[{ key: "vendor_not", value: "Archived" }]`.
- `savedSearchCreate` and `savedSearchUpdate` run a best-effort query validation pass before staging. The current reserved-filter table covers `ORDER` saved searches with `reference_location_id`, which Shopify rejects as `Search terms is invalid, 'reference_location_id' is a reserved filter name`. The current compatibility table covers `PRODUCT` saved searches where `collection_id` is mutually exclusive with `tag`, `error_feedback`, and `published_status`, returning `Query has incompatible filters: collection_id, <filter>`. `collection_id` by itself remains valid.
- Top-level saved-search filter tokens are validated against per-resource allowlists before staging. The current local allowlist coverage is:
  - `PRODUCT`: `collection_id`, `created_at`, `error_feedback`, `handle`, `id`, `inventory_total`, `product_type`, `published_at`, `published_status`, `sku`, `status`, `tag`, `title`, `updated_at`, `vendor`
  - `COLLECTION`: `collection_type`, `handle`, `id`, `product_id`, `product_publication_status`, `publishable_status`, `published_at`, `published_status`, `title`, `updated_at`
  - `ORDER`: `channel_id`, `created_at`, `customer_id`, `email`, `financial_status`, `fulfillment_status`, `id`, `location_id`, `name`, `processed_at`, `sales_channel`, `status`, `tag`, `test`, `updated_at`
  - `DRAFT_ORDER`: `created_at`, `customer_id`, `email`, `id`, `name`, `status`, `tag`, `updated_at`
  - `FILE`: `created_at`, `filename`, `id`, `media_type`, `original_source`, `status`, `updated_at`
  - `DISCOUNT_REDEEM_CODE`: `code`, `created_at`, `discount_id`, `id`, `status`, `updated_at`
- Unknown top-level filters for covered resource types return `Query is invalid, '<field>' is not a valid filter`. Create failures return `savedSearch: null` with `field: ["input", "query"]`; update failures return the captured non-null payload echo without staging effective state and use `field: ["input", "searchTerms"]`. Resource types without an allowlist continue to fall through to existing validation instead of producing guessed false positives.
- Failed query validation on create returns `savedSearch: null`. Captured 2025-01 update behavior differs: Shopify returns a non-null `savedSearchUpdate.savedSearch` echo containing the submitted invalid query plus `userErrors`, but the proxy keeps that failed update out of effective staged state.
- Mutation payloads preserve the submitted `query` ordering, while downstream connection reads expose the normalized stored query. The captured `savedSearchUpdate` validation branch also keeps valid query changes visible in the payload when an overlong name is rejected.
- Inline missing/null required input object fields are rejected before local staging with Shopify-style top-level GraphQL coercion errors. Captured 2025-01 evidence covers missing `name`/`query`/`resourceType` on `savedSearchCreate` and missing `id` on `savedSearchUpdate`; those payloads contain `errors` and no resolver `userErrors`.
- An explicitly supplied empty saved-search `query: ""` is valid on create and update. The local model stages the empty string instead of returning a blank-query userError.
- Query parsing uses the shared Admin search-query term helpers for token interpretation, with saved-search-specific stored-query projection layered on top. The local model now covers the captured quoted/grouped `OR` expression and top-level negated-filter normalization from HAR-458, plus the HAR-729 reserved-filter and compatibility guardrails. The shared term parser is total, so Shopify parser-invalid branches that depend on per-resource filter typing, such as numeric/date comparator validation, remain TODOs for a follow-up capture instead of guessed local behavior.
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
- HAR-718 added `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/saved-searches/saved-search-required-input-validation.json` and `config/parity-specs/saved-searches/saved-search-required-input-validation.json` for required input coercion and explicit empty-query create behavior.
- The HAR-402 `2026-04` resource-root capture confirmed `PRODUCT`, `COLLECTION`, `ORDER`, `DRAFT_ORDER`, `FILE`, and `DISCOUNT_REDEEM_CODE` create/read/delete behavior, the customer-create deprecation userError, default order/draft-order saved-search records, and the fact that most saved-search connection roots reject `query:` arguments in that API version.
- `config/parity-specs/saved-searches/saved-search-resource-roots.json` replays that HAR-402 capture through the generic parity runner with strict JSON targets for create payloads, downstream resource-root reads, cleanup deletes, and post-delete reads. Expected differences are limited to deterministic local IDs/legacy IDs.
- HAR-458 added `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/saved-searches/saved-search-query-grammar.json` and `config/parity-specs/saved-searches/saved-search-query-grammar.json` for quoted/grouped saved-search grammar. The executable parity target proves Shopify's mutation-payload preservation, downstream stored-query normalization, `searchTerms` shape, and negated-filter extraction for a grouped `OR` expression.
- HAR-720 added `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/saved-searches/saved-search-name-uniqueness.json` and `config/parity-specs/saved-searches/saved-search-name-uniqueness.json` for duplicate-name validation. The capture proves duplicate create returns `savedSearch: null`, while duplicate update returns the same `userErrors[{ field: ["input", "name"], message: "Name has already been taken" }]` and a non-null payload echo with the existing name plus submitted valid query; the proxy keeps that failed update out of effective staged state.
- HAR-729 added `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/saved-searches/saved-search-query-grammar-validation.json` and `config/parity-specs/saved-searches/saved-search-query-grammar-validation.json` for query validation guardrails. The capture proves ORDER `reference_location_id` reserved-filter rejection, PRODUCT `collection_id` incompatibility with `tag`, `error_feedback`, and `published_status`, PRODUCT `collection_id` positive control behavior, and the non-staged update payload echo for PRODUCT `collection_id + tag`.
- `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/saved-searches/saved-search-unknown-filter-field.json` and `config/parity-specs/saved-searches/saved-search-unknown-filter-field.json` prove PRODUCT `made_up_filter` rejection with `field: ["input", "query"]` and the known-filter positive control for `vendor:Acme`.
- `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/saved-searches/saved-search-default-record-update-delete.json` and `config/parity-specs/saved-searches/saved-search-default-record-update-delete.json` prove persisted ORDER/DRAFT_ORDER default saved searches can be updated or deleted by ID, and that downstream `orderSavedSearches`/`draftOrderSavedSearches` reads reflect the staged update/delete.
