---
title: 'Admin Platform Utility Roots'
description: 'Coverage notes and fidelity boundaries for Admin Platform Utility Roots.'
---

This endpoint group covers Shopify Admin GraphQL platform and utility roots that are not owned by a single merchant resource family: `publicApiVersions`, generic Node dispatch, jobs, taxonomy, domains, backup regions, staff access, and Flow helper mutations.

## Current support and limitations

### Supported roots

Read roots:

- `publicApiVersions`
- `node(id:)`
- `nodes(ids:)`
- `job(id:)`
- `taxonomy`
- `domain(id:)`
- `backupRegion`
- `staffMember`
- `staffMembers`

Mutation roots:

- `backupRegionUpdate`
- `flowGenerateSignature`
- `flowTriggerReceive`

### Local behavior

Snapshot reads are conservative and only model shapes backed by checked-in evidence:

- `publicApiVersions` returns the captured Admin API version window.
- `node(id:)` and `nodes(ids:)` dispatch by GID type to an existing local detail handler or serializer. They preserve input order for `nodes(ids:)`, return `null` for malformed/missing/unsupported IDs, and do not create domain support by themselves.
- Supported generic Node families include records that already exist in normalized local state for products, product options and option values, product variants, catalog/inventory records, metafields, selling plans, customers and payment methods, B2B companies and selected nested records, app billing/access records, store/shop/location/business-entity records, files, saved searches, payment terms, finance/POS/dispute no-data records, bulk operations, metafield/metaobject definitions, orders/fulfillments/returns/draft orders, gift cards, delivery profiles and selected nested records, discount wrappers, marketing/events/webhooks/segments, markets and price lists, taxonomy categories, and supported online-store records.
- Unsupported generic Node implementors and resource families without a local lifecycle/read model return Shopify-like `null` entries instead of partial fabricated objects.
- `job(id:)` resolves staged or fixture-backed generic `Job` nodes. Collection product-membership jobs staged by supported collection mutations read back as completed with a selected `query { __typename }` QueryRoot link. Unknown arbitrary Job GIDs preserve the captured compatibility payload shape.
- `domain(id:)` resolves the effective snapshot shop `primaryDomain` by ID when present; unknown IDs return `null`.
- `backupRegion` returns staged or snapshot backup-region state, then derives a shop-domain-scoped `MarketRegionCountry` from effective shop country data when checked-in evidence backs that shop/country pair. Unbacked domain/country combinations return `null`.
- `taxonomy.categories(...)` reads normalized taxonomy category records from snapshot/local state. It supports captured hierarchy fields, raw Shopify cursors, selected `pageInfo`, simple term matching over captured `id`, `name`, and `fullName`, and hierarchy filters limited to categories already present in local state. The proxy does not invent taxonomy rows.
- `staffMember` and `staffMembers` return the captured field-level `ACCESS_DENIED` blocker for the current credential posture.
- The by-id not-found parity scenario records implemented singular `id:` read roots returning `null` for non-existent GIDs. Credential-restricted roots preserve their captured Shopify error envelopes without expanding local support for those domains.

LiveHybrid/cassette behavior:

- Cold `publicApiVersions`, `taxonomy`, and selected `node` / `nodes` reads can forward to cassette/upstream responses when no local platform state or staged serializer-owned resource is available.
- Once local state exists, supported reads use the local serializer path so snapshot behavior and read-after-write effects remain local.

Mutation behavior:

- `backupRegionUpdate` stages the selected fallback region in local admin-platform state and updates downstream `backupRegion` reads without mutating Shopify at runtime when the modeled caller/shop is Markets-eligible or when no stricter access model exists.
- `backupRegionUpdate` returns the captured `ACCESS_DENIED` top-level envelope without staging when modeled app installation, delegated-token, or shop state proves Markets access is unavailable.
- Explicit country updates require backed country evidence plus an active non-legacy region-type market covering that country. Unknown countries and uncovered countries return captured `REGION_NOT_FOUND` `MarketUserError` payloads without staging.
- Present `region` input objects with missing, `null`, or non-enum `countryCode` fail as top-level GraphQL input coercion errors before staging; omitted or explicit `null` `region` inputs still behave as current-state reads.
- Omitted or explicit `null` `region` inputs behave as idempotent current backup-region reads with `userErrors: []` in local parity.
- `flowGenerateSignature` short-circuits proxy-local Flow trigger IDs, returns a deterministic local signature, stores only payload/signature SHA-256 hashes in meta state, and keeps the raw mutation in the mutation log for commit replay. Unknown Flow trigger IDs return Shopify's captured `RESOURCE_NOT_FOUND` top-level error.
- `flowTriggerReceive` records proxy-local trigger receipts for non-blank handles, stores compact payload metadata and hashes, and does not deliver an external Flow trigger at runtime. Body-only requests must match the captured body schema and a shop-scoped Flow trigger registration model; because that registration bucket is not modeled, body-only trigger references are rejected conservatively.

