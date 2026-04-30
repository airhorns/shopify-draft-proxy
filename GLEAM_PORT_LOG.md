# GLEAM_PORT_LOG.md

A chronological log of the Gleam port. Each pass adds a new dated entry
describing what landed, what was learned, and what is now blocked or
unblocked. The acceptance criteria and design constraints live in
`GLEAM_PORT_INTENT.md`; this file is the running narrative.

Newer entries go at the top.

---

## 2026-04-30 - Pass 33: store-properties locations, business entities, and publishables

Completes the parity-backed Store Properties root batch in the Gleam dispatcher.
The port now covers the 15 implemented Store Properties registry roots: shop
and shop-policy behavior from Pass 32, business-entity reads, location
catalog/detail/identifier reads, local location lifecycle guardrails, and
publishable publish/unpublish staging for the captured Product and Collection
publication projections. The parity runner seeds captured Store Properties
baselines and publishable mutation payloads so all 20 checked-in
`config/parity-specs/store-properties/*.json` scenarios execute on both
targets.

The TypeScript Store Properties runtime remains in place. This pass ports the
implemented registry roots and parity-backed projections, but the TS module
still owns broader cross-domain helpers for unported Products, Markets,
Shipping/Fulfillments, and Online Store flows until those Gleam domains exist.

| Module                                                             | Change                                                                                                                                                   |
| ------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`                  | Adds JSON-shaped Store Properties records and mutation-payload records for captured Location, BusinessEntity, Product, and Collection projections.       |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                  | Adds base/staged locations, business entities, publishables, payload fixtures, deletion markers, ordered listing, and effective lookup helpers.          |
| `gleam/src/shopify_draft_proxy/proxy/store_properties.gleam`       | Adds Store Properties read roots, local location lifecycle validation/staging, publishable mutation staging, generic projection, and mutation logging.   |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`            | Routes legacy Store Properties query roots and serializes the new slices through `__meta/state` for local observability.                                 |
| `gleam/test/parity/runner.gleam`                                   | Seeds remaining Store Properties capture fixtures for business entities, locations, publishable payloads, and collection publication readback.           |
| `gleam/test/parity_test.gleam`                                     | Enables all 20 Store Properties parity specs as executable Gleam parity evidence.                                                                        |
| `gleam/test/shopify_draft_proxy/proxy/store_properties_test.gleam` | Adds direct coverage for location read/edit/log/meta-state behavior, business-entity reads, and publishable collection staging/read-after-write effects. |

Validation: `gleam test --target javascript` is green at 702 tests on the host
Node runtime. `gleam test --target erlang` is green at 698 tests via the
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container with the repository
root mounted because the host still lacks `escript`. The Store Properties parity
report shows 20 spec files and 20 Gleam parity registrations, with no missing or
extra registrations.

### Findings

- The implemented Store Properties registry batch is smaller than the TypeScript
  module boundary: the TS file also contains helper behavior used by domains not
  yet present in the Gleam port.
- Publishable parity can be modeled from captured payload fixtures without
  claiming the full Products domain; staged Product/Collection records are
  limited to the selected publication projections needed by Store Properties
  scenarios.
- Location validation branches that fail Shopify guardrails do not create
  mutation-log entries; successful local lifecycle mutations preserve the
  original mutation document and staged resource IDs.

### Risks / open items

- Deleting `src/proxy/store-properties.ts` remains deferred until the dependent
  Products, Markets, Shipping/Fulfillments, and Online Store Gleam slices that
  rely on its helper behavior have their own ported equivalents.
- The new Store Properties records intentionally preserve captured JSON-shaped
  projections. Stronger typed records should be introduced only when an owning
  domain needs local lifecycle logic beyond these parity-backed fields.
- Location lifecycle support covers the captured validation/success behavior in
  the Store Properties parity suite; fulfillment-service, carrier-service, and
  delivery-profile location interactions still belong to the future
  Shipping/Fulfillments port.

### Pass 34 candidates

- Start Shipping/Fulfillments substrate so fulfillment-service, carrier-service,
  delivery-profile, and shipping-settings roots can consume ported Location
  state without reaching back into the TypeScript module.
- Start Products publication substrate so Product and Collection publishable
  projections can move from captured Store Properties rows into typed product
  and collection records.
- Continue Markets or Online Store ports where Store Properties shop/location
  read effects are now available as local Gleam state.

---

## 2026-04-30 — Pass 32: store-properties shop and policy foundation

Ports the Store Properties shop slice into the Gleam dispatcher. The new domain
covers local `shop` reads from effective base/staged shop state and
`shopPolicyUpdate` local staging, including policy validation, synthetic
timestamps/IDs, downstream `shop.shopPolicies` read-after-write behavior, and
mutation-log observability for successful staged updates. Admin Platform
`node`/`nodes` now resolves Store Properties-owned `Shop`, `ShopAddress`, and
`ShopPolicy` records when the shop slice is seeded.

The TypeScript Store Properties implementation remains in place because the
larger domain still owns locations, fulfillment services, carrier services,
business entities, payment settings branches, publication helpers, and other
store-adjacent roots that are not ported in this pass.

| Module                                                             | Change                                                                                                                                         |
| ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`                  | Adds typed Shop, Domain, ShopAddress, plan/resource-limit/features/payment-settings, and ShopPolicy records.                                   |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                  | Adds base/staged shop state, effective-shop lookup, base seeding, and staged shop replacement helpers.                                         |
| `gleam/src/shopify_draft_proxy/proxy/store_properties.gleam`       | Adds Store Properties query/mutation handling for `shop` and `shopPolicyUpdate`, policy serialization, validation, local staging, and logging. |
| `gleam/src/shopify_draft_proxy/proxy/admin_platform.gleam`         | Resolves Store Properties-owned Relay Node records for `Shop`, `ShopAddress`, `ShopPolicy`, and primary `Domain` from effective shop state.    |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`            | Routes Store Properties query/mutation capabilities and serializes the shop slice through `__meta/state`.                                      |
| `gleam/test/parity/runner.gleam`                                   | Seeds Store Properties captures from `readOnlyBaselines.shop.data.shop` and supports wildcard expected-difference paths.                       |
| `gleam/test/parity_test.gleam`                                     | Enables `shop-baseline-read`, `shopPolicyUpdate-parity`, and `admin-platform-store-property-node-reads` as executable Gleam parity evidence.   |
| `gleam/test/shopify_draft_proxy/proxy/store_properties_test.gleam` | Adds direct coverage for empty reads, seeded shop projection, policy staging, validation, mutation logging, and Admin Platform Node reads.     |

Validation: `gleam test --target javascript` is green at 670 tests on the host
Node runtime. `gleam test --target erlang` is green at 666 tests via the
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container because the host
lacks `escript`. `gleam format`, `corepack pnpm gleam:format:check`,
`corepack pnpm gleam:smoke:js`, and `git diff --check` are green.

### Findings

- Store Properties is a singleton-heavy domain: `shopPolicyUpdate` replaces the
  effective shop row with an updated policy list rather than maintaining a
  separate policy collection.
- Captured parity specs already use wildcard expected-difference paths such as
  `$.shop.shopPolicies[*].updatedAt`; the Gleam parity diff needed matching
  support so existing specs could run without rewriting fixtures.
- Validation-only `shopPolicyUpdate` user errors do not create mutation-log
  entries; successful local policy updates record the staged policy ID and
  preserve the original mutation document.

### Risks / open items

- Store Properties coverage is limited to `shop` and `shopPolicyUpdate`;
  location, fulfillment-service, carrier-service, publication, business-entity,
  and payment-settings roots still need separate domain passes.
- Snapshot file loading for the full TS normalized state shape is still not
  ported; parity evidence seeds the shop slice from captured fixtures in the
  runner.
- Admin Platform generic Node dispatch is still only as broad as the owning
  Gleam resource domains that have been ported.

### Pass 33 candidates

- Continue Store Properties with locations and fulfillment/carrier-service
  lifecycle roots, reusing the new shop slice where those reads nest under
  shop state.
- Continue Admin Platform parity seeding for backup-region and taxonomy utility
  captures now that store-property Node reads are executable.
- Continue Marketing upstream hydration and parity-runner seeding so captured
  Marketing read/update scenarios can execute against the Gleam proxy.

---

## 2026-04-30 — Pass 31: admin-platform utility roots

Ports a broad Admin Platform utility batch into the Gleam dispatcher. The new
domain covers the safe local/no-data read roots (`publicApiVersions`, `node`,
`nodes`, `job`, `domain`, `backupRegion`, `taxonomy`, `staffMember`, and
`staffMembers`) plus the locally handled utility mutations
`flowGenerateSignature`, `flowTriggerReceive`, and `backupRegionUpdate`.
Successful mutations stage only in memory, preserve raw mutation documents in
the mutation log, and keep sensitive Flow payload/signature data hashed in the
state slice.

The TypeScript Admin Platform implementation remains in place because the full
generic Node resolver matrix still depends on unported product/customer/order,
store-property, delivery-profile, markets, and payments substrate. This pass
intentionally models the utility subset without claiming those downstream
families are ported.

| Module                                                           | Change                                                                                                                                                    |
| ---------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`                | Adds backup-region and Admin Platform Flow audit record types.                                                                                            |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                | Adds base/staged backup-region state plus staged Flow signature/trigger audit buckets and helpers.                                                        |
| `gleam/src/shopify_draft_proxy/proxy/admin_platform.gleam`       | Adds Admin Platform utility read serialization, staff access blockers, Flow utility mutation handling, backup-region local staging, and log recording.    |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`          | Routes Admin Platform capabilities and legacy root detection for query and mutation paths.                                                                |
| `gleam/test/shopify_draft_proxy/proxy/admin_platform_test.gleam` | Adds direct and dispatcher coverage for utility reads, staff errors, backup-region read-after-write, Flow validation, Flow staging, and mutation logging. |

Validation: `gleam test --target erlang` is green at 644 tests via the
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container. The JavaScript
target is green at 651 tests by compiling with the same container and running
the generated gleeunit bundle with the host Node runtime because the container
does not include `node`. Targeted touched-file `gleam format --check ...` is
green.

### Findings

- Admin Platform is a coordinator surface: the utility roots can be ported now,
  but full Relay `node`/`nodes` parity should expand only as owning resource
  domains land in Gleam.
- Validation-only utility mutations should not create mutation-log entries;
  successful local Flow and backup-region mutations do record staged log
  entries with original documents for commit replay.
- Flow utility staging can keep the observable local behavior without storing
  raw signatures or payloads in the state buckets.

### Risks / open items

- The current Gleam `flowGenerateSignature` signature is deterministic and
  runtime-test-backed, but it is not yet HMAC-identical to the TypeScript helper;
  live success parity is still deferred because the conformance app has no safe
  valid Flow trigger capture.
- Generic Node dispatch is still null-only for Admin Platform in Gleam until
  the relevant resource domains and node serializers are ported.
- Taxonomy remains limited to Shopify-like empty/no-data connection shapes; the
  captured taxonomy hierarchy catalog still needs runner seeding and taxonomy
  record state before parity can be enabled.

### Pass 32 candidates

- Port Admin Platform parity-runner seeding for utility roots that only require
  backup-region/no-data behavior, then enable the safe parity subset.
- Port Store Properties read substrate next so Admin Platform `domain`,
  `shopAddress`, and `shopPolicy` node dispatch can resolve real local records.
- Continue Marketing upstream hydration and parity-runner seeding so captured
  Marketing read/update scenarios can execute against the Gleam proxy.

---

## 2026-04-30 — Pass 30: marketing state/read/mutation foundation

Continues Marketing beyond the empty-read stub. The Gleam port now has
normalized Marketing activity, event, and engagement state buckets; store-backed
activity/event reads; connection filters, cursors, and sort handling; and local
staging for the supported Marketing mutation roots without runtime Shopify
writes. This broadens HAR-471 beyond BulkOperations with another substantive
endpoint family in the same PR.

The TypeScript Marketing implementation remains in place because full parity
runner enablement and upstream hydration are still not complete; this pass
ports the local lifecycle foundation and direct integration-test coverage.

| Module                                                      | Change                                                                                                                                                           |
| ----------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`           | Adds JSON-shaped `MarketingValue`, `MarketingRecord`, and `MarketingEngagementRecord` state types.                                                               |
| `gleam/src/shopify_draft_proxy/state/store.gleam`           | Adds base/staged Marketing buckets, effective activity/event/engagement listing, remote-id lookup, external delete helpers, and channel engagement delete paths. |
| `gleam/src/shopify_draft_proxy/proxy/marketing.gleam`       | Replaces the empty stub with stateful reads, connection search/sort/pagination, native/external activity mutations, engagement create/delete, and log recording. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`     | Routes Marketing queries with store/variables and routes Marketing mutations through the local dispatcher.                                                       |
| `gleam/test/shopify_draft_proxy/proxy/marketing_test.gleam` | Expands coverage from empty reads to stateful reads, filters, pagination, native/external activity lifecycle, validation, logs, and engagement deletion.         |

Validation: `gleam test --target erlang` is green at 636 tests via the
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container. The JavaScript
target is green at 643 tests by compiling with the same container and running
the generated gleeunit bundle with the host Node runtime because the container
does not include `node`. Targeted touched-file `gleam format --check ...` is
green.

### Findings

- The Marketing state needs a JSON-shaped ADT because activity/event payloads
  intentionally preserve arbitrary Shopify-selected fields while still letting
  the query projector walk them safely on both targets.
- Validation-only Marketing failures should not create mutation-log entries;
  successful locally staged roots do record the original raw mutation document
  and staged ids for commit replay.
- Channel-level engagement deletion depends on known Marketing event
  `channelHandle` values rather than fabricated channel catalogs.

### Risks / open items

- Upstream Marketing hydration is not ported yet, so live-hybrid Marketing
  reads still need a future pass before parity scenarios that seed from live
  captures can run against Gleam.
- The generic Marketing parity specs are still not enabled in Gleam; enabling
  them should wait until hydration/seeding and comparison coverage are ported.
- The TypeScript Marketing module remains the authority until parity evidence
  is executable for the full domain.

### Pass 31 candidates

- Port Marketing upstream hydration and parity-runner seeding so captured
  Marketing read/update scenarios can execute against the Gleam proxy.
- Start product read substrate work required by full `bulkOperationRunQuery`
  JSONL export parity.
- Continue bulk-operations with `bulkOperationRunMutation` once inner import
  executors are available in Gleam.

---

## 2026-04-30 — Pass 29: bulk-operation state/read/cancel foundation

Continues bulk-operations beyond the empty-read stub. The Gleam port now has a
real BulkOperation store slice, effective local reads, catalog filtering /
pagination, current operation derivation, local `bulkOperationCancel`, and a
local `bulkOperationRunQuery` staging shell that records supported mutation-log
metadata without runtime Shopify writes.

This is not the full TypeScript bulk executor yet: product JSONL export contents
and `bulkOperationRunMutation` import replay remain deferred until the relevant
product/import substrate is available in Gleam. The old null/empty read contract
still holds when no BulkOperation state exists.

| Module                                                            | Change                                                                                                                                                                |
| ----------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`                 | Adds `BulkOperationRecord`, mirroring the TS record plus a temporary `resultJsonl` holder until the result-file route ports.                                          |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                 | Adds base/staged BulkOperation buckets, ordering, effective lookup/listing, result staging, staged cancel, and presence APIs.                                         |
| `gleam/src/shopify_draft_proxy/proxy/bulk_operations.gleam`       | Replaces the stub with read projections, search filtering, cursor pagination, current-operation lookup, run-query shell, cancel handling, and mutation-log recording. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`           | Routes BulkOperations queries with store/variables and routes supported BulkOperations mutations through the local dispatcher.                                        |
| `gleam/test/shopify_draft_proxy/proxy/bulk_operations_test.gleam` | Expands coverage from empty reads to stateful reads, filtering, pagination, current roots, run-query staging/logging, and cancel read-after-write.                    |

Validation: `gleam test --target javascript` is green at 643 tests.
`gleam test --target erlang` is green at 636 tests via the
`erlang:27` container fallback because the host lacks a local Gleam/BEAM
toolchain. Targeted touched-file `gleam format --check ...` is green.

### Findings

- BulkOperation reads can reuse the shared search-query parser and connection
  helpers cleanly; the endpoint module only owns domain-specific positive-term
  matching, sort decisions, cursor choice, and projection.
- The local cancel semantics depend on distinguishing staged operations from
  base-only operations. A non-terminal base-only operation still returns the
  captured "does not exist" user error because local cancel only mutates staged
  jobs.
- Gleam currently stores generated result JSONL on `BulkOperationRecord`
  instead of a sibling `bulkOperationResults` map. That keeps the state slice
  useful without claiming the not-yet-ported HTTP result-file surface.

### Risks / open items

- `bulkOperationRunQuery` currently stages a completed local query job shell
  with zero generated records; full product JSONL export parity still requires
  the product state/read substrate in Gleam.
- `bulkOperationRunMutation` remains unrouted in Gleam because replaying inner
  Admin mutations requires product/customer/location import executors that have
  not been ported.
- The generic bulk-operations parity scenario is still not enabled in Gleam;
  enabling it should wait until runner seeding and product export output can
  satisfy the captured operation counters and result metadata.

### Pass 30 candidates

- Continue bulk-operations by adding parity-runner seeding for captured
  BulkOperation jobs and enabling the read/cancel subset that no longer depends
  on product export output.
- Continue marketing beyond the empty-read stub by porting the activity and
  engagement state slices.
- Start product read substrate work required by full `bulkOperationRunQuery`
  JSONL export parity.

---

## 2026-04-29 — Pass 28: function owner metadata parity seeding

Enables `functions-owner-metadata-local-staging` as executable Gleam
parity evidence. The scenario's capture carries explicit
`seedShopifyFunctions` records for installed validation and cart
transform Functions, including owner `appKey`, `description`, and
selected `app` fields. The runner now hydrates those records before
the primary mutation and mirrors the local-runtime seed counter
advance so the captured synthetic ids and timestamps line up.

This pass also closes a Functions-domain substrate gap: handled
Functions mutations now record a staged mutation-log entry after the
domain response is built, matching the TS runtime's supported-mutation
observability and preserving commit replay metadata.

| Module                                                               | Change                                                                                                                                             |
| -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`                    | Adds `ShopifyFunctionAppRecord` and threads optional owner app metadata through `ShopifyFunctionRecord`.                                           |
| `gleam/src/shopify_draft_proxy/proxy/functions.gleam`                | Projects seeded Function owner app fields, preserves app metadata when reusing known Functions, and records staged Functions mutation-log entries. |
| `gleam/test/parity/runner.gleam`                                     | Adds capture seeding for `functions-owner-metadata-local-staging` from `seedShopifyFunctions`.                                                     |
| `gleam/test/parity_test.gleam`                                       | Enables the owner metadata parity scenario.                                                                                                        |
| `gleam/test/shopify_draft_proxy/proxy/functions_mutation_test.gleam` | Adds a direct mutation-log assertion for Functions mutations.                                                                                      |

Validation: `gleam test --target javascript` is green at 643 tests.
`gleam test --target erlang` is green at 636 tests via the
`erlang:27` container fallback because the host lacks `escript`. The
targeted touched-file `gleam format --check ...` invocation is green.
The TypeScript `gleam-interop` Vitest smoke is green.

### Findings

- Function owner metadata belongs on `ShopifyFunctionRecord`, not on
  each validation/cart-transform record. Reusing a known Function now
  preserves the app owner payload for all downstream projections.
- The owner metadata fixture expects the local-runtime seed phase to
  have advanced the synthetic counters once before the primary request.
  Keeping that fixture-specific behavior in the parity runner avoids
  mutating checked-in specs or captures.
- Functions mutations were missing the same staged mutation-log
  observability that the TS route adds after supported local handling.

### Risks / open items

- `functions-metadata-local-staging` remains intentionally disabled
  because its fixture was previously verified as divergent from both
  TS and Gleam output.
- The host workspace still lacks local Erlang tooling (`escript`), so
  BEAM validation depends on the `erlang:27` container fallback.

### Pass 29 candidates

- Continue bulk-operations beyond the empty-read stub by porting the
  state slice and real catalog reads.
- Continue marketing beyond the empty-read stub by porting the activity
  and engagement state slices.
- Revisit the divergent `functions-metadata-local-staging` fixture
  only if the capture is regenerated or its comparison contract is
  corrected.

---

## 2026-04-29 — Pass 27: gift-card search parity seeding

Promotes the `gift-card-search-filters` parity spec from a documented
runner gap into executable Gleam parity coverage. The spec's primary
request is a lifecycle setup mutation (`giftCardUpdate` +
`giftCardCredit` + `giftCardDebit`) against a captured real gift card,
so the runner now seeds the proxy's base gift-card state from the
capture before driving that setup request. The comparison targets then
exercise the same staged local read-after-write state as the TS parity
harness.

This pass also fills the missing gift-card search predicates needed by
the captured filter scenario: `created_at`, `expires_on`,
`initial_value`, `customer_id`, `recipient_id`, and `source`.
`updated_at` intentionally remains ignored, matching the TS handler
and the captured Shopify behavior for the scenario's future-date
query.

| Module                                                 | Change                                                                                                                                       |
| ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/test/parity/runner.gleam`                       | Adds scenario precondition seeding for `gift-card-search-filters`, including capture-to-`GiftCardRecord` and configuration decoding helpers. |
| `gleam/src/shopify_draft_proxy/state/types.gleam`      | Adds `GiftCardRecord.source` so the local model can evaluate `source:` search terms.                                                         |
| `gleam/src/shopify_draft_proxy/state/store.gleam`      | Adds base upsert helpers for gift cards and gift-card configuration, mirroring the TS parity harness precondition path.                      |
| `gleam/src/shopify_draft_proxy/proxy/gift_cards.gleam` | Ports the remaining captured search predicates and sets locally-created cards to `source: "api_client"`.                                     |
| `gleam/test/parity_test.gleam`                         | Enables `gift-card-search-filters` as a first-class parity test.                                                                             |

Validation: `gleam test --target javascript` is green at 641 tests.
`gleam test --target erlang` is green at 634 tests via the `erlang:27`
container fallback because the host lacks `escript`. The targeted
touched-file `gleam format --check ...` invocation is green. The
TypeScript `gleam-interop` Vitest smoke is green.

### Findings

- The parity runner can stay spec-compatible without mutating
  `config/parity-specs/**`: seed decisions live in runner code, keyed
  by scenario id, and decode only data already present in the capture.
- Gift-card search must preserve TS's permissive unknown-field
  behavior. Some fields, such as `updated_at`, are intentionally not
  interpreted even when the query uses them.

### Risks / open items

- The host workspace still lacks local Erlang tooling (`escript`), so
  BEAM validation currently depends on a container fallback.
- The next parity-seeding candidates are the existing function
  metadata scenarios that need pre-installed Shopify function records
  from capture data.

### Pass 28 candidates

- Add capture seeding for `functions-owner-metadata-local-staging`.
- Continue bulk-operations beyond the empty-read stub by porting the
  state slice and real catalog reads.
- Continue marketing beyond the empty-read stub by porting the activity
  and engagement state slices.

---

## 2026-04-29 — Pass 26: bulk-operations domain (empty-read stub)

Adds a new `BulkOperationsDomain` covering the always-on read shape
for the bulk-operations API. Same Pass 22k pattern: every singular
root returns null, the connection root returns the empty-connection
shape. The dispatcher routes the existing `BulkOperations` capability;
legacy fallback recognises the three query roots by name.

The full TS module (`src/proxy/bulk-operations.ts`, ~1462 LOC) covers
the run-query / run-mutation / cancel lifecycle, the stored bulk-
operation overlay (with status transitions, JSONL import-log replay
for `objects` substitution, and polling-friendly id-vs-window
validation), and connection pagination with
`createdAt`/`completedAt`/`status:` query filters. None of that ships
in this pass; the next bulk-operations pass will port the state slice
(`BulkOperationRecord`, the active/historical id pair, the
`BulkOperationImportLogEntry` shape) and start filling in real reads.

| Module                                                            | Change                                                                                                                                                                                                                                          |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/bulk_operations.gleam`       | New module (~80 LOC). Public surface: `is_bulk_operations_query_root`, `handle_bulk_operations_query`, `wrap_data`, `process`, `BulkOperationsError`.                                                                                           |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`           | New `BulkOperationsDomain` variant. `BulkOperations → BulkOperationsDomain` capability mapping (queries only). Legacy fallback adds the bulk-operations predicate after marketing. Query dispatcher arm calls `bulk_operations.process(query)`. |
| `gleam/test/shopify_draft_proxy/proxy/bulk_operations_test.gleam` | +5 tests: predicate, two singular nulls (`bulkOperation`, `currentBulkOperation`), two empty connections (`bulkOperations` with both `nodes`/`pageInfo` and `edges` selection sets), envelope wrapping.                                         |

Both `--target erlang` and `--target javascript` are green at 629
passing tests (the headline counter sweeps every test, including
parity-runner cases that now exercise the new domain).

### What still doesn't move

- **State slice.** No `BulkOperationRecord` yet, and none of the
  store helpers (`getEffectiveBulkOperationById`,
  `listEffectiveBulkOperations`, `stageBulkOperation`,
  `setActiveBulkOperationId`, etc.).
- **Mutations.** `bulkOperationRunQuery`, `bulkOperationRunMutation`,
  `bulkOperationCancel` remain unrouted.
- **Connection filters.** `bulkOperations(query:)` parses a small
  grammar (`createdAt:>=...`, `status:COMPLETED`) that the empty-
  connection serializer doesn't need but the real read path will.
- **Import-log replay.** TS reads a JSONL fixture to substitute
  `objects` payloads; the state slice port will need to decide
  whether this stays an upstream concern or moves into the local
  store overlay.

---

## 2026-04-29 — Pass 25: marketing domain (empty-read stub)

Adds a new `MarketingDomain` covering the always-on read shape for
marketing activities and events. Same Pass 22k pattern: every singular
root returns null, every connection root returns the empty-connection
shape. The dispatcher routes the existing `Marketing` capability;
legacy fallback recognises the four query roots by name.

The full TS module (`src/proxy/marketing.ts`, ~1285 LOC) covers
marketing-activity lifecycle (8 mutation roots — create/update/
external/upsert/delete/deleteExternal/deleteAllExternal plus
marketingEngagementCreate and marketingEngagementsDelete), channel-
handle inspection, query-grammar filters, and connection pagination
by tactic/status. None of that ships in this pass; the next marketing
pass will port the state slice (`MarketingRecord`,
`MarketingEngagementRecord`, `EffectiveMarketingActivityRecord`) and
start filling in real reads.

