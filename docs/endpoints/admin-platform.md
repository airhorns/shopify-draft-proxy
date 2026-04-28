# Admin Platform Utility Roots

This endpoint group covers Admin GraphQL platform/utility roots that do not belong to a merchant resource family yet:

- queries: `publicApiVersions`, `node`, `nodes`, `job`, `taxonomy`, `domain`, `backupRegion`, `staffMember`, `staffMembers`
- mutations: `flowGenerateSignature`, `flowTriggerReceive`

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

### Mutation Safety

`flowGenerateSignature` and `flowTriggerReceive` remain unsupported side-effect utility mutations. Runtime requests still use the unsupported-mutation passthrough escape hatch, but the operation registry and logs mark them as known unsafe Flow utility gaps. Do not mark either root supported until local signing/trigger delivery behavior and raw commit replay semantics exist.

## Historical and developer notes

- Conformance evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/admin-platform-utility-roots.json`.