### Boundaries

- Generic Node dispatch remains unsupported for families without an owning local lifecycle/read model, including product taxonomy, product variant components, quantity price breaks, delivery profile item IDs, order delivery methods, B2B staff/catalog nested records, and non-empty finance/POS/dispute records.
- `staffMember` and `staffMembers` are access-blocked only; authorized staff catalog behavior requires separate staff identity evidence and a staff state model.
- `backupRegion` country support is limited to country pairs backed by checked-in market-region evidence. Current `backupRegionUpdate` success coverage includes CA, US, and the harry-test-heelo captured region set AE/AT/AU/BE/CH/CZ/DE/DK/ES/FI/MX; unbacked country codes still return `REGION_NOT_FOUND`.
- `taxonomy.categories(...)` is not exhaustive global taxonomy coverage; missing captured rows produce Shopify-like empty connections.
- Flow helper mutations record local metadata only. They do not deliver Flow triggers or prove external Flow automation execution.
- No root listed here is registry-only. Validation-only branches include GraphQL input coercion for required arguments and captured local guardrails that fail before staging.

### Evidence

- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/admin-platform/admin-platform-utility-roots.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/markets-baseline.json`
- `fixtures/conformance/very-big-test-store.myshopify.com/2026-04/markets/markets-baseline.json`
- `config/parity-specs/admin-platform/admin-platform-utility-reads.json`
- `config/parity-specs/admin-platform/admin-platform-supported-node-reads.json`
- `config/parity-specs/admin-platform/admin-platform-product-option-node-reads.json`
- `config/parity-specs/admin-platform/admin-platform-metafield-node-reads.json`
- `config/parity-specs/admin-platform/admin-platform-market-region-node-read.json`
- `config/parity-specs/admin-platform/admin-platform-market-web-presence-node-read.json`
- `config/parity-specs/admin-platform/admin-platform-finance-risk-node-no-data.json`
- `config/parity-specs/admin-platform/admin-platform-store-property-node-reads.json`
- `config/parity-specs/admin-platform/admin-platform-selling-plan-node-reads.json`
- `config/parity-specs/admin-platform/by-id-not-found-read.json`
- `config/parity-specs/admin-platform/admin-platform-taxonomy-hierarchy-node-reads.json`
- `config/parity-specs/admin-platform/admin-platform-backup-region-update.json`
- `config/parity-specs/admin-platform/admin-platform-backup-region-update-validation.json`
- `config/parity-specs/admin-platform/admin-platform-backup-region-update-extended.json`
- `config/parity-specs/admin-platform/admin-platform-backup-region-update-no-region-market.json`
- `config/parity-specs/admin-platform/admin-platform-backup-region-update-access-blocker.json`
- `config/parity-specs/admin-platform/admin-platform-flow-generate-signature.json`
- `config/parity-specs/admin-platform/admin-platform-flow-generate-signature-required-args.json`
- `config/parity-specs/admin-platform/admin-platform-flow-trigger-receive.json`
- `config/parity-specs/admin-platform/admin-platform-flow-trigger-receive-validation.json`
- `config/parity-specs/admin-platform/admin-platform-flow-trigger-receive-body-validation.json`
- `config/parity-specs/admin-platform/admin-platform-flow-trigger-receive-body-schema-gaps.json`
- `tests/unit/__snapshots__/admin-platform-node-coverage.test.ts.snap`

### Validation

- `corepack pnpm parity -- admin-platform-utility-reads`
- `corepack pnpm parity -- admin-platform-supported-node-reads`
- `corepack pnpm parity -- admin-platform-by-id-not-found-read`
- `corepack pnpm parity -- admin-platform-backup-region-update`
- `corepack pnpm parity -- admin-platform-flow-generate-signature`
- `corepack pnpm parity -- admin-platform-flow-trigger-receive`
- `corepack pnpm conformance:check`