| Module                                                      | Change                                                                                                                                                                                                                            |
| ----------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/marketing.gleam`       | New module (~85 LOC). Public surface: `is_marketing_query_root`, `handle_marketing_query`, `wrap_data`, `process`, `MarketingError`.                                                                                              |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`     | New `MarketingDomain` variant. `Marketing → MarketingDomain` capability mapping (queries only). Legacy fallback adds the marketing predicate after metaobject-definitions. Query dispatcher arm calls `marketing.process(query)`. |
| `gleam/test/shopify_draft_proxy/proxy/marketing_test.gleam` | +6 tests: predicate, two singular nulls (`marketingActivity`, `marketingEvent`), two empty connections (`marketingActivities`, `marketingEvents`), envelope wrapping.                                                             |

Test count: 584 → 590. Both `--target erlang` and `--target javascript`
are green.

### Drive-by

A `MetaCommit -> dispatch_meta_commit_sync(proxy, request)` arm in
`process_request` was referencing an undefined function (left over
from in-progress commit-dispatch work that brought in `gleam/fetch`,
`gleam/httpc`, and `gleam/javascript/promise`). Added a one-line stub
that delegates to the existing `commit_not_implemented_response()` so
the build is unblocked; the in-progress commit work can replace the
body when it's ready without changing the call site.

### What still doesn't move

- **State slice.** No `MarketingRecord` /
  `MarketingEngagementRecord` /
  `EffectiveMarketingActivityRecord` types yet, and none of the 16
  `runtime.store.*` helpers (`getEffectiveMarketingActivityById`,
  `getEffectiveMarketingActivityByRemoteId`,
  `listEffectiveMarketingActivities`,
  `stageMarketingActivity`, etc.).
- **Mutation lifecycle.** All marketing mutations remain unrouted
  until the state slice ports.
- **Query-grammar filters.** `marketingActivities(query: ...)`
  accepts a filter grammar (`tactic:AD AND status:ACTIVE`) parsed by
  the shared search-query parser; the empty-connection serializer
  doesn't need it but the real read path will.

---

## 2026-04-29 — Pass 24: metaobject-definitions domain (empty-read stub)

Adds a new `MetaobjectDefinitionsDomain` covering the always-on read
shape for both metaobjects and metaobject definitions. Mirrors the
Pass 22k pattern: every singular root returns null, every connection
root returns the empty-connection shape (`nodes`/`edges` empty,
`pageInfo` all-false-with-null-cursors). The dispatcher now routes the
existing `Metaobjects` capability and the legacy fallback recognises
the six query roots by name, so unimplemented Admin clients stop
falling through to the upstream proxy.

The full TS module (`src/proxy/metaobject-definitions.ts`, ~2700 LOC)
covers metaobject + definition lifecycle (create/update/upsert/delete
/bulkDelete plus definitionCreate/Update/Delete plus
standardMetaobjectDefinitionEnable), field-level validation,
type-scoped enumeration, handle/type lookups, and connection
pagination with field-value query filters. None of that ships in this
pass — the next metaobjects pass will port the state slice
(`MetaobjectDefinitionRecord`, `MetaobjectRecord`,
`MetaobjectFieldDefinitionRecord`, the validation record, and the
capabilities record) and start filling in real reads.

Mutation routes are intentionally not added: the
`metaobject{Create,Update,Upsert,Delete,BulkDelete}` and
`metaobjectDefinition{Create,Update,Delete}` plus
`standardMetaobjectDefinitionEnable` mutations stay on the existing
`No mutation dispatcher implemented` arm until the store slice lands.

| Module                                                                   | Change                                                                                                                                                                                                                                                                      |
| ------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/metaobject_definitions.gleam`       | New module (~110 LOC). Public surface: `is_metaobject_definitions_query_root`, `handle_metaobject_definitions_query`, `wrap_data`, `process`, `MetaobjectDefinitionsError`.                                                                                                 |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`                  | New `MetaobjectDefinitionsDomain` variant. `Metaobjects → MetaobjectDefinitionsDomain` capability mapping (queries only). Legacy fallback adds the metaobject-definitions predicate after localization. Query dispatcher arm calls `metaobject_definitions.process(query)`. |
| `gleam/test/shopify_draft_proxy/proxy/metaobject_definitions_test.gleam` | +8 tests: predicate, four singular nulls (`metaobject`, `metaobjectByHandle`, `metaobjectDefinition`, `metaobjectDefinitionByType`), two empty connections (`metaobjects`, `metaobjectDefinitions`), envelope wrapping.                                                     |

Test count: 576 → 584. Both `--target erlang` and `--target javascript`
are green.

### What still doesn't move

- **State slice.** No `MetaobjectDefinitionRecord` /
  `MetaobjectRecord` / `MetaobjectFieldDefinitionRecord` /
  `MetaobjectFieldDefinitionValidationRecord` /
  `MetaobjectDefinitionCapabilitiesRecord` types yet, and no store
  helpers (`getEffectiveMetaobjectDefinitionById`,
  `findEffectiveMetaobjectDefinitionByType`,
  `listEffectiveMetaobjects`, etc.). Once those land, the singular
  reads can return real records and the connection roots can
  paginate against staged data.
- **Mutation lifecycle.** All nine metaobject(-definition) mutation
  roots are deferred. They share field-validation primitives
  (`validateMetaobjectField`, capability inspection) that should port
  alongside the state slice in one cohesive follow-up pass.
- **Field-value query filters.** `metaobjects(query: ...)` accepts a
  filter grammar (`fields:title:foo AND ...`) handled by the same
  search-query parser used elsewhere; the empty-connection serializer
  doesn't need it but the real read path will.

---

## 2026-04-29 — Pass 23: localization domain (read + 5 mutation roots)

Adds the localization slice end-to-end: a new
`LocalizationDomain` covering the always-on read surfaces
(`availableLocales`, `shopLocales`, `translatableResource(s)`,
`translatableResourcesByIds`) and all five mutation roots
(`shopLocale{Enable,Update,Disable}` plus
`translations{Register,Remove}`), wired through the registry
dispatcher and the legacy-name fallback.

Without the Products domain there is no real `TranslatableResource`
catalog to enumerate, so two design choices kept the surface useful:

- **Default catalog of eight ISO codes** seeded inline in
  `localization.gleam` (en/fr/de/es/it/pt-BR/ja/zh-CN). The store may
  override the list via `replace_base_available_locales`. Default
  shop locales likewise return `[en, primary, published]` until a
  staged record shadows them.
- **Resource synthesis from staged translations** in
  `find_resource_or_synthesize` — staging a translation makes its
  `resourceId` reachable via `translatableResource` and
  `translatableResourcesByIds`, even though the underlying Product
  isn't in the store. This preserves register→read parity for the
  parts of the API that don't need the full Products domain.

Translation mutations always validate against `find_resource`, which
currently returns `None` for every gid — so any
`translationsRegister`/`translationsRemove` against a real Product id
deterministically returns `RESOURCE_NOT_FOUND`. That matches the TS
contract for unknown resources; the success path will activate
automatically once Products ports.

| Module                                                                  | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`                       | New `LocaleRecord`, `ShopLocaleRecord`, `TranslationRecord` resource types (~50 LOC).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                       | Extends `BaseState`/`StagedState` with `available_locales`, `shop_locales`, `translations` (and matching `deleted_*` markers). 12 helpers: `replace_base_available_locales`, `list_effective_available_locales`, `upsert_base_shop_locales`, `stage_shop_locale`, `disable_shop_locale`, `get_effective_shop_locale`, `list_effective_shop_locales`, `translation_storage_key` (`<resource_id>::<locale>::<market_id?>::<key>`), `stage_translation`, `remove_translation`, `remove_translations_for_locale`, `list_effective_translations`, `has_localization_state`. ~350 LOC. |
| `gleam/src/shopify_draft_proxy/proxy/localization.gleam`                | New module (~1100 LOC). Public surface: `is_localization_query_root`, `is_localization_mutation_root`, `handle_localization_query`, `wrap_data`, `process`, `process_mutation`, `MutationOutcome`. Private `AnyUserError` sum (`TranslationError(field, message, code)` / `ShopLocaleError(field, message)`). `@internal pub` on `TranslatableContent`/`TranslatableResource` so the types stay reachable for the future Products port without unused-constructor warnings.                                                                                                      |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`                 | `LocalizationDomain` added to the `Domain` sum. Capability-based (`Localization → LocalizationDomain`) and legacy-name fallback (the five query roots + five mutation roots) both routed. Query and mutation dispatch arms call `localization.process`/`process_mutation`.                                                                                                                                                                                                                                                                                                       |
| `gleam/test/shopify_draft_proxy/proxy/localization_test.gleam`          | +11 read-path tests: predicates, default availableLocales catalog (all 8 codes), store override, default shopLocales (primary+published), staged-record shadowing, `published: false` filter, `translatableResource` null vs. synthesized, `translatableResourcesByIds` empty + synthesized.                                                                                                                                                                                                                                                                                     |
| `gleam/test/shopify_draft_proxy/proxy/localization_mutation_test.gleam` | +11 mutation-path tests: data envelope, shopLocaleEnable success + invalid-locale userError, shopLocaleUpdate success + unknown-locale userError, shopLocaleDisable success + primary-locale userError, translationsRegister and translationsRemove resource-not-found + blank-input userError chains.                                                                                                                                                                                                                                                                           |

Test count: 554 → 576. Both `gleam test --target erlang` and
`gleam test --target javascript` are green.

### What still doesn't move

- **Real translatable-resource enumeration.** `find_resource` and
  `list_resources` return `None`/`[]` for every input. Once the
  Products domain ports its `ProductRecord` and
  `ProductMetafieldRecord` types, both helpers should derive a
  `TranslatableResource` from the matching record (mirroring the TS
  `findResource` reducer). At that point the synthesize-on-staged-
  translation path can stay as a fallback or be retired.
- **Market-scoped translations.** `marketIds` arguments produce a
  `MARKET_CUSTOM_CONTENT_NOT_ALLOWED` user error pending a real
  Markets domain; the storage key already accommodates an optional
  `market_id` so future support is purely a validation change.
- **Outdated/digest reconciliation.** `translatable_content_digest`
  is stored verbatim and compared on register; the digest-vs-content
  mismatch path that the TS handler exercises is gated on
  `find_resource` returning a real record, so it stays dormant until
  the Products port.

---

## 2026-04-29 — Pass 22l: standardMetafieldDefinitionEnable validation parity

Adds a minimal mutation handler for `standardMetafieldDefinitionEnable`
to the existing metafield-definitions domain, covering the
`findStandardMetafieldDefinitionTemplate` user-error branches. Without
the standard-template catalog seeded, every well-formed request falls
through to the captured `TEMPLATE_NOT_FOUND` branch (`field: null`,
"A standard definition wasn't found...") matching the
`standard-metafield-definition-enable-validation` parity scenario's
single target. The success branch that creates a real metafield
definition is deferred until the catalog ports.

The user-error projection respects the request's `userErrors` selection
set (`field`/`message`/`code`); `createdDefinition` is `SrcNull` so its
sub-selection collapses to `null`.

| Module                                                            | Change                                                                                                                                                                                                                                                                                                        |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/metafield_definitions.gleam` | New `UserError(field, message, code)` type, `MutationOutcome`, `is_metafield_definitions_mutation_root/1`, `process_mutation`, `handle_standard_metafield_definition_enable`, `find_standard_template_user_errors` (3 branches: missing args / id supplied / namespace+key supplied), `user_error_to_source`. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`           | Mutation dispatcher gains a `MetafieldDefinitionsDomain` case (capability-based via `Metafields → MetafieldDefinitionsDomain` and legacy fallback by mutation root name).                                                                                                                                     |
| `gleam/test/parity_test.gleam`                                    | +`standard_metafield_definition_enable_validation_test`.                                                                                                                                                                                                                                                      |

Test count: 527 → 528. Both targets green.

### What still doesn't move

- Standard-template catalog: success-path projection of
  `createdDefinition` (id/namespace/key/ownerType/name/type) needs the
  `STANDARD_METAFIELD_DEFINITION_TEMPLATES` table ported. Once seeded,
  the id-supplied branch can also distinguish "id not in catalog" from
  "id not in catalog for ownerType".
- The four lifecycle mutations
  (`metafieldDefinition{Create,Update,Delete,Pin,Unpin}`) — deferred
  until parity scenarios exercise them.

---

## 2026-04-29 — Pass 22k: minimal metafield-definitions domain (empty-read parity)

Adds a new `MetafieldDefinitionsDomain` with the lightest possible
serializer — `metafieldDefinition` → null, `metafieldDefinitions` →
empty connection — modeled on the `events.gleam` pattern. Enables the
`metafield-definitions-product-empty-read` parity scenario, whose
checked targets are `$.data.missing` (null) and `$.data.empty` (empty
connection). The other roots in the document (`byIdentifier`,
`metafieldDefinitions`, `filteredByQuery`, `seedCatalog`) serialize to
null/empty too; they're not compared by the spec.

The TS module (`src/proxy/metafield-definitions.ts`, ~1550 LOC) covers
definition lifecycle, validation, capability inspection, and seeded
catalog reads. None of that is needed for the empty-read scenario —
that's deferred until a parity spec actually exercises it.

| Module                                                            | Change                                                                                                                                                              |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/metafield_definitions.gleam` | New file (~85 LOC). Selection-driven serializer using `serialize_empty_connection` for `metafieldDefinitions` and `json.null()` for everything else.                |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`           | New `MetafieldDefinitionsDomain` enum value with both capability-based dispatch (`Metafields → MetafieldDefinitionsDomain`) and legacy-fallback by root-field name. |
| `gleam/test/parity_test.gleam`                                    | +`metafield_definitions_product_empty_read_test`.                                                                                                                   |

Test count: 526 → 527 (one new parity test exercising 2 targets).
Both targets green.

### What still doesn't move

- Definition lifecycle (create/update/delete/pin/unpin), validation,
  capability inspection, seeded catalog reads — all deferred until a
  parity spec needs them.

---

## 2026-04-29 — Pass 22j: customerSegmentMembersQuery / customerSegmentMembers / customerSegmentMembership read roots

Closes Pass 22i's "what still doesn't move" list. Stages the
`CustomerSegmentMembersQueryRecord` at create time (with `done: true,
currentCount: 0` since the proxy has no customer-store integration
to evaluate membership) and adds the three downstream read roots so
`customer-segment-members-query-lifecycle` parity passes
end-to-end (4 targets: create-empty-numeric, lookup-created,
members-by-query-id-empty, membership-unknowns).

The members connection always returns an empty page — without a
`CustomerRecord` store slice the proxy has no candidates to filter,
which is exactly the captured branch the spec exercises. The
membership root filters by segment existence (unknown segments are
dropped, matching TS `flatMap` over `getEffectiveSegmentById`); the
captured scenario uses unknown segment ids → empty array.

| Module                                                     | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`          | New `CustomerSegmentMembersQueryRecord(id, query, segment_id, current_count, done)`.                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `gleam/src/shopify_draft_proxy/state/store.gleam`          | `BaseState`/`StagedState` gain `customer_segment_members_queries` + `customer_segment_members_query_order` slices. New `stage_customer_segment_members_query` + `get_effective_customer_segment_members_query_by_id`.                                                                                                                                                                                                                                                                                                      |
| `gleam/src/shopify_draft_proxy/proxy/segments.gleam`       | `is_segment_query_root/1` now matches `customerSegmentMembers` / `customerSegmentMembersQuery` / `customerSegmentMembership`. Dispatcher routes each to a new serializer. `handle_customer_segment_members_query_create` now stages the record before returning the create-shape response. New helpers: `serialize_customer_segment_members_query`, `serialize_customer_segment_members_connection`, `serialize_customer_segment_membership`, plus selection-driven projections for statistics, pageInfo, and memberships. |
| `gleam/test/shopify_draft_proxy/proxy/segments_test.gleam` | `is_segment_query_root_test` flipped to expect the three new roots.                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `gleam/test/parity_test.gleam`                             | +`customer_segment_members_query_lifecycle_test`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |

Test count: 525 → 526 (one new parity test exercising 4 targets).
Both targets green.

### What still doesn't move

- Customer staging + membership evaluator: with no `CustomerRecord`
  store slice, members connections always return totalCount=0. The
  TS runtime test for tagged-customer pagination
  (`tests/integration/customer-segment-member-flow.test.ts`) exercises
  these branches; the parity spec deliberately limits its checked
  scenario to the empty branch (per the spec's `notes` field), so this
  is not a parity gap — it's a future port pass when the customer
  domain lands.
- The `customerSegmentMembers` connection currently ignores the
  `error: 'this async query cannot be found in segmentMembers'` branch
  (resolved.missing_query_id) since the parity scenario doesn't
  exercise it. Easy follow-up if a captured spec needs it.

---

## 2026-04-29 — Pass 22i: port `customerSegmentMembersQueryCreate` mutation

Closes the Pass 22f-documented segments gap. Adds the mutation
dispatcher case + handler so `segment-query-grammar-not-contains`
parity passes end-to-end (4 targets: segmentCreate, segment read,
member-query-create, segmentDelete).

The Gleam port deliberately scopes smaller than the TS handler
(`src/proxy/segments.ts:996`): we don't yet stage the
`CustomerSegmentMembersQueryRecord` into the store, evaluate
membership against `listEffectiveCustomers`, or implement the
member lookup queries. With an empty store, members.length is
always 0 and the response shape matches Shopify's freshly-queued
state (`currentCount: 0`, `done: false`) regardless. That covers
the not-contains parity scenario; the
`customer-segment-members-query-lifecycle` scenario (which exercises
downstream `customerSegmentMembersQuery` lookup) still needs the
store staging + member evaluator + `customerSegmentMembers` query
ported.

| Module                                                     | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/segments.gleam`       | `is_segment_mutation_root/1` now matches `customerSegmentMembersQueryCreate`. New private types `CustomerSegmentMembersQueryPayload` + `CustomerSegmentMembersQueryResponse`. New `handle_customer_segment_members_query_create` reads `input.query` / `input.segmentId`, falls back to `segment.query` when only `segmentId` is provided (matching TS line 1006-1007), validates via new `validate_customer_segment_members_query` + `validate_member_query_string` (member-query-mode error format — no `Query ` prefix), mints a synthetic `CustomerSegmentMembersQuery` GID on success, and projects the standard mutation payload (`customerSegmentMembersQuery`/`userErrors`) with `currentCount: 0`/`done: false` / `userErrors: []`. |
| `gleam/test/parity_test.gleam`                             | NOTE replaced with `segment_query_grammar_not_contains_test`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `gleam/test/shopify_draft_proxy/proxy/segments_test.gleam` | `is_segment_mutation_root_test` flipped to assert `customerSegmentMembersQueryCreate` IS now a mutation root.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |

Test count: 524 → 525 (one new parity test exercising the full
not-contains lifecycle). Both targets green.

### What still doesn't move

- `customer-segment-members-query-lifecycle` (downstream lookup +
  member counts) — needs store staging, the `customerSegmentMembersQuery`
  read root, and the customer membership evaluator.
- Empty-store assumption: any future test that seeds customers via
  Pass 22b snapshot seeding and exercises a real-membership member
  query will need the membership evaluator added then.

---

## 2026-04-29 — Pass 22h: runner gains `fromCapturePath` + webhook conformance parity

Enabling `webhook-subscription-conformance` exposed the next runner
gap from Pass 22a. Five of seven targets passed immediately
(create payload, detail-after-create, delete payload,
detail-after-delete, validation-branches), but webhook-update-payload
and webhook-detail-after-update each had three mismatches —
`callbackUrl`, `metafieldNamespaces`, and `includeFields` all retained
the post-create values instead of applying the update input.

Root cause: the spec's update target uses
`{"webhookSubscription": {"fromCapturePath":
"$.lifecycle.update.variables.webhookSubscription"}}` to reuse the
captured input dict. The runner's `substitute/2` only recognised
`fromPrimaryProxyPath` markers; `fromCapturePath` markers passed
through as literal `{fromCapturePath: ...}` objects. The proxy's
update handler then read no recognisable input fields, took every
"input absent → keep existing" branch, and the response carried
the create record forward. No bug in webhooks.gleam — purely a
runner capability gap.

Adds `as_capture_ref/1`, threads the capture JsonValue through
`substitute/3`, and surfaces `CaptureRefUnresolved(path)` errors
parallel to the existing `PrimaryRefUnresolved`. The runner now
substitutes both ref kinds during inline-template variable
resolution.

| Module                           | Change                                                                                                                                                                                                                  |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/test/parity/runner.gleam` | `RunError` gains `CaptureRefUnresolved(path)`; `substitute/2` becomes `substitute/3` taking the capture JsonValue; new `as_capture_ref/1` recognises `{"fromCapturePath": "..."}` markers parallel to `as_primary_ref`. |
| `gleam/test/parity_test.gleam`   | +`webhook_subscription_conformance_test`.                                                                                                                                                                               |

Test count: 523 → 524 (one new parity test exercising 7 conformance
targets across create / detail / update / delete / validation
branches). Both targets green.

### What this unlocks

Any captured-vs-proxy parity spec that uses `fromCapturePath` for
inline variable substitution can now run. Spot-check of
`config/parity-specs/` shows this pattern appearing in several other
captured specs (gift-card-lifecycle, app-billing-access-local-staging,
discount-code-basic-lifecycle), each blocked on additional domain-port
gaps but no longer blocked on the runner.

Plus: a real-world parity test covering webhook lifecycle
end-to-end (create → detail-read → update → detail-read → delete →
detail-read-null → required-arg validation), proving the existing
webhooks port is conformance-correct against the live capture.

---

## 2026-04-29 — Pass 22f/g: parity-test sweep against ported domains

After Pass 22e, walked the remaining captured parity specs against
the already-ported Gleam domains looking for cheap wins. Two
substantive port gaps surfaced; one zero-port parity scenario landed.

### Landed: event-empty-read

`gleam/src/shopify_draft_proxy/proxy/events.gleam` already mirrors the
TS handler (read-only Events surface — `event` → null, `events` → empty
connection, `eventsCount` → exact zero). Wiring is in place via
`draft_proxy.gleam` `EventsDomain`. Adding the parity test was a
single `check(...)` line — passes on first run.

| Module                         | Change                                                                                  |
| ------------------------------ | --------------------------------------------------------------------------------------- |
| `gleam/test/parity_test.gleam` | +`event_empty_read_test`. Also documented the segments and metafields gaps (see below). |

Test count: 522 → 523. Both targets green.

### Documented: segment query-grammar (not-contains)

`config/parity-specs/segments/segment-query-grammar-not-contains.json`
exercises four ops — `segmentCreate`, `segment` (read), `customerSegmentMembersQueryCreate`, `segmentDelete`. The first, second, and fourth are
already dispatched by the Gleam segments port; the third is not — there
is no `customerSegmentMembersQueryCreate` case in
`gleam/src/.../proxy/segments.gleam`'s mutation dispatcher and no
backing helpers (`stage_customer_segment_members_query`,
`validate_customer_segment_members_query`,
`list_customer_segment_members_for_query`,
`projectMutationPayload`-style serialiser, the
`CustomerSegmentMembersQueryRecord` type). Tracked as a
follow-up port; left a NOTE in `parity_test.gleam` rather than a
red test.

### Documented: metafield-definitions empty read

`config/parity-specs/metafields/metafield-definitions-product-empty-read.json`
reads `metafieldDefinition`/`metafieldDefinitions`. The Gleam
`proxy/metafields.gleam` is currently _only_ a helper module —
serialises individual metafields nested under parent records. There
is no top-level `MetafieldDefinitions` query domain dispatcher. The
TS port has root-field handlers and a definitions store; porting that
surface is a multi-pass effort. Documented as a NOTE.

### What this unlocks / what doesn't move

Adds one captured-vs-proxy comparison covering the read-only Events
surface end-to-end. The two NOTEs replace previously-undocumented
gaps with explicit pointers to where the missing surfaces live in
the TS port. No production code changed in this pass — it's pure
parity surfacing + survey.

---

## 2026-04-29 — Pass 22e: saved-search defaults + query-grammar / resource-roots parity

Folds the Pass 22d parser into the static default saved searches and
unlocks two more parity scenarios that didn't need any further code
changes — the parser already handled them.

### Static defaults now derive filters via the parser

`makeDefaultSavedSearch` in TS spreads
`parseSavedSearchQuery(savedSearch.query)` into each default record.
The Gleam port's `defaults_for_resource_type/1` returned the static
records as-is with `filters: []` / `search_terms: ""` (load-bearing
TODO from the original saved-searches port). Now wraps the static
list through `derive_default_saved_search_query_parts/1`, which
calls `parse_saved_search_query/1` and rebuilds the record with the
derived `query` (canonical), `search_terms`, and `filters` fields.

| Module                                                           | Change                                                                                                                                                                                                                                                                                |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/saved_searches.gleam`       | `defaults_for_resource_type/1` now maps each static record through `derive_default_saved_search_query_parts/1`. New helper applies the parser and returns a `SavedSearchRecord(..record, query: parsed.canonical_query, search_terms: parsed.search_terms, filters: parsed.filters)`. |
| `gleam/test/parity_test.gleam`                                   | +`saved_search_query_grammar_test`, +`saved_search_resource_roots_test`. Both pass with no further code changes — the Pass 22d parser handles the OR/grouped/quote-normalization/negated-filter case correctly.                                                                       |
| `gleam/test/shopify_draft_proxy/proxy/saved_searches_test.gleam` | `order_saved_searches_full_node_shape_test` updated to expect parsed `filters: [{key:"status",value:"open"}, {key:"fulfillment_status",value:"unshipped,partial"}]` instead of the prior empty list.                                                                                  |

Test count: 520 → 522 (two new parity tests, no new unit failures).
Both targets green.

### What this unlocks

Three saved-search parity scenarios are now parity-clean:

- `saved-search-local-staging` (Pass 22d landed).
- `saved-search-query-grammar` — the OR-with-grouped-AND-with-negated-filter case from HAR-458.
- `saved-search-resource-roots` — read-after-delete read against the static defaults; failed before because the static records had `filters: []`.

The deeper saved-search work (per-resource filtering with parsed
query against staged + base records, `hydrateSavedSearchesFromUpstreamResponse`)
remains untouched; no parity scenario in `config/parity-specs/`
currently exercises it.

---

## 2026-04-29 — Pass 22d: saved-search query parsing (filters / searchTerms / canonical query)

Closes the saved-search-local-staging parity gap surfaced in Pass 22a.
The Pass 8 saved-searches port stored the raw `input.query` string
verbatim — `searchTerms` got the whole query and `filters[]` was
empty, so a create with `query: "title:Codex 1777309108817"` would
round-trip with `searchTerms: "title:Codex 1777309108817"` /
`filters: []` instead of live Shopify's `searchTerms: "1777309108817"` /
`filters: [{key:"title", value:"Codex"}]` / canonical
`query: "1777309108817 title:Codex"`.

