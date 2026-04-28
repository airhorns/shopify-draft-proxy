# Admin Platform Utility Roots

This endpoint group covers Admin GraphQL platform/utility roots that do not belong to a merchant resource family yet:

- queries: `publicApiVersions`, `node`, `nodes`, `job`, `taxonomy`, `domain`, `backupRegion`, `staffMember`, `staffMembers`
- mutations: `backupRegionUpdate`, `flowGenerateSignature`, `flowTriggerReceive`

HAR-315 conformance evidence lives at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/admin-platform-utility-roots.json`.

## Current support and limitations

### Snapshot Read Behavior

The local snapshot handler is intentionally conservative and only models shapes backed by the HAR-315 capture:

- `publicApiVersions` returns the captured 2026-04 version window. Refresh the fixture and constant when Shopify rotates supported Admin API versions.
- `node(id:)` returns `null` for unknown ids.
- `nodes(ids:)` returns one `null` entry per unknown input id, preserving input order.
- `job(id:)` mirrors the captured arbitrary-job behavior: a requested Job GID returns a completed job payload with that id and a selected `query { __typename }` QueryRoot link. The proxy does not model async job lifecycle state yet.
- `domain(id:)` resolves the effective snapshot shop `primaryDomain` by id when one is present; unknown ids return `null`.
- `backupRegion` returns the captured `MarketRegionCountry` slice for the current conformance shop. Broader shop-country-to-region id mapping remains a gap.
- `taxonomy.categories(...)` returns an empty connection shape for the captured unmatched search/no-data branch. The global non-empty taxonomy catalog is not modeled.
- `staffMember` and `staffMembers` return the captured field-level `ACCESS_DENIED` blocker locally. Authorized staff identity/catalog reads require an eligible app/store and a separate local staff model before support can broaden.

### Mutation Behavior

`backupRegionUpdate` stages the selected fallback region in the in-memory admin platform state and updates downstream snapshot `backupRegion` reads without mutating Shopify at runtime. HAR-374 conformance covers the current conformance shop's idempotent `CA` success branch and `REGION_NOT_FOUND` validation for an unknown country code. The local country mapping is intentionally narrow until more shop-country captures exist.

`flowGenerateSignature` is locally short-circuited for proxy-local Flow trigger IDs. The proxy returns a deterministic local signature, stores only payload/signature SHA-256 hashes in meta state, and keeps the original raw mutation in the mutation log for eventual commit replay. Unknown Flow trigger IDs mirror the captured Shopify `RESOURCE_NOT_FOUND` top-level error.

`flowTriggerReceive` records proxy-local trigger receipts for handles with the `local-` / HAR-374 local prefix. It does not deliver any external Flow trigger at runtime. The staged meta state records only the handle, payload byte count, and payload hash; the raw payload remains in the mutation log request body for commit replay. Captured validation branches include unknown handle and payloads whose JSON representation exceeds 50000 bytes.

## Historical and developer notes

- Conformance evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/admin-platform-utility-roots.json`.
- Executable parity specs: `admin-platform-backup-region-update.json`, `admin-platform-flow-generate-signature.json`, and `admin-platform-flow-trigger-receive.json`.