Wires `parse_saved_search_query` into `make_saved_search`, ported
from `parseSavedSearchQuery` in `src/proxy/saved-searches.ts`. The
generic-purpose `search_query_parser.gleam` module was already
ported (parse_search_query_term / strip_search_query_value_quotes /
search_query_term_value etc.); only the saved-search-domain glue
needed adding.

| Module                                                        | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/saved_searches.gleam`    | +`ParsedSavedSearchQuery` ADT + `parse_saved_search_query/1` (public). Private helpers `split_saved_search_top_level_tokens` (depth-aware paren/quote tokenizer over `string.to_graphemes`), `is_grouped_token`, `is_boolean_token`, `is_filter_candidate`, `filter_value_for_term`, `render_saved_search_filter` (handles `_not` suffix unwinding for `-key:value`), `normalize_saved_search_term` + `escape_saved_search_term_for_stored_query` + `normalize_saved_search_quoted_values` (all mirror their TS counterparts byte-for-byte). `make_saved_search` now passes the result through to `query` / `search_terms` / `filters` instead of `query: raw, search_terms: raw, filters: []`. |
| `gleam/test/parity_test.gleam`                                | +`saved_search_local_staging_test` (was a 9-line NOTE explaining the gap).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `gleam/test/shopify_draft_proxy/proxy/draft_proxy_test.gleam` | `meta_state_reflects_staged_saved_search_test` updated to expect `searchTerms: ""` + `filters: [{key:"tag", value:"promo"}]` for input `query: "tag:promo"` (the prior assertion was the broken pre-port shape).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |

Test count: 519 → 520 (saved-search local-staging parity test now
green). Both targets green.

### What still doesn't move

Other saved-search parity scenarios (`saved-search-query-grammar`,
`saved-search-resource-roots`) still need: per-resource-root
filtering of staged + base records by parsed query (currently the
Gleam port uses naive `matches_query` substring matching), and
`hydrateSavedSearchesFromUpstreamResponse`. Those gaps are real port
work, not blocked by this pass. Tracked separately under Pass 22d
follow-ups.

---

## 2026-04-29 — Pass 22c: webhook validation `locations` + functions fixture investigation

Tightens the webhook required-argument validator so its error envelope
matches live Shopify, and resolves the parity gaps Pass 22a surfaced
in `webhooks/` and `functions/`. The webhook fix is a real port gap;
the functions gap turned out to be a fixture-correctness issue, not a
port bug.

### Webhook validation: `locations: [{line, column}]`

Live Shopify's `errors[]` envelope for `missingRequiredArguments` and
`argumentLiteralsIncompatible` carries a `locations` array between
`message` and `path`, pointing at the offending field token in the
source body. The Gleam port's `mutation_helpers.build_*_error`
builders were emitting the structured `extensions` and `path` fields
but had no `locations`, so the parity diff was loud.

Fix threads the source `document` and field AST `Location` down into
the error builders:

| Module                                                             | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/mutation_helpers.gleam`       | +`field_loc: Option(Location)` and +`source_body: String` parameters on `validate_required_field_arguments`, `validate_required_id_argument`, `build_missing_required_argument_error`, `build_null_argument_error`. New private helpers `field_location/1` (extracts the AST loc once per field) and `locations_payload/2` (uses `graphql/location.get_location` to convert the start offset into `{line, column}` and renders the JSON shape). When `field_loc` is `None`, no `locations` key is emitted — keeps the no-source-body error path stable. |
| `gleam/src/shopify_draft_proxy/proxy/webhooks.gleam`               | Three callsites (`webhookSubscriptionCreate`, `webhookSubscriptionUpdate`, `webhookSubscriptionDelete`) now pass the parsed `document` string through.                                                                                                                                                                                                                                                                                                                                                                                                  |
| `gleam/test/shopify_draft_proxy/proxy/mutation_helpers_test.gleam` | Rewritten — every validator test threads a `document` through, and a new `build_missing_required_argument_error_with_location_test` asserts the full envelope shape including `locations:[{line:2,column:3}]` for a multi-line document.                                                                                                                                                                                                                                                                                                                |
| `gleam/test/shopify_draft_proxy/proxy/webhooks_test.gleam`         | Existing `_top_level_error_test`s updated to expect `locations:[{line:1,column:12}]` (column 12 = the start of `webhookSubscription{Create,Update,Delete}` in `mutation { …`).                                                                                                                                                                                                                                                                                                                                                                          |
| `gleam/test/shopify_draft_proxy/proxy/draft_proxy_test.gleam`      | `graphql_webhook_subscription_create_missing_topic_top_level_error_test` updated similarly.                                                                                                                                                                                                                                                                                                                                                                                                                                                             |

Result: `webhook-subscription-required-argument-validation.json`
parity test now green; the spec's previously-disabled "TEMP DEBUG"
note is gone. Test count: 517 → 519 (one new with-location shape
test, plus webhook parity test now passes through). Both targets
green.

### Functions metadata: fixture is divergent, not the port

`functions-metadata-local-staging.json` claimed the proxy should emit
`MutationLogEntry/2` at `T+1s`. The Gleam port emits
`MutationLogEntry/1` at `T+0s`. Suspected port gap initially, but
running the **TS port** directly against the same primary variables
(via a temporary `tests/integration/debug-functions.test.ts`)
produced `MutationLogEntry/1 + T+0s` — identical to Gleam. Both ports
match each other; the capture fixture diverges from BOTH.

The capture (`fixtures/.../functions-metadata-flow.json`) is
hand-written and aspirational. Either the fixture needs to be
regenerated against the real proxy, or the spec needs
`expectedDifferences` rules tagging
`shopify-gid:Validation`/`MutationLogEntry` ids and `iso-timestamp`
matchers. Tracked as a fixture-correctness follow-up — `parity_test`
now carries a sharp NOTE comment explaining this finding so future
passes don't re-investigate. Debug integration test was deleted.

`functions-owner-metadata-local-staging` remains deferred to Pass 22b
seeding (the capture starts from a store with pre-installed Function
records carrying `appKey`/`description`/`app` metadata that the proxy
has no way to know about without snapshot seeding).

### Why the runner stays unchanged

Pass 22a's runner machinery was correct — every gap it surfaced was
either a domain-port gap (webhook locations, addressed here) or a
fixture-correctness gap (functions metadata) or needs seeding (Pass
22b). The validator now matches live Shopify shape, so
`webhook-subscription-required-argument-validation.json` rolls into
the green parity column.

---

## 2026-04-29 — Pass 22a: per-target proxyRequest + variable derivation

Extends the Pass 21 runner so it can drive multi-target lifecycle
specs — i.e. specs whose `comparison.targets[*]` each fire their own
proxy request, optionally with `variables` derived from the _primary_
proxy response via `{"fromPrimaryProxyPath": "$..."}` markers. This
unblocks the lifecycle-shaped scenarios in `apps/`, `functions/`,
`saved-searches/`, `webhooks/` etc. where the spec creates an entity,
then reads/updates/deletes it by the id the proxy just allocated.

### Module changes

| Module                           | Change                                                                                                    | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| -------------------------------- | --------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/test/parity/spec.gleam`   | +`TargetRequest` ADT, +`VariablesInline`, +`rules_for/2` helper                                           | `Target` now carries `request: TargetRequest` (`ReusePrimary` \| `OverrideRequest(ProxyRequest)`). `ParityVariables` gains `VariablesInline(template: JsonValue)` for inline literal/templated variables blocks. The decoder switch is `decode.optional_field("proxyRequest", ReusePrimary, decode.map(proxy_request_decoder(), OverrideRequest))`.                                                                                           |
| `gleam/test/parity/runner.gleam` | +state-threading via `list.try_fold`, +`substitute/2`, +`as_primary_ref/1`, +`PrimaryRefUnresolved` error | `run_targets` now threads `#(DraftProxy, List(TargetReport))` forward so target N+1 sees the records target N created. `actual_response_for` dispatches `ReusePrimary` (no extra HTTP) vs `OverrideRequest` (load doc + resolve variables against the _primary_ proxy response, then execute). `substitute` walks a template `JsonValue`, replacing leaf objects of the shape `{"fromPrimaryProxyPath": "$..."}` with the value at that path. |

Test count: 517 → 517 (machinery verified backwards-compatible — no
new green tests added because every multi-target lifecycle scenario
the runner can now drive surfaced a real domain-port gap, documented
below). Both targets green.

### Parity gaps surfaced (NOT runner bugs)

The runner correctly drove each multi-target lifecycle to completion
with `fromPrimaryProxyPath` substitution and reported the diffs. Each
of these is a Gleam-vs-TS-port domain gap that needs follow-up:

- **saved-search-local-staging** (saved-searches): `SavedSearch.filters[]`
  comes back empty (filter-expr parsing not implemented),
  `SavedSearch.searchTerms` includes the filter expression (should be
  residual term only), and `SavedSearch.query` field-order
  canonicalisation diverges from live Shopify's `<filter> <terms>`
  shape.
- **webhook-subscription-required-argument-validation** (webhooks):
  GraphQL parse/validate error payload missing the `locations` field
  that live Shopify emits.
- **functions-metadata-local-staging /
  functions-owner-metadata-local-staging** (functions): id allocation
  ordering differs by 1 (e.g. `Validation/2` vs `/1`),
  `shopifyFunction.appKey` and `description` metadata not populated
  from the deploy payload.
- **gift-card-search-filters** (gift-cards): runner needs to seed gift
  cards into the proxy store before driving the search request — that
  capability is Pass 22b.

### What landed

- Per-target `proxyRequest` overrides with full state-threading: a
  target's request executes against the proxy mutated by every prior
  target in the same scenario, exactly as the TS engine does it.
- Inline `variables` blocks with `fromPrimaryProxyPath` substitution
  applied recursively: array elements, nested objects, and bare
  leaf-objects all participate. Resolution is JSONPath into the
  _primary_ proxy response (target requests don't see each other's
  responses, only the primary's — matches the TS engine).
- A new `PrimaryRefUnresolved` error variant so a typoed JSONPath in
  an inline-variables block fails loud rather than silently producing
  `null`.

### Risks / non-goals

- Snapshot seeding is still not implemented, so any spec that needs
  pre-existing state in the proxy store (segments-baseline-read, the
  live functions read, gift-card search) remains skipped. Pass 22b.
- The runner doesn't model spec-level fixture overrides (e.g. specs
  that point at a _different_ capture per target). None of the
  ported-domain specs use that today.
- No equivalent of the TS engine's `setSyntheticIdentity` injection —
  the Gleam proxy generates its own ids deterministically per-store
  and the synthetic-gid matcher already filters those mismatches
  where parity is documented.

### Pass 22 candidates

- **Pass 22b — snapshot seeding**: parse the capture's "before" state
  (or a sibling fixture) into proxy-store records before the primary
  request fires. Unblocks segments-baseline-read, live functions
  reads, app billing reads, gift-card-search-filters, and any future
  scenario whose interesting behaviour depends on seeded data.
- **Domain follow-ups** (in priority order, since the runner just
  surfaced the actionable list): functions metadata population +
  id-ordering, saved-search filter parsing + query canonicalisation,
  webhook GraphQL error `locations`.

---

## 2026-04-29 — Pass 21: pure-Gleam parity test runner (MVP)

User-driven detour ahead of the localization port: stand up parity
tests in pure Gleam so we can prove the ported domains actually
process Admin GraphQL requests end to end against captured Shopify
fixtures, without leaning on the TS engine.

The legacy harness (`tests/unit/conformance-parity-scenarios.test.ts`

- `scripts/conformance-parity-lib.ts`) is left in place — it is too
  TS-coupled (it calls `runtime.store` / `handleAppMutation` /
  `handleApps*` directly, not over HTTP) to plug a Gleam target into.
  The Gleam runner replaces it incrementally: same parity-spec JSON
  shape, same captured fixtures, same expected-difference matchers, but
  drives `draft_proxy.process_request` over an HTTP-shaped envelope.
  Capture scripts and the spec library stay TS for now.

### Module table

| Module                               | Lines | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ------------------------------------ | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/test/parity/json_value.gleam` | +144  | Self-describing JSON ADT (`JNull`, `JBool`, `JInt`, `JFloat`, `JString`, `JArray`, `JObject`) plus a recursive `from_dynamic` that round-trips `gleam/json`'s output, a deterministic `to_string` for diff-message rendering, and `field`/`index` helpers used by the JSONPath walker.                                                                                                                                                                                                                              |
| `gleam/test/parity/jsonpath.gleam`   | +137  | Minimal JSONPath: `$`, `$.foo`, `$.foo.bar`, `$[N]`, `$.foo[N].bar`. No filters, no recursive descent, no wildcards — that's the entire vocabulary the parity specs use today. `lookup/2` parses + evaluates in one shot for the runner's hot path.                                                                                                                                                                                                                                                                 |
| `gleam/test/parity/diff.gleam`       | +271  | Structural JsonValue diff (`Mismatch{path, expected, actual}`) with two `expectedDifferences` rule kinds: `IgnoreDifference{path}` and `MatcherDifference{path, matcher}`. Implements four matchers: `non-empty-string`, `any-string`, `any-number`, `iso-timestamp` (permissive `T…Z`/offset shape check), `shopify-gid:<Type>` (with optional `?shopify-draft-proxy=synthetic` suffix). Anything else is exact-string match. Path tracking matches the JSONPath grammar so rules can be addressed surgically.     |
| `gleam/test/parity/spec.gleam`       | +148  | Decoder for parity-spec JSON: `Spec{scenario_id, capture_file, proxy_request, targets, expected_differences}`, `ProxyRequest{document_path, variables: VariablesFromCapture/VariablesFromFile/NoVariables}`, `Target{name, capture_path, proxy_path, expected_differences}`. The optional `ignore: true` field on a difference flips the rule kind to `IgnoreDifference`. Per-target `proxyRequest` overrides and `fromPrimaryProxyPath` variable derivation are intentionally not modelled yet.                    |
| `gleam/test/parity/runner.gleam`     | +220  | Orchestration: load spec, load capture, read GraphQL document, resolve variables (capture-jsonpath or sibling-file), build `{"query":…,"variables":…}` envelope, drive `draft_proxy.process_request`, parse the response body back into `JsonValue`, compare each target's `capturePath` slice of the capture against the `proxyPath` slice of the response. Returns `Report{scenario_id, targets[*]: TargetReport{name, mismatches}}`. `RunnerConfig{repo_root}` defaults to `..` because tests run from `gleam/`. |
| `gleam/test/parity_test.gleam`       | +73   | Six gleeunit tests, one per supported scenario, plus a runner self-check that confirms `into_assert` actually surfaces non-empty mismatch lists as failures (so the green tests can't be silent no-ops).                                                                                                                                                                                                                                                                                                            |
| `gleam/gleam.toml`                   | +1    | Adds `simplifile = ">= 2.0.0 and < 3.0.0"` as a dev-only dependency. Filesystem reads are needed for the spec/capture/document files which sit outside the gleam project tree. Runtime deps stay at `gleam_stdlib` + `gleam_json`.                                                                                                                                                                                                                                                                                  |

Test count: 511 → 517 (+6 — five parity scenarios + one runner
self-check). Both targets green.

### Scenarios covered

The MVP runner supports specs whose `comparison.targets[*]` reuse the
spec's primary `proxyRequest` (no per-target overrides). That gives
us, across the six ported domains:

| Domain   | Spec                                                | Targets                                                |
| -------- | --------------------------------------------------- | ------------------------------------------------------ |
| segments | `segment-create-invalid-query-validation`           | 1                                                      |
| segments | `segment-update-unknown-id-validation`              | 1                                                      |
| segments | `segment-delete-unknown-id-validation`              | 1                                                      |
| webhooks | `webhook-subscription-catalog-read`                 | 1                                                      |
| apps     | `delegate-access-token-current-input-local-staging` | 1 (uses `iso-timestamp` + `non-empty-string` matchers) |

`functions/functions-live-owner-metadata-read` was wired up but
removed from the suite when the run surfaced a real seeding gap: the
proxy returns `null` for `cartFunction`/`validationFunction` because
the empty store has no Function records, while the capture was taken
against a store with conformance Functions deployed. That's not a
runner bug — it's the absence of snapshot-seeding. Scenarios that
need pre-seeded state (`segments-baseline-read`, the live functions
read) are deferred until the runner gains seeding support.

### What landed

- A working JSON ↔ JsonValue round trip on both targets, exercised
  end to end by the parity runner. The dynamic-decoder approach
  (`from_dynamic`) handles every shape the parity captures use.
- A small JSONPath subset that's exactly enough for the spec
  vocabulary. The same syntax is reused inside the diff for
  `expectedDifferences` rules so paths line up byte-for-byte.
- A diff that's both structural (recursive walk, list of mismatches
  with locations) and matcher-aware (`expectedDifferences` rules are
  applied as a post-filter, not embedded in the walk — keeps the diff
  generic).
- Repo-root path resolution that's configurable on the runner so a
  consumer outside `gleam/` (a future top-level wrapper, or CI from
  the repo root) can pass an absolute `repo_root` instead of `..`.
- Coverage of the simpler validation specs across three of the six
  ported domains (segments, webhooks, apps), driving real GraphQL
  requests through `draft_proxy.process_request` and comparing
  against captured Shopify responses. The proxy is byte-for-byte
  parity with the live Shopify capture for every covered scenario.

### Findings

- **`expectedDifferences` is mostly empty.** Of the five passing
  specs, only `delegate-access-token-current-input-local-staging`
  uses it (two rules: synthetic token is `non-empty-string`,
  `createdAt` is `iso-timestamp`). The validation specs have empty
  rule lists — userError parity is exact, including message text,
  field paths, and ordering.
- **The proxy's user-error messages are byte-identical** to live
  Shopify for `Name can't be blank`, `Query can't be blank`,
  `Segment does not exist`, and the multi-error `'foo' filter cannot
be found.` shape. This is a non-trivial parity result — the
  segments port (Pass 20) caught the right error format on the first
  try, with no rework against the captured fixtures.
- **GraphQL parse errors round-trip cleanly.** The webhook
  `webhook-subscription-catalog-read` spec issues a multi-root
  query (`webhookSubscription` + `webhookSubscriptions` +
  `webhookSubscriptionsCount`) and the proxy's parsed-operation
  dispatcher handles all three under one document.
- **The functions seeding gap is a generic gap, not domain-specific.**
  Every "live read" scenario for a domain assumes the proxy was
  pre-seeded from the capture's evidence block. The TS parity engine
  does this implicitly via `runtime.store.upsert*` calls before the
  request executes; a Gleam analog needs a deterministic
  spec-driven seeding step (probably reading
  `liveCaptureFiles[].evidence` or a sibling `seed.json`).
- **No filesystem-related portability issues.** `simplifile` works
  identically on both targets for the file reads we do. No FFI
  needed.

### Risks / open items

- Per-target `proxyRequest` overrides + `fromPrimaryProxyPath`
  variable derivation are unimplemented. ~14 specs across all six
  ported domains use this pattern (multi-step lifecycle scenarios
  like `gift-card-lifecycle`, `segment-query-grammar-not-contains`,
  `saved-search-local-staging`). These are the "real" parity tests;
  the validation specs we cover today are the cheap ones.
- Snapshot seeding from captures isn't implemented. Without it, any
  read-against-existing-state scenario fails (functions live read,
  segments baseline read, app billing reads, gift-card searches).
- ISO-timestamp matcher is a shape check, not a strict format check.
  Permissive enough for the parity surface but it would accept
  `2024-99-99T99:99:99Z` — we trade strictness for not pulling in a
  date library.
- The runner's `RunError` rendering in `panic as` panics with the
  message but discards the structured value, so failures are visible
  in test output but not introspectable. Adequate for gleeunit.
- The legacy vitest file still runs the same scenarios. Keeping both
  in CI is fine as a cross-check during the porting period; the user
  asked for an "eventual" cutover, not an immediate one.

### Pass 22 candidates

1. **Per-target `proxyRequest` overrides** — adds the second-step
   request shape: each target can specify its own document path,
   variables (`variablesCapturePath` / `variablesPath` / inline
   `variables`), and `fromPrimaryProxyPath` derivation that pulls a
   value from the primary response into the next request's variables.
   Unlocks lifecycle scenarios across all six domains. Largest single
   win for parity coverage.
2. **Snapshot seeding** — add a pre-execute hook that reads a seed
   block from the spec (or a referenced JSON file) and stages it into
   the proxy's store before the request runs. Unlocks every "live
   read" parity scenario.
3. **Localization domain port** (originally Pass 21). Independent of
   the parity work; reads/mutates are scoped to translatable
   resources and don't require any new runner features.

---

## 2026-04-29 — Pass 20: segments domain (segment reads + segmentCreate/Update/Delete with hand-coded query validator)

Ports the "owned" slice of `src/proxy/segments.ts` to a new
`proxy/segments.gleam`. Lands the three query roots (`segment`,
`segments`, `segmentsCount`) and the three core mutations
(`segmentCreate` / `segmentUpdate` / `segmentDelete`).

Customer-segment-membership surfaces (`customerSegmentMembers`,
`customerSegmentMembersQuery`, `customerSegmentMembership`,
`customerSegmentMembersQueryCreate`) and upstream-hybrid surfaces
(`segmentFilters`, `segmentFilterSuggestions`,
`segmentValueSuggestions`, `segmentMigrations`) are intentionally
deferred — they need a `CustomerRecord` store slice and an
upstream-hybrid plumbing path that haven't ported yet.

Notable: query validation is hand-coded against ~5 string-shape
predicates instead of a regex set, because the project only depends
on `gleam_stdlib` + `gleam_json` (no `gleam_regexp`). Each TS regex
in `validateSegmentQueryString` has a corresponding hand-rolled
matcher.

### Module table

| Module                                                        | Lines | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------------------------------------------------------------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/shopify_draft_proxy/state/types.gleam`                   | +9    | Adds `SegmentRecord` (`id: String`, `name/query/creation_date/last_edit_date: Option(String)`). Every field except `id` is nullable to match the Admin GraphQL schema.                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `src/shopify_draft_proxy/state/store.gleam`                   | +50   | Extends `BaseState` and `StagedState` with `segments: Dict(String, SegmentRecord)`, `segment_order: List(String)`, `deleted_segment_ids: Dict(String, Bool)`. Adds `upsert_staged_segment`, `delete_staged_segment`, `get_effective_segment_by_id`, `list_effective_segments` — modeled exactly on the saved-search slice (dict + order + deletion markers, where deletion markers suppress records in the effective getter).                                                                                                                                                           |
| `src/shopify_draft_proxy/proxy/segments.gleam`                | +1073 | New module. Public surface: `SegmentsError(ParseFailed)`, `is_segment_query_root`, `is_segment_mutation_root`, `handle_segments_query`, `wrap_data`, `process`, `process_mutation`, `MutationOutcome`, `UserError`, `normalize_segment_name`, `resolve_unique_segment_name`, `validate_segment_query`. Five hand-rolled string-shape matchers replace the TS regex set: `parse_supported_segment_query` (number_of_orders comparators + customer_tags CONTAINS), `customer_tags_contains_match`, `email_subscription_status_match`, `customer_tags_equals_match`, `email_equals_match`. |
| `src/shopify_draft_proxy/proxy/draft_proxy.gleam`             | +25   | Wires `SegmentsDomain`: `Ok(SegmentsDomain) -> segments.process(…)` for queries, `segments.process_mutation(…)` for mutations, capability arms `Segments -> Ok(SegmentsDomain)` for both query/mutation, and the legacy fallback `segments.is_segment_query_root(name)` / `segments.is_segment_mutation_root(name)`.                                                                                                                                                                                                                                                                    |
| `test/shopify_draft_proxy/proxy/segments_test.gleam`          | +153  | New file. 10 read-path tests covering the predicates, `segment(id:)` (record / missing / missing-arg / nullable fields), `segments(first:)` connection (empty / seeded), and `segmentsCount`.                                                                                                                                                                                                                                                                                                                                                                                           |
| `test/shopify_draft_proxy/proxy/segments_mutation_test.gleam` | +220  | New file. 17 mutation tests covering all 3 mutation roots (success / blank-name / missing-id / blank-name-on-update / missing-query / invalid-query / customer_tags-equals-operator-error / name-only update preserves query), the `{"data": …}` envelope, the `resolveUniqueSegmentName` " (N)" suffix collision logic (single + double + self-rename-no-collision), and the `is_segment_mutation_root` predicate.                                                                                                                                                                     |

**Test count: 484 → 511** (+27). Both targets clean (Erlang OTP 28 +
JS ESM).

### What landed

`segmentCreate` mints a synthetic gid via
`make_synthetic_gid(identity, "Segment")` — note the unsuffixed form
`gid://shopify/Segment/1`, **not** the
`?shopify-draft-proxy=synthetic` form that `make_proxy_synthetic_gid`
produces for gift cards. Mirrors TS `proxy/segments.ts` which uses
`makeSyntheticGid('Segment')` — segment ids are intended to look like
real upstream ids, not proxy-synthetic ones.

`resolve_unique_segment_name` walks effective segments, gathers used
names, and recurses with `" (N)"` suffix until a free slot is found.
Takes an `Option(String) current_id` so `segmentUpdate` skips its own
record when checking for collisions — preventing self-suffix-bumping
when an update keeps its existing name. Mirrors TS
`resolveUniqueSegmentName` exactly.

`validate_segment_query` runs in `segment-mutation` mode (TS terminology)
— error messages prefix with `Query`. The TS regex set has 5 patterns:
`^number_of_orders\s*(=|>=|<=|>|<)\s*(\d+)$`,
`^customer_tags\s+(NOT\s+)?CONTAINS\s+'([^']+)'$`,
`^email_subscription_status\s*=\s*'[^']+'$`,
`^customer_tags\s*=\s*(.+)$` (operator-error trigger),
`^email\s*=` (filter-not-found trigger). Each became a hand-coded
function using `string.starts_with` / `string.trim_start` /
`string.length` deltas to detect required-whitespace, plus
`is_single_quoted_value` for the `'…'` literal shape and
`is_all_digits` (delegating to `int.parse` rather than character
inspection — string-only `gleam_stdlib` API, no character iteration).
The "canned error" pass for `"not a valid segment query ???"`
returns the exact two-message sequence from the TS handler.

`segmentDelete` produces `deletedSegmentId` as a top-level payload
field, not nested under `segment` (the segment field projects to
`null` on delete). Mirrors TS `SegmentDeletePayload` exactly.

### Findings

- **The dict-with-order + deletion-markers shape is fully formulaic
  now.** Six domains in (saved-search, webhooks, apps, functions,
  gift cards, segments). The store slice fits in ~50 LOC without any
  design decisions left — copy the previous slice, rename, done.
  Future ports of resource-collection domains will likely take less
  time on the store than on the GraphQL projection.
- **No-regex validation is tractable for small, stable predicate
  sets.** Five hand-rolled matchers cost ~80 LOC of straight-line
  prefix/whitespace/digit parsing. The cost was clearly less than
  wiring `gleam_regexp` through the build for one domain. If a
  later pass ever needs ≥10+ regex patterns or backtracking
  behavior, revisit.
- **`make_synthetic_gid` vs `make_proxy_synthetic_gid` is a real
  choice with cross-domain inconsistency.** Pass 19 (gift cards)
  used `make_proxy_synthetic_gid` → `?shopify-draft-proxy=synthetic`
  suffix. Pass 20 (segments) uses `make_synthetic_gid` → unsuffixed.
  Both mirror TS exactly; the choice is per-resource and follows the
  TS handler. Test fixtures and assertions must use the right form
  or look-by-id misses. (This bit me on the first mutation test run
  — three tests had the wrong gid format and failed before I fixed
  them by trusting the actual output.)
- **`validate_segment_query` returns `List(UserError)`, not `Result`.**
  Mirroring the TS pattern that accumulates errors rather than
  short-circuiting — though in practice each pattern path emits at
  most one message. Worth keeping the list shape because the canned
  `"not a valid segment query ???"` path emits two messages.

### Risks / open items

- **Customer-segment-membership surfaces deferred.** The Admin
  schema also defines `customerSegmentMembers`,
  `customerSegmentMembersQuery`, `customerSegmentMembership`, and
  the `customerSegmentMembersQueryCreate` mutation. None of these
  ported here because they need a `CustomerRecord` store slice.
  Consumers that resolve a segment to its customer membership will
  hit the legacy fallback path (no proxy mirror) until customers
  port.
- **Upstream-hybrid suggestion surfaces deferred.**
  `segmentFilters`, `segmentFilterSuggestions`,
  `segmentValueSuggestions`, and `segmentMigrations` all rely on
  upstream-hybrid plumbing — the proxy mirrors what upstream returns
  rather than minting it. The plumbing path hasn't ported, so these
  return null/empty instead of forwarding. Flagged in the module
  doc comment.
- **Query validation is intentionally narrow.** Only
  `number_of_orders` comparators, `customer_tags CONTAINS '…'`, and
  `email_subscription_status = '…'` are recognized as valid. Any
  other valid Admin segment query (orders count, abandoned checkouts,
  product-purchase predicates, etc.) emits a "filter cannot be
  found" error. This matches the TS port's intentionally narrow
  validation surface — proxy-validated queries are a tiny subset of
  what real Admin accepts. Real-world consumers passing more complex
  queries will get spurious user errors and need to either skip
  validation or expand the matcher set.

### Pass 21 candidates

- **`localization`** — locales + currencies. Read-mostly, modest
  size. Tests well from a real consumer surface and unblocks
  shop-currency reading (which would in turn re-route the
  Pass 19 `giftCardConfiguration` fallback).
- **`inventory-shipments`** — inventory shipment domain, ~20K.
  Heavier on records but conceptually a simple CRUD on a single
  resource.
- **`shop` / `staffMember` / `currentAppInstallation`** — small
  singleton slices that several other domains assume in their
  fallbacks. Could be a quick "infrastructure" pass.
- **`customers`** (substrate only) — a `CustomerRecord` store slice
  - the `customer(id:)` / `customers(...)` query roots, no mutations.
    Would unblock the deferred Pass 20 surfaces (customer-segment
    membership) and the deferred Pass 19 recipient-resolution path.

Pass 21 should likely be **localization** — smallest delta, real
consumer surface, and re-routes the Pass 19 currency fallback.

---

## 2026-04-29 — Pass 19: gift cards domain (giftCard reads + 7 mutation roots + singleton configuration)

Ports `src/proxy/gift-cards.ts` (~30K) to a new `proxy/gift_cards.gleam`.
Lands the four query roots (`giftCard`, `giftCards`, `giftCardsCount`,
`giftCardConfiguration`) and all seven mutation roots
(`giftCardCreate` / `giftCardUpdate` / `giftCardCredit` /
`giftCardDebit` / `giftCardDeactivate` /
`giftCardSendNotificationToCustomer` /
`giftCardSendNotificationToRecipient`). Introduces
`GiftCardRecord`, `GiftCardTransactionRecord`,
`GiftCardRecipientAttributesRecord`, and `GiftCardConfigurationRecord`
shapes plus the per-record store slice (dict + order; no deletion
markers — gift cards never delete) and singleton-`Option` slice for
configuration. Threads `GiftCardsDomain` through the dispatcher
(capability + legacy fallback). The `MutationOutcome` shape carries
through unchanged from the apps/webhooks/saved-search/functions
chain.

### Module table

| Module                                                          | Lines | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| --------------------------------------------------------------- | ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/shopify_draft_proxy/state/types.gleam`                     | +59   | Adds `GiftCardTransactionRecord`, `GiftCardRecipientAttributesRecord`, `GiftCardRecord`, `GiftCardConfigurationRecord`. `GiftCardRecord` carries unsigned `Money` for both `initial_value` and `balance`; transaction signing for debits is the handler's responsibility. `recipient_attributes: Option(GiftCardRecipientAttributesRecord)` is `None` for cards minted without recipient input — the serializer falls back to a constructed attributes record built from `recipient_id`.                                                                                                                                                                                                                                                              |
| `src/shopify_draft_proxy/state/store.gleam`                     | +130  | Extends `BaseState` and `StagedState` with three new fields: `gift_cards: Dict(String, GiftCardRecord)`, `gift_card_order: List(String)`, `gift_card_configuration: Option(GiftCardConfigurationRecord)`. Adds `stage_create_gift_card`, `stage_update_gift_card` (delegates to create — gift cards never delete), `get_effective_gift_card_by_id`, `list_effective_gift_cards`, `set_staged_gift_card_configuration`, `get_effective_gift_card_configuration` (falls back to `default_gift_card_configuration` — `0.0 CAD` for both limits, matching TS `getEffectiveGiftCardConfiguration` line 2618-2632 of `state/store.ts`).                                                                                                                     |
| `src/shopify_draft_proxy/proxy/gift_cards.gleam`                | +2185 | New module. Public surface: `GiftCardsError(ParseFailed)`, `is_gift_card_query_root`, `is_gift_card_mutation_root`, `handle_gift_card_query`, `wrap_data`, `process`, `process_mutation`, `MutationOutcome`, `UserError`. Inline serialization for `GiftCard` and `GiftCardTransaction` with manual `InlineFragment` + `FragmentSpread` handling against named-type conditions. Decimal helpers mirror TS `formatDecimalAmount` (round to 2dp, trim a single trailing zero, but never below `<int>.0`). Code helpers mirror TS `normalizeGiftCardCode` — when the caller omits `code`, mint `proxy<8-digit-zero-padded-id>`; `lastCharactersFromCode` returns the trailing 4 chars; `maskedCode` is `•••• •••• •••• <last4>` (Unicode bullet U+2022). |
| `src/shopify_draft_proxy/proxy/draft_proxy.gleam`               | +20   | Wires the new dispatch arm: `Ok(GiftCardsDomain) -> gift_cards.process(…)` for queries and `gift_cards.process_mutation(…)` for mutations (signature: `store, identity, request_path, document, variables` — same shape as functions), the capability arms `GiftCards -> Ok(GiftCardsDomain)` for both query/mutation, and the legacy fallback `gift_cards.is_gift_card_query_root(name)` / `gift_cards.is_gift_card_mutation_root(name)`.                                                                                                                                                                                                                                                                                                            |
| `test/shopify_draft_proxy/proxy/gift_cards_test.gleam`          | +250  | New file. 13 read-path tests covering `is_gift_card_query_root` / `is_gift_card_mutation_root`, `giftCard(id:)` (record / missing / missing-arg / balance / `disabledAt` <-> `deactivatedAt` aliasing), `giftCards(first:)` connection (empty / seeded), `giftCardsCount`, `giftCardConfiguration` default fallback, and the inline `transactions` connection projection.                                                                                                                                                                                                                                                                                                                                                                             |
| `test/shopify_draft_proxy/proxy/gift_cards_mutation_test.gleam` | +260  | New file. 10 mutation tests covering all 7 mutation roots (success path), the `giftCardCreate { initialValue: 0 }` user-error path, the `{"data": …}` envelope, and the `is_gift_card_mutation_root` predicate.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |

**Test count: 461 → 484** (+23). Both targets clean (Erlang OTP 28 +
JS ESM).

### What landed

`stage_create_gift_card` doubles as `stageUpdateGiftCard` because gift
cards are append-only — `giftCardDeactivate` flips an `enabled` flag
and stamps `deactivated_at` instead of removing the record. The store
slice carries no `deleted_gift_card_ids` set, which is structurally
lighter than the validations/cart-transforms slices from Pass 18.

`giftCardCredit` and `giftCardDebit` share a single
`handle_gift_card_transaction` helper, parameterized over kind
(`"CREDIT"` / `"DEBIT"`), the input field name (`creditAmount` /
`debitAmount`), the wrapping input key (`creditInput` / `debitInput`),
and the payload typename (`GiftCardCreditPayload` /
`GiftCardDebitPayload`). The store-side balance math always uses
unsigned magnitudes — credit adds, debit subtracts — and the resulting
transaction record carries the absolute amount; the handler signs
debit transactions on emission only.

`giftCardConfiguration` is a singleton like `taxAppConfiguration` from
Pass 18: `Option(GiftCardConfigurationRecord)` on both `BaseState` and
`StagedState`, no dict, no order list. The default fallback returns
`0.0 CAD` for both `issueLimit` and `purchaseLimit` — verified
against TS `state/store.ts:2618-2632` to match exactly. (Earlier
draft used `1000.0 / 5000.0 CAD`; corrected to match TS.)

`giftCardUpdate` differentiates "key present with null" vs "key
absent" via `dict_has_key`, mirroring the TS
`Object.prototype.hasOwnProperty.call` pattern. This matters for
`recipientAttributes` — passing `null` clears existing attributes;
omitting the key preserves them. `recipientId` takes precedence over
`recipientAttributes.id` when both are provided; when neither is
provided, the existing record's recipient is preserved.

`GiftCard.__typename` always projects to `"GiftCard"`;
`GiftCardTransaction.__typename` always projects to
`"GiftCardTransaction"` (not `GiftCardCreditTransaction` /
`GiftCardDebitTransaction` despite the kind discriminator). This
matches TS `serializeGiftCardTransaction` line 279 — surprised me on
the first test pass and required adjusting expected output.

### Findings

- **Singletons + dict-with-order is becoming the canonical shape.**
  Five domains in (saved-search, webhooks, apps, functions, gift
  cards), four use the dict-with-order pattern for collection
  resources and `Option(Record)` for singletons. The shape is
  formulaic now: `{plural}: Dict(String, Record)`,
  `{singular}_order: List(String)`, optional `deleted_{plural}_ids`
  set when the resource supports deletion. Future ports will follow
  this layout without further design work.
- **Inline-fragment handling is per-domain boilerplate.** Both
  `GiftCard` and `GiftCardTransaction` require manual
  `InlineFragment` + `FragmentSpread` walking with type-condition
  matching against the parent typename. The generic
  `project_graphql_value` helper from `graphql_helpers` does not
  cover this case — it only walks plain `Field` selections. Pass 19
  carries this as inline copy in the gift-cards module; a future
  pass should consider extracting a shared `walk_typed_selections`
  helper.
- **The `MutationOutcome` envelope continues to pay off.** Five
  domains share the shape; the per-handler boilerplate is now
  muscle memory. The dispatcher arm is template — store + identity
  in, `MutationFieldResult` out, store + identity threaded forward.
- **`makeProxySyntheticGid` vs `makeSyntheticGid` matters.**
  Gift cards mint via `makeProxySyntheticGid('GiftCard')`, which
  produces gids like
  `gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic` — the
  `?shopify-draft-proxy=synthetic` suffix is part of the canonical
  id and round-trips through the store. Transactions mint via
  `makeSyntheticGid('GiftCardCreditTransaction' /
'GiftCardDebitTransaction')` — no suffix. Test fixtures must use
  the right form or look-by-id misses.

### Risks / open items

- **Gift card transaction `__typename` is uniform.** The TS handler
  emits `"GiftCardTransaction"` regardless of credit/debit, even
  though credit/debit transactions are distinct types in the Admin
  schema. Real upstream responses may emit the discriminated
  typenames; the proxy will need an upstream-hybrid path to surface
  those, which Pass 19 does not deliver.
- **`giftCardSendNotificationToCustomer` and
  `giftCardSendNotificationToRecipient` are no-ops on the store
  side.** They return the gift card unchanged. Real Shopify queues
  email delivery; the proxy never will. Consumers that branch on
  notification side-effects will see no observable change — flagged
  in the handler comment.
- **Default `giftCardConfiguration` fallback uses `'CAD'` literally,
  not the shop currency.** TS `getEffectiveGiftCardConfiguration`
  reads shop currency first, then falls back to `'CAD'`. The Gleam
  port short-circuits to `'CAD'` because shop-currency reading isn't
  ported yet. When the shop / locale port lands, this fallback will
  need re-routing.

### Pass 20 candidates

The next domain port should be a small read-only slice now that the
mutation muscle is well-developed. Candidates:

- **`segments`** — read-only-ish, ~12K, schema-light. Three query
  roots (`segment`, `segments`, `segmentsCount`) + a couple of
  mutation roots. Parallels saved-searches structurally but with a
  query-language field instead of free-form filters.
- **`localization`** — locales + currencies. Read-mostly, modest
  size. Tests well from a real consumer surface.
- **`inventory-shipments`** — inventory shipment domain, ~20K.
  Heavier on records but conceptually a simple CRUD on a single
  resource.

Pass 20 should likely be **segments** — it's the smallest gap and
unblocks several other admin surfaces that filter by segment.

---

## 2026-04-29 — Pass 18: functions domain (Shopify Functions / validation / cartTransform / tax-app)

Ports `src/proxy/functions.ts` (~23K) to a new `proxy/functions.gleam`.
Lands the five query roots (`validation`, `validations`,
`cartTransforms`, `shopifyFunction`, `shopifyFunctions`) and all six
mutation roots (`validationCreate` / `validationUpdate` /
`validationDelete`, `cartTransformCreate` / `cartTransformDelete`,
`taxAppConfigure`). Introduces the `ShopifyFunctionRecord`,
`ValidationRecord`, `CartTransformRecord`, and
`TaxAppConfigurationRecord` shapes plus the per-record store slices,
and threads the `FunctionsDomain` through the dispatcher (capability

- legacy fallback). The `MutationOutcome` shape carries through
  unchanged from apps/webhooks/saved-search.

### Module table

| Module                                                         | Lines | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| -------------------------------------------------------------- | ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/shopify_draft_proxy/state/types.gleam`                    | +73   | Adds `ShopifyFunctionRecord`, `ValidationRecord`, `CartTransformRecord`, `TaxAppConfigurationRecord`. Comment on `ShopifyFunctionRecord` documents the deliberate omission of an `app: jsonObjectSchema.optional()` field — the proxy never mints app metadata locally so `app` projects to `null` until upstream hydration lands.                                                                                                                                                                                                                                                                                                                                                  |
| `src/shopify_draft_proxy/state/store.gleam`                    | +334  | Extends `BaseState` and `StagedState` with 11 new fields: `shopify_functions` + order, `validations` + order + deletion markers, `cart_transforms` + order + deletion markers, `tax_app_configuration: Option(...)` (singleton — no order/deletion-markers). Adds `upsert_staged_shopify_function`, `get_effective_shopify_function_by_id`, `list_effective_shopify_functions` (no deletion markers; functions can't be deleted), `upsert_staged_validation`, `delete_staged_validation`, `get_effective_validation_by_id`, `list_effective_validations`, the cart_transform parallels, and `set_staged_tax_app_configuration` / `get_effective_tax_app_configuration`.             |
| `src/shopify_draft_proxy/proxy/functions.gleam`                | +900  | New module. Public surface: `FunctionsError(ParseFailed)`, `is_function_query_root`, `is_function_mutation_root`, `handle_function_query`, `wrap_data`, `process`, `process_mutation`, `MutationOutcome`, `UserError`, `normalize_function_handle`, `shopify_function_id_from_handle`, `title_from_handle`. Mutation pipeline includes `ensure_shopify_function` (4-step lookup-or-mint: by id / by handle / by normalized handle / handle-derived-id, then mint), `FunctionReference` for capturing input function references, and the 6 per-root handlers. Read path serializes connections via the existing `paginate_connection_items` + `serialize_connection` infrastructure. |
| `src/shopify_draft_proxy/proxy/draft_proxy.gleam`              | +6    | Wires the new dispatch arm: `Ok(FunctionsDomain) -> functions.process(…)` for queries and `functions.process_mutation(…)` for mutations (note: takes `request_path` only, NOT origin), the capability arms `Functions -> Ok(FunctionsDomain)`, and the legacy fallback `functions.is_function_query_root(name)` / `functions.is_function_mutation_root(name)`.                                                                                                                                                                                                                                                                                                                      |
| `test/shopify_draft_proxy/proxy/functions_test.gleam`          | +330  | New file. 19 read-path tests covering `is_function_query_root`, all 5 query roots, the `enable`/`enabled` aliasing on Validation, `functionId`-falls-back-to-`shopifyFunctionId`, the embedded `shopifyFunction` projection, the `apiType` filter on `shopifyFunctions`, and the `normalize_function_handle` / `shopify_function_id_from_handle` / `title_from_handle` helpers.                                                                                                                                                                                                                                                                                                     |
| `test/shopify_draft_proxy/proxy/functions_mutation_test.gleam` | +280  | New file. 18 mutation tests covering `is_function_mutation_root`, the `{"data": …}` envelope, all 6 mutation roots (success + user-error variants), the `ensure_shopify_function` reuse-existing path, the `validationCreate` enable/blockOnFailure defaults, and the `cartTransformCreate` top-level-args fallback (TS quirk).                                                                                                                                                                                                                                                                                                                                                     |

**Test count: 424 → 461** (+37). Both targets clean (Erlang OTP 28 +
JS ESM).

### What landed

The functions domain shares the apps/webhooks `MutationOutcome` envelope:
`data: Json`, `store: Store`, `identity: SyntheticIdentityRegistry`,
`staged_resource_ids: List(String)`. Same dispatcher contract — the
mutation handler never emits top-level GraphQL errors; every failure
routes through `userErrors`.

`ensure_shopify_function` is the load-bearing helper for the
validation/cart-transform create + update paths. It checks four
positions in order: exact-id match (when `functionId` is supplied),
exact-handle match, normalized-handle match, and handle-derived-id
match. If none hits, it mints — handle-derived id when a handle is
supplied, synthetic gid otherwise. The minted record carries the
caller-supplied API type (`VALIDATION` / `CART_TRANSFORM`) and a
title derived from the handle if available, else the handler's
fallback ("Local validation function" / "Local cart transform
function"). Result is that re-creating a validation against an
already-known function reuses that function — the per-test
`validation_create_reuses_existing_function_test` asserts this.

`tax_app_configuration` is modeled as a singleton via
`Option(TaxAppConfigurationRecord)` on both `BaseState` and
`StagedState` — no order array, no deletion markers, no dictionary.
The TS shape is one configuration per shop, which fits this exactly.
`taxAppConfigure(ready: Boolean)` sets `state` to either `READY` or
`NOT_READY` based on the boolean and stamps `updatedAt` from the
identity registry. Missing the `ready` arg emits a `INVALID` user
error.

`cartTransformCreate` carries a TS quirk we mirror precisely: the
input can either nest the fields under a `cartTransform: { … }` key
or pass them at the top level. The handler tries the nested object
first and falls back to the args dict — which means
`cartTransformCreate(cartTransform: { functionHandle: "x" })` and
`cartTransformCreate(functionHandle: "x")` both work. Test coverage
is `cart_transform_create_falls_back_to_top_level_args_test`.

`normalize_function_handle` does the work the TS regex does in one
line: trim → lowercase → fold over graphemes replacing each run of
non-`[a-z0-9_-]` characters with a single `-` → strip leading and
trailing `-` → return `local-function` if the result is empty. The
fold uses an `in_bad_run` flag rather than collapsing dashes
post-hoc, which means runs of varying-length disallowed chars all
collapse to one `-`. `shopify_function_id_from_handle` is a thin
wrapper that prefixes with `gid://shopify/ShopifyFunction/`.

### Findings

- **The `MutationOutcome` envelope keeps paying off.** Four domains
  (`webhooks`, `saved_searches`, `apps`, `functions`) now use the
  same shape. The boilerplate in each handler is identical: take
  store + identity, return `#(MutationFieldResult, Store,
SyntheticIdentityRegistry)`. Once the registry threading is in
  muscle memory, mutation porting is mechanical.
- **Singletons fit `Option` on the state slice.** Tax-app
  configuration is the first singleton resource in the port. No
  dict, no order list, no deletion markers — just `Option(Record)`
  on both `BaseState` and `StagedState`, with staged-over-base
  resolution in the effective getter. Cleaner than the
  dict-with-one-key alternative.
- **Functions never get deleted, so no deletion markers.** The TS
  schema has no `deleteShopifyFunction` mutation; `ShopifyFunction`
  records are append-only. The store slice for shopify functions
  has only the dict + order list — no `deleted_*_ids` field. This
  is structurally lighter than the validation / cart-transform
  slices, which carry the full deletion machinery.
- **Three different mutation-input shapes converge through the same
  `field_args` helper.** `validationCreate` reads `args.validation`
  (nested), `cartTransformCreate` reads `args.cartTransform` OR
  `args` (TS quirk), `taxAppConfigure` reads `args.ready` (top-level).
  The `input_object` helper returns `Option(Dict)` so each handler
  can branch on `Some/None` without re-implementing dict lookup.

### Risks / open items

- **`shopifyFunction.app` is hardcoded to `null`.** Real upstream
  hydration may surface app metadata; the record carries no `app`
  field today. When the upstream-hybrid pass for functions lands,
  this will need re-shaping. The deferred-field comment in
  `state/types.gleam` flags this explicitly.
- **No upstream hybrid path.** The functions handler stages locally
  for every mutation — there's no path that invokes upstream and
  staged-merges the result. Other domains (orders, products) will
  need this; functions does not.
- **No metafield projection.** `Validation` and `CartTransform` both
  have `metafield`/`metafields` selections in TS that route through
  the metafields infrastructure. The Gleam port projects `metafield:
null` and `metafields: <empty connection>` — sufficient for the
  proxy's local-staging story but a real metafield hookup will need
  an additional pass.
- **Pagination on connection roots ignores `first`/`after`.** Same
  Pass 16/17 limitation: the connection serializer paginates against
  the empty default window. Functions are typically few in number
  per shop so this is unlikely to bite, but the limitation carries
  forward.

### Test additions

- `functions_test.gleam` (19 tests):
  `is_function_query_root_test`,
  `validation_by_id_returns_record_test`,
  `validation_by_id_missing_returns_null_test`,
  `validation_by_id_missing_argument_returns_null_test`,
  `validation_enable_and_enabled_alias_test`,
  `validation_embedded_shopify_function_test`,
  `validation_function_id_falls_back_to_shopify_function_id_test`,
  `validations_connection_empty_test`,
  `validations_connection_returns_seeded_test`,
  `cart_transforms_connection_empty_test`,
  `cart_transforms_connection_returns_seeded_test`,
  `shopify_function_by_id_returns_record_test`,
  `shopify_function_by_id_missing_returns_null_test`,
  `shopify_functions_connection_empty_test`,
  `shopify_functions_connection_returns_all_test`,
  `shopify_functions_connection_filters_by_api_type_test`,
  `normalize_function_handle_basic_test`,
  `shopify_function_id_from_handle_test`,
  `title_from_handle_test`.
- `functions_mutation_test.gleam` (18 tests):
  `is_function_mutation_root_test`,
  `process_mutation_returns_data_envelope_test`,
  `validation_create_with_handle_mints_records_test`,
  `validation_create_missing_function_emits_user_error_test`,
  `validation_create_reuses_existing_function_test`,
  `validation_create_defaults_enable_and_block_test`,
  `validation_update_changes_title_and_enable_test`,
  `validation_update_unknown_id_emits_user_error_test`,
  `validation_delete_removes_record_test`,
  `validation_delete_unknown_id_emits_user_error_test`,
  `cart_transform_create_with_handle_mints_records_test`,
  `cart_transform_create_falls_back_to_top_level_args_test`,
  `cart_transform_create_missing_function_emits_user_error_test`,
  `cart_transform_delete_removes_record_test`,
  `cart_transform_delete_unknown_id_emits_user_error_test`,
  `tax_app_configure_ready_true_test`,
  `tax_app_configure_ready_false_test`,
  `tax_app_configure_missing_ready_emits_user_error_test`.

---

## 2026-04-29 — Pass 17: apps domain mutation path

Completes the apps domain mutation path. All 10 mutation roots now
stage locally and round-trip through the projector: `appUninstall`,
`appRevokeAccessScopes`, `delegateAccessTokenCreate` /
`delegateAccessTokenDestroy`, `appPurchaseOneTimeCreate`,
`appSubscriptionCreate` / `appSubscriptionCancel` /
`appSubscriptionLineItemUpdate` / `appSubscriptionTrialExtend`,
`appUsageRecordCreate`. Introduces the `MutationOutcome` envelope
(mirroring `webhooks.process_mutation`), the lazy-bootstrap helper
`ensure_current_installation`, the `confirmation_url` / `token_hash`
/ `token_preview` helpers, and a dual-target sha256 FFI shim
(`crypto_ffi.erl` + `crypto_ffi.js`) since Gleam stdlib does not
include hashing. Wires `AppsDomain` into the mutation dispatcher
both via capability (`Apps -> Ok(AppsDomain)`) and the legacy
predicate `apps.is_app_mutation_root`.

### Module table

| Module                                                    | Lines | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| --------------------------------------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/shopify_draft_proxy/proxy/apps.gleam`                | +1100 | Adds `MutationOutcome`, `UserError`, `is_app_mutation_root`, `process_mutation`, the 10 per-root handlers, `ensure_current_installation` (threading `(store, identity, origin) -> #(installation, store, identity)`), `default_app`, `confirmation_url`, `token_hash`, `token_preview`, `trailing_segment` (strips `?v=1&index=N` suffix from line item GIDs), `read_arg_bool`/`read_arg_int`/`read_money_input`/`read_line_item_plan`, `record_log` / `build_log_entry` (capability `domain: "apps"`, `execution: "stage-locally"`), 7 projection functions (`project_uninstall_payload`, `project_revoke_payload`, `project_delegate_create_payload`, `project_delegate_destroy_payload`, `project_purchase_create_payload`, `project_subscription_create_payload`, `project_subscription_payload` (alias), `project_usage_record_payload`), and `user_errors_source` / `user_error_to_source` (with optional `code` field for `UNKNOWN_SCOPES` / `ACCESS_TOKEN_NOT_FOUND`). |
| `src/shopify_draft_proxy/crypto.gleam`                    | +18   | New cross-target hashing module. Single export: `sha256_hex(input: String) -> String`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `src/shopify_draft_proxy/crypto_ffi.erl`                  | +6    | Erlang shim: `crypto:hash(sha256, …)` + `binary:encode_hex(_, lowercase)`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `src/shopify_draft_proxy/crypto_ffi.js`                   | +5    | Node ESM shim: `createHash('sha256').update(s).digest('hex')`. Byte-identical to the Erlang side.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `src/shopify_draft_proxy/proxy/draft_proxy.gleam`         | +5    | Wires the new dispatch arm: `Ok(AppsDomain) -> apps.process_mutation(…, origin, …)`, the capability arm `Apps -> Ok(AppsDomain)`, and the legacy fallback `apps.is_app_mutation_root(name)`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `test/shopify_draft_proxy/proxy/apps_mutation_test.gleam` | +476  | New test file. 19 tests covering `is_app_mutation_root`, the `{"data": …}` envelope, all 10 mutation roots (success + user-error variants), the default-app/installation auto-bootstrap, the sha256 round-trip via the same FFI shim the handler uses (declared with a relative path `../../shopify_draft_proxy/crypto_ffi.js` for the JS target).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |

**Test count: 405 → 424** (+19). Both targets clean (Erlang OTP 28 +
JS ESM).

### What landed

The `MutationOutcome` envelope from `webhooks.process_mutation` is
the load-bearing template — `apps.process_mutation` returns a
`Result(MutationOutcome, AppsError)` and apps mutations never emit
top-level GraphQL errors. Every failure mode (unknown subscription
id, unknown scope, missing access token) goes through `userErrors`,
so the `Ok` branch always wraps `{"data": {...}}`. The legacy
test-file pattern of "missing required arg → `errors[]`" doesn't
apply on this domain.

`ensure_current_installation` lazily mints a default app + default
installation when the store has none, threading the identity
registry through three `make_synthetic_gid` calls. The default app
gets handle `shopify-draft-proxy`, api_key
`shopify-draft-proxy-local-app`, and the same two requested scopes
the read-path tests already use. `stage_app_installation` auto-sets
`current_installation_id` if neither the base nor staged state
already has one — this is the only mechanism that wires the new
installation up as "current"; no separate setter call needed.

`token_hash` is the wire between the create and destroy handlers:
`delegateAccessTokenCreate` stores the lowercase-hex sha256 of the
raw token and the destroy handler looks the record up via
`store.find_delegated_access_token_by_hash`. Tokens are returned
to the caller exactly once at create-time; the store never holds
the raw form. `token_preview` emits `[redacted]` for short tokens
and `[redacted]<last4>` otherwise.

The line item update handler's `cappedAmount` shallow-merge from TS
collides with Gleam's typed sum: `AppRecurringPricing` has no
`capped_amount` field so the recurring branch falls through and
leaves pricing unchanged. Documented inline; realistic shop
emissions use `AppUsagePricing` for cappedAmount updates.

`trailing_segment` handles a quirk of synthetic line item GIDs:
they carry a `?v=1&index=N` suffix used by the read-path projector
to disambiguate line items within a subscription. The
`confirmation_url` builder needs the bare numeric segment for the
URL, so it splits on `/` then on `?`.

### Findings

- **The MutationOutcome shape is the right abstraction.** Three
  domains now use it (`webhooks`, `saved_searches`, `apps`) with
  the same fields: `data: Json`, `store: Store`, `identity:
SyntheticIdentityRegistry`, `staged_resource_ids: List(String)`.
  Threading `identity` through every handler is non-trivial — each
  GID mint or timestamp advances the registry — but the pattern is
  now muscle memory.
- **FFI shim discovery: relative paths in test files matter.** The
  test file lives at `test/shopify_draft_proxy/proxy/`, so its
  `@external(javascript, "...", "...")` shim needs
  `../../shopify_draft_proxy/crypto_ffi.js` (two parent traversals)
  to reach the FFI module under `src/`. The Erlang side just uses
  the bare module name `crypto_ffi`.
- **`is_test` rename pattern continues.** `test` is reserved in
  Gleam, so the field is `is_test: Bool` on records and the GraphQL
  response key stays `test` because the source builder names it
  explicitly. No projector change.
- **Capability + legacy fallback pays off again.** Adding 10
  mutation roots required only a 5-line edit to the dispatcher: one
  arm, one capability mapping, one predicate. No regressions in
  existing capability routing for webhooks/saved-searches.
- **Apps mutations carry a richer `userError` shape.** The optional
  `code` field (`UNKNOWN_SCOPES` / `ACCESS_TOKEN_NOT_FOUND`) is the
  first place this domain's `UserError` diverges from
  `webhooks.UserError`. The projection emits `code: null` when
  `None`, matching the wire shape Shopify produces.

### Risks / open items

- **No top-level error envelope tests.** Apps mutations don't
  produce one — every failure routes through `userErrors`. Future
  domains may, so `MutationOutcome`-vs-error-envelope routing logic
  will need to grow. For now `process_mutation` always succeeds.
- **Pagination on mutation projections is not exercised.** The
  Pass 16 limitation (no `first`/`after` honoring on connections)
  carries forward; the `appSubscriptionCreate` payload nests a
  `lineItems` array inside the subscription source but doesn't go
  through `serialize_connection`. If a test exercises
  `appSubscription { lineItems(first: 1) { … } }` this will need
  lifting.
- **`delegateAccessScope` arg type quirk.** TS treats it as
  `[String!]`; Gleam reads it as a single string via
  `read_arg_string` and falls back to `accessScopes` (a list).
  Tests use the list form. Sub-pass-able if a real test exercises
  the array form.

### Unblocked / next

Apps domain is feature-complete (read + mutation). Next bottleneck
is one of: customer mutations (5 roots), product mutations (the
biggest surface, ~30 roots), or order mutations. The
`MutationOutcome` + `ensure_*` + projection pattern from this pass
ports directly.

---

## 2026-04-29 — Pass 16: apps domain read path

Completes the apps domain read path. Lands a new
`shopify_draft_proxy/proxy/apps.gleam` mirroring the read shape of
`src/proxy/apps.ts`: the six query roots (`app`, `appByHandle`,
`appByKey`, `appInstallation`, `appInstallations`,
`currentAppInstallation`), per-record source projections for every
apps record type, the `__typename`-discriminated
`AppSubscriptionPricing` sum, and the three child connections
(`activeSubscriptions` array, `allSubscriptions` /
`oneTimePurchases` / `usageRecords` connections). Adds `AppsDomain`
to the dispatcher: capability-driven for registry-loaded operations,
legacy-fallback predicate `apps.is_app_query_root` for unmigrated
tests.

### Module table

| Module                                            | Lines | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ------------------------------------------------- | ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/shopify_draft_proxy/proxy/apps.gleam`        | +560  | New module. Surfaces: `AppsError`, `is_app_query_root`, `handle_app_query`, `wrap_data`, `process`. Internal: `serialize_root_fields` / `root_payload_for_field` dispatch, six per-root serializers, `app_to_source` / `app_installation_to_source` / `subscription_to_source` / `line_item_to_source` / `usage_record_to_source` / `one_time_purchase_to_source` / `access_scope_to_source` / `money_to_source` / `pricing_to_source` (the sum-type discriminator), three connection-source builders (`subscription_connection_source`, `one_time_purchase_connection_source`, `usage_record_connection_source`) plus a tiny shared `page_info_source`. |
| `src/shopify_draft_proxy/proxy/draft_proxy.gleam` | +18   | Wires `AppsDomain` into both the capability-driven dispatch (added `Apps -> Ok(AppsDomain)` to `capability_to_query_domain`) and the legacy fallback (`apps.is_app_query_root`). New `AppsDomain` variant on `Domain`. Added `import shopify_draft_proxy/proxy/apps`.                                                                                                                                                                                                                                                                                                                                                                                    |
| `test/shopify_draft_proxy/proxy/apps_test.gleam`  | +330  | 19 new tests: `is_app_query_root` predicate, all six query roots (happy path + missing/null), inline-fragment-based `__typename` split for `AppRecurringPricing` vs `AppUsagePricing`, child connections (active subscriptions array, oneTimePurchases connection, usageRecords connection), access-scope projection, and the `process` envelope wrap. Standard `run(store, query)` helper using `apps.handle_app_query`.                                                                                                                                                                                                                                |

**Test count: 386 → 405** (+19). Both targets clean (Erlang OTP 28 +
JS ESM).

### What landed

The read path is a pure function of `Store` — it never auto-creates
the default app or installation. That's a deliberate match to the TS
behavior: `handleAppQuery` reads only; `ensureCurrentInstallation`
is mutation-only. So the dispatcher signature didn't need to grow:
`apps.process(store, query, variables) -> Result(Json, AppsError)`
mirrors `webhooks.process` / `saved_searches.process` exactly.

Three connection-shaped fields (`allSubscriptions`,
`oneTimePurchases`, `usageRecords`) need to round-trip through the
`SourceValue` projector rather than the more direct
`serialize_connection` helper, because they're nested inside a parent
record whose outer projection owns the field selection. The pattern
that fell out: build a `SourceValue` shaped like a connection
(`{__typename, edges, nodes, pageInfo, totalCount}`) and let
`project_graphql_value` walk into it. `serialize_connection` handles
only the top-level `appInstallations` connection where the field
selection is owned directly.

The `AppSubscriptionPricing` sum type pattern-matches in
`pricing_to_source`: variant constructors emit different `__typename`
values plus their own field set. Inline-fragment selections like
`... on AppRecurringPricing { interval price { amount } }` then
go through `default_type_condition_applies` and gate cleanly. This
is the first port where a sum-type-discriminated union round-trips
through the projector — the webhook endpoint sum did the same shape
but inside a single record field, not at the top level of a record.

Field selection projection treats `is_test`/`test` as a Gleam keyword
clash carried over from Pass 15; the renamed Gleam field is `is_test`
but the GraphQL response key stays `test` because the `SourceValue`
record is built explicitly by name in the source builder.

### Findings

- **The `SourceValue` model scales to apps.** Pass 11's substrate
  designed for webhooks now carries 11 record types through the
  projector with no friction. Connections-as-source-values is the
  reusable pattern for nested connections; only the topmost
  connection needs `serialize_connection`.
- **Sum types as discriminated unions translate cleanly.** The
  `AppRecurringPricing` / `AppUsagePricing` split projects through
  the existing inline-fragment machinery without any new code in
  `graphql_helpers`. This is reassuring for the upcoming
  `MetafieldOwner` / `Node` interfaces in customers/products.
- **Domain modules are stabilizing in shape.** `apps.gleam`,
  `webhooks.gleam`, and `saved_searches.gleam` now share an almost
  identical scaffold: `Error` type, `is_*_query_root` predicate,
  `handle_*_query` returning `Result(Json, _)`, `wrap_data`,
  `process` for the dispatcher. Future read-path ports
  (delivery-settings, customers, products) can copy this structure.
- **The dispatcher's two-track resolution (capability + legacy
  predicate) is paying off.** Adding `AppsDomain` was a 5-line edit
  in three places: capability case, legacy fallback, and the
  dispatch arm. No risk of breaking existing routing because the
  predicates are name-disjoint.
- **JS-ESM parity continues.** No FFI in this pass; everything ran
  on both targets first try.

### Risks / open items

- **Mutation path is the next bottleneck.** Apps has 10 mutation
  roots (the largest mutation surface so far): purchaseOneTimeCreate,
  subscriptionCreate/Cancel/LineItemUpdate/TrialExtend,
  usageRecordCreate, revokeAccessScopes, uninstall,
  delegateAccessTokenCreate/Destroy. Each touches synthetic identity
  - store + identity registry. Significant code volume.
- **`ensureCurrentInstallation` deferred.** The lazy-bootstrap helper
  is used by 4 of the 10 mutations; it's not in this pass because
  the read path doesn't need it. The mutation pass will need to
  thread it through `(store, identity)` and bring in
  `confirmationUrl` / `tokenHash` / `tokenPreview` helpers (the
  latter requires a sha256 FFI — no `gleam_crypto` in stdlib).
- **No connection-arg honoring on apps connections.** The
  `subscription_connection_source` etc. emit a fixed page (no `first`
  / `after` filtering) because the SourceValue route doesn't see the
  field-arg machinery. The TS passes the same simplification through
  `paginateConnectionItems` with default options — but if a future
  test exercises pagination on a subscription connection, this will
  need lifting.
- **Connection `pageInfo` is hard-coded `hasNextPage: false`.** Same
  reason as above — there's no pagination state plumbed through the
  source builders. Acceptable for the current TS parity (the source
  arrays are short) but not a long-term shape.

### Recommendation for Pass 17

Land the apps **mutation path**. Concrete pieces, in order of
expected friction:

1. **`appUninstall` + `appRevokeAccessScopes`.** Smallest surface;
   they only flip an existing installation's `uninstalled_at` /
   `access_scopes`. No new helpers needed beyond `ensureCurrentInstallation`.
2. **`delegateAccessTokenCreate` + `delegateAccessTokenDestroy`.**
   Needs a sha256 FFI shim. Implement once with two adapters
   (`erlang:crypto:hash/2` and Node's `node:crypto.createHash`).
3. **`appPurchaseOneTimeCreate` + `appSubscriptionCreate`.**
   Establishes the `confirmationUrl` + synthetic-id plumbing.
   Subscription pulls in `appSubscriptionLineItemUpdate` next.
4. **`appSubscriptionCancel` + `appSubscriptionTrialExtend`.**
   Status-flip mutations on existing subscriptions.
5. **`appUsageRecordCreate`.** The richest payload: walks the
   subscription→line-item→capped-amount chain to validate. Save for
   last.

Expected delta: ~1100 LOC (handler + helpers + tests). The pattern
from `webhooks.process_mutation` is the load-bearing template:
`MutationOutcome { data, store, identity, staged_resource_ids }` is
the right shape and the validators from `mutation_helpers` already
carry the right error envelopes. After Pass 17 the apps domain
should be feature-complete, freeing Pass 18+ to start on
delivery-settings or customers.

---

## 2026-04-29 — Pass 15: apps domain — types & store slice

Foundation pass for the apps domain. Lands the seven new record types
(`AppRecord`, `AppInstallationRecord`, `AppSubscriptionRecord`,
`AppSubscriptionLineItemRecord`, `AppOneTimePurchaseRecord`,
`AppUsageRecord`, `DelegatedAccessTokenRecord`), plus the supporting
shapes (`Money`, `AccessScopeRecord`, `AppSubscriptionPricing` sum,
`AppSubscriptionLineItemPlan`), and adds the corresponding base/staged
slices and store helpers. **No proxy handler yet** — the read/write
ports are deferred to Pass 16+.

### Module table

| Module                                            | Lines | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ------------------------------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/shopify_draft_proxy/state/types.gleam`       | +130  | New types: `Money`, `AccessScopeRecord`, `AppRecord`, `AppSubscriptionPricing` (sum: `AppRecurringPricing` / `AppUsagePricing`), `AppSubscriptionLineItemPlan`, `AppSubscriptionLineItemRecord`, `AppSubscriptionRecord`, `AppOneTimePurchaseRecord`, `AppUsageRecord`, `DelegatedAccessTokenRecord`, `AppInstallationRecord`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `src/shopify_draft_proxy/state/store.gleam`       | +400  | Seven new entity tables on `BaseState` / `StagedState` plus `current_installation_id` (Option). Helpers: `upsert_base_app`, `stage_app`, `get_effective_app_by_id`, `find_effective_app_by_handle`, `find_effective_app_by_api_key`, `list_effective_apps`, `upsert_base_app_installation` (atomic install + app), `stage_app_installation`, `get_effective_app_installation_by_id`, `get_current_app_installation`, `stage_app_subscription`, `get_effective_app_subscription_by_id`, `stage_app_subscription_line_item`, `get_effective_app_subscription_line_item_by_id`, `stage_app_one_time_purchase`, `get_effective_app_one_time_purchase_by_id`, `stage_app_usage_record`, `get_effective_app_usage_record_by_id`, `list_effective_app_usage_records_for_line_item`, `stage_delegated_access_token`, `find_delegated_access_token_by_hash`, `destroy_delegated_access_token`. |
| `test/shopify_draft_proxy/state/store_test.gleam` | +180  | 11 new tests covering each entity table: upsert/stage/get, the two app lookups (by handle, by api_key), installation singleton bootstrap, the per-line-item usage-records filter, and the destroy-then-find round trip on delegated tokens.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |

Tests: 386 / 386 on Erlang OTP 28 and JavaScript ESM. Net +11 tests
(375 → 386); all new tests are in the existing `state/store_test`
suite.

### What landed

The TS schema models all pricing details as `Record<string, jsonValue>`
inside the line-item `plan` field — Gleam types it precisely as a
`AppSubscriptionPricing` sum with two variants (`AppRecurringPricing`
and `AppUsagePricing`), each carrying only the fields its `__typename`
implies. This makes impossible combinations (e.g. a recurring plan
with `cappedAmount`) unrepresentable rather than runtime-checked.

`Money` is defined as a top-level record so future domain ports can
reuse it instead of copying. `AccessScopeRecord` is similarly
domain-agnostic — the shape is shared with the access-scopes-API
endpoints whenever those land.

The `current_installation_id` is modelled as a `Option(String)`
field on both base and staged state, mirroring TS where the proxy
treats the current installation as a singleton bootstrapped on first
mutation. Staged wins; on first stage it auto-promotes if no current
is set on either side. `upsert_base_app_installation` (used by
hydration) atomically writes both the installation and its app to base.

`destroy_delegated_access_token` doesn't physically remove the token —
it stages a copy with `destroyed_at` set, mirroring TS. This keeps
the find-by-hash lookup honest (the token is still findable by hash;
callers check `destroyed_at`).

The seven entity tables follow the same shape (dict + order list, no
`deleted_*_ids` since apps records aren't tombstoned the way saved
searches and webhook subscriptions are — uninstalls are modelled by
setting `uninstalled_at` on the installation, and subscription
cancellation flips `status`). The new entities all use the simpler
"staged-over-base, no soft-delete" lookup pattern.

### Findings

- **The "no soft-delete" decision shapes the lookup helpers.**
  Saved searches and webhooks both have `deleted_*_ids` in both
  base and staged, with the lookup helpers checking those before
  returning a record. None of the apps entities work that way —
  uninstalls and subscription-cancels just mutate a status field.
  That's a strict subset of the saved-search/webhook lookup, so
  the apps helpers are simpler.
- **`record(..r, status: …)` for cancellation; sum types for
  pricing.** The Gleam record-update spread mirrors TS `{...r, status}`
  exactly. For the discriminated-union pricing details, sum types
  with named record variants give us projection-time type checking
  for free — when `proxy/apps.gleam` lands in Pass 16, it'll pattern
  match on `AppRecurringPricing` vs `AppUsagePricing` rather than
  fishing through a `Record<string, unknown>`.
- **`is_test` instead of `test`.** `test` is a Gleam keyword reserved
  for the test runner and rejected as a record field name. Renamed
  the field on `AppSubscriptionRecord` and `AppOneTimePurchaseRecord`.
  Anywhere the GraphQL field name is `test`, the projector / handler
  in Pass 16 will need an explicit mapping (TS shape → Gleam shape →
  back to TS-shaped JSON).
- **`types_mod` qualified import in store.gleam.** `destroy_delegated_access_token`
  needs to construct an updated `DelegatedAccessTokenRecord` via the
  spread syntax. The unqualified-imported constructor lookup
  resolves the type at the construction site, but the spread needs
  the qualified type reference. Aliasing the module to `types_mod`
  on import (instead of the default `types`) avoids a name collision
  with another `types` symbol elsewhere in the file. Worth keeping
  in mind for handler ports — a top-level `types as types_mod`
  alias is clearer than `import gleam/_/types` everywhere.

### Risks / open items

- **No proxy handler yet.** Pass 15 is foundation only; the read
  path (6 query roots) and write path (9 mutation roots) ship
  separately. The store helpers are exercised only by the unit
  tests so far — first real use is the Pass 16 read path.
- **`upsert_base_app_installation` and `stage_app_installation`
  current-id semantics differ slightly from TS.** TS implicitly
  sets `currentAppInstallation` whenever the proxy mints its own;
  upstream-hydrated installations don't auto-promote. The Gleam
  port currently auto-promotes both flavors. Worth revisiting in
  Pass 16 once the handler is reading the store back — if the
  consumer ends up reading the wrong installation, `stage_app_installation`
  needs a "don't promote" variant (or the handler has to clear
  staged.current_installation_id before staging).
- **No `__meta/state` serialization for any apps slice.** Carries
  forward from Pass 13 (webhooks). The dispatcher works
  independently of meta-state; this is a gap for offline
  introspection, not a runtime gap.
- **`AppRecord.title` is `Option(String)` to model the upstream
  `nullable` schema, but the proxy's locally-minted default app
  always populates it.** Handler should use `Some("...")` directly
  in Pass 16; consumers should handle `None` only on hydration.

### Recommendation for Pass 16

Land the apps **read path** — the 6 query roots (`app`, `appByHandle`,
`appByKey`, `appInstallation`, `appInstallations`,
`currentAppInstallation`) plus `defaultApp` / `ensureCurrentInstallation`
helpers. Mirrors Pass 12's webhook-read shape. Should land:

- `proxy/apps.gleam` with a `process_query` entry point and the
  `default_app` / `ensure_current_installation` helpers.
- The serializers for each record type (`AppRecord`,
  `AppInstallationRecord`, `AppSubscriptionRecord`, etc.),
  including the `_typename` discrimination on the
  `AppSubscriptionPricing` sum.
- Connection serialization for `appInstallations` (one connection
  with the current installation) and for the
  `subscription.lineItems` / `lineItem.usageRecords` /
  `installation.allSubscriptions` / `installation.oneTimePurchases`
  child connections.
- Dispatcher wiring on the registry and legacy-fallback paths in
  `proxy/draft_proxy.gleam`.

Pass 17 takes the **write path** (9 mutation roots), which exercises
the lifted `mutation_helpers` for the first time outside webhooks.
Pass 18 takes hydration + meta-state serialization.

---

## 2026-04-29 — Pass 14: shared mutation_helpers module

Pure refactor. Lifts the AST-level required-argument validator, the
three structured-error builders, the `id`-only validator variant, and
the resolved-arg readers out of `proxy/webhooks.gleam` into a new
`proxy/mutation_helpers.gleam` module. `proxy/saved_searches.gleam`
now uses the shared `read_optional_string`. No behavior change — the
goal is to lock in the shape before domain #3 has to copy it.

### Module table

| Module                                                       | Lines | Notes                                                                                                                                                                                                                                                                                                                                       |
| ------------------------------------------------------------ | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/shopify_draft_proxy/proxy/mutation_helpers.gleam`       | +334  | New. Public surface: `RequiredArgument`, `validate_required_field_arguments`, `validate_required_id_argument`, `find_argument`, `build_missing_required_argument_error`, `build_null_argument_error`, `build_missing_variable_error`, `read_optional_string`, `read_optional_string_array`.                                                 |
| `src/shopify_draft_proxy/proxy/webhooks.gleam`               | −260  | Removed local copies of the validator + error builders + readers; `handle_delete` now calls `validate_required_id_argument` and destructures `#(resolved_id, errors)` instead of the local `DeleteIdValidation` record.                                                                                                                     |
| `src/shopify_draft_proxy/proxy/saved_searches.gleam`         | −10   | Removed local `read_optional_string`; imports the shared one.                                                                                                                                                                                                                                                                               |
| `test/shopify_draft_proxy/proxy/mutation_helpers_test.gleam` | +260  | New. 22 unit tests covering the validator (happy / missing / multi-missing-joined / null literal / unbound variable / null variable / bound variable), the id validator (literal / missing / null / bound variable / unbound variable), the three error-builder JSON shapes, and the readers (present / absent / wrong-type / list filter). |

Tests: 375 / 375 on Erlang OTP 28 and JavaScript ESM. Net +22
(353 → 375); all new tests are in the new module-level suite.

### What landed

The split between AST validation and resolved-arg-dict execution
that webhooks introduced in Pass 13 is the load-bearing structural
choice — only the AST distinguishes "argument omitted" from "literal
null" from "unbound variable", and each maps to a distinct GraphQL
error code (`missingRequiredArguments` / `argumentLiteralsIncompatible`
/ `INVALID_VARIABLE`). Pass 14 lifts that pair (validator + readers)
out of the domain handler so the next domain doesn't have to choose
between copying ~250 LOC or rolling its own envelope shape.

`validate_required_id_argument` is the small generalization:
in Pass 13 it lived in webhooks as `validate_webhook_subscription_delete_id`
returning a domain-specific `DeleteIdValidation` record. The lifted
version returns `#(Option(String), List(Json))` — the resolved id
when validation passed (so the caller can skip a second
`get_field_arguments` lookup), or an error list. Any future
`*Delete` mutation (apps, segments, …) can use it directly.

`find_argument` was made public — it's a small AST utility but
useful for handlers that need to inspect a specific argument node
after validation passed (e.g. a custom shape check on a known-present
input object). Pass 13's webhook handlers used it internally; making
it public costs nothing and saves the next caller from re-implementing
linear-list lookup.

`read_optional_string` and `read_optional_string_array` are pure
sugar over `dict.get` + variant matching, but they're the exact
readers both saved-searches and webhooks have copy-pasted. Lifting
them now blocks the third copy.

### Findings

- **The AST-vs-resolved split lifts cleanly.** No domain-specific
  glue leaked into the helpers; the abstractions are the same ones
  TS uses. `RequiredArgument(name, expected_type)` mirrors the
  TS `[name, expectedType]` tuple exactly, with the type string
  used verbatim in the error message.
- **Parallel saved-searches / webhooks envelopes preserved on
  purpose.** Saved-searches still uses semantic `userErrors` for its
  validation failures; webhooks uses the structured top-level error
  envelope. The two are _not_ unified because the TS source
  differentiates them — `saved-searches.ts` runs validation through
  a domain-specific `validate*` function that emits user errors,
  while `webhooks.ts` runs `validateRequiredFieldArguments` and emits
  top-level errors. The Gleam port mirrors the upstream divergence
  rather than fighting it.
- **The `dict.get` + ResolvedValue pattern is the only thing the
  readers need.** No source-of-truth indirection through `SourceValue`
  or the store — these helpers operate purely on resolved arg dicts.
  That keeps them dependency-light: any handler that has a resolved
  arg dict can use them, regardless of whether it's writing to staged
  state or reading from upstream.

### Risks / open items

- **No shared `read_optional_int` / `read_optional_bool` /
  `read_optional_object` yet.** Webhooks doesn't need them; saved-
  searches doesn't need them. The next domain might. Worth lifting
  on first reuse rather than speculatively now.
- **`__meta/state` still doesn't serialize webhook subscriptions.**
  Carried over from Pass 13 — the dispatcher works end-to-end, but
  the meta-state endpoint that consumers use for offline introspection
  only knows about `savedSearches`. Small follow-on for any pass
  that adds a meta-state consumer.
- **No structured `userErrors` builder yet.** Both domains hand-build
  their `{field, message}` shape inline. Symmetric to the top-level
  builders that just landed; lifting these would let a future domain
  emit consistent user-error envelopes without copying the JSON
  shape literal.

### Recommendation for Pass 15

Two viable directions:

1. **Webhook subscription hydration** (`upstream-hybrid` read path).
   This was option (1) in Pass 13's recommendation; Pass 14 taking
   the helper-unification path means option (1) is still the next
   big viability checkpoint. Pulls live records from Shopify and
   stages them locally — unlocks running the proxy against a real
   store.
2. **Start a new domain — `apps`** (`src/proxy/apps.ts`, ~967 LOC,
   6 query roots + 9 mutation roots, 6 record types in
   `state/types.ts:2336-2411`). Bigger surface than webhooks; would
   exercise the lifted helpers immediately and surface whatever
   second-pass abstraction opportunities they don't yet cover (e.g.
   `read_optional_int`, structured user-error builders).

Domain #3 has more signal: it forces the helpers to prove their
generality, and it's the next concrete viability checkpoint after
hydration. Hydration is the bigger user-visible feature.

---

## 2026-04-29 — Pass 13: webhook mutations

Closes the webhooks domain write path. Lands `process_mutation` plus
three handlers (`webhookSubscriptionCreate` / `Update` / `Delete`),
the AST-level required-argument validator that produces the structured
top-level error envelope TS uses (`extensions.code` =
`missingRequiredArguments` / `argumentLiteralsIncompatible` /
`INVALID_VARIABLE`), input readers + projection, mutation log
recording, and dispatcher wiring on both the registry and legacy
fallback paths.

### Module table

| Module                              | Lines | Notes                                                                                   |
| ----------------------------------- | ----- | --------------------------------------------------------------------------------------- |
| `proxy/webhooks.gleam`              | +600  | `process_mutation`, three handlers, validator, input readers, projection, log recording |
| `proxy/draft_proxy.gleam`           | +30   | `WebhooksDomain` mutation arm + `is_webhook_subscription_mutation_root` legacy fallback |
| `test/proxy/webhooks_test.gleam`    | +200  | 11 mutation tests (success, top-level errors, user errors, update/delete)               |
| `test/proxy/draft_proxy_test.gleam` | +50   | 3 end-to-end dispatcher tests for create/missing-topic/blank-uri                        |

353 tests on Erlang OTP 28 + JS ESM (was 339 prior to this pass). +14 net.

### What landed

**`process_mutation`** (`proxy/webhooks.gleam`)

Mirrors the TS `handleWebhookSubscriptionMutation` entry point.
Returns `Result(MutationOutcome, WebhooksError)`, where
`MutationOutcome` carries `data: Json` (the _complete envelope_),
the updated `Store`, the threaded `SyntheticIdentityRegistry`, and
`staged_resource_ids: List(String)`. Multiple mutation root fields
in one document are folded across; per-field
`MutationFieldResult { key, payload, staged_resource_ids,
top_level_errors }` accumulates into either a `{"data": {...}}` or
`{"errors": [...]}` envelope based on whether `top_level_errors` is
non-empty after the fold. This matches the TS short-circuit:
top-level argument-validation failures replace the whole payload;
per-field user errors live alongside successful sibling fields.

**Three handlers** (`handle_create`, `handle_update`, `handle_delete`)

Each takes the resolved field arguments + the staging store + the
identity registry and returns a `MutationFieldResult`. Shapes:

- **Create.** Resolves `webhookSubscription` input, validates URI
  (blank → `userErrors[{field: ["webhookSubscription", "callbackUrl"], message: "Address can't be blank"}]`),
  mints a synthetic gid (`gid://shopify/WebhookSubscription/N?shopify-draft-proxy=synthetic`),
  mints deterministic `created_at`/`updated_at` via
  `synthetic_identity.make_synthetic_timestamp`, populates a fresh
  `WebhookSubscriptionRecord` from the input, and stages it.
- **Update.** Resolves `id` + `webhookSubscription` input, looks up
  the existing record (`get_effective_webhook_subscription_by_id`),
  applies overrides via `apply_webhook_update_input` (using
  `WebhookSubscriptionRecord(..existing, ...)` to preserve fields
  not present in input — equivalent to TS's `{...existing, ...overrides}`),
  mints a fresh `updated_at`, and stages the merged record. Unknown
  id → user error.
- **Delete.** Validates the id is non-empty (top-level error if blank
  string literal), looks up the existing record, calls
  `delete_staged_webhook_subscription`. Unknown id → user error
  payload (`deletedWebhookSubscriptionId: null`).

**AST-level validator** (`validate_required_field_arguments`)

The TS helper inspects `field.arguments` (the AST) — _not_ the
resolved value dict — to distinguish three cases that all manifest
as "missing" downstream:

1. **Argument absent from AST** → `missingRequiredArguments` with
   the argument list joined by `, `.
2. **Argument present with literal `null` (`NullValue`)** →
   `argumentLiteralsIncompatible`, "Expected type 'X!'".
3. **Argument bound to a variable that is `null`/missing in the
   variables dict** → `INVALID_VARIABLE`, "Variable 'name' has not
   been provided" / "got invalid value null".

Mirrored by walking `Argument.value` against `NullValue`,
`VariableValue { name }` (with `dict.get(variables, name) ->
NullVal | Error(_)`), and "absent from list". The execution path
keeps using the resolved arg dict (`get_field_arguments`) — only
validation reads the AST.

**Dispatcher wiring** (`proxy/draft_proxy.gleam`)

Two arms added (mirrors Pass 12's read-path wiring):

```gleam
// capability path
Webhooks -> Ok(WebhooksDomain)
// legacy fallback
case webhooks.is_webhook_subscription_mutation_root(name) {
  True -> Ok(WebhooksDomain)
  False -> Error(Nil)
}
```

The `WebhooksDomain` arm in `route_mutation` calls
`webhooks.process_mutation(store, identity, path, query, variables)`,
re-records nothing if the call returns `Error(_)` (validator
internal failure surface), or records the resulting Json envelope
and forwards the new store / identity / staged ids on success.

### Findings

- **Top-level errors are envelope-shape, not status code.** Both
  successful payloads and validation failures are HTTP 200 — the
  difference is `{data: {...}}` vs `{errors: [...]}`. Holding the
  full envelope in `MutationOutcome.data` (rather than just the
  per-field payload) keeps the fold simple: append per-field errors
  to a single list, then emit one envelope at the end.
- **AST inspection is necessary, not optional.** Resolved-arg
  inspection cannot tell `null` apart from `undefined` from
  `unbound variable`. Each maps to a distinct GraphQL error code.
  The split between "validate against AST" and "execute against
  resolved dict" is small but load-bearing — same shape as TS.
- **`..existing` spread = TS object spread.** Field preservation in
  `apply_webhook_update_input` reads identically to JS:
  `WebhookSubscriptionRecord(..existing, uri: ..., name: ...)` is
  exactly `{...existing, uri: ..., name: ...}`. No helper needed.
- **Identity threading is uniform.** Both timestamp minting and gid
  minting flow through `SyntheticIdentityRegistry`; the registry
  threads back out of `MutationOutcome` so subsequent mutations see
  the incremented counter. Determinism preserved across multi-root
  documents.
- **Parallel implementation, not unification.** Saved-searches still
  emits the simpler `userErrors` flow (no top-level error envelope,
  no AST validator). Pass 12's recommendation flagged the choice;
  this pass kept them parallel because the TS handlers themselves
  diverge — saved-searches' `validateSavedSearchInput` returns
  `userErrors`, and only webhooks goes through
  `validateRequiredFieldArguments`. A future pass that unifies them
  must first decide whether to upgrade saved-searches to the
  structured form.

### Risks / open items

- **No `__meta/state` serialization for webhook subscriptions yet.**
  The dispatcher test confirms the mutation routes correctly via
  response body, but the in-store assertion lives in
  `webhooks_test`. Adding a `webhookSubscriptions` slice to the meta
  state serializer is small and should land alongside any consumer
  that wants to introspect staged webhook state from outside the
  store.
- **`Location` field is not emitted.** AST `Location` carries only
  character offsets, not line/column numbers; the `locations` field
  on the GraphQL error envelope is optional and we drop it. If a
  consumer ever asserts on it, we'll need to compute line/column
  from offsets.
- **`INVALID_VARIABLE` path for non-null variables.** Currently the
  validator only fires when the variable resolves to `null` /
  missing. The TS validator also catches type mismatches (e.g. an
  Int variable bound to a String literal). We don't validate types
  yet — that's a downstream-coercion concern, not a validation one,
  and the existing argument-resolver already handles common cases.
  Untested in either direction.
- **No log entry for top-level error mutations.** When validation
  fires, the per-field handler short-circuits before
  `record_mutation_log_entry` runs. TS records "failed" log entries
  for these; the Gleam port currently does not. Symmetric with
  saved-searches' "failed" entries (which the meta_log test
  exercises) — worth aligning.

### Recommendation for Pass 14

Two viable directions, ordered by signal-to-effort:

1. **Webhook subscription hydration** (`upstream-hybrid` read path).
   Pass 12 lands the read handler; the upstream-hybrid integration
   that pulls live records from Shopify and stages them locally is
   still TS-only. This unlocks running the proxy against a real
   store and is the next big viability checkpoint.
2. **Unify validator helpers + structured saved-search errors.**
   Lift `validate_required_field_arguments` and the input-reader
   helpers into a shared `proxy/mutation_helpers` module, and
   upgrade saved-searches to emit the same top-level error envelope
   as webhooks. Pure refactor — no new behavior, but locks in the
   shape before a third domain has to copy it.

The hydration path has more user-visible value but more surface
area; the helper unification is small and de-risks domain #3.

---

## 2026-04-29 — Pass 12: webhooks query handler + dispatcher wiring + store slice

Builds on Pass 11's substrate. Lands the read path for the webhooks
domain end to end: store slice, `handle_webhook_subscription_query`
implementing all three root payloads, and dispatcher wiring so an
incoming GraphQL request that names `webhookSubscription{,s,sCount}`
gets routed to the new module — both via the registry capability path
and the legacy fallback predicate. Mutations are still deferred to
Pass 13.

### Module table

| Module                           | Lines | Notes                                                                                                                                         |
| -------------------------------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `state/store.gleam`              | +130  | Webhook fields on Base/Staged + 5 accessors mirroring saved-search slice                                                                      |
| `proxy/webhooks.gleam`           | +280  | `handle_webhook_subscription_query`, `process`, root-field dispatch, projection helpers                                                       |
| `proxy/draft_proxy.gleam`        | +12   | New `WebhooksDomain` variant; capability + legacy fallback dispatch                                                                           |
| `test/state/store_test.gleam`    | +90   | 6 tests for the new webhook slice (upsert / staged-overrides / delete / list ordering / reset)                                                |
| `test/proxy/webhooks_test.gleam` | +135  | 8 query-handler tests (single, connection, count, topic filter, endpoint typename projection, uri fallback, legacyResourceId, root predicate) |

339 tests on Erlang OTP 28 + JS ESM (was 329 prior to this pass). +10 net.

### What landed

**Store slice** (`state/store.gleam`)

Three new fields each on `BaseState` and `StagedState`:

- `webhook_subscriptions: Dict(String, WebhookSubscriptionRecord)`
- `webhook_subscription_order: List(String)`
- `deleted_webhook_subscription_ids: Dict(String, Bool)`

Five accessors, mirroring the saved-search slice byte-for-byte:

- `upsert_base_webhook_subscriptions(store, records)` — base-state
  upsert that clears any deleted markers (in either base or staged)
  for the same id
- `upsert_staged_webhook_subscription(store, record)` — staged
  upsert; appends to the staged order list only if the record id
  isn't already known
- `delete_staged_webhook_subscription(store, id)` — drops the
  staged record and sets the staged deleted-marker
- `get_effective_webhook_subscription_by_id(store, id)` — staged
  wins over base; either side's deleted marker suppresses
- `list_effective_webhook_subscriptions(store)` — ordered ids first
  (deduped across base+staged), then unordered ids sorted by id

The pre-existing saved-search constructors on `BaseState`/`StagedState`
needed to switch from positional to `..base`/`..staged` spread
because the records grew new fields. No semantic change — the spread
just preserves the rest of the record.

**Query handler** (`proxy/webhooks.gleam`)

The TS `handleWebhookSubscriptionQuery` dispatches per-root-field;
the Gleam port mirrors that exactly with `root_payload_for_field`
matching against `webhookSubscription` / `webhookSubscriptions` /
`webhookSubscriptionsCount`. Each root produces:

- **Single:** `webhookSubscription(id:)` — looks the record up via
  `get_effective_webhook_subscription_by_id`, projects the supplied
  selection set; missing id or missing record both return `null`.
- **Connection:** `webhookSubscriptions(first/last/after/before, query, format, uri, topics, sortKey, reverse)` —
  list → field-arg filter → query filter → sort → paginate. Uses
  `paginate_connection_items` + `serialize_connection` from
  `graphql_helpers` (same plumbing the saved-search connection uses).
  Inline-fragment flattening on both `selectedFieldOptions` and
  `pageInfoOptions`, matching TS.
- **Count:** `webhookSubscriptionsCount(query, limit)` — no
  aggregator helper exists yet; the implementation walks the
  selection set directly and emits `count`/`precision` keys, with
  `precision` set to `AT_LEAST` when the unfiltered length exceeds
  the limit.

Projection: rather than wire projection-options through
`project_graphql_value` (which would have meant a new helper-API
parameter), the source dict is pre-populated with the
`uri`-with-fallback value, the legacy resource id, and a per-variant
endpoint sub-object that carries its `__typename`. This is how TS
`webhookProjectionOptions` injects `uri` — by the time the projector
walks the selection set, the override is already in the source dict.
Inline-fragment type conditions on `endpoint` then resolve via the
existing `defaultGraphqlTypeConditionApplies` path.

**Dispatcher wiring** (`proxy/draft_proxy.gleam`)

Three small additions:

1. New `WebhooksDomain` variant in the dispatcher's local `Domain`
   enum.
2. `Webhooks` arm in `capability_to_query_domain` (registry-driven
   path).
3. `is_webhook_subscription_query_root` arm in
   `legacy_query_domain_for` (no-registry fallback so existing tests
   without a loaded registry can still route webhook queries).

Mutation routing intentionally untouched in this pass — the mutation
arm in `mutation_domain_for` only knows `SavedSearches` for now and
falls through for everything else, which is the right behavior until
Pass 13.

### Findings

- **Projection options weren't needed.** The TS handler uses
  `webhookProjectionOptions` to swap in a fallback `uri` value at
  projection time. Pre-computing into the source dict gets us the
  same observable result for far less code. If a future endpoint
  needs more sophisticated dynamic field synthesis (e.g. a derived
  field whose value depends on the requested selection set), the
  projection helpers will need a hook — but the current bar is very
  low. **Recommendation:** keep deferring projection-options support
  until two consumers need it.
- **Sum types pay off in the projector.** `endpoint_to_source` is a
  three-line `case`; the TS equivalent is a `switch`-on-typename plus
  defensive `?? null` for each variant's optional payload. The Gleam
  variant guarantees the right fields exist on the right variants, so
  the projector emits exactly the keys GraphQL expects without runtime
  guards.
- **Store slice clones cleanly.** Adding a second resource type to
  `BaseState`/`StagedState` was mechanical — one `..spread` change in
  the existing saved-search constructors and the rest is new lines.
  This pattern will scale.
- **Dispatcher wiring is two-line per domain.** Once the handler
  exposes `process` + `is_<x>_query_root`, the dispatcher just needs
  one capability-arm and one legacy-fallback-arm. No domain-specific
  data flows back through the proxy — `Store` is threaded forward
  uniformly.

### Risks / open items

- **`limit` arg coercion.** TS does `Math.floor(rawLimit)` on a
  number; Gleam already enforces `IntVal` from JSON parsing, so the
  port doesn't need to coerce. If a test ever sends `limit: 1.5`
  through variables (FloatVal), the port treats it as no-limit. The
  TS path would coerce. Untested in either direction; flagged here for
  the Pass-13 review.
- **Sort key mismatch tolerance.** Both ports accept arbitrary
  strings and fall through to `Id`. Confirmed parity by
  `parse_sort_key("nonsense") == IdKey`.
- **Registry round-trip not exercised end-to-end.** No
  `webhookSubscriptions` registry entry is loaded in any test; the
  legacy fallback predicate is what the new tests hit. The capability
  path will start being exercised once the production registry JSON
  loads in `draft_proxy_test`. Not blocking — same pattern as
  saved-searches when it first landed.
- **Mutation handler gap.** Pass 13 needs to port
  `webhookSubscriptionCreate/Update/Delete` (~400 TS LOC) plus the
  argument validation helpers (`buildMissingRequiredArgumentError`
  etc.). The validation helpers are webhook-specific in TS but
  generic in shape — worth lifting to a shared module when porting.

### Recommendation for Pass 13

Webhook mutations. Target the same shape as saved-searches:
`process_mutation` returning a `MutationOutcome` (data + store +
identity + staged ids), three handlers (create/update/delete), and
shared input-reader / validator helpers. The TS `validateRequiredFieldArguments`
helper produces structured GraphQL errors with `extensions.code` and
`path`; the saved-search port currently emits simpler `userErrors` —
worth deciding whether to upgrade saved-searches to match or keep
parallel implementations until a consumer needs the structured form.

---

## 2026-04-29 — Pass 11: webhooks substrate (state types + URI marshaling + filter/sort)

First real consumer of Pass 10's `search_query_parser` and
`resource_ids` modules. Lands the **substrate slice** of the webhooks
domain: state types, URI ↔ endpoint marshaling, term matching, query
filtering, field-argument filtering, and sort key handling. The
GraphQL handler entry points (`handleWebhookSubscriptionQuery` /
`handleWebhookSubscriptionMutation`) and the store integration still
need to land in a follow-on pass (12) — but the pure substrate is now
testable and verifiable in isolation.

### Module table

| Module                           | Lines | Notes                                                                             |
| -------------------------------- | ----- | --------------------------------------------------------------------------------- |
| `state/types.gleam`              | +35   | `WebhookSubscriptionEndpoint` sum type (3 variants) + `WebhookSubscriptionRecord` |
| `proxy/webhooks.gleam`           | ~225  | URI marshaling, term matcher, filter+sort                                         |
| `test/proxy/webhooks_test.gleam` | ~370  | 32 tests covering URI round-trip, filters, sorting                                |

323 tests on Erlang OTP 28 + JS ESM (was 291 after Pass 10). +32 net.

### What landed

State types in `state/types.gleam`:

- `WebhookSubscriptionEndpoint` is a sum type with three variants
  (`WebhookHttpEndpoint(callback_url)`, `WebhookEventBridgeEndpoint(arn)`,
  `WebhookPubSubEndpoint(pub_sub_project, pub_sub_topic)`) — one variant
  per endpoint kind. Unrepresentable combinations (e.g. an HTTP
  endpoint with an ARN) are now compile errors. The TS schema is one
  record with all four optional fields plus a `__typename`
  discriminator; the Gleam variant carries only the fields its kind
  actually uses.
- `WebhookSubscriptionRecord` ports the eleven fields directly,
  with `Option(...)` for nullable slots and `List(String)` for
  `include_fields` / `metafield_namespaces` (which default to `[]`).

`proxy/webhooks.gleam`:

- `endpoint_from_uri(uri) -> WebhookSubscriptionEndpoint` — URI
  scheme dispatch (pubsub:// / arn:aws:events: / else → HTTP).
- `uri_from_endpoint(Option(endpoint)) -> Option(String)` — round-trips
  back to a URI when the endpoint carries the necessary fields.
- `webhook_subscription_uri(record)` — explicit `uri` field wins;
  falls back to `uri_from_endpoint(record.endpoint)`.
- `webhook_subscription_legacy_id(record)` — trailing GID segment
  (`gid://shopify/WebhookSubscription/123` → `"123"`).
- `matches_webhook_term(record, term) -> Bool` — positive-term matcher
  for `apply_search_query_terms`, with case-folded field dispatch
  covering `id` (exact match against full GID _or_ legacy id),
  `topic`, `format`, `uri` / `callbackurl` / `callback_url` /
  `endpoint`, `created_at` / `createdat`, `updated_at` / `updatedat`,
  and a no-field fallback that text-searches id+topic+format.
- `filter_webhook_subscriptions_by_query` — wires `matches_webhook_term`
  into `apply_search_query_terms` with `ignored_keywords: ["AND"]`.
- `filter_webhook_subscriptions_by_field_arguments(records, format, uri, topics)` —
  composable optional filters; when all three are `None` / `[]` the
  list is returned unchanged.
- `WebhookSubscriptionSortKey` enum (`CreatedAtKey | UpdatedAtKey |
TopicKey | IdKey`) plus `parse_sort_key` (case-insensitive, unknown
  values fall through to `IdKey`) and
  `sort_webhook_subscriptions_for_connection(records, key, reverse)`
  with stable tiebreak on the GID's numeric tail via
  `compare_shopify_resource_ids`.

### Findings

- **The first real consumer validates the substrate cleanly.** Both
  `search_query_parser` (`apply_search_query_terms`) and `resource_ids`
  (`compare_shopify_resource_ids`) plug into webhooks without any
  shape changes. The generic `fn(a, SearchQueryTerm) -> Bool` matcher
  pattern is exactly what was needed — `matches_webhook_term` matches
  that signature directly.
- **The `id` field's "exact-match-against-full-GID-OR-legacy-id"
  behavior is non-obvious.** A query like `id:1` matches a record
  with id `gid://shopify/WebhookSubscription/1` because the legacy
  id ("1") matches. This is an Admin GraphQL convention worth
  documenting in the file — the test `filter_by_query_id_exact_test`
  covers it.
- **Sum types beat the TS discriminator + optional-fields pattern.**
  TS expressed the three endpoint variants as one schema with all
  fields optional, then narrowed via `__typename` checks. The Gleam
  sum type makes each variant only carry the fields its kind needs,
  collapsing several runtime guards (e.g. `endpoint.callbackUrl ?? null`
  becomes pattern matching on `WebhookHttpEndpoint(callback_url: u)`).
- **`Option(String)` semantics for sort tiebreaks need explicit
  handling.** TS's `(left.createdAt ?? '').localeCompare(...)` collapses
  null and empty into the same bucket; the Gleam port uses
  `option.unwrap("", _)` + `string.compare` to match. Important when
  records have null timestamps (e.g. defaults, in-flight creates).
- **The pure-substrate scope was the right cut.** ~225 LOC of
  webhooks logic lands in one pass with full test coverage, no
  store integration, no GraphQL handler plumbing. The full 920-LOC
  TS module would not have fit in one pass without skipping
  test depth.

### Risks / deferred work

- **Mutations not yet ported.** `webhookSubscriptionCreate`,
  `webhookSubscriptionUpdate`, `webhookSubscriptionDelete` (~400 TS
  LOC) need a follow-on pass. They depend on input validation
  helpers, the synthetic-identity FFI, and store integration that
  isn't yet wired up.
- **No store integration yet.** `Store` doesn't have
  `list_effective_webhook_subscriptions` or
  `get_effective_webhook_subscription_by_id` accessors; the Pass 12
  store extension needs to add these.
- **No dispatcher wiring yet.** `draft_proxy.gleam` doesn't route
  `webhookSubscription{,s,sCount}` queries or the three mutations
  to this module. Pass 12 will register the `Webhooks` capability
  domain in `operation_registry` and add a dispatch path in
  `draft_proxy`.

### Recommendation

Pass 12 should land the remaining webhooks pieces:

1. Add `Webhooks` to `CapabilityDomain` in `operation_registry`.
2. Extend `Store` with `list_effective_webhook_subscriptions` and
   `get_effective_webhook_subscription_by_id`.
3. Port `handleWebhookSubscriptionQuery` (`webhookSubscription`,
   `webhookSubscriptions`, `webhookSubscriptionsCount` root payloads)
   using the now-landed `paginate_connection_items` and
   `serialize_connection` helpers.
4. Port the three mutation handlers + their validation helpers.
5. Wire dispatch in `draft_proxy.gleam` to delegate
   `Webhooks` domain operations to the new module.

That's another full-pass-sized chunk; Pass 12 might split into 12a
(query handler + store) and 12b (mutations + dispatch).

---

## 2026-04-29 — Pass 10: search-query parser + resource-id ordering substrate

Lands the two domain-agnostic substrate modules every domain handler
that exposes a `query: "..."` argument depends on. The TS source
`src/search-query-parser.ts` (483 LOC) ports to ~750 LOC of Gleam, and
`src/shopify/resource-ids.ts` (16 LOC) ports to ~50 LOC. Both modules
are now consumable by future domain ports (webhooks, products, orders,
customers — every domain that takes a `query`).

### Module table

| Module                                           | Lines | Notes                                                                |
| ------------------------------------------------ | ----- | -------------------------------------------------------------------- |
| `shopify_draft_proxy/search_query_parser.gleam`  | ~750  | Tokenizer + recursive-descent parser, generic match/apply helpers    |
| `shopify_draft_proxy/shopify/resource_ids.gleam` | ~50   | GID numeric ordering + nullable string compare                       |
| `test/search_query_parser_test.gleam`            | ~520  | 52 tests across term parsing, matching, term lists, parser, generics |
| `test/shopify/resource_ids_test.gleam`           | ~85   | 8 tests covering numeric/lexicographic/nullable ordering             |

291 tests on Erlang OTP 28 + JS ESM (was 239 after Pass 9). +52 net.

### What landed

`search_query_parser.gleam` mirrors the entire TS public surface:

- Sum types: `SearchQueryComparator` (5 variants), `SearchQueryTerm`,
  recursive `SearchQueryNode` (TermNode | AndNode | OrNode | NotNode),
  closed-enum `SearchQueryStringMatchMode`.
- Options records with `default_*` constructor functions:
  `SearchQueryParseOptions`, `SearchQueryTermListOptions` (collapsed
  from TS's two separate types — the simpler function ignores
  `drop_empty_values`), `SearchQueryStringMatchOptions`.
- Term parsing: `parse_search_query_term`, `consume_comparator`,
  `normalize_search_query_value`, `strip_search_query_value_quotes`,
  `search_query_term_value`.
- Match helpers: `matches_search_query_string` (with prefix `*`,
  word-prefix mode, exact/includes), `matches_search_query_number`
  (using `gleam/float.parse` with int fallback),
  `matches_search_query_text`, `matches_search_query_date` (using the
  existing `iso_timestamp.parse_iso` FFI; takes explicit `now_ms: Int`
  rather than introducing a `Date.now()` FFI).
- Tokenizer + recursive descent: `tokenize`, `parse_search_query`,
  `parse_or_expression`, `parse_and_expression`, `parse_unary_expression`.
- Generics: `matches_search_query_term`, `matches_search_query_node`,
  `apply_search_query`, `apply_search_query_terms` — all parametric
  over `a` with a positive-term matcher callback `fn(a, SearchQueryTerm) -> Bool`.

`resource_ids.gleam` provides:

- `compare_shopify_resource_ids(left, right) -> Order` — extracts the
  trailing integer from a GID and compares numerically; falls back to
  lexicographic compare when either side fails to parse. Returns
  `gleam/order.Order` directly so callers can hand it to `list.sort`
  unmodified, which is cleaner than the TS signed-integer convention.
- `compare_nullable_strings(left, right) -> Order` — explicit
  `Some(_) < None` ordering.

### Findings

- **Regex elimination kept the parser pure-stdlib.** The TS uses two
  regexes: `/:(?:<=|>=|<|>|=)?$/u` and `/[^a-z0-9]+/u`. Both are
  shallow patterns that unfold cleanly into chained `string.starts_with`
  / `string.ends_with` / character iteration. Avoiding `gleam/regexp`
  keeps the dependency footprint smaller and avoids a JS/Erlang
  regex-engine difference surface.
- **The recursive-descent parser is shorter in Gleam than expected.**
  Rather than threading a mutable index, every parser function returns
  `#(Option(SearchQueryNode), List(SearchQueryToken))`. Caller passes
  the consumed-token list in, gets the remaining tokens back. Pure
  data flow, no state record, ~120 LOC for the full Pratt-style cascade
  (`or → and → unary`).
- **Generics-with-callback fell out naturally.** TS's
  `SearchQueryTermMatcher<T>` ports to a plain `fn(a, SearchQueryTerm) -> Bool`
  parameter. Same shape, same call sites, no class wrappers.
- **`iso_timestamp.parse_iso` FFI from earlier passes was a free reuse.**
  Date matching just composes existing primitives — no new FFI.
- **Term parsing's "split on first colon" is `string.split_once`
  on the head, not a custom char walk.** Cleaner than the TS regex
  `/^([^:]*):(.*)/`.
- **`SearchQueryTermListOptions` collapsed two TS types into one.**
  TS had `SearchQueryTermListOptions` and `SearchQueryTermListParseOptions`
  with different fields. The Gleam port merges them and ignores
  `drop_empty_values` from the simpler entry point. Saves callers
  from constructing two record types.
- **`gleam/order.{Lt, Eq, Gt}` is the right return type for compare
  helpers** — `list.sort` consumes it directly. The TS signed-integer
  pattern would have been a needless adapter.

### Risks

- **`matches_search_query_date` requires the caller to plumb `now_ms`
  through.** This is more correct than embedding `Date.now()` (it
  makes the matcher pure and testable), but it's a behavioral
  divergence from TS where `now` was implicit. Any future domain that
  uses date matching has to thread a clock value down.
- **`apply_search_query_terms` ignores `drop_empty_values`.** Mirrors
  the TS `parseSearchQueryTerms` behavior, but the merged-record
  shape is a little surprising — a future caller might wrongly
  expect `drop_empty_values: True` to take effect for the term-list
  entry point. The doc comment flags this; long-term, adding a
  `default_term_list_parse_options()` constructor that omits the
  field would tighten the contract.
- **The substrate is in place but no domain consumes it yet.** Until
  a domain like webhooks or products lands a `query: "..."` filter
  that calls `apply_search_query`, this module's value is latent.
  The next pass should be a real consumer.

### Recommendation

Pass 11 candidates, ranked:

1. **Webhooks domain (~920 TS LOC)** — well-bounded, single resource
   type with subscription state, exercises `apply_search_query`
   for `webhookSubscriptions(query: "...")`, plus the existing
   capability/connection/store substrate. The cleanest first
   real-domain consumer of the search parser.
2. **Products domain** — biggest blast radius, will exercise more
   of the connection/edge substrate, but the metafield/file
   substrate already landed. Probably too large for one pass.
3. **Orders domain** — depends on customer + line-item substrate
   that hasn't fully landed. Hold for later.

Pass 11 should likely be webhooks.

---

## 2026-04-29 — Pass 9: registry-driven dispatch (capability wiring)

Wires Pass 8's capabilities into `draft_proxy.gleam`'s dispatcher. With
a registry attached, query and mutation routing now go through
`capabilities.get_operation_capability` and key off the `domain` enum;
without a registry, the legacy hardcoded predicates still work — so
existing tests keep passing while new code can opt in.

### Module table

| Module                              | Change                                                                                                                                      |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `proxy/draft_proxy.gleam`           | +`registry` field + `with_registry` setter; `query_domain_for` / `mutation_domain_for` try capability first, fall back to legacy predicates |
| `test/proxy/draft_proxy_test.gleam` | +3 tests covering capability-driven dispatch with a synthetic 3-entry registry                                                              |

231 tests on Erlang OTP 28 + JS ESM (was 228).

### What landed

- `DraftProxy.registry: List(RegistryEntry)` — defaults to `[]` so
  `proxy.new()` keeps the Pass 1–8 behavior; `proxy.with_registry(r)`
  attaches a parsed registry.
- Capability resolution is the _first_ check in dispatch. When the
  registry is non-empty and `get_operation_capability` returns a
  recognised domain (`Events`, `SavedSearches`, `ShippingFulfillments`),
  routing keys off it. When the registry is empty _or_ the capability
  is `Unknown`, the dispatcher falls through to
  `legacy_query_domain_for` / `legacy_mutation_domain_for` (the old
  predicate-based code).
- Three tests exercising the new path:
  - `registry_drives_query_dispatch_test` — `events` query routes via
    `Events` capability.
  - `registry_drives_mutation_dispatch_test` — `savedSearchCreate`
    mutation routes via `SavedSearches` capability.
  - `registry_unknown_root_falls_back_to_400_test` (poorly named —
    actually verifies `productSavedSearches` continues to succeed via
    legacy fallback when the synthetic registry doesn't include it).

### Findings

- **Belt-and-braces dispatch is the right migration shape.** Keeping
  the legacy fallback meant zero existing tests broke. Once every
  consumer site loads the production registry, the fallback can come
  out — but until then the cost of dual-mode dispatch is one extra
  case per resolution path. Cheap.
- **Registry-driven and predicate-driven dispatch reach the same
  result for shared roots.** `events` resolves to the same handler in
  both paths. The migration's not changing behavior, just where the
  decision lives.
- **The synthetic test registry is small (3 entries).** Tests don't
  need the full 666-entry production registry to exercise the
  capability-driven path. Keeps the test isolated and fast — and
  documents the minimum entry shape for future domain ports to
  reference.

### Risks unchanged / new

- **`product*SavedSearches` family still relies on the legacy
  predicate**, because the synthetic test registry doesn't include
  them. Production deployment with the full registry will move them
  to the capability path; the legacy fallback exists for safety.
- **`with_registry` is opt-in.** Real consumers must remember to call
  it. A future pass should add a `from_config` constructor that
  loads + parses the JSON in one shot, so attaching the registry is
  the default.

### Recommendation

Pass 10 candidates:

1. Add a JS/Erlang FFI loader so `from_config(path) -> DraftProxy`
   reads the registry and attaches it in one call. Wires the proxy
   for "real" use without leaving registry plumbing on the consumer.
2. Port the next small read-only domain. `markets`, `localization`,
   and `online-store` are all under 1k LOC in TS; any of them
   exercises the capability dispatcher with a fresh consumer.
3. Begin the customers slice — substantial, but the substrate
   (metafields, capabilities, connection helpers) is now in place.

I'd take option 1 first — it's a tiny, mechanical change that
removes the test-vs-production discrepancy in how the registry gets
loaded, and unblocks every subsequent domain pass from having to
think about loader plumbing.

---

## 2026-04-29 — Pass 8: operation-registry + capabilities

Substrate port. `src/proxy/operation-registry.ts` (67 LOC) loads the
6642-line `config/operation-registry.json` and exposes
`findOperationRegistryEntry` + `listImplementedOperationRegistryEntries`.
`src/proxy/capabilities.ts` (61 LOC) consumes it to map a parsed
operation onto a `(domain, execution, operationName)` triple — the
dispatch decision the proxy uses to decide whether to handle a query
locally, stage a mutation, or fall through to the upstream API.

This pair is foundational: every future domain handler that wants to
participate in the registry-driven router needs both modules in place.
Until now we've been hardcoding `is_saved_search_query_root`-style
predicates in `draft_proxy.gleam`; landing capabilities lets a future
pass replace those with a single registry walk.

### Module table

| Module                                     | LOC | Status                          |
| ------------------------------------------ | --- | ------------------------------- |
| `proxy/operation_registry.gleam`           | 220 | New: parser + lookup helpers    |
| `proxy/capabilities.gleam`                 | 165 | New: `get_operation_capability` |
| `test/proxy/operation_registry_test.gleam` | 120 | 9 tests                         |
| `test/proxy/capabilities_test.gleam`       | 165 | 10 tests                        |

228 gleeunit tests passing on Erlang OTP 28 and JS ESM (was 209). The
production registry JSON (666 entries) decodes cleanly through the
Gleam parser, verified via a one-shot Node script that imports the
compiled module.

### What landed

- `OperationType` (Query | Mutation), `CapabilityDomain` (26 explicit
  variants + Unknown), and `CapabilityExecution` (OverlayRead |
  StageLocally | Passthrough) sum types. The variants are 1:1 with the
  TS `CapabilityDomain` and `CapabilityExecution` unions; we map
  kebab-case JSON values (e.g. `"admin-platform"`) to Gleam
  PascalCase constructors via a closed `parse_domain` table.
- `RegistryEntry` record with all 8 fields (`name`, `type_`, `domain`,
  `execution`, `implemented`, `match_names`, `runtime_tests`,
  `support_notes`). `support_notes` uses
  `decode.optional_field("supportNotes", None, decode.optional(...))`
  so the field can be missing or null — both branches converge on
  `None`.
- `parse(json: String) -> Result(List(RegistryEntry), DecodeError)`.
  Decodes the full 6642-line config file in one shot. Validates closed
  enums (domain, execution, type) and rejects malformed inputs at the
  decode boundary, matching the TS `operationRegistrySchema.parse(...)`
  contract.
- `find_entry(registry, type_, names)` — first-match-wins lookup that
  walks `names` in order, skipping `None` and empty strings, returning
  the first registry entry whose type matches and whose
  `match_names` contains the candidate. Mirrors TS behavior exactly.
- `list_implemented(registry)` — filters out `implemented: false`
  entries.
- `OperationCapability { type_, operation_name, domain, execution }`
  in `capabilities.gleam`. The `get_operation_capability` function
  reproduces the TS resolution algorithm:
  1. Find first root field whose match-name resolves to an implemented
     entry of the right type.
  2. Otherwise, walk all candidates (root fields + operation name,
     deduplicated, order-preserving).
  3. If matched, prefer the operation's declared `name` over the
     matched candidate iff both resolve to the same registry entry —
     this is the `operationNameEntry` cleverness in `capabilities.ts`.
  4. Fall back to `(Unknown, Passthrough)` with `op.name ?? rootFields[0]`
     when nothing matches.

### What's deferred

- **Loader / FFI shim.** TS uses `import …json with { type: 'json' }`
  to bake the registry into the bundle. Gleam doesn't have a portable
  static-import mechanism for JSON, so the parsing API takes a string
  the consumer reads at startup. A target-specific loader (Node's `fs`
  on JS, `file` on Erlang) belongs in a separate module — not
  blocking.
- **Wiring `get_operation_capability` into the dispatcher.** Right
  now `draft_proxy.gleam` checks `is_saved_search_query_root`
  directly. The next step is to load the registry once at boot and
  replace the predicate with a capability lookup. Held to keep this
  pass focused on the substrate.
- **Caching/indexing.** TS builds a `Map<matchName, entry>` at module
  load. Gleam version walks the (~666-entry) implemented list per
  call — fine for now, easy to upgrade to a `dict.Dict` if dispatch
  shows up in profiles.

### Findings

- **`gleam/json` + `gleam/dynamic/decode` is the right shape for this.**
  The decoder reads almost identically to a Zod schema:
  ```gleam
  use name <- decode.field("name", decode.string)
  use type_ <- decode.field("type", operation_type_decoder())
  ...
  decode.success(RegistryEntry(...))
  ```
  Closed-enum decoding via `decode.then(decode.string)` + a `case`
  expression is more verbose than Zod's `z.enum([...])` but compiles
  to a tighter check (the variant enumeration is exhaustive at the
  type level, so adding a new domain in the JSON without updating
  `parse_domain` is caught by the decoder, not at runtime).
- **`decode.optional_field` semantics differ from `decode.field`.**
  `optional_field("k", default, inner)` returns `default` only when the
  key is _absent_. To also accept explicit `null`, the inner decoder
  must be `decode.optional(...)`, which itself returns `None` for
  null. The combination handles both shapes.
- **Operation-name resolution is delicate.** The `operationNameEntry`
  rule in TS — "prefer `op.name` over the matched root field iff
  both point to the same registry entry" — is easy to mis-port. The
  test `prefers_root_field_over_operation_name_test` covers this:
  with `name: "Product"` + `rootFields: ["product"]`, both resolve to
  the `product` entry, and the operation name wins.
- **No need for IO/effect modeling.** Splitting the parser
  (`parse(input: String)`) from the loader avoids cross-target IO
  entirely. The library is pure; consumers do their own string IO.
  This is the same pattern the GraphQL parser uses
  (`parser.parse(source)` is pure; the request body is read by the
  HTTP shim).
- **Real-world JSON validates.** Verified by compiling the module to
  JS, then `node -e 'parse(readFileSync(...))'` against the production
  config. All 666 entries pass; no decoder rejections. This is a
  meaningful viability signal — the JSON schema (with optional
  `supportNotes`, closed-enum domain/execution) maps cleanly to Gleam
  sum types without escape hatches.

### Risks unchanged / new

- **Adding a new domain requires updating Gleam code.** Closed enums
  catch typos at decode time, but every new domain in the JSON now
  needs a Gleam variant. The TS port has the same constraint — both
  the union type and the JSON schema enum need updating — but in
  Gleam the cost is also a `parse_domain` case branch. Acceptable;
  the alternative (string-typed domain) loses exhaustiveness on the
  consumer side.
- **Memory cost of carrying the full registry.** 666 entries × ~8
  small fields each is negligible (probably <100KB on each runtime).
  No risk; flagged only because we'd previously raised it as a
  concern.

### Recommendation

Pass 9 should wire the capability lookup into `draft_proxy.gleam`'s
dispatch. Currently `route_query` / `route_mutation` check
`saved_searches.is_saved_search_query_root` directly. Replacing that
with a capability lookup gives us the registry-driven dispatch the TS
proxy uses, and it's a small change — load the registry once at
boot, thread it through `dispatch_graphql`, and replace the predicate
with `case capability.domain { SavedSearches -> ... ; _ -> ... }`.

This unblocks adding new domains: each domain just registers its
handlers; the dispatcher routes by capability without further
modifications.

After that, picking up another small read-only domain (events is
already half-done; `delivery-settings`, `markets`, `localization` are
next-smallest) becomes a copy-and-adapt exercise rather than a
plumbing exercise.

---

## 2026-04-29 — Pass 7: metafields read-path substrate

Substrate port. `src/proxy/metafields.ts` is imported by 7 different
domain modules (`admin-platform`, `customers`, `metafield-definitions`,
`products`, `online-store`, `payments`, `store-properties`). Porting
the read-path subset now means future domain ports — products,
customers, and the smaller stores below them — get a working
projection helper for free.

The mutation paths (`upsertOwnerMetafields`, `normalizeOwnerMetafield`,
`mergeMetafieldRecords`, `readMetafieldInputObjects`) were
deliberately deferred because they depend on
`src/proxy/products/metafield-values.ts` (360 LOC of value
normalization + JSON shape coercion) which is its own port.

### Module table

| Module                             | LOC | Status                                                                              |
| ---------------------------------- | --- | ----------------------------------------------------------------------------------- |
| `proxy/metafields.gleam`           | 188 | New: `MetafieldRecordCore`, compare-digest builder, projection + connection helpers |
| `test/proxy/metafields_test.gleam` | 130 | 11 unit tests                                                                       |

209 gleeunit tests passing on Erlang OTP 28 and JS ESM (was 198).

### What landed

- `MetafieldRecordCore` record with the same 10 fields the TS type
  declares. Optional fields (`type_`, `value`, `compare_digest`,
  `json_value`, `created_at`, `updated_at`, `owner_type`) are
  `Option(...)` so callers can pass through whatever shape the
  upstream record holds.
- `make_metafield_compare_digest` — `draft:` prefix + base64url of a
  6-element JSON array `[namespace, key, type, value, jsonValue,
updatedAt]`. Mirrors `Buffer.toString('base64url')` semantics
  (no padding) using `bit_array.base64_url_encode(_, False)`.
- `serialize_metafield_selection_set` — projects a metafield record
  onto a list of selection nodes. All 12 fields the TS handler
  recognizes (`__typename`, `id`, `namespace`, `key`, `type`,
  `value`, `compareDigest`, `jsonValue`, `createdAt`, `updatedAt`,
  `ownerType`, `definition`) plus the `null` default.
- `serialize_metafield_selection` — convenience wrapper around the
  selection-set projector.
- `serialize_metafields_connection` — connection-shaped serialization
  with cursor = `id` and pagination via the existing
  `paginate_connection_items`. Variables are threaded through, so
  paginated reads via `$first` / `$after` work end-to-end (already
  exercised in Pass 6 for saved searches).

### What's deferred

- **Mutation path** (`upsertOwnerMetafields`, `normalizeOwnerMetafield`,
  `mergeMetafieldRecords`, `readMetafieldInputObjects`): blocked on
  `metafield-values.ts` (360 LOC: `parseMetafieldJsonValue`,
  `normalizeMetafieldValue`, type-shape coercion table). Can land
  before any consumer domain's mutation pass needs it.
- **Owner-scoped wrapping** (`OwnerScopedMetafieldRecord<OwnerKey>` in
  TS): the TS type adds an owner ID under a string-keyed property
  (e.g. `productId: "..."`). In Gleam we'll likely model this as the
  consumer wrapping `MetafieldRecordCore` in a record that adds the
  owner field, rather than parametric polymorphism over key names.
- **Definition lookup** (`'definition'` case): TS returns null too,
  but only because the read-path doesn't have access to definitions.
  Eventually `metafield-definitions.gleam` will own this and the
  serializer here will route to it.

### Findings

- **Read-path projection translates very cleanly.** ~100 LOC TS →
  ~150 LOC Gleam. The biggest verbosity tax was on `Option(String)`
  unwrapping for `null` cases in the JSON output — TS's `?? null`
  collapses to a tiny ternary, Gleam's pattern match needs an
  explicit `Some(s) -> json.string(s)` / `None -> json.null()`.
  Net cost: one extra helper (`option_string_to_json`) used 6 times.
- **`bit_array.base64_url_encode` matches `Buffer.toString('base64url')`
  exactly.** Including the no-padding behavior. No FFI needed; the
  digest survives JSON round-trip identically on both targets.
- **`json.array` requires a transformer fn even when the items are
  already `Json`.** Slight ergonomic friction (`fn(x) { x }`) but
  type-safe — the API is consistent with `list.map`-style helpers.
- **Test setup is tedious for `Selection` values.** The cleanest way
  to construct a real `Selection` for the projection test is to
  parse a query string and pull the root field. We don't have an
  AST builder/literal syntax. Acceptable — every test is one line of
  `first_root_field("{ root { ... } }")` plumbing.
- **The connection helper is genuinely reusable.** `paginate_connection_items`
  - `serialize_connection` did not need any modification to support
    the new metafields shape. This is the same helper saved-searches
    uses, and it slotted in for metafields with no friction. Strong
    evidence that the substrate's connection abstraction is correctly
    factored.

### Risks unchanged / new

- **Field-projection inconsistency between domains.** Saved-searches
  uses an explicit per-field `case` in `project_saved_search`;
  metafields uses the same pattern. As more domains land, the
  per-field projection table will grow large. Worth considering a
  helper that takes a `dict.Dict(String, fn(record) -> Json)` and
  walks selections — but only if the duplication starts hurting.
- **`compareDigest` alignment with TS is unverified.** The Gleam
  output uses the same algorithm but I haven't compared a digest
  side-by-side with TS. Adding a parity test against a known TS
  output would close this; deferred until consumers actually rely on
  the digest.
- **`Option(Json)` for `json_value` is awkward.** `gleam/json` doesn't
  expose a `Json` value that round-trips through dynamic data — once
  you've built a `Json`, you can serialize it to a string but you
  can't introspect it. Carrying it as `Option(Json)` works for our
  read-only path, but the mutation port will need a different shape
  (probably `Option(JsonValue)` defined as an enum mirroring
  `gleam_json`'s constructors).

### Recommendation

Pass 8 should validate the metafields helper from a real consumer
context. The cheapest validation: extend `saved_searches` with a
synthetic `metafields(...)` connection (saved searches don't
actually expose them in TS — pure validation harness), or pick the
smallest real consumer and port a slice. Given saved_searches is
already comfortable territory, picking up `metafield-definitions`
(1550 LOC) or a thin slice of `customers` is the next signal-rich
move.

Alternatively, the `operation-registry` + `capabilities` pair
(67 + 61 LOC plus the 6642-line config JSON) would unblock
capability-based dispatch — necessary for any domain whose
`handleQuery`/`handleMutation` methods key off the registry. But
loading 310 KB of JSON cleanly across both targets requires either
codegen or a config-injection pattern; not blocking, but worth
factoring deliberately.

I'd pick a slice of `customers` next (~50-80 LOC of real handler
code, exercising `MetafieldRecordCore` + projection in context).

---

## 2026-04-29 — Pass 6: GraphQL variables threading

Pure-substrate widening between two domain ports. The dispatcher used
to assume every operation was self-contained (inline arguments only);
this pass widens the request body parser to accept
`{ query, variables? }` and threads the resulting
`Dict(String, root_field.ResolvedValue)` from the dispatcher down
through `route_query` / `route_mutation` into every saved-searches
handler. The arg resolver and AST already supported variables — only
the request-body parser, the dispatcher plumbing, and the call sites
into `root_field.get_field_arguments` were missing.

### Module table

| Module                                 | LOC delta | Status                                                               |
| -------------------------------------- | --------- | -------------------------------------------------------------------- |
| `proxy/draft_proxy.gleam`              | +25       | Variables decoder + threading                                        |
| `proxy/saved_searches.gleam`           | +14       | Variables on every public + private handler                          |
| `test/proxy/saved_searches_test.gleam` | +3        | Updated 3 call sites with `dict.new()`                               |
| `test/proxy/draft_proxy_test.gleam`    | +37       | 3 new tests covering create-with-vars, query-with-vars, omitted-vars |

198 gleeunit tests passing on Erlang OTP 28 and JS ESM.

### What landed

- A recursive `decode.Decoder(root_field.ResolvedValue)` that
  enumerates every JSON-shaped value (bool / int / float / string /
  list / object) with a `decode.success(NullVal)` fallback. Uses
  `decode.recursive` to defer construction so the inner closure can
  refer to itself, and `decode.one_of` to try each shape in order.
  Order is bool → int → float → string → list → dict → null because
  on Erlang `false` is `0` for some primitive checks; bool-first
  makes the union unambiguous.
- `parse_request_body` extended via `decode.optional_field` so a body
  without `variables` defaults to `dict.new()`. Existing tests
  (which all omit `variables`) keep passing untouched.
- `dispatch_graphql` carries the new `body.variables` into both
  branches; `route_query` and `route_mutation` grow a
  `variables: Dict(String, root_field.ResolvedValue)` parameter.
- `saved_searches.process` / `process_mutation` /
  `handle_saved_search_query` / `serialize_root_fields` /
  `serialize_saved_search_connection` / `list_saved_searches` /
  `handle_mutation_fields` / `handle_create` / `handle_update` /
  `handle_delete` all thread variables; the four call sites that
  previously passed `dict.new()` now pass the actual map.

### What's deferred

- **Multi-pass arg resolution.** TS resolves arguments once at the
  dispatcher and re-uses the dict; this port still calls
  `get_field_arguments` per handler. Functionally equivalent, just
  redundant work. Worth inlining when we land another mutation
  domain that re-walks the same field.
- **Operation name selection.** A document with multiple operations
  needs `operationName` to choose; `parse_operation` currently picks
  the first. Not yet a problem for proxy traffic (the recorded
  parity requests all have one operation each), but it'll need to be
  threaded the same way variables now are.

### Findings

- **`decode.recursive` works exactly the way you'd want.** No
  trampolining or thunking required at call sites — the inner
  closure is invoked lazily. This was the part I was most worried
  about; it took ~10 lines.
- **`decode.one_of` is the right primitive for sum-type-shaped JSON.**
  The error semantics (return the first matching decoder, otherwise
  bubble up the very first failure) compose cleanly with
  `decode.success` as a default branch.
- **The dispatcher signature is starting to feel heavy.** Both
  `route_query` and `route_mutation` now take 5+ parameters; the
  saved-searches mutation handlers take 7. The pattern works, but
  another pass that adds a parameter (e.g. `operationName`,
  request id, fragments cache) probably warrants a `Dispatch`
  context record. Not blocking; a code-shape signal.
- **Existing tests caught zero regressions.** The 195 previously-
  passing tests all continued to pass after threading without any
  test edits beyond updating the 3 direct call sites in
  `saved_searches_test.gleam`. The substrate factoring is healthy.
- **Test coverage for the new path is shallow.** I added three new
  tests (variables-driven create, variables-driven query with
  pagination + reverse, omitted-variables fallback) but every other
  saved-searches test still exercises only the inline-args path.
  Consider widening at least one read-path test per query field if
  variables become the dominant client pattern.

### Risks unchanged / new

- **No coercion of variable types.** GraphQL spec says a variable
  declared `Int!` should reject a JSON `"1"`; we accept whatever the
  JSON object literally holds. This matches the TS proxy (which
  also relies on `JSON.parse` types), but if a Shopify client ships
  a variant that depends on coercion the proxy will diverge silently.
- **Default values from variable definitions are not honored.** If a
  query declares `query Q($limit: Int = 10)` and the request omits
  `limit`, the AST default is ignored — the variable resolves to
  `NullVal` and the handler falls back to its own default. Matches
  `resolveValueNode`'s `?? null` semantics so we're spec-aligned with
  TS, but worth documenting if a real divergence shows up.
- **`decode.optional_field` only handles missing keys, not explicit
  null.** A body with `"variables": null` will fail decoding instead
  of defaulting to empty. None of the parity-recorded requests do
  this; flagging in case a real client does.

### Recommendation

Pass 7 should be the next domain port — pick a small, read-only
substrate consumer to keep momentum. The two cheapest options:

1. **`shopAlerts` / `pendingShopAlerts`** — single-field read, no
   pagination, no store coupling. Probably ~80 LOC including tests.
2. **`metafieldDefinitions` connection** — exercises the connection
   helpers in a different shape (not saved-search defaults, real
   schema-driven records) and pressure-tests the variables path
   under a non-trivial argument set (`namespace`, `key`, `ownerType`).

Either is a self-contained domain port with no new substrate work.
After that, the long pole is `customers` — both because customer
records are 50+ fields and because `customerCreate` / `customerUpdate`
exercise the full mutation envelope (including userErrors with
nested input paths).

---

## 2026-04-29 — Pass 5: savedSearchUpdate + savedSearchDelete

Closed the saved-search write-path domain. With create from Pass 4
already in place, this pass added `savedSearchUpdate` and
`savedSearchDelete`, exercising the full pattern: input-id resolution
against staged records, validation that drops invalid keys instead of
rejecting the whole input, and identity-tagged log entries on both
success and failure. Saved searches is now the first fully-ported
write-capable domain in Gleam. 195 gleeunit tests pass on both
`--target erlang` and `--target javascript` (6 new mutation
integration tests).

### What is additionally ported and working

| Module                            | LOC   | TS counterpart                      |
| --------------------------------- | ----- | ----------------------------------- |
| `proxy/saved_searches` (extended) | ~1110 | `proxy/saved-searches` (CRUD, ~75%) |
| `test/.../draft_proxy_test`       | ~585  | parity tests (CRUD coverage)        |

Update flow: read input, resolve `input.id` via
`store.get_effective_saved_search_by_id` (staged-wins-over-base);
validate without `requireResourceType` (since the existing record
already carries a resource type); on validation errors strip the
offending `name` / `query` keys via `sanitized_update_input` and
re-merge the survivors with the existing record; payload either
echoes the freshly-merged record or, when sanitization rejected
everything, the existing record unchanged. Delete flow: same id
resolution, then `store.delete_staged_saved_search` if found,
projecting `deletedSavedSearchId` as the input id on success or null
on validation failure.

`make_saved_search` was generalised to accept
`existing: Option(SavedSearchRecord)`, threading the existing record's
`id` / `legacyResourceId` / `cursor` / `resourceType` through
unchanged when present, and falling back to the input or fresh
synthetic gid when absent. `build_create_log_entry` was renamed to
`build_log_entry` and parametrised on root-field name so create,
update, and delete share one log-entry constructor that produces the
right `rootFields` / `primaryRootField` / `capability.operationName`
/ `notes` for each.

The dispatcher in `handle_mutation_fields` now dispatches all three
saved-search root fields (`savedSearchCreate`,
`savedSearchUpdate`, `savedSearchDelete`); the `MutationOutcome`
record was already shaped to thread store + identity + staged ids
back to the dispatcher, so adding two more handlers was a 3-line
match-arm change plus the handlers themselves.

### What is deliberately deferred

- **GraphQL variables threading.** Mutation inputs are still inline
  literals — `parse_request_body` only extracts `query`. The next
  domain that needs variable inputs (or an `ID!` argument referenced
  from a JSON variable) will want this widened first.
- **The full search-query parser.** Updates that override `query`
  still ship `searchTerms` = raw query and `filters: []`; structured
  filter behaviour lands when the parser ports.
- **`hydrateSavedSearchesFromUpstreamResponse`.** Live-hybrid only.

### Findings

- **The CRUD pattern lands cleanly under the existing substrate.**
  Once create existed, update + delete were ~150 LOC of handler each
  with no new helpers — input id resolution is just
  `store.get_effective_saved_search_by_id`, sanitized input is a
  `dict.delete` fold over the validation errors, and the
  `MutationOutcome` record absorbed the new staged/failed mix without
  new fields.
- **`Option(SavedSearchRecord)` + `case existing { Some(...) -> ...
None -> ... }` reads better than the TS `??` fallback chain.**
  Each field of the merged record has its own explicit fallback
  expression instead of a chained `?? existing?.field ?? ''`. The
  handful of extra lines is worth the readability.
- **Sharing `project_create_payload` between create and update was
  natural** — both project `{ savedSearch, userErrors }` and the
  variant differs only in whether `record_opt` falls back to
  `existing` (update) or `null` (create). Re-using the same projector
  with an `Option`-typed argument means the GraphQL projection
  pipeline (selection sets, fragments, `__typename`) only lives in
  one place.
- **Static defaults are not in the staged store, so they cannot be
  deleted.** A delete against a static-default id surfaces the same
  "Saved Search does not exist" user error as a delete against an
  unknown id. This matches the TS handler's behaviour: deletes only
  affect records that have been staged or hydrated into base state.
  Captured as a deliberate test case so future regressions are
  caught.

### Risks unchanged / new

- **The synthetic-id counter advances per mutation regardless of
  outcome.** A failed create still mints a `MutationLogEntry` gid;
  a failed delete also mints one. This is fine but worth keeping in
  mind when tests assert specific id values across multiple mutations
  in one proxy lifetime.
- **GraphQL variables remain absent.** The next mutation domain that
  takes anything beyond a primitive id+name+query input will need
  variables threading first; deferring it cost ~5 LOC of test
  ergonomics here (escaped-quote string literals) and won't scale.
- **`state/store.ts` still has ~5450 LOC unported.** Each subsequent
  domain pass eats into this; the saved-search slice is now load-
  bearing under a CRUD workload, which validates the dict-of-records
  - parallel order-list pattern for other domains.

### Recommendation

The next pass should be GraphQL variables threading. Cheap (~50 LOC
of substrate widening), unblocks every meaningful mutation domain
beyond saved searches, and stays in pure substrate territory before
the next domain port. Concretely: extend `parse_request_body` to
accept an optional `variables` object (decoded as
`Dict(String, Json)` then converted to
`Dict(String, root_field.ResolvedValue)`), thread the dict through
`dispatch_graphql` → `route_query` / `route_mutation` → handler →
`root_field.get_field_arguments`. The decoder + arg-resolver already
support variables; only the request-body parser and dispatcher
plumbing are missing.

After variables: pick a write-capable domain that touches enough of
the store to force a second store slice. `customers` is a good
candidate (write surface includes `customerCreate`, `customerUpdate`,
`customerDelete`, with rich nested input shapes that need variables

- store coverage; the read path also pages, so the pagination
  substrate gets re-exercised).

---

## 2026-04-29 — Pass 4: store slice + savedSearchCreate mutation

Picked up the long pole identified at the end of Pass 3: ported the
saved-search slice of `state/store.ts` plus the mutation log, threaded
a `Store` through `DraftProxy`, wired the saved-search read path to
the store, and ported `savedSearchCreate` end-to-end. The first
write-path domain is now alive in Gleam — staged records flow through
mutations, the meta routes (`/__meta/log`, `/__meta/state`,
`/__meta/reset`) reflect real state, and a subsequent
`orderSavedSearches(query: ...)` query surfaces the freshly-staged
record. 189 gleeunit tests pass on both `--target erlang` and
`--target javascript`.

### What is additionally ported and working

| Module                            | LOC  | TS counterpart                               |
| --------------------------------- | ---- | -------------------------------------------- |
| `state/types`                     | ~35  | `state/types` (saved-search slice)           |
| `state/store`                     | ~350 | `state/store` (saved-search slice + log)     |
| `proxy/saved_searches` (extended) | ~860 | `proxy/saved-searches` (read + create, ~60%) |
| `proxy/draft_proxy` (extended)    | ~590 | dispatcher: store-threaded, mutation route   |

`state/store` ports the saved-search slice of `BaseState` /
`StagedState` (the maps, the order arrays, and the
`deleted_saved_search_ids` markers), plus the mutation log:
`OperationType`, `EntryStatus`, `Capability`, `InterpretedMetadata`,
`MutationLogEntry`. Operations: `new`, `reset`,
`upsert_base_saved_searches`, `upsert_staged_saved_search`,
`delete_staged_saved_search`, `get_effective_saved_search_by_id`
(staged-wins-over-base, deleted-marker-suppresses),
`list_effective_saved_searches` (ordered ids first, then unordered
sorted by id), `record_mutation_log_entry`, `get_log`. The Gleam port
returns updated `Store` records from every mutator instead of
mutating in place.

`proxy/saved_searches` extends with `savedSearchCreate`:
`MutationOutcome` record threading `data` + `store` + `identity` +
`staged_resource_ids`; `is_saved_search_mutation_root` predicate;
`process_mutation` dispatcher; full validation pipeline (input
required; name non-blank, ≤40 chars; query non-blank; resource type
required, supported, and `CUSTOMER` deprecated); proxy-synthetic
gid + log entry minted via the synthetic-identity registry; record
upserted as staged; log entry recorded with status `Staged` on
success or `Failed` on validation errors.

`proxy/draft_proxy` now owns a `Store` field, threads it through
every dispatch, threads `MetaReset` through both
`synthetic_identity.reset` and `store.reset`, and routes mutations
via a new `route_mutation` arm that consumes the saved-search
`MutationOutcome` to update both the store and the synthetic-identity
registry. The `/__meta/log` and `/__meta/state` responses now
serialize real store data — a regression sentinel against the
empty-state placeholders Pass 2 shipped.

### What is deliberately deferred

- **`savedSearchUpdate` and `savedSearchDelete`.** Both follow the
  same shape as create but need synthetic-gid → input-id resolution
  against staged records. Bundled as a single follow-up pass.
- **The full search-query parser** (`src/search-query-parser.ts`,
  ~480 LOC). Newly-created records ship `searchTerms` = raw query
  string and `filters: []`; this matches the TS handler's output for
  records the parser hasn't run against yet, so the round-trip is
  faithful. Still load-bearing for the next read-path domain that
  actually needs structured filters.
- **GraphQL variables threading.** The dispatcher's
  `parse_request_body` only extracts `query`, not `variables`. The
  saved-search mutation tests therefore use inline arguments. A
  separate pass will widen `parse_request_body` and thread variables
  into `root_field.get_field_arguments`.
- **`hydrateSavedSearchesFromUpstreamResponse`.** Live-hybrid only,
  needs upstream response shapes; the rest of the live-hybrid plumbing
  is still ahead of the read mode.

### Findings

- **Threading immutable `Store` through the dispatcher with
  record-update syntax (`Store(..s, base_state: new_base, …)`) is the
  right ergonomics.** Each store mutator returns a fresh `Store`; the
  call sites read like the TS class but with explicit threading.
  `MutationOutcome` carries store + identity + staged ids back from
  each handler so the dispatcher does not have to reach into multiple
  return values.
- **`MutationOutcome` record beats tuples for cross-domain
  contracts.** When the dispatcher needs to thread three pieces of
  state back from a handler (next store, next identity, staged ids)
  on top of a `Json` data envelope, a named record reads cleanly and
  scales — when other domains add their own mutation handlers they
  can return the same record without growing the dispatcher's match
  arms.
- **Module/parameter name shadowing was the only real surprise.** A
  function parameter named `store: Store` and a module imported as
  `shopify_draft_proxy/state/store` collide on field-access syntax —
  `store.list_effective_saved_searches(store)` parses as field access
  on the value. Resolved by importing the function directly:
  `import shopify_draft_proxy/state/store.{type Store,
list_effective_saved_searches}`. Worth keeping in mind for every
  module whose name overlaps with the natural parameter name.
- **Extracting `state/types.gleam` for `SavedSearchRecord` /
  `SavedSearchFilter` was necessary** to break a cycle between
  `state/store` and `proxy/saved_searches`. The TS layout puts these
  in `state/types.ts` for the same reason; the Gleam version follows
  suit.
- **Synthetic identity threading exposes counter-coupling between
  identity-using functions.** Every gid mint advances the
  `next_synthetic_id` counter, so mutations that mint _both_ a
  resource gid _and_ a log-entry gid produce predictable id pairs
  (`SavedSearch/1`, `MutationLogEntry/2`). Tests can lean on this
  determinism, but any reordering of mints inside a handler will
  shift downstream ids. The TS version has the same property; the
  Gleam port preserves it.

### Risks unchanged / new

- **`state/store.ts` is 5800 LOC**, of which ~350 LOC ported here
  cover the saved-search slice. The next ~5450 LOC will land
  slice-by-slice as their domains port. The pattern (Dict for
  records, parallel order list, deleted-id marker) is now proven and
  re-usable.
- **The search-query parser is still a self-contained 480-LOC
  port** that several domains will want. Now load-bearing on
  saved-search update/delete reaching full parity (input id
  resolution against staged records is itself fine, but tests will
  want structured `filters` to assert on).
- **The dispatcher does not yet thread GraphQL variables.** The next
  mutation domain that takes non-trivial input shapes (anything with
  a list, or any `ID` argument referencing prior staged state) will
  want variables threading first. Cheap to do — `parse_request_body`
  becomes a 4-line widening — but worth doing as its own pass so the
  domain handlers can assume variables are present.

### Recommendation

The store substrate is now proven. Three credible next passes:

1. **Saved-search update + delete.** Closes the saved-search domain.
   Forces synthetic-gid → input-id resolution against staged records,
   which every other write-path domain will need. ~150 LOC of handler
   plus tests, no new substrate.
2. **GraphQL variables threading.** ~50 LOC to widen
   `parse_request_body` and `root_field.get_field_arguments`. Strict
   prerequisite for any non-trivial mutation handler. Pure substrate.
3. **`search-query-parser.ts` port.** ~480 LOC of stand-alone
   parser. Unblocks structured filter behaviour across saved searches,
   products, orders. No state coupling.

Pick (1) for a finished domain milestone — saved searches becomes the
first fully-ported write-capable domain, demonstrating the full
write-path pattern (validate → mint identity → upsert staged → log).
Pick (2) if the next domain after saved searches needs variables.
Pick (3) if widening read-surface speed is the priority.

---

## 2026-04-29 — Pass 3: pagination machinery + saved_searches read path

Forced the connection-pagination port by picking `saved_searches` as
the next domain. The TS handler is 643 LOC; this pass ports the
read path against static defaults only — store-backed CRUD and the
search-query parser are deferred. 171 gleeunit tests pass on both
`--target erlang` and `--target javascript`.

### What is additionally ported and working

| Module                             | LOC  | TS counterpart                           |
| ---------------------------------- | ---- | ---------------------------------------- |
| `proxy/graphql_helpers` (extended) | ~700 | `proxy/graphql-helpers` (~70%)           |
| `proxy/saved_searches`             | ~310 | `proxy/saved-searches` (read path, ~30%) |
| `proxy/draft_proxy` (extended)     | ~360 | dispatcher branch added                  |

`proxy/graphql_helpers` now has the full pagination pipeline:
`paginate_connection_items`, `serialize_connection`,
`serialize_connection_page_info`, `build_synthetic_cursor`, plus the
supporting `ConnectionWindow(a)`, `ConnectionWindowOptions`,
`ConnectionPageInfoOptions`, and `SerializeConnectionConfig(a)`
records. `proxy/saved_searches` ports the static `ORDER` and
`DRAFT_ORDER` defaults (4 and 5 entries respectively), the
`matchesQuery` substring filter, the `reverse` argument, and the
9-way root-field → resource-type mapping.

### What is deliberately deferred

- **The store-backed list/upsert/delete flow.** The Gleam store
  is not yet ported, so user-staged saved searches don't surface and
  mutations return a 400. Lifted only when the store lands.
- **The full search-query parser** (`src/search-query-parser.ts`,
  ~480 LOC). Stored `query` strings are not split into structured
  `searchTerms` / `filters` here; static defaults already carry the
  shape they need (empty `searchTerms` and `filters` on the
  port-shipping records). When the parser ports, hydration of
  upstream payloads becomes possible.
- **`hydrateSavedSearchesFromUpstreamResponse`.** Live-hybrid only,
  needs the store and the parser.

### Findings

- **Generic `serialize_connection<T>` translated cleanly via a
  configuration record.** The TS function takes a wide options object
  with several callbacks; in Gleam a `SerializeConnectionConfig(a)`
  record with named fields reads better than a positional argument
  list and avoids the explosion the spike worried about. Pattern
  match on selection name (`nodes` / `edges` / `pageInfo`) inside the
  helper, dispatch to caller-supplied `serialize_node` for projection.
- **`ConnectionPageInfoOptions` defaults via record-update syntax
  (`ConnectionPageInfoOptions(..default(), include_cursors: False)`)
  is the right ergonomic for connection options.** It lets per-call
  overrides stay obvious and lets the defaults move centrally.
- **Threading `ResolvedValue` from `root_field` into pagination
  was the right call** rather than reinventing JSON-ish source values
  for argument reading. `paginate_connection_items` accepts
  `Dict(String, ResolvedValue)` (matching the TS variables shape) and
  re-uses `root_field.get_field_arguments` to pull `first/last/after/
before/query/reverse` out of the field. No duplicate decoder.
- **Adding a domain stays a 5-minute, two-file change** even now that
  the dispatcher has a connection-shaped domain in it. The
  `domain_for` lookup composes cleanly with
  `saved_searches.is_saved_search_query_root` (the TS predicate
  ports verbatim).
- **`project_graphql_object` carried the saved-search node shape
  without modification.** Passing the record through `src_object` →
  `project_graphql_value` produced byte-identical JSON to the TS
  output (verified against the integration-test expectations) for
  `__typename`, `legacyResourceId`, nested `filters { key value }`,
  aliases, fragment spreads, and inline fragments.

### Risks unchanged / new

- **Store remains the long pole** and is now blocking saved-search
  _mutations_ and _staged reads_. The next bottleneck-driven domain
  port should be one whose read path also exercises the store, so we
  can stop kicking the can on `state/store.ts`.
- **The search-query parser is a self-contained 480-LOC port** that
  several domains will want (saved searches, products, orders). It's
  worth doing as a stand-alone pass before the third domain that
  needs it — the alternative is building the same scaffolding three
  times.

### Recommendation

The substrate now covers: routing, parsing, projection, pagination,
connection serialisation, fragment inlining, and synthetic identity.
That is enough to port any _read-only_ domain with non-trivial
defaults. The next pass should either (a) port `state/store.ts`
slice-by-slice, starting with the saved-search slice so this domain
can reach full parity, or (b) port `search-query-parser.ts` so the
read paths that depend on it (products, orders) can land
search-filter behaviour without the store landing first. Pick (a) if
you want a finished domain; pick (b) if you want to widen the read
surface fastest.

---

## 2026-04-29 — Pass 2: meta routes, projection helper, second domain

Extended the spike with the rest of the meta routes, the projector
that almost every domain handler depends on, and a second
read-only domain to validate the dispatcher extension pattern.

### What is additionally ported and working

| Module                             | LOC  | TS counterpart                                        |
| ---------------------------------- | ---- | ----------------------------------------------------- |
| `proxy/graphql_helpers` (extended) | ~340 | `proxy/graphql-helpers` (~40%)                        |
| `proxy/draft_proxy` (extended)     | ~340 | `proxy-instance` + `proxy/routes` (meta + dispatcher) |
| `proxy/delivery_settings`          | ~90  | `proxy/delivery-settings`                             |

`proxy/graphql_helpers` now has `project_graphql_object`,
`project_graphql_value`, and `get_document_fragments` — the recursive
selection-set projector that almost every domain handler is built
on. `proxy/draft_proxy` now routes `/__meta/health`, `/__meta/config`,
`/__meta/log`, `/__meta/state`, `/__meta/reset`, plus a clean two-line
extension point per new domain (`Domain` sum type +
`domain_for(name)` lookup). 133 gleeunit tests pass on both
`--target erlang` and `--target javascript`.

### Findings reinforced

- **The projection helper port was straightforward.** Inline-fragment
  type-condition gating, fragment-spread inlining, list element-wise
  projection, `nodes`-from-`edges` synthesis, and aliases all
  translated without surprises. The `SourceValue` sum type
  (`SrcNull | SrcString | SrcBool | SrcInt | SrcFloat | SrcList |
SrcObject`) is the Gleam analogue of TypeScript's
  `Record<string, unknown>` and reads cleanly in handler code.
- **Adding a new domain is now a 5-minute, two-file change.** Port
  the TS handler to Gleam (typically a thin wrapper around
  `project_graphql_object` over a default record), add a `Domain`
  variant in `draft_proxy.gleam`, extend `domain_for`. The
  `delivery_settings` handler took longer to write tests for than to
  port. This is exactly the property the rest of the port needs.
- **The dispatcher's `respond` helper unifies error paths cleanly.**
  Each domain returns `Result(Json, _)` from its `process` function
  and the dispatcher wraps it in either a 200 or a 400 with a
  uniform error envelope. Adding more domains does not multiply
  error-handling code.

### Findings unchanged

The store + types remains the long pole. Pagination machinery
(`paginateConnectionItems`, `serializeConnection` with cursors) is
the next non-trivial helper that will need a real port — `events`
dodged it via the empty-connection specialisation, and
`delivery_settings` doesn't paginate at all. `saved_searches` is the
natural next step to force the pagination port.

---

## 2026-04-28 — Pass 1: end-to-end viability spike

A first viability spike has run end-to-end through Gleam: HTTP-shaped
request → JSON body parse → custom GraphQL parser → operation summary
→ events-domain dispatcher → empty-connection serializer → JSON
response. 98 gleeunit tests pass on both `--target erlang` and
`--target javascript`. The port is concrete enough now to surface real
strengths and risks rather than speculate.

### What is ported and working

| Module                           | LOC  | TS counterpart                               |
| -------------------------------- | ---- | -------------------------------------------- |
| `graphql/source` + `location`    | ~80  | `language/source`, `location`                |
| `graphql/token_kind` + `token`   | ~70  | `language/tokenKind`, `tokenKind`            |
| `graphql/character_classes`      | ~60  | `language/characterClasses`                  |
| `graphql/lexer`                  | ~530 | `language/lexer`                             |
| `graphql/ast`                    | ~140 | `language/ast` (executable subset)           |
| `graphql/parser`                 | ~720 | `language/parser`                            |
| `graphql/parse_operation`        | ~100 | `graphql/parse-operation`                    |
| `graphql/root_field`             | ~200 | `graphql/root-field`                         |
| `state/synthetic_identity` + FFI | ~180 | `state/synthetic-identity`                   |
| `proxy/graphql_helpers` (slice)  | ~110 | `proxy/graphql-helpers` (15%)                |
| `proxy/events`                   | ~80  | `proxy/events`                               |
| `proxy/draft_proxy` (skeleton)   | ~190 | `proxy-instance` + `proxy/routes` (skeleton) |

Roughly **2.5K LOC of Gleam** replacing roughly the same TS surface,
with FFI proven on both targets via the ISO timestamp helpers.

### Strengths

- **Sum types + exhaustive matching catch GraphQL shape bugs at
  compile time.** Adding a new `Selection` variant (e.g.
  `InlineFragment`) makes every consumer fail to compile until it
  decides what to do — exactly the property the proxy needs to keep
  null-vs-absent handling honest.
- **`Result`-threaded parsing replaces graphql-js's mutable lexer
  cleanly.** The recursive descent reads as well as the TS original;
  the immutable state threading didn't add meaningful boilerplate
  beyond `use … <- result.try(…)`.
- **Cross-target parity is real.** Every test passes on both BEAM and
  JS, including FFI-bound timestamp formatting. The platform-specific
  cost was small (one `.erl` + one `.js` file, ~10 lines each).
- **Public API translates 1:1.** `process_request(request) ->
(response, proxy)` mirrors the TS `processRequest`, with the
  registry threaded explicitly to preserve immutability — no design
  compromise required.

### Risks and open questions

- **Store + types is the long pole.** `src/state/store.ts` is 5800
  lines with 449+ methods; `src/state/types.ts` is 2800 lines of
  resource record definitions. This is the single biggest porting
  cost and was deliberately deferred in the spike. It will dominate
  the calendar; the events handler skipped the store entirely because
  events are read-only and always empty in the proxy. Most other
  domains will not have that escape hatch.
- **Deep generic helpers like `serializeConnection<T>` need a different
  shape in Gleam.** The TS version takes callbacks (`serializeNode`,
  `getCursorValue`) and is reused across every connection-shaped
  field. In Gleam, parametric polymorphism handles this, but the
  number of arguments grows quickly; the spike sidestepped by
  specializing for the empty-items case. For real domains we'll need
  a more carefully designed connection helper, possibly with a
  configuration record instead of positional callbacks.
- **Mutable-API ergonomics.** Threading the proxy through every call
  is correct but verbose. The right pattern long-term is probably a
  `gleam_otp` actor that owns the registry + store, with handlers
  that send messages — but that's only worth introducing when there's
  enough state to justify it. For now the explicit threading is fine
  and matches Gleam idioms.
- **No date/time stdlib.** ISO 8601 formatting requires FFI; this is
  per-target boilerplate that scales linearly with the number of
  date/time operations. Manageable, but a friction point.
- **Block strings, descriptions, schema definitions deliberately
  omitted from the parser.** Operation documents in
  `config/parity-requests/**` don't use them — but if any future
  Shopify client introduces block string arguments the parser will
  need extending. Documented as a known gap in `lexer.gleam` /
  `parser.gleam`.

### Recommendation

Continue the port. The substrate is sound; the GraphQL parser is the
hardest subjective port (4 of the 12 substrate modules) and it landed
without surprises. The next bottleneck is mechanical: porting
`state/types.ts` resource records and the corresponding slices of
`state/store.ts`, one domain at a time. Start with `delivery-settings`
or `saved-searches` — both are small and have minimal store coupling
— before tackling `customers` or `products`.
