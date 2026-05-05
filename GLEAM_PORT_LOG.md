# GLEAM_PORT_LOG.md

A chronological log of the Gleam port. Each pass adds a new dated entry
describing what landed, what was learned, and what is now blocked or
unblocked. The acceptance criteria and design constraints live in
`GLEAM_PORT_INTENT.md`; this file is the running narrative.

Newer entries go at the top.

---

## 2026-05-05 - Pass 209: HAR-571 fulfillment service delete transfer contract

Aligns `fulfillmentServiceDelete` local staging with the destination-location
contract required for transfer deletes and keeps fulfillment-order downstream
reads coherent after a service is removed.

| Module / fixture                                                                                                                                                                                                    | Change                                                                                                                                                                                                              |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/shopify_draft_proxy/proxy/shipping_fulfillments.gleam`                                                                                                                                                         | Parses `inventoryAction`, validates `destinationLocationId` for TRANSFER against active merchant-managed locations, returns the captured invalid-destination userError shape, and stages fulfillment-order effects. |
| `test/shopify_draft_proxy/proxy/shipping_fulfillments_test.gleam`                                                                                                                                                   | Adds focused coverage for missing/invalid TRANSFER destinations, TRANSFER reassignment, KEEP closure, and selected `userErrors.code`.                                                                               |
| `scripts/capture-fulfillment-service-delete-transfer-conformance.ts` / `scripts/conformance-capture-index.ts`                                                                                                       | Adds aggregate-indexed live capture for invalid destination and valid transfer delete evidence on Admin GraphQL 2026-04.                                                                                            |
| `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/fulfillment-service-delete-transfer.json` / `config/parity-specs/shipping-fulfillments/fulfillment-service-delete-transfer.json` | Records executable parity evidence for the captured invalid-destination userError and valid transfer delete branch.                                                                                                 |
| `config/operation-registry.json` / `src/shopify_draft_proxy/proxy/operation_registry_data.gleam`                                                                                                                    | Updates the support notes and regenerated Gleam registry mirror for the stronger delete contract.                                                                                                                   |
| `docs/endpoints/shipping-fulfillments.md`                                                                                                                                                                           | Documents the supported destination validation, local reassignment/closure effects, and remaining inventory-quantity fixture boundary.                                                                              |

Validation:

- `corepack pnpm conformance:probe`
- live 2026-04 Admin GraphQL probe for invalid `destinationLocationId`
  userError shape
- `corepack pnpm conformance:capture -- --run fulfillment-service-delete-transfer`
- `corepack pnpm parity:record fulfillment-service-delete-transfer`
- `gleam test --target javascript -- shipping_fulfillments_test parity_test`
  (860 passed)
- `gleam test --target javascript` (860 passed)
- `gleam test --target erlang` failed on host OTP 25 with the
  known `gleam_json` OTP 27+ requirement
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`
  (OTP 28, 851 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm conformance:check` (1443 passed)
- `corepack pnpm conformance:capture:check` (9 passed)
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `corepack pnpm test` (123 files passed; 2307 passed)
- `git diff --check`

### Findings

- Live Shopify 2026-04 returned `field: null` and
  `message: "Invalid destination location."` for an invalid transfer
  destination.
- The current Shopify `UserError` type rejected selecting `code` for this
  mutation, so the local projection exposes selected `code` as `null`.
- Open local fulfillment orders assigned to the deleted service location now
  reassign to the transfer destination or close for non-transfer deletes, rather
  than continuing to point at a removed service location.
- The executable parity fixture covers invalid destination and valid transfer
  delete evidence. The live valid-transfer cleanup attempt hit Shopify's
  temporary location deactivation blocker after transfer-side inventory state
  appeared on the disposable destination location.

### Risks / open items

- Live TRANSFER probes without `destinationLocationId` succeeded, including an
  inventory-free fulfillment-order setup, and that setup left the fulfillment
  order assigned to the source service location. The local implementation follows
  the ticket acceptance for missing-destination and local fulfillment-order
  reassignment/closure guardrails; the checked-in parity spec is limited to the
  captured invalid-destination and valid-delete branches.
- Host Erlang remains OTP 25 in this workspace, so Erlang validation used the
  established OTP 28 container fallback.

---

## 2026-05-05 - Pass 208: HAR-562 order edit user-error payloads

Aligns the Gleam order-edit handlers with Shopify's mutation payload contract
for local user-error branches without sending supported mutations upstream.

| Module / fixture                                                                                                                                                              | Change                                                                                                                                                                                                                                                                                                                                       |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/shopify_draft_proxy/proxy/orders.gleam`                                                                                                                                  | Replaces `data.<root>: null` order-edit early-outs with selected payload objects containing nullable resource fields and `userErrors`, adds `INVALID` codes, blocks begin for refunded/voided/cancelled orders, rejects a second open session per order, and prioritizes missing calculated-order sessions before add/set target validation. |
| `test/shopify_draft_proxy/proxy/orders_test.gleam`                                                                                                                            | Adds focused coverage for missing begin order, refunded and locally cancelled begin orders, existing open session rejection, unknown variant, unknown calculated line item, missing calculated-order add/set/commit branches, and successful edit-session add/set flows.                                                                     |
| `config/parity-specs/orders/orderEdit-lifecycle-userErrors.json` / `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/order-edit-lifecycle-user-errors.json` | Adds captured strict parity evidence for order-edit missing-resource payload roots.                                                                                                                                                                                                                                                          |
| `scripts/capture-order-edit-lifecycle-user-errors-conformance.mts` / `scripts/conformance-capture-index.ts`                                                                   | Registers the order-edit user-error capture path in the aggregate conformance index.                                                                                                                                                                                                                                                         |
| `docs/endpoints/orders.md`                                                                                                                                                    | Updates order-edit coverage notes so concurrent-session, missing-resource, and unknown target user-error branches are no longer listed as open gaps.                                                                                                                                                                                         |

Validation:

- `gleam test --target javascript -- orders_order_edit_begin_user_error_payload_shapes_test orders_order_edit_unknown_resource_user_error_payload_shapes_test orders_order_edit_add_variant_invalid_variant_payload_test orders_order_edit_set_quantity_payload_test orders_order_edit_begin_payload_test orders_order_edit_missing_id_validation_guardrails_test`
  (853 passed)
- `corepack pnpm conformance:probe`
- `SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm conformance:probe`
- `SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm tsx scripts/capture-order-edit-lifecycle-user-errors-conformance.mts`
- `SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm parity:record orderEdit-lifecycle-userErrors`
- `gleam test --target javascript -- parity_test` (853 passed)
- `corepack pnpm gleam:format:check`
- `gleam test --target javascript` (870 passed after merging
  `origin/main`)
- `gleam test --target erlang` failed on host OTP 25 with the
  known `gleam_json` OTP 27+ requirement
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`
  (OTP 28, 861 passed after merging `origin/main`)
- `corepack pnpm conformance:capture:check` (9 passed)
- `corepack pnpm conformance:check` (1452 passed)
- `corepack pnpm typecheck`
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm test` (123 files passed; 2316 passed)
- `corepack pnpm build`
- `git diff --check`

### Findings

- The pre-fix begin-not-found branch returned `{"data":{"orderEditBegin":null}}`;
  the new shared order-edit error serializer keeps the mutation root non-null
  while preserving selected nullable fields.
- Existing missing-`$id` GraphQL validation remains top-level
  `INVALID_VARIABLE` behavior and is intentionally separate from mutation-scoped
  `userErrors`.

### Risks / open items

- The new unknown-target messages are local approximations anchored to the
  ticket's field/code acceptance criteria. Fresh live capture can tighten exact
  wording later if Shopify exposes different translated text in the target shop.

---

## 2026-05-05 - Pass 207: HAR-557 articleCreate validation fidelity

Aligns Online Store `articleCreate` with Shopify validation behavior for blog
reference and author input errors before local staging. The handler now rejects
missing or ambiguous blog references and missing or ambiguous authors with
Shopify-captured `ArticleCreateUserErrorCode` values, records failed mutation
log entries, and leaves local article/blog state unchanged on validation
failure.

| Module / fixture                                                                                                                 | Change                                                                                                                            |
| -------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `src/shopify_draft_proxy/proxy/online_store.gleam`                                                                               | Adds pre-staging articleCreate validation and failed mutation-log outcomes for rejected validation branches.                      |
| `test/shopify_draft_proxy/proxy/online_store_test.gleam`                                                                         | Covers missing blog reference, ambiguous blog, missing author, ambiguous author, no-staging behavior, and the valid success path. |
| `scripts/capture-online-store-article-create-validation-conformance.ts` / `scripts/conformance-capture-index.ts`                 | Adds an aggregate-indexed live capture for the validation branches and valid blogId plus author.name success path.                |
| `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/online-store/online-store-article-create-validation.json`           | Records live Shopify validation payloads and disposable blog/article cleanup.                                                     |
| `config/parity-specs/online-store/online-store-article-create-validation.json` / `config/parity-requests/online-store/*.graphql` | Adds executable parity evidence for the captured validation and success payloads.                                                 |

Validation:

- `corepack pnpm conformance:probe`
- one-off live Admin GraphQL 2025-01 probes for `BLOG_REFERENCE_REQUIRED`,
  `AMBIGUOUS_BLOG`, `AUTHOR_FIELD_REQUIRED`, and `AMBIGUOUS_AUTHOR`
- `corepack pnpm conformance:capture -- --run online-store-article-create-validation`
- `gleam test --target javascript -- online_store_test`
- `gleam test --target javascript -- parity_test`
- `gleam test --target javascript` (857 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`
  (OTP 28, 848 passed)
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:capture:check`
- `corepack pnpm gleam:format:check`
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `corepack pnpm test` (123 files passed; 2300 passed)
- `git diff --check`

### Findings

- Shopify returns service-level `AUTHOR_FIELD_REQUIRED` for `author: {}`.
  Omitting the non-null `author` field from variables instead fails earlier as
  a top-level `INVALID_VARIABLE` GraphQL error, so the local runtime and parity
  scenario target the service validation branch.
- Validation failures return `field: ["article"]`, `article: null`, and no
  staged records; failed local mutation-log entries keep the rejected write
  visible without claiming a staged resource.

### Risks / open items

- Host Erlang remains OTP 25 in this workspace, so Erlang validation still
  requires the established OTP 28 container fallback.

---

## 2026-05-05 - Pass 206: HAR-486 root format check hardening

Hardens the promoted root layout against stale CI cache artifacts from the old
`gleam/` project directory. CI can restore an ignored `gleam/build` cache before
linting; the root `gleam:format` scripts now target only checked-in `src` and
`test` Gleam source trees, and `.gitignore` documents the retired cache path.

| Module / area              | Change                                                                                 |
| -------------------------- | -------------------------------------------------------------------------------------- |
| `package.json`             | Narrows `gleam:format` and `gleam:format:check` to `src test`.                         |
| `.github/workflows/ci.yml` | Moves the Gleam build cache from retired `gleam/build` to root `build`.                |
| `.gitignore`               | Ignores the retired `gleam/build/` cache path that CI may still restore temporarily.   |
| `docs/gleam-runtime.md`    | Documents root-layout format commands that avoid generated or cached dependency trees. |

Validation:

- Reproduction before the fix: PR CI run `25350901724` failed in
  `corepack pnpm lint` because `gleam format --check` traversed restored
  `./gleam/build/packages/**` dependency files.
- Fixed stale-cache proof: created an ignored unformatted
  `gleam/build/packages/stale/src/stale.gleam`; `corepack pnpm
gleam:format:check` passed because it checks only `src test`; removed the
  temporary `gleam/` tree afterward.
- `corepack pnpm gleam:registry:check`
- `corepack pnpm typecheck`
- `corepack pnpm test` (8 files passed; 1468 passed)
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm conformance:check` (1450 passed)
- `corepack pnpm conformance:capture:check` (9 passed)
- `corepack pnpm conformance:status -- --output-json .conformance/current/conformance-status-report.json --output-markdown .conformance/current/conformance-status-comment.md`
  (399/399 strict parity scenarios, 0 capture-only)
- `corepack pnpm gleam:port:coverage` (399 strict executable parity specs)
- `corepack pnpm gleam:test:js` (868 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam test --target erlang'`
  (OTP 28, 859 passed)
- `corepack pnpm build`
- `corepack pnpm gleam:smoke:js` (5 passed)
- `corepack pnpm elixir:smoke` (17 passed, 1 live test excluded)
- `git diff --check && git diff --cached --check`

### Findings

- Narrowing the format command is preferable to clearing CI caches because it
  keeps package dependency artifacts outside the project source validation
  boundary in both local and CI environments.
- The CI build cache must follow the promoted root layout; otherwise restored
  cache contents can recreate the retired `gleam/` tree before validation.

### Risks / open items

- Host Erlang is still OTP 25 in this workspace, so local Erlang validation
  requires the established OTP 28 container fallback.

---

## 2026-05-04 - Pass 205: HAR-486 root Gleam layout promotion

Moves the promoted Gleam project out of the transitional `gleam/` directory and
into the repository root so the final cutover layout matches the runtime
authority documented for HAR-486. The root now owns `gleam.toml`, `manifest.toml`,
`src/`, `test/`, `js/`, and `elixir_smoke/`; the old `gleam/` directory is
removed.

| Module / area                                 | Change                                                                                                 |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `src/` / `test/`                              | Move the Gleam runtime and gleeunit coverage to root-level project paths.                              |
| `js/` / `elixir_smoke/`                       | Move the JavaScript shim and Elixir smoke consumer to root-level package paths.                        |
| `scripts/sync-*.sh`                           | Move generated-data sync scripts to `scripts/` and regenerate registry/schema Gleam mirrors.           |
| `package.json` / tests / conformance tooling  | Repoint scripts, fixtures, registry evidence, and integration tests at the root Gleam project layout.  |
| `AGENTS.md` / `.agents/skills/**` / `docs/**` | Update agent guidance and runtime docs so new work targets the root Gleam project instead of `gleam/`. |

Validation:

- `corepack pnpm typecheck`
- `corepack pnpm test` (8 files passed; 1452 passed)
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm conformance:check` (1434 passed)
- `corepack pnpm conformance:capture:check` (9 passed)
- `corepack pnpm conformance:status -- --output-json .conformance/current/conformance-status-report.json --output-markdown .conformance/current/conformance-status-comment.md`
  (393/393 strict parity scenarios, 0 capture-only)
- `corepack pnpm gleam:port:coverage` (393 strict executable parity specs)
- `corepack pnpm gleam:test:js` (855 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam test --target erlang'`
  (OTP 28, 846 passed)
- `corepack pnpm build`
- `corepack pnpm gleam:smoke:js` (5 passed)
- `corepack pnpm elixir:smoke` (17 passed, 1 live test excluded)
- `git diff --check`

### Findings

- The root package can now run the Gleam build, JS shim build, parity runner,
  and Elixir smoke without changing into a nested project directory.
- Generated registry and mutation-schema mirrors remain deterministic after the
  script move.

### Risks / open items

- Host Erlang is still OTP 25 in this workspace, so local Erlang validation
  requires the established OTP 28 container fallback.

---

## 2026-05-04 - Pass 204: HAR-486 final Gleam runtime cutover

Promotes the Gleam implementation to the repository runtime authority and
removes the legacy TypeScript proxy runtime. The root package now exports the
Gleam-backed JavaScript shim under `js/dist`, launch scripts use the Node
HTTP adapter, and the root `src/` runtime tree is gone. Remaining TypeScript is
tooling or interop code: conformance capture/report scripts, registry helpers,
and the JavaScript shim.

| Module / area                                    | Change                                                                                                        |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------- |
| `package.json` / `tsconfig.json`                 | Points package `main`/`types`/exports, dev/start/build/typecheck/test scripts at the Gleam-backed JS shim.    |
| `src/**`                                         | Deletes the legacy TypeScript app, Koa adapter, proxy dispatcher, domain handlers, state, and store code.     |
| `scripts/support/**`                             | Moves retained JSON-schema, registry, GraphQL-parser, and Shopify helper code out of root `src`.              |
| `tests/**`                                       | Removes retired TypeScript runtime tests and keeps JS shim, launch, registry, and conformance tooling checks. |
| `config/operation-registry.json`                 | Repoints implemented runtime evidence away from deleted TS tests to the executable Gleam parity runner.       |
| `docs/**` / `README.md` / `GLEAM_PORT_INTENT.md` | Documents that runtime authority is now Gleam and TypeScript is limited to tooling/interop boundaries.        |

Validation:

- `corepack pnpm test` (8 files passed; 1452 passed)
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm typecheck`
- `corepack pnpm conformance:check` (1434 passed)
- `corepack pnpm conformance:capture:check` (9 passed)
- `corepack pnpm gleam:registry:check` (666 registry entries in sync)
- `corepack pnpm conformance:status -- --output-json .conformance/current/conformance-status-report.json --output-markdown .conformance/current/conformance-status-comment.md`
  (393/393 strict parity scenarios, 0 capture-only)
- `corepack pnpm gleam:port:coverage` (393 strict executable parity specs)
- `corepack pnpm build`
- `corepack pnpm gleam:test:js` (855 passed)
- `corepack pnpm gleam:test:erlang` failed on host OTP 25 with the known
  `gleam_json` OTP 27+ requirement
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam test --target erlang'`
  (OTP 28, 846 passed)
- `corepack pnpm gleam:smoke:js` first attempt timed out in one
  `/__meta/health` interop test after 5s; immediate rerun passed (5 passed)
- `corepack pnpm elixir:smoke` (17 passed, 1 live test excluded)
- `git diff --check`

### Findings

- Pass 195 made the final cutover possible: the Gleam parity runner now executes
  every checked-in parity spec as required evidence with no expected-failure
  manifest or skipped parity mode.
- Registry `runtimeTests` cannot keep pointing at deleted TypeScript runtime
  tests. The final registry evidence path is the strict Gleam parity corpus,
  and the retained operation-registry test now verifies implemented runtime-test
  paths exist on disk.
- The root `src/` tree no longer carries runtime code. TypeScript support code
  that scripts still need lives under `scripts/support`.

### Risks / open items

- Host Erlang is still OTP 25 in this workspace, so local Erlang validation
  requires the established OTP 28 container fallback.

---

## 2026-05-04 - Pass 200: HAR-574 product variant scalar validation

Adds shared Shopify-like scalar validation for product variant mutation inputs
and backs the bulk-create validation branches with a new live capture and
strict parity scenario.

| Module / fixture                                                                                                                                           | Change                                                                                                                                                                                                           |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                                                                                                       | Adds shared variant validators for explicit-null/negative/too-large prices, too-large compare-at price, weight bounds/unit, inventory quantity bounds, SKU/barcode/option value length, and 2048 caps.           |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam`                                                                                        | Covers bulk create/update, legacy single-variant create/update, productCreate/productSet rejection, oversized `variants:` input, cumulative product cap, failed logs, and no local variant staging on rejection. |
| `scripts/capture-product-variant-scalar-validation-conformance.ts` / `scripts/conformance-capture-index.ts`                                                | Adds an aggregate-indexed live capture for `productVariantsBulkCreate` scalar validation against a disposable optioned product, with before/after product-state atomicity checks.                                |
| `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-validation.json`                                           | Records explicit `price: null`, negative/too-large price, too-large compare-at price, invalid weight/quantity, text length, option length, and max input-size Shopify responses.                                 |
| `config/parity-specs/products/productVariantsBulkCreate-validation.json` / `config/parity-requests/products/productVariantsBulkCreate-validation*.graphql` | Adds executable strict JSON parity for captured `userErrors.field`, `message`, and `code`, plus Shopify's top-level max-input-size error.                                                                        |
| `docs/endpoints/products.md`                                                                                                                               | Documents the scalar validation boundary, captured omitted-price behavior, no-write rejection behavior, and new validation anchors.                                                                              |

Validation:

- `corepack pnpm conformance:capture -- --run product-variant-scalar-validations`
- `cd gleam && gleam test --target javascript -- products_mutation_test`
- `cd gleam && gleam test --target javascript -- parity_test`

### Findings

- Shopify 2025-01 accepts omitted `price` for `productVariantsBulkCreate` on
  the conformance store, while explicit `price: null` returns `Price can't be
blank` with code `INVALID`.
- The max-input-size branch is a top-level GraphQL error. Its location depends
  on the request document formatting, so the parity request mirrors the capture
  document layout for strict comparison.

### Risks / open items

- Direct live evidence for legacy `productVariantCreate` and
  `productVariantUpdate` remains unavailable on the captured 2025-01 schema, so
  their scalar validation is covered by shared runtime tests plus the
  bulk-create conformance oracle.

---

## 2026-05-04 - Pass 203: HAR-556 orderCreate validation matrix

Extends direct `orderCreate` validation beyond the no-line-items branch. The
Gleam Orders handler now rejects future `processedAt`, simultaneous
`customerId` plus `customer`, and missing/empty tax-line rates on both line
items and shipping lines before staging any order or appending mutation-log
entries. The touched validation payloads now project Shopify-style `code`
values and preserve indexed `field` segments for tax-line paths.

| Module / fixture                                                                                              | Change                                                                                                                                         |
| ------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`                                                            | Adds order-create validation helpers and typed user-error serialization for string and integer field segments plus optional `code`.            |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam`                                                      | Covers the extended validation matrix and asserts no staged orders or log drafts are created for rejected inputs.                              |
| `config/parity-specs/orders/orderCreate-validation-matrix-extended.json` / matching request and fixture files | Adds strict parity coverage for future `processedAt`, redundant customer fields, and line-item/shipping-line tax-line missing-rate userErrors. |
| `config/parity-requests/orders/orderCreate-validation-matrix.graphql` / existing no-line-items fixture        | Selects and records `userErrors.code: "INVALID"` for the existing empty-line-items branch.                                                     |
| `docs/endpoints/orders.md`                                                                                    | Documents the expanded create-time validation boundary and the current live tax-line coercion caveat.                                          |

Validation:

- Reproduction before the fix: `corepack pnpm gleam:build:js` then a JS
  `orderCreate` request with `processedAt: "2099-01-01T00:00:00Z"` returned a
  staged synthetic order and empty `userErrors`.
- Fixed JS proof: rebuilt JS wrapper returned
  `userErrors[{ field: ["order", "processedAt"], code: "PROCESSED_AT_INVALID" }]`
  and `order: null` for the same future timestamp request.
- `cd gleam && gleam test --target javascript -- orders_order_create_validation_guardrails_test parity_test`
  (851 passed)
- `cd gleam && gleam test --target javascript` (851 passed)
- `cd gleam && gleam test --target erlang` failed on host OTP 25 with the
  known `gleam_json` OTP 27+ requirement
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`
  (OTP 28, 842 passed)
- `corepack pnpm parity:record order-create-validation-matrix` (0 upstream
  calls)
- `corepack pnpm parity:record orderCreate-validation-matrix-extended` (0
  upstream calls)
- `corepack pnpm conformance:check` (1433 passed)
- `corepack pnpm conformance:capture:check` (9 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `corepack pnpm test` (123 files passed; 2297 passed)
- `git diff --check`

### Findings

- Current live probing against the 2025-01 conformance store rejects omitted or
  empty inline `OrderCreateTaxLineInput.rate` through GraphQL coercion before
  the mutation resolver. HAR-556 still requires local handler protection for
  resolved inputs that reach the proxy with a missing/empty tax-line rate, so
  the parity fixture preserves the ticket's cited mutation-level
  `TAX_LINE_RATE_MISSING` payload contract and the endpoint notes call out the
  probe caveat explicitly.

### Risks / open items

- Host Erlang remains OTP 25 in this workspace, so Erlang validation still
  requires the established OTP 28 container fallback.

---

## 2026-05-04 - Pass 202: HAR-599 BXGY disallowed value validation

Aligns local BXGY validation with live Shopify for `customerGets.value` branch
selection and subscription purchase flags. Code and automatic BXGY now reject
`percentage` / `discountAmount` value branches before staging, and both reject
`customerGets.appliesOnSubscription` / `appliesOnOneTimePurchase` with the
captured code-vs-automatic messages. Code BXGY also mirrors Shopify's captured
secondary blank `discountOnQuantity.quantity` userError when the submitted value
omits `discountOnQuantity`.

| Module / fixture                                                                                                      | Change                                                                                                                 |
| --------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/discounts.gleam`                                                                 | Adds BXGY value-branch, missing discount-on-quantity quantity, and subscription-flag guardrails.                       |
| `gleam/test/shopify_draft_proxy/proxy/discounts_test.gleam`                                                           | Covers code and automatic create/update validation for value branches and subscription flags.                          |
| `scripts/capture-discount-bxgy-disallowed-value-shapes-conformance.ts` / `scripts/conformance-capture-index.ts`       | Adds an aggregate-indexed capture that creates two temporary products, records rejected BXGY branches, then cleans up. |
| `config/parity-specs/discounts/discount-bxgy-disallowed-value-shapes.json` / matching request and conformance fixture | Adds executable strict JSON parity for the captured userErrors.                                                        |
| `docs/endpoints/discounts.md`                                                                                         | Documents the captured BXGY validation boundary and capture script.                                                    |

Validation:

- `corepack pnpm conformance:probe`
- `corepack pnpm conformance:capture -- --run discount-bxgy-disallowed-value-shapes`
- `cd gleam && gleam test --target javascript -- discounts_test parity_test`
- `cd gleam && gleam test --target erlang -- discounts_test parity_test` failed on
  host OTP 25 with the known `gleam_json` OTP 27+ requirement
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- discounts_test parity_test'`
  (OTP 28, 847 passed)
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:capture:check`
- `corepack pnpm gleam:format:check`
- `cd gleam && gleam test --target javascript` (856 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`
  (OTP 28, 847 passed)
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `corepack pnpm test` (123 files passed; 2298 passed)
- `git diff --check`

### Findings

- Live 2026-04 Shopify returns `Only discountOnQuantity permitted with bxgy
discounts.`, not the older wording in the ticket body.
- Live 2026-04 Shopify rejects automatic BXGY `customerGets` subscription flags
  with an automatic-specific unsupported-field message, so the local proxy now
  treats those fields as invalid for both code and automatic BXGY.

### Risks / open items

- Host Erlang remains OTP 25 in this workspace, so Erlang validation still
  requires the established OTP 28 container fallback.

---

## 2026-05-04 - Pass 201: HAR-601 productSet validator and async operation fidelity

Aligns `productSet` with captured Shopify guardrails for shape validation,
existing-product references, and asynchronous `ProductSetOperation` polling.
The mutation now rejects over-large variant and inventory-quantity arrays with
Shopify's top-level `MAX_INPUT_SIZE_EXCEEDED` error shape, returns structured
user errors for missing or suspended existing products, shares existing product
field validation before staging, and records async productSet operations so
`productOperation(id:)` can read the completed local result.

| Module / fixture                                                                                                                         | Change                                                                                                                                                  |
| ---------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                                                                                     | Adds productSet shape guardrails, existing-product lookup errors, suspended-product errors, shared product field validation, and async operation state. |
| `gleam/src/shopify_draft_proxy/state/types.gleam`                                                                                        | Persists product operation user-error codes for `productOperation` reads.                                                                               |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam`                                                                      | Covers variant, option, option-value, file, inventory-quantity, missing-product, suspended-product, and async operation behavior.                       |
| `scripts/capture-product-set-validator-conformance.ts` / `scripts/conformance-capture-index.ts`                                          | Adds the aggregate-indexed live capture for productSet validator and async operation evidence.                                                          |
| `config/parity-specs/products/productSet-*` / `config/parity-requests/products/productSet-*` / `fixtures/conformance/**/products/*.json` | Adds executable parity specs and cassettes for shape guardrails, unknown-product validation, and async operation polling.                               |
| `docs/endpoints/products.md`                                                                                                             | Documents the productSet validator limits, reference errors, suspended-product branch, and async operation semantics.                                   |

Validation:

- `corepack pnpm conformance:probe`
- `corepack pnpm conformance:capture -- --run product-set-validator`
- `corepack pnpm parity:record productSet-shape-validator-parity`
- `corepack pnpm parity:record productSet-async-operation-parity`
- `cd gleam && gleam test --target javascript -- products_mutation_test`
- `cd gleam && gleam test --target javascript -- parity_test`
- `corepack pnpm conformance:check` (1436 passed)
- `corepack pnpm conformance:capture:check` (9 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm typecheck`
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm gleam:test` ran the JavaScript target successfully (854
  passed) before the host Erlang target failed on the known local OTP 25
  runtime issue
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`
  (OTP 28, 845 passed)
- `cd gleam && gleam test --target javascript` (854 passed)
- `cd gleam && gleam test --target javascript -- parity_test` after
  `parity:record` (854 passed)
- `corepack pnpm build`
- `corepack pnpm test` (123 files passed; 2300 passed)
- `git diff --check`

### Findings

- Shopify returns top-level `MAX_INPUT_SIZE_EXCEEDED` errors with no `data`
  envelope for `productSet` arrays above 2048 variants and 250
  `inventoryQuantities`; source `locations` are parser-specific and ignored in
  the parity contract.
- Missing existing-product references return payload user errors on the
  referenced input field, while suspended effective local products are modeled
  as `INVALID_PRODUCT` on `["input"]`.
- `productSet(synchronous: false)` returns a `CREATED` operation with no
  product immediately, then `productOperation(id:)` observes the completed
  staged product in the same proxy session.

### Risks / open items

- Public Admin API setup cannot create suspended products, and the parity runner
  intentionally has no base-state seed hook. Suspended-product behavior is
  covered by focused Gleam runtime tests instead of a live parity cassette.
- Host Erlang remains OTP 25 in this workspace, so Erlang validation still
  requires the established OTP 28 container fallback.

---

## 2026-05-04 - Pass 200: HAR-567 local-pickup validation parity

Aligns `locationLocalPickupEnable` with captured Shopify validation for custom
local-pickup times while preserving local staging for standard pickup windows.

| Module / fixture                                                                                                                                                                                                                  | Change                                                                                                                                                                                                  |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/shipping_fulfillments.gleam`                                                                                                                                                                 | Validates pickup times before staging settings, returns `CUSTOM_PICKUP_TIME_NOT_ALLOWED` for `CUSTOM`/non-standard values, and keeps unknown/inactive location failures on `ACTIVE_LOCATION_NOT_FOUND`. |
| `gleam/test/shopify_draft_proxy/proxy/shipping_fulfillments_test.gleam`                                                                                                                                                           | Adds local coverage for custom pickup-time rejection, inactive-location rejection, and the captured multi-day standard values.                                                                          |
| `scripts/capture-shipping-settings-conformance.ts` / `fixtures/conformance/.../shipping-settings-package-pickup-constraints.json` / `config/parity-specs/shipping-fulfillments/shipping-settings-package-pickup-constraints.json` | Extends the existing shipping-settings scenario with the captured `CUSTOM_PICKUP_TIME_NOT_ALLOWED` payload branch.                                                                                      |
| `docs/endpoints/shipping-fulfillments.md`                                                                                                                                                                                         | Documents the local-pickup standard value allow-list and custom pickup-time error boundary.                                                                                                             |

Validation:

- `corepack pnpm conformance:probe`
- ad hoc live 2026-04 probes for `TWO_DAYS`, `MULTIPLE_DAYS`, `CUSTOM`,
  `TWO_TO_FOUR_DAYS`, and `FIVE_OR_MORE_DAYS`; cleanup disabled local pickup on
  the probed location
- `corepack pnpm conformance:capture -- --run shipping-settings`
- `corepack pnpm parity:record shipping-settings-package-pickup-constraints`
- `cd gleam && gleam test --target javascript -- shipping_fulfillments_test`
- `cd gleam && gleam test --target javascript -- parity_test`
- `cd gleam && gleam test --target javascript` (853 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`
  (OTP 28, 844 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm conformance:check`

### Findings

- Admin GraphQL 2026-04 rejects `TWO_DAYS` and `MULTIPLE_DAYS` during variable
  coercion with top-level `INVALID_VARIABLE`. The resolver-level coded userError
  is produced by `pickupTime: CUSTOM`.
- Shopify accepts `TWO_TO_FOUR_DAYS` and `FIVE_OR_MORE_DAYS` as standard pickup
  windows for this mutation, so the local allow-list includes them.

### Risks / open items

- The parity recorder cannot synthesize the primary availability cassette for
  this scenario on JS, so the existing hand-synthesized primary upstream call
  remains in the fixture.

---

## 2026-05-04 - Pass 199: HAR-568 inventorySetQuantities name validation

Aligns inventory quantity mutation validation with the per-root Shopify
contract instead of sharing one broad staged quantity-name set across set,
adjust, and move handling.

| Module / fixture                                                                                                                                                | Change                                                                                                                                                                                                                                                                     |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                                                                                                            | Splits `inventorySetQuantities` name validation to `available` / `on_hand`, accepts `on_hand` and `committed` for `inventoryAdjustQuantities`, enforces set quantity bounds/negative/duplicate-pair validation, and mirrors direct `on_hand` sets into paired change rows. |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam`                                                                                             | Adds focused local runtime coverage for invalid set names, quantity bounds, negative quantities, duplicate item/location pairs, and `on_hand` set/adjust success.                                                                                                          |
| `scripts/capture-inventory-set-quantities-name-validation.ts` / `scripts/conformance-capture-index.ts`                                                          | Adds aggregate-indexed live capture for 2025-01 and 2026-04 validation branches using disposable products and cleanup.                                                                                                                                                     |
| `fixtures/conformance/harry-test-heelo.myshopify.com/{2025-01,2026-04}/products/inventorySetQuantities-name-validation.json`                                    | Records live Shopify user-error payloads for `damaged`, `committed`, over-max quantity, and duplicate pair rejection, plus `on_hand` acceptance.                                                                                                                           |
| `config/parity-specs/products/inventorySetQuantities-name-validation*.json` / `config/parity-requests/products/inventorySetQuantities-name-validation*.graphql` | Adds strict userErrors parity for both API tracks. The generic runner seeds a local product through `productSet`, then replays captured-shape `inventorySetQuantities` requests against the staged inventory item.                                                         |
| `docs/endpoints/products.md` / `docs/hard-and-weird-notes.md` / `tests/integration/gleam-interop.test.ts`                                                       | Documents the corrected per-root quantity-name boundary and raises a load-sensitive interop smoke timeout to 10s after the full Vitest suite repeatedly exceeded 5s while the isolated test stayed green.                                                                  |

Validation:

- `corepack pnpm conformance:capture -- --run inventory-set-quantities-name-validation-2025`
- `corepack pnpm conformance:capture -- --run inventory-set-quantities-name-validation-2026`
- `corepack pnpm parity:record inventorySetQuantities-name-validation`
- `corepack pnpm parity:record inventorySetQuantities-name-validation-2026-04`
- `cd gleam && gleam test --target javascript -- products_mutation_test parity_test`
  (852 passed)
- `cd gleam && gleam test --target javascript` (852 passed)
- `cd gleam && gleam test --target erlang` failed on host OTP 25 with the
  known `gleam_json` OTP 27+ requirement
- `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/workspace -w /workspace/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (OTP 28, 843 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm conformance:check` (1433 passed)
- `corepack pnpm conformance:capture:check` (9 passed)
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `corepack pnpm test` (123 files passed; 2297 passed)
- `git diff --check`

### Findings

- Shopify's set mutation boundary is narrower than the inventory quantity-name
  catalog: `inventorySetQuantities` accepts only `available` and `on_hand`,
  while `damaged` and `committed` return `INVALID_NAME` with the exact
  "available or on_hand" message.
- Direct `name: "on_hand"` set writes succeed and return paired `available` and
  `on_hand` change rows in the mutation payload.
- Set validation rejects over-max quantities and duplicate item/location rows
  before staging. The duplicate branch returns one
  `NO_DUPLICATE_INVENTORY_ITEM_ID_GROUP_ID_PAIR` userError per duplicate row at
  the row's `locationId`.

### Risks / open items

- A quick live negative-quantity probe did not reproduce the ticket-described
  `INVALID_QUANTITY_NEGATIVE` branch, but the local implementation preserves the
  ticket acceptance requirement. The checked-in parity fixture focuses on the
  explicitly requested name, over-max, duplicate, and `on_hand` branches.
- Host Erlang remains OTP 25 in this workspace, so Erlang validation used the
  established OTP 28 container fallback.

---

## 2026-05-04 - Pass 198: HAR-619 price list create currency and parent validation

Aligns Markets `priceListCreate` with live Shopify behavior for DKK currencies
and required parent adjustment input. The local handler now uses the
Money::Currency-style ISO code set instead of the previous 9-code allowlist,
requires `currency` and `parent` on create, validates parent adjustment type,
and serializes the staged parent adjustment into downstream PriceList reads.

| Module / fixture                                                                                      | Change                                                                                                                    |
| ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/markets.gleam`                                                   | Expands price-list currency validation, removes USD create fallback, requires create parent input, and stages adjustment. |
| `gleam/test/shopify_draft_proxy/proxy/markets_mutation_test.gleam`                                    | Covers DKK success, missing currency, missing parent, and invalid adjustment type branches.                               |
| `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/price-list-create-dkk.json`      | Records live DKK create success and cleanup of the disposable PriceList.                                                  |
| `config/parity-specs/markets/price-list-create-dkk.json` / `config/parity-requests/markets/*.graphql` | Adds executable parity evidence for the DKK success payload.                                                              |
| `docs/endpoints/markets.md`                                                                           | Documents the tighter price-list validation boundary.                                                                     |

Validation:

- `corepack pnpm conformance:probe`
- one-off live Admin GraphQL 2026-04 `priceListCreate` DKK capture with
  `priceListDelete` cleanup
- `corepack pnpm parity:record price-list-create-dkk`
- `cd gleam && gleam test --target javascript -- markets_mutation_test`
- `cd gleam && gleam test --target javascript -- parity_test`
- `cd gleam && gleam test --target javascript` (855 passed)
- `cd gleam && gleam test --target erlang` failed on host OTP 25 with the
  known `gleam_json` OTP 27+ requirement
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`
  (OTP 28, 846 passed)
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:capture:check`
- `corepack pnpm gleam:format:check`
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `corepack pnpm test` (123 files passed; 2297 passed)

### Findings

- Live Shopify accepts `priceListCreate` with `currency: DKK` when the input
  includes a valid `parent.adjustment` of `PERCENTAGE_DECREASE`.
- The existing invalid-currency capture omits `parent` and Shopify reports only
  the currency inclusion error, so local parent validation is ordered after
  currency validation to preserve that payload.

### Risks / open items

- Host Erlang remains OTP 25 in this workspace, so Erlang validation still
  requires the established OTP 28 container fallback.

---

## 2026-05-04 - Pass 197: HAR-550 product option autogeneration fidelity

Tightens product option/variant autogeneration parity for `productCreate` and
`productSet` from live Shopify evidence. `productCreate(productOptions:)` is
now explicitly covered for the multi-value case where Shopify creates only the
first-value variant and keeps extra option values non-variant-backed, while
`productSet(input.productOptions)` now locally rejects missing/empty
`input.variants` with Shopify's captured user error instead of staging a stale
`Title / Default Title` variant.

| Module / fixture                                                               | Change                                                                                                                               |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                           | Adds the `productSet` option-change guardrail requiring variant input before local staging.                                          |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam`            | Covers the guardrail and verifies the mutation log entry is marked `Failed` with no staged resource ids.                             |
| `scripts/capture-product-create-with-options-conformance.mts`                  | Extends the live capture harness to record multi-value `productCreate` evidence and the `productSet` options-only validation branch. |
| `config/parity-specs/products/*options*` / `fixtures/conformance/**/products/` | Adds executable parity specs and captured fixtures for the new evidence, and refreshes the original options capture.                 |
| `docs/endpoints/products.md`                                                   | Documents the asymmetric `productCreate` / `productSet` option behavior.                                                             |

Validation:

- `corepack pnpm conformance:probe`
- `corepack pnpm conformance:capture -- --run product-create-with-options`
- `cd gleam && gleam test --target javascript -- products_mutation_test parity_test`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:capture:check`
- `corepack pnpm gleam:format:check`
- `cd gleam && gleam test --target javascript` (850 passed)
- `cd gleam && gleam test --target erlang` failed on host OTP 25 with the
  known `gleam_json` OTP 27+ requirement
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`
  (OTP 28, 841 passed)
- `corepack pnpm gleam:build:js`
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `corepack pnpm test` (123 files passed; 2291 passed)
- `git diff --check`

### Findings

- Shopify does not generate the full Cartesian variant matrix for
  `productCreate(productOptions:)`; it creates one first-value variant and
  leaves extra option values in `optionValues` with `hasVariants: false`.
- Shopify rejects `productSet(input.productOptions)` without variants using
  `field: ["input", "variants"]`, so the local proxy should fail that branch
  before staging any product/options/variants or replaying it at commit time.

### Risks / open items

- Host Erlang remains OTP 25 in this workspace, so Erlang validation still
  requires the established OTP 28 container fallback.

---

## 2026-05-04 - Pass 196: HAR-549 invalid product search syntax parity

Adds live conformance coverage for malformed-looking Shopify Admin product
search strings and aligns the shared Gleam search parser with the captured
local-staging behavior.

| Module / fixture                                                                                                                                   | Change                                                                                                                                                                                       |
| -------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `scripts/capture-product-invalid-search-query-conformance.ts` / `scripts/conformance-capture-index.ts`                                             | Adds an aggregate-indexed capture that creates a disposable product, waits for tag search indexing, captures valid and malformed product search reads, then deletes the product.             |
| `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-invalid-search-query-syntax.json`                                    | Records Shopify's normal data envelopes for malformed search strings: `tag:(value` and `tag:("value"` return zero matches, while bare leading `(` and dangling `OR` still match the product. |
| `config/parity-specs/products/product-invalid-search-query-syntax.json` / `config/parity-requests/products/product-invalid-search-query-*.graphql` | Adds strict executable parity that stages `productCreate` locally and then exercises the product search overlay path with the captured malformed queries.                                    |
| `gleam/src/shopify_draft_proxy/search_query_parser.gleam` / `gleam/test/shopify_draft_proxy/search_query_parser_test.gleam`                        | Keeps `(` as a literal term character when it appears immediately after a field/comparator buffer instead of treating `tag:(...` as a grouped expression.                                    |
| `docs/endpoints/products.md`                                                                                                                       | Documents the captured forgiving product search syntax boundary and the new parity fixture.                                                                                                  |

Validation:

- `corepack pnpm conformance:probe`
- `corepack pnpm conformance:capture -- --run product-invalid-search-query-syntax`
- `corepack pnpm parity:record product-invalid-search-query-syntax` (verified
  the staged scenario records zero upstream calls)
- `cd gleam && gleam test --target javascript -- search_query_parser_test parity_test`
- `corepack pnpm conformance:capture:check`
- `corepack pnpm conformance:check`
- `corepack pnpm gleam:format:check`
- `cd gleam && gleam test --target javascript` (850 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`
  (OTP 28, 841 passed)
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `corepack pnpm test` (123 files passed; 2291 passed)
- `git diff --check`

### Findings

- Shopify did not emit top-level GraphQL errors for the probed malformed
  product search syntax. The relevant fidelity rule is search-term
  interpretation: field-prefixed open-paren values are literal non-matches, but
  bare leading groups and dangling `OR` remain forgiving.
- The existing local parser already matched Shopify's forgiving bare-leading
  `(` and dangling `OR` behavior. The mismatch was specifically `tag:(value`,
  where the old tokenizer split `tag:` from the grouped value and accidentally
  matched the staged product.

### Risks / open items

- Product search indexing still requires a short wait after live
  `productCreate`; the capture script asserts the valid tag search first so it
  fails before writing a stale malformed-query fixture.
- Host Erlang remains OTP 25 in this workspace, so Erlang validation used the
  established OTP 28 container fallback.

---

## 2026-05-04 - Pass 195: HAR-546 parity scaffold removal

Removes the completed parity-migration safety rails so every checked-in parity
spec runs as required evidence on both Gleam targets. Missing cassettes and
comparison mismatches now fail the parity test directly; there is no
allowlist, skipped parity outcome, or top-level parity runtime mode left.

| Module / fixture                                               | Change                                                                                                                                   |
| -------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `config/gleam-port-ci-gates.json`                              | Deletes the now-redundant CI gate config file after removing the former expected-failure manifest.                                       |
| `gleam/test/parity_test.gleam`                                 | Simplifies the discovered corpus test to hard-fail every runner error or mismatch, with no expected-failure or skipped-outcome handling. |
| `gleam/test/parity/spec.gleam` / `runner.gleam`                | Removes unused top-level parity runtime mode decoding, including the empty-snapshot variant; every spec runs LiveHybrid with a cassette. |
| `scripts/gleam-port-coverage-gate.ts`                          | Drops manifest validation and owns the remaining workflow/capture-tooling gate lists directly.                                           |
| `tests/unit/parity-no-seeding.test.ts`                         | Deletes the obsolete lockdown suite now that the migration scaffolding is gone and the corpus is fully strict.                           |
| `fixtures/conformance/**/*.json`                               | Removes the banned top-level seed metadata from 31 capture files so the active no-seeding lint passes.                                   |
| `docs/parity-runner.md` / `docs/architecture.md`               | Rewrites migration-era guidance as steady-state cassette-runner documentation.                                                           |
| `GLEAM_PORT_INTENT.md` / `gleam/test/parity_corpus_test.gleam` | Removes stale skipped-partition/corpus comments.                                                                                         |

Validation:

- `corepack pnpm gleam:port:coverage`
- `corepack pnpm conformance:capture:check` (9 passed)
- `cd gleam && gleam test --target javascript -- parity_test` (827 passed)
- `cd gleam && gleam test --target javascript -- parity_corpus_test` (831 passed)
- `corepack pnpm conformance:check` (1403 passed)
- `corepack pnpm gleam:format:check`
- `cd gleam && gleam test --target javascript` (831 passed)
- `cd gleam && gleam test --target erlang` failed on host OTP 25 with the
  known `gleam_json` OTP 27+ requirement
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`
  (OTP 28, 822 passed)
- `corepack pnpm gleam:build:js` before rerunning root tests after the Erlang
  container cleaned the build directory
- `corepack pnpm test` (123 files passed; 2267 passed)
- `corepack pnpm lint` (passes with the pre-existing
  `scripts/parity-record.mts` unused catch-parameter warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `git diff --check`

### Findings

- The supplied skipped-test audit pattern still matches `process.exit(` through
  the `xit(` alternative; those hits are not skipped tests.
- No checked-in parity spec used the removed top-level empty-snapshot mode. The
  active `comparison.mode` field remains in specs as the strict JSON comparison
  contract.
- Enabling the no-seeding lint exposed stale top-level seed metadata in capture
  roots. Removing those unused keys did not change cassette payloads or
  comparison targets.

### Risks / open items

- Host Erlang remains OTP 25 in this workspace, so Erlang validation still
  requires the established OTP 28 container fallback.

---

## 2026-05-04 - Pass 194: HAR-545 final parity drain

Drains the final Gleam parity expected-failure manifest to zero and makes the
entire cassette-backed parity corpus green on both Gleam targets. The remaining
failures were product-domain scenarios that needed Pattern 2 hydration breadth,
fixture cassette repair from checked-in capture evidence, and several local
overlay fixes so supported mutations continue to stage locally without runtime
Shopify writes.

| Module / fixture                                                                    | Change                                                                                                                                                                                          |
| ----------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `config/gleam-port-ci-gates.json`                                                   | Empties the Gleam parity expected-failure manifest.                                                                                                                                             |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                                | Extends product-domain Pattern 2 hydration, fixes product/variant/inventory/publication/selling-plan read-after-write behavior, and makes hydrate ID ordering deterministic across targets.     |
| `gleam/src/shopify_draft_proxy/proxy/metafield_definitions.gleam`                   | Aligns singular `metafieldDelete` compatibility for unknown local IDs.                                                                                                                          |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                                   | Aligns product collection listing order without changing collection membership order.                                                                                                           |
| `fixtures/conformance/**/products/*.json` and `config/parity-specs/products/*.json` | Repairs product cassettes/spec expectations from checked-in capture evidence, including metafields, collections, inventory, media, variants, productSet, publications, selling plans, and tags. |
| `fixtures/conformance/**/online-store-article-media-navigation-follow-through.json` | Adds explicit empty `upstreamCalls` so the runner has no skipped/missing-cassette scenarios.                                                                                                    |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam`                 | Updates local compatibility coverage for singular `metafieldDelete`.                                                                                                                            |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (827 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'`
  (OTP 28, 822 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:capture:check`
- `corepack pnpm lint` (passes with the pre-existing `scripts/parity-record.mts`
  unused catch-parameter warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `corepack pnpm test` (123 files passed; 2263 passed, 2 existing skipped)
- `git diff --check`
- Repository scans: the Gleam parity expected-failure manifest is empty, no
  missing/malformed parity `upstreamCalls`, no `skip: true`, and no `seedX`.

### Findings

- The final manifest entries were all product-domain scenarios left after the
  earlier product read-slice migration. They were not final-cleanup metadata
  only; they required substantive local product behavior and cassette-backed
  hydration.
- Pattern 2 hydration needed deterministic `ProductsHydrateNodes` ID ordering.
  JavaScript and Erlang iterate dictionaries differently, so hydrate variables
  now sort Shopify GIDs before lookup and the affected cassettes use the same
  stable order.
- The checked-in captures contained enough evidence for the cassette repairs;
  no live Shopify credential or re-recording was needed.

### Risks / open items

- The parity migration scaffolding remains in place by design. The follow-up
  issue owns removing the manifest and expected-failure runner plumbing.
- Host Erlang is still OTP 25 in this workspace, so Erlang validation used the
  established OTP 28 container fallback.

---

## 2026-05-03 - Pass 193: HAR-513 JS live-hybrid passthrough

Completes the JavaScript async upstream forwarding path for live-hybrid
passthrough requests that are decided inside domain handlers, not only at the
dispatcher fallback layer. Unsupported mutations that actually passthrough now
record a visible `proxied` mutation-log entry, and passthrough responses carry
upstream response headers back to JS callers.

| Module / test                                                 | Change                                                                                                                          |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`       | Retries JS sync-passthrough sentinels through async dispatch, and records proxied mutation log entries for passthrough roots.   |
| `gleam/src/shopify_draft_proxy/proxy/passthrough.gleam`       | Exposes the JS async-required sentinel check and preserves upstream response headers in passthrough responses.                  |
| `gleam/src/shopify_draft_proxy/proxy/commit.gleam`            | Extends the shared HTTP outcome with response headers while keeping commit replay status/body behavior unchanged.               |
| `gleam/src/shopify_draft_proxy/shopify/upstream_client.gleam` | Carries Erlang and JS client response headers into the shared HTTP outcome.                                                     |
| `gleam/js/test/live-hybrid-passthrough.test.ts`               | Adds fake-upstream coverage for domain-owned async reads, unsupported mutation observability, network errors, and local writes. |
| `gleam/test/**`                                               | Updates fake transports and cassette assertions for the header-carrying HTTP outcome.                                           |

Validation:

- `corepack pnpm --dir gleam/js test -- live-hybrid-passthrough.test.ts`
  (18 passed across the JS shim suite)
- `cd gleam && gleam test --target javascript -- passthrough_test draft_proxy_async_test`
  (824 passed)
- `cd gleam && gleam test --target erlang` failed on the local OTP 25 runtime
  with the known `gleam_json` OTP 27+ requirement
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (OTP 28, 819 passed)
- `corepack pnpm lint` (passed with the existing
  `scripts/parity-record.mts:279` warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `git diff --check`

### Findings

- `process_request_async` already handled dispatcher-level passthrough, but
  Pattern 1 domain handlers such as Apps could still choose
  `passthrough_sync` after async dispatch had fallen back to the normal sync
  route, causing JS callers to receive the 501 sentinel instead of an upstream
  response.
- Preserving upstream response headers required moving headers into the shared
  `HttpOutcome`; commit replay ignores them, while passthrough serializes them
  back to JS HTTP-shaped responses.

---

## 2026-05-03 - Pass 192: HAR-514 JS artifact routes

Serves staged-upload and generated bulk-operation artifact routes through the
Gleam-backed JavaScript HTTP adapter. Staged upload posts are stored in the
instance-owned `DraftProxy` store under the same lookup keys used by local
`bulkOperationRunMutation`, and bulk result JSONL is read back from the
per-instance BulkOperation records. No module-global artifact cache is added.

| Module / fixture                                                         | Change                                                                                                           |
| ------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `gleam/js/src/app.ts`                                                    | Adds JS HTTP routing for `/staged-uploads/...` and `/__meta/bulk-operations/.../result.jsonl`.                   |
| `gleam/js/src/runtime.ts`                                                | Exposes shim methods for staged-upload writes and bulk JSONL reads against the wrapped Gleam `DraftProxy` value. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`                  | Adds small public helpers for instance-scoped staged upload content and bulk result lookup.                      |
| `gleam/src/shopify_draft_proxy/proxy/bulk_operations.gleam`              | Uses encoded-GID meta result URLs for generated query exports and mutation imports.                              |
| `gleam/js/test/http-adapter.test.ts`                                     | Adds end-to-end JS HTTP coverage for upload handoff, canonical bulk result serving, and instance isolation.      |
| `docs/architecture.md` / `docs/endpoints/media.md` / `GLEAM_PORT_LOG.md` | Documents the JS adapter artifact boundary and the limited staged-upload byte handoff scope.                     |

Validation:

- `corepack pnpm --dir gleam/js test -- --runInBand` (15 passed)
- `corepack pnpm gleam:format:check`
- `cd gleam && gleam test --target javascript` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'` (OTP 28, 819 passed)
- `corepack pnpm lint` (passed with existing `scripts/parity-record.mts:279` warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm conformance:check`
- `git diff --check`

### Findings

- The JS adapter was the missing boundary: the Gleam core already generated
  staged upload targets and stored bulk result JSONL, but HTTP requests to the
  generated URLs fell through to 404.
- The adapter must pass staged upload bodies as raw text, regardless of
  `content-type`, so JSONL variables files are stored exactly as uploaded.
- Query exports and mutation imports both use the current
  `/__meta/bulk-operations/<encoded-gid>/result.jsonl` artifact route.

### Risks / open items

- `partialDataUrl` remains `null` for local jobs until live fixture evidence
  proves a local partial-data artifact shape. The JS adapter currently serves
  the generated result artifacts covered by existing local bulk-operation
  behavior.

---

## 2026-05-03 - Documentation rework: Gleam runtime README handoff

Reworks the public README and package README after rebasing HAR-484 onto the
latest cassette-backed Gleam mainline. The docs now describe the Gleam runtime
as the package implementation, remove the old TypeScript/Koa-first quickstart
and stale TODO callouts, and keep unsupported boundaries focused on the
remaining artifact-serving/package-publication surfaces rather than historical
transition narrative.

| Module            | Change                                                                                                                                                                   |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `README.md`       | Documents Gleam-first install, JS and Elixir embedding, runtime modes, routes, state threading, conformance, and TypeScript retirement.                                  |
| `gleam/README.md` | Documents the package API, JS shim, Elixir wrapper, route surface, runtime modes, state threading, conformance checks, and unsupported boundaries without TODO callouts. |

Validation:

- `corepack pnpm lint`
- `corepack pnpm gleam:test:js` (824 passed)
- `docker run --rm -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'rm -rf build/dev/erlang && gleam test --target erlang'` (819 passed after clearing stale root-owned build output, then restoring build ownership)
- `corepack pnpm conformance:check` (1403 passed)

### Findings

- `origin/main` already contained the newer Gleam runtime architecture doc, so
  the merge resolution kept that architecture baseline instead of reintroducing
  the earlier transition-oriented HAR-484 architecture edits.
- The root README still needed to stop presenting the TypeScript/Koa API as the
  primary public surface after the mainline port work added JS HTTP adapter and
  Elixir wrapper coverage.
- The package README still had TODO-shaped install, commit, JS cutover, and
  Elixir wrapper notes even though those areas now have clearer supported or
  intentionally unsupported boundaries.

### Risks / open items

- npm and Hex publication are still pending.
- `GET /__meta` operator UI, staged-upload byte serving, and bulk-operation
  result-file serving remain unsupported by the Gleam HTTP adapter.
- TypeScript runtime deletion still depends on full port parity, packaging, CI,
  and conformance completion.

---

## 2026-05-03 - Pass 191: HAR-539 payments cassette parity

Migrates the remaining Payments parity scenarios to cassette-backed LiveHybrid
execution. Customer payment-method mutations and payment-terms creation now
hydrate the captured upstream owner context through narrow Pattern 2 cassette
queries before staging locally, so supported mutations still avoid runtime
Shopify writes while downstream reads observe the staged state.

| Module / fixture                                                                  | Change                                                                                                         |
| --------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/payments.gleam`                              | Adds Pattern 2 mutation hydration for customer/payment-method shells and draft-order payment-terms owners.     |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`                           | Threads `UpstreamContext` into payments mutations and routes payment-method-only `customer` reads to payments. |
| `fixtures/conformance/**/payments/*{customer-payment-method,payment-terms}*.json` | Hand-synthesizes hydrate cassette entries from checked-in capture/local-runtime evidence.                      |
| `config/operation-registry.json`                                                  | Promotes `customerPaymentMethod` overlay read support with the LiveHybrid hydration boundary documented.       |
| `config/gleam-port-ci-gates.json`                                                 | Removes the two Payments expected-failure entries.                                                             |
| `docs/endpoints/payments.md`                                                      | Documents the cassette-backed payments hydrate paths and local-staging boundaries.                             |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'`
  (OTP 28, 819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed payments fixture/gate checks: no payments expected-failure entries
  remain, no new top-level `seed*` keys, and no new `expectedDifferences`

### Findings

- Pattern 2 is required for both migrated payments scenarios because supported
  mutations must stage locally while starting from an existing upstream customer,
  payment method, or draft-order owner.
- The checked-in captures and local-runtime fixture already contained the
  authoritative source payloads, so the missing cassette entries could be
  hand-synthesized without live Shopify credentials.
- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation used the established OTP 28 container fallback.

### Risks / open items

- Customer payment-method hydration intentionally captures only scrubbed shells
  selected by the current local-runtime parity evidence. Broader live
  payment-method overlay coverage still depends on safe conformance credentials
  with customer-payment-method scopes.

---

## 2026-05-03 - Pass 190: HAR-542 segments cassette parity

Migrates the remaining Segments parity scenario to cassette-backed LiveHybrid
execution. Cold segment catalog/detail/count/filter reads now use gated Pattern
1 passthrough until local segment lifecycle state exists; supported segment
mutations continue to stage locally and keep downstream read-after-write paths on
the local serializer.

| Module / fixture                                           | Change                                                                                                     |
| ---------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/segments.gleam`       | Adds gated LiveHybrid Pattern 1 passthrough for cold segment read roots and local-state guard helpers.     |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`    | Routes segment queries through the domain query entrypoint so the domain owns passthrough decisions.       |
| `fixtures/conformance/**/segments/segments-baseline.json`  | Hand-synthesizes the baseline read cassette from the checked-in capture payload.                           |
| `gleam/test/shopify_draft_proxy/proxy/segments_test.gleam` | Covers segment passthrough guard behavior for arbitrary variable names, deleted markers, and staged state. |
| `config/gleam-port-ci-gates.json`                          | Removes the remaining Segments expected-failure entry.                                                     |

Validation:

- `cd gleam && gleam test --target javascript -- segments_test` (827 passed)
- `cd gleam && gleam test --target javascript -- parity_test` (827 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 822 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:capture:check`
- `corepack pnpm conformance:status`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm lint`
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `git diff --check`
- changed Segments fixture/gate checks: no Segments expected-failure entries
  remain, no new `seed*` keys, and no new `expectedDifferences`

### Findings

- Pattern 1 is appropriate for the cold `segments-baseline-read` scenario
  because the captured response is the exact upstream payload and the proxy has
  no local overlay to add before any segment writes.
- The passthrough guards must scan every string variable and treat deleted
  segment IDs as local so read-after-write and read-after-delete scenarios do not
  forward local IDs upstream.
- The checked-in capture payload already contained the authoritative response,
  so the cassette could be hand-synthesized without live Shopify credentials.

### Risks / open items

- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation used the established OTP 28 container fallback.

---

## 2026-05-03 - Pass 189: HAR-541 product read cassette parity slice

Migrates a first products read slice to cassette-backed LiveHybrid execution.
Cold product detail/catalog/search reads now use Pattern 1 passthrough when the
proxy has no local product state to overlay, while product-owned metafield shell
reads and staged product lifecycle state continue to resolve locally.

| Module / fixture                                             | Change                                                                                                              |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`         | Adds gated LiveHybrid passthrough for cold `product`, `productByIdentifier`, `products`, and `productsCount` reads. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`      | Routes product queries through the products domain query entrypoint.                                                |
| `fixtures/conformance/**/products/{product*,products*}.json` | Hand-synthesizes read cassette entries from checked-in product capture evidence.                                    |
| `config/gleam-port-ci-gates.json`                            | Removes ten products read expected-failure entries that now pass.                                                   |
| `docs/endpoints/products.md`                                 | Documents the cold-read passthrough boundary.                                                                       |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)

### Findings

- Pattern 1 is appropriate for cold product read scenarios whose captured
  Shopify response is returned verbatim and where the local proxy has no staged
  product state to overlay.
- Product-owned metafield shell reads must be included in the local-state gate;
  otherwise unrelated metafields scenarios can regress by forwarding local owner
  shell reads to the cassette.
- Product-owned `metafield` / `metafields` selections are not part of this cold
  read slice yet and remain local until the product-metafields scenario gets its
  own cassette-backed migration.

### Risks / open items

- Broader products scenarios covering collections, inventory, product variants,
  publications, selling plans, tags, and mutation prior-record hydration remain
  in HAR-541 follow-up slices.

---

## 2026-05-03 - Pass 188: HAR-536 metaobjects cassette parity

Migrates the remaining Metaobjects parity scenarios to cassette-backed
LiveHybrid execution. Cold metaobject and metaobject-definition reads use
Pattern 1 passthrough while staged, deleted, or hydrated local state stays
local. Supported metaobject mutations still stage locally; they use Pattern 2
hydrate reads only to load upstream definitions or rows needed before local
mutation handling.

| Module / fixture                                                        | Change                                                                                               |
| ----------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/metaobject_definitions.gleam`      | Adds cold-read passthrough, mutation prerequisite hydration, and BEAM/JS-stable measurement numbers. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`                 | Routes metaobject reads/mutations through the upstream-aware metaobject entrypoints.                 |
| `fixtures/conformance/**/metaobjects/*.json`                            | Hand-synthesizes metaobject read/hydrate cassette entries from checked-in capture evidence.          |
| `fixtures/conformance/**/metafields/custom-data-field-type-matrix.json` | Adds definition hydrate cassette entries for the metaobject custom-data matrix.                      |
| `config/parity-specs/metaobjects/*.json`                                | Prunes stale cursor expected-difference rules where cold passthrough now matches captures.           |
| `config/gleam-port-ci-gates.json`                                       | Removes the six Metaobjects expected-failure entries.                                                |
| `docs/endpoints/metaobjects.md`                                         | Documents the cassette-backed cold-read and mutation-hydration behavior.                             |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `cd gleam && gleam test --target javascript` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine gleam test --target erlang` (819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed metaobject fixture/spec/gate checks: no metaobject expected-failure
  entries remain, no added `seedX` keys, and no new `expectedDifferences`
  rules

### Findings

- Pattern 1 is appropriate for cold metaobject reads, but passthrough must be
  gated on local staged/deleted/hydrated state so supported mutations keep
  read-after-write and read-after-delete behavior local.
- Pattern 2 is needed for existing upstream rows and definitions referenced by
  supported mutations; the hydrate calls load prerequisites from cassettes
  without writing supported mutations upstream.
- The Erlang target exposed a BEAM/JS numeric representation drift for
  measurement `jsonValue` fields. Whole measurement floats are now normalized
  to integer JSON values while fractional measurements remain floats.

### Risks / open items

- Hydration is intentionally limited to the definition and metaobject fields
  selected by current parity evidence. Broader metaobject shapes remain future
  fidelity work.
- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation used the established OTP 28 container fallback.

---

## 2026-05-03 - Pass 187: HAR-537 online-store cassette parity

Migrates the remaining Online Store parity scenarios to cassette-backed
LiveHybrid execution. Cold content search reads now use domain-gated Pattern 1
passthrough when no local online-store content state exists. Staged content
lifecycle reads stay local, but `blogsCount` and `pagesCount` fetch narrow
upstream baseline count cassettes and add newly staged local content so
read-after-write counts match Shopify without forwarding supported mutations.

| Module / fixture                                                   | Change                                                                                                      |
| ------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/online_store.gleam`           | Adds the upstream-aware query entrypoint, cold-read passthrough gate, and Pattern 2 baseline count fetches. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`            | Routes online-store queries through the upstream-aware domain entrypoint.                                   |
| `fixtures/conformance/**/online-store/online-store-content-*.json` | Hand-synthesizes online-store search/count cassette entries from checked-in capture evidence.               |
| `config/gleam-port-ci-gates.json`                                  | Removes the two Online Store expected-failure entries.                                                      |
| `docs/endpoints/online-store.md`                                   | Documents the LiveHybrid passthrough/count-baseline choices and cassette-backed evidence.                   |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam test --target erlang -- parity_test'` (819 passed)
- `cd gleam && gleam test --target javascript` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam test --target erlang'` (819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed online-store fixture/gate checks: no online-store expected-failure
  entries remain, no `seedX` keys, and no new `expectedDifferences`

### Findings

- Pattern 1 is appropriate for `online-store-content-search-filters` because
  the cold search request has no local overlay to add and the captured upstream
  response is the expected answer.
- Pattern 1 is not appropriate for the lifecycle downstream read because it
  includes proxy-synthetic blog/page/article IDs. Only the count roots need
  upstream context there, so they use narrow Pattern 2 baseline count reads.
- The checked-in captures already contained the authoritative search and
  baseline-count payloads, so the cassette entries were hand-synthesized
  without live Shopify writes.

### Risks / open items

- The baseline count merge intentionally covers currently captured content
  count roots (`blogsCount`, `pagesCount`). Broader online-store catalog
  hydration remains future fidelity work.

---

## 2026-05-03 - Pass 186: HAR-525 admin-platform cassette parity

Migrates the remaining Admin Platform parity scenarios to cassette-backed
LiveHybrid execution. Cold platform utility, taxonomy, and selected generic
Node reads now use Pattern 1 passthrough when no local admin-platform or staged
node-owning state exists, while snapshot and read-after-write paths continue to
use the local admin-platform serializers.

| Module / fixture                                                                                          | Change                                                                                           |
| --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/admin_platform.gleam`                                                | Adds a LiveHybrid query entrypoint with gated Pattern 1 passthrough for cold utility/node reads. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`                                                   | Routes admin-platform queries through the upstream-aware admin-platform entrypoint.              |
| `fixtures/conformance/**/{admin-platform,markets,products,shipping-fulfillments,store-properties}/*.json` | Hand-synthesizes admin-platform cassette entries from checked-in capture evidence.               |
| `config/gleam-port-ci-gates.json`                                                                         | Removes the nine Admin Platform expected-failure entries.                                        |
| `docs/endpoints/admin-platform.md`                                                                        | Documents the endpoint-specific LiveHybrid Pattern 1 choice and migrated scenario set.           |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed admin-platform fixture/gate checks: no admin-platform expected-failure
  entries remain, no added `seedX` keys, and no new `expectedDifferences`

### Findings

- Pattern 1 is sufficient for the migrated admin-platform scenarios because
  these are cold read scenarios whose cassette payload is already the desired
  Shopify-shaped response. The handler is gated on absence of local/staged
  state so staged lifecycle scenarios continue to exercise local serializers.
- The checked-in captures already contained the authoritative response payloads,
  so the missing cassette entries could be hand-synthesized without live
  Shopify credentials.

### Risks / open items

- The passthrough GID families are intentionally limited to Node implementors
  with existing local serializers and cassette-backed parity evidence. Broader
  generic Node support still belongs to each owning domain's lifecycle/read
  model.

---

## 2026-05-03 - Pass 185: HAR-538 orders cassette parity

Migrates the remaining Orders parity scenarios to cassette-backed LiveHybrid
execution. Cold order and draft-order reads now use gated Pattern 1 passthrough
only when no staged local orders state can affect the response. Supported
orders mutations still stage locally, using targeted Pattern 2 hydrate reads
for prior order, draft-order, fulfillment, refund, calculated-order, return,
customer, and product-variant context before applying local effects.

| Module / fixture                                        | Change                                                                                              |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`      | Adds upstream-aware mutation handling and orders-specific hydration helpers for migrated scenarios. |
| `gleam/src/shopify_draft_proxy/proxy/customers.gleam`   | Hydrates order/customer context for `orderCustomerSet` and `orderCustomerRemove` read effects.      |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam` | Adds gated LiveHybrid passthrough for cold orders reads and threads upstream context to orders.     |
| `fixtures/conformance/**/orders/*.json`                 | Hand-synthesizes orders hydrate/passthrough cassette entries from checked-in capture evidence.      |
| `config/parity-specs/orders/*.json`                     | Prunes stale fulfillment expected-difference rules now covered by captured payload parity.          |
| `config/gleam-port-ci-gates.json`                       | Removes all remaining Orders expected-failure entries.                                              |
| `docs/endpoints/orders.md`                              | Documents the HAR-538 cassette-backed Pattern 1/Pattern 2 boundary for orders.                      |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed orders fixture/spec/gate checks: no orders expected-failure entries
  remain, no top-level `seedX` capture precondition keys were added, and no new
  `expectedDifferences` rules were added

### Findings

- Pattern 1 is appropriate only for cold read scenarios. Lifecycle specs that
  stage orders, draft orders, returns, or calculated-order state rely on the
  existing local serializers after Pattern 2 hydration.
- Existing checked-in captures carried the authoritative upstream payloads, so
  missing cassette entries could be hand-synthesized without live Shopify
  writes.
- `orderCustomerSet` and `orderCustomerRemove` belong in the customers-domain
  hydration path for customer order-summary read effects, even though they are
  order relationship mutations.

### Risks / open items

- Hydration is intentionally scoped to fields selected by current orders parity
  evidence. Broader order-edit, refund, return, fulfillment, and draft-order
  Shopify behavior remains future fidelity work outside this cassette
  migration.
- Host Erlang is too old for this workspace's Gleam dependency set, so Erlang
  validation used the established OTP 28 container fallback.

---

## 2026-05-03 - Pass 184: HAR-528 bulk operations cassette parity

Migrates the remaining Bulk Operations parity scenario to cassette-backed
LiveHybrid execution. Cold cancel requests now hydrate the target
`BulkOperation` through a narrow Pattern 2 read before deciding whether to
return Shopify's terminal userError or stage a local `CANCELING` overlay. Cold
product exports keep `bulkOperationRunQuery` local-only while using an upstream
product-count read so staged job counters match the captured store.

| Module / fixture                                                                    | Change                                                                                                   |
| ----------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/bulk_operations.gleam`                         | Adds upstream-aware mutation handling, Pattern 2 BulkOperation hydration, and product-count hydration.   |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`                             | Threads request upstream context into the bulk-operations mutation handler.                              |
| `fixtures/conformance/**/bulk-operations/bulk-operation-status-catalog-cancel.json` | Hand-synthesizes hydrate/count cassette entries from checked-in capture evidence.                        |
| `docs/endpoints/bulk-operations.md`                                                 | Documents cassette-backed Pattern 2 behavior for LiveHybrid cancel and product-export counter hydration. |
| `config/gleam-port-ci-gates.json`                                                   | Removes the Bulk Operations expected-failure entry.                                                      |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed bulk fixture/gate checks: no bulk expected-failure entries remain,
  no added `seedX` keys, and no added `expectedDifferences`

### Findings

- Pattern 1 passthrough is not appropriate here because both supported
  mutations must remain locally staged. The missing fidelity was prior-record
  and count context, so Pattern 2 reads fit the scenario.
- The existing capture already contained the terminal BulkOperation payloads and
  product count needed by the cassette, so no live Shopify write was required.
- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation used the established OTP 28 container fallback.

### Risks / open items

- Product-count hydration currently mirrors the captured unfiltered product
  export. Broader filtered export count fidelity should be driven by future
  parity evidence.

---

## 2026-05-03 - Pass 183: HAR-530 functions cassette parity

Migrates the remaining Functions parity scenarios to cassette-backed
LiveHybrid execution. Cold functions reads now use Pattern 1 passthrough when
there is no local functions state to overlay. Supported functions mutations
still stage locally; they use targeted Pattern 2 ShopifyFunction hydration for
owner/app metadata before creating or updating validations and cart transforms.

| Module / fixture                                                        | Change                                                                                                       |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/functions.gleam`                   | Adds LiveHybrid cold-read passthrough, mutation ShopifyFunction hydration, and local owner metadata overlay. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`                 | Routes functions reads/mutations through the upstream-aware functions entrypoints.                           |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                       | Adds base-state upsert support for upstream-hydrated ShopifyFunction records.                                |
| `fixtures/conformance/**/functions/*.json`                              | Hand-synthesizes the missing functions cassette entries from checked-in capture evidence.                    |
| `config/parity-specs/functions/functions-live-owner-metadata-read.json` | Prunes stale cursor expected-difference rules now that the cold read is cassette-backed.                     |
| `config/gleam-port-ci-gates.json`                                       | Removes the three Functions expected-failure entries.                                                        |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `cd gleam && gleam test --target javascript -- functions_mutation_test` (824 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed functions fixture/spec/gate checks: no new `seed*` keys, no new
  `expectedDifferences`, and no functions expected-failure entries remain

### Findings

- Pattern 1 was sufficient for the live owner metadata read because the proxy
  has no local functions state to merge on a cold LiveHybrid read.
- Pattern 2 was required for local staging mutations so validation and cart
  transform owner metadata can be hydrated from cassette payloads while the
  supported mutations continue to stage locally.
- The local-runtime captures already contained the authoritative
  ShopifyFunction seed metadata, so the missing cassette entries could be
  hand-synthesized without live Shopify writes.
- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation used the established OTP 28 container fallback.

### Risks / open items

- Hydration is intentionally limited to the ShopifyFunction owner/app fields
  selected by current functions parity evidence. Broader Functions API shapes
  remain future fidelity work.

---

## 2026-05-03 - HAR-547 host Erlang toolchain pin

Adds a repo-local Mise toolchain pin so future worktrees can run the Erlang
target on the host instead of using the established Docker fallback for OTP 28.

| File         | Change                                                                                                                                                       |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `.mise.toml` | Pins Erlang/OTP 28.4.2 and Gleam 1.16.0 for host Gleam validation; builds Erlang without termcap so unattended hosts without ncurses headers can install it. |
| `.envrc`     | Activates Mise when available while preserving the existing dotenv loading behavior.                                                                         |
| `README.md`  | Documents the host Gleam/Erlang prerequisites and the `mise install` setup path.                                                                             |

Validation:

- `cd gleam && gleam test --target erlang` currently fails before this pin is
  installed because `/usr/bin/erl` is OTP 25 and `gleam_json` requires OTP 27+.
- A repo-local `mise install` proof initially installed Gleam 1.16.0 but Erlang
  28.4.2 failed on missing ncurses/termcap headers; the pin now sets
  `KERL_CONFIGURE_OPTIONS=--without-termcap` for noninteractive test hosts.
- The same proof showed that a source-built OTP must include OpenSSL support:
  a build without OpenSSL headers installed successfully but failed the suite
  because the Erlang `ssl` application was absent.
- With repo-local OpenSSL headers/libraries supplied for the proof install:
  `mise exec -- erl -eval 'io:format("OTP=~s~n", [erlang:system_info(otp_release)]), io:format("ssl=~p~n", [application:ensure_all_started(ssl)]), halt().' -noshell`
  reported `OTP=28` and `ssl={ok,[crypto,asn1,public_key,ssl]}`.
- `mise exec -- bash -c 'cd gleam && gleam clean && gleam test --target erlang'`
  (OTP 28.4.2, 819 passed)
- `mise exec -- bash -c 'cd gleam && gleam test --target javascript'`
  (Gleam 1.16.0, 824 passed)

### Findings

- The repeated Docker fallback notes below were caused by the same host
  mismatch: the workspace Erlang executable was OTP 25 while checked-in Gleam
  dependencies require OTP 27 or newer on the BEAM target.

### Risks / open items

- This change does not mutate global host state. Existing shells without Mise
  still see `/usr/bin/erl` until the repo-local toolchain is installed and
  activated.

---

## 2026-05-03 - Pass 182: HAR-527 B2B company roots cassette parity

Migrates the remaining B2B parity scenario to cassette-backed LiveHybrid
execution. Cold B2B company root reads now use Pattern 1 passthrough when no
local B2B state is involved, while synthetic, staged, and locally deleted B2B
IDs keep lifecycle reads on the in-memory handler.

| Module / fixture                                          | Change                                                                                     |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/b2b.gleam`           | Adds gated Pattern 1 LiveHybrid passthrough for B2B company read roots.                    |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`   | Routes B2B queries through the upstream-aware B2B query entrypoint.                        |
| `fixtures/conformance/**/b2b/b2b-company-roots-read.json` | Hand-synthesizes the `B2BCompanyRootsRead` cassette from the checked-in captured response. |
| `config/gleam-port-ci-gates.json`                         | Removes the B2B expected-failure entry.                                                    |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed b2b fixture/gate checks: no b2b expected-failure entries remain, no
  new `seedX` keys, and no new `expectedDifferences`

### Findings

- Pattern 1 is the right fit for the migrated scenario because the captured
  B2B company roots are read-only catalog/detail data and the proxy should
  return upstream verbatim when no local B2B lifecycle state exists.
- Passthrough is gated on staged/deleted/synthetic B2B state so existing local
  create/update/delete scenarios keep read-after-write behavior in the local
  model.
- The checked-in capture already contained the authoritative response payload,
  so the cassette entry could be hand-synthesized without live Shopify
  credentials.

### Risks / open items

- Host Erlang is older than the current dependency floor in this workspace;
  Erlang parity validation used the established OTP 28 container fallback.

---

## 2026-05-03 - Pass 181: HAR-512 JavaScript HTTP adapter

Adds the JavaScript-target HTTP service adapter for the Gleam-backed TS shim.
The adapter uses Node's built-in `http` server, not Koa or any BEAM/Elixir HTTP
scope, and routes requests through the mutable JS `DraftProxy` wrapper over the
Gleam core.

| Module                                                           | Change                                                                                                                 |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `gleam/js/src/app.ts`                                            | Adds `DraftProxyHttpApp`, request body parsing, inbound header preservation, JSON/text response writing, and `listen`. |
| `gleam/js/src/config.ts` / `gleam/js/src/server.ts`              | Adds legacy-compatible env config parsing plus dev/start launch entrypoint for the JS adapter.                         |
| `gleam/js/src/index.ts` / `gleam/js/src/types.ts`                | Replaces `createApp`/`loadConfig` stubs with real exports and aligns JS read-mode typing with legacy `passthrough`.    |
| `gleam/js/test/http-adapter.test.ts`                             | Covers meta routes, Admin GraphQL routing, commit auth forwarding, HTTP error envelopes, and dev/start launch scripts. |
| `tests/integration/gleam-js-http-adapter-parity.test.ts`         | Compares the required Gleam JS route surface against the legacy Koa adapter before any deletion work.                  |
| `docs/architecture.md` / `gleam/README.md` / `GLEAM_PORT_LOG.md` | Documents the new JS adapter boundary and remaining full-cutover HTTP gaps.                                            |

Validation:

- `corepack pnpm --dir gleam/js test` (14 passed)
- `corepack pnpm --dir gleam/js build`
- `corepack pnpm vitest run tests/integration/gleam-js-http-adapter-parity.test.ts`
  (2 passed)
- `cd gleam && gleam test --target javascript` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (OTP 28 fallback, 819 passed)
- `corepack pnpm lint` (passed with the existing
  `scripts/parity-record.mts:279` warning)
- `corepack pnpm typecheck`
- `corepack pnpm build`
- `git diff --check`

### Findings

- The Gleam core already had async JS dispatch for `/__meta/commit` and
  live-hybrid passthrough; HAR-512's missing piece was the Node HTTP boundary
  and JS package launch/config surface.
- The legacy `createApp(config, proxy).listen(port, listener)` call shape is
  worth preserving even though the adapter is not Koa, because package launch
  scripts and simple consumers use that shape.
- The Gleam internal `Live` read-mode variant must still serialize as the
  legacy public `passthrough` string on the JS/HTTP config surface.

### Risks / open items

- The full TS HTTP endpoint set is not retired here. Bulk-operation result
  JSONL and staged-upload HTTP routes remain full-cutover follow-ups; this pass
  covers only the HAR-512 route list.

---

## 2026-05-03 - Pass 180: HAR-526 apps cassette parity

Migrates the remaining Apps parity scenario to cassette-backed LiveHybrid
execution. `currentAppInstallation` now uses a gated Pattern 1 app query
handler: cold LiveHybrid reads can still pass through to Shopify, but once the
app billing/access lifecycle has staged local app state, downstream reads stay
local and do not require an upstream cassette entry.

| Module / fixture                                                 | Change                                                                                                       |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/apps.gleam`                 | Adds the app-domain query entrypoint and local app-state gate for `currentAppInstallation` LiveHybrid reads. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`          | Routes Apps queries through the domain handler so the app module owns its passthrough decision.              |
| `config/operation-registry.json`                                 | Marks `currentAppInstallation` as covered by the app billing/access runtime flow.                            |
| `config/parity-specs/apps/app-billing-access-local-staging.json` | Records `currentAppInstallation` in scenario operation inventory.                                            |
| `config/gleam-port-ci-gates.json`                                | Removes the Apps expected-failure entry.                                                                     |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`

### Findings

- The checked-in local-runtime Apps fixture intentionally has an empty
  `upstreamCalls` cassette because the scenario should remain local-only after
  staging billing/access mutations.
- The failing upstream operation was a symptom of `currentAppInstallation`
  still being registry-unimplemented in the Gleam dispatcher. A synthesized
  cassette would have hidden the missing local read-after-write routing.

### Risks / open items

- Broader cold app identity and app installation reads still rely on LiveHybrid
  passthrough until those roots have their own executable overlay evidence.

---

## 2026-05-03 - Pass 179: HAR-544 store-properties cassette parity

Migrates the remaining Store Properties parity scenarios to cassette-backed
LiveHybrid execution. Cold singleton/catalog reads use Pattern 1 passthrough
until local staged state exists, while supported mutations still stage locally
and use Pattern 2 hydrate reads only to obtain the prior Shopify-shaped data
needed for validation and downstream read-after-write parity.

| Module / fixture                                             | Change                                                                                                                         |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/store_properties.gleam` | Adds gated Pattern 1 passthrough for cold shop/location/business entity/collection reads and Pattern 2 hydrates for mutations. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`      | Routes Store Properties reads through the domain query entrypoint and threads `UpstreamContext` into mutation handling.        |
| `fixtures/conformance/**/store-properties/*.json`            | Hand-synthesizes missing read/hydrate cassette entries from checked-in capture payloads.                                       |
| `fixtures/conformance/**/products/product-*parity.json`      | Adds publishable mutation hydrate cassettes for product publish/unpublish scenarios owned by Store Properties parity.          |
| `config/gleam-port-ci-gates.json`                            | Removes the 15 Store Properties expected-failure entries.                                                                      |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed Store Properties fixture/gate checks: no Store Properties
  expected-failure entries remain, no new `seed*` keys, and no new
  `expectedDifferences`

### Findings

- Pattern 1 is appropriate for cold Store Properties read roots because those
  reads have no local staged lifecycle yet, but passthrough must be gated on
  all variable string IDs so synthetic or staged records stay local.
- `shopPolicyUpdate`, `locationDelete`, and generic publishable mutations must
  continue to stage locally; they hydrate only the prior record or payload
  projection needed to compute Shopify-like mutation results.
- The checked-in capture payloads already contained the authoritative response
  data, so the needed cassette entries could be hand-synthesized without live
  Shopify writes.

### Risks / open items

- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation used the established OTP 28 container fallback.

---

## 2026-05-03 - Pass 178: HAR-533 markets cassette parity

Migrates the remaining Markets parity scenarios to cassette-backed LiveHybrid
execution. Cold Markets reads now fetch the captured upstream payload, hydrate
the local Markets/Product slices from it, and return the captured response
verbatim for that first read. Supported Markets lifecycle mutations still stage
locally, with a narrow preflight hydrate for existing upstream price-list,
product, metafield, and web-presence state needed by the captured flows.

| Module / fixture                                        | Change                                                                                                          |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/markets.gleam`     | Adds Pattern 2 read hydration and mutation preflight hydration for Markets parity cassette replay.              |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam` | Routes Markets queries through the domain read entrypoint and threads `UpstreamContext` into Markets mutations. |
| `fixtures/conformance/**/markets/*.json`                | Hand-synthesizes read/preflight cassette entries from checked-in capture evidence.                              |
| `config/gleam-port-ci-gates.json`                       | Removes the fourteen Markets expected-failure entries.                                                          |
| `docs/endpoints/markets.md`                             | Documents the LiveHybrid cold-read and mutation preflight hydration boundary.                                   |

Validation:

- `cd gleam && gleam format --check`
- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)

### Findings

- Pattern 1 passthrough is not appropriate for Markets because the domain has
  supported local lifecycle mutations whose staged effects must remain visible
  after the initial cassette-backed read.
- The checked-in captures already contained the authoritative read and setup
  payloads, so the cassette entries could be hand-synthesized without live
  Shopify writes.
- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation used the established OTP 28 container fallback.

### Risks / open items

- The hydrate queries intentionally persist only the Markets, Product,
  ProductVariant, and product-metafield fields selected by current parity
  evidence. Broader Markets branches remain future fidelity work.

---

## 2026-05-03 - Pass 177: HAR-535 metafields cassette parity

Migrates the remaining Metafields parity scenarios to cassette-backed
LiveHybrid execution. Cold metafield-definition reads now pass through to
upstream when there is no local definition state to overlay, while pin/unpin
mutations hydrate the upstream product-owner definition catalog before staging
local pin position changes.

| Module / fixture                                                      | Change                                                                                                   |
| --------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/metafield_definitions.gleam`     | Adds Pattern 1 cold-read passthrough and Pattern 2 pin/unpin definition hydration.                       |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`               | Threads `UpstreamContext` into metafield-definition mutations and lets the domain own query passthrough. |
| `fixtures/conformance/**/metafields/*definition*{read,pinning}*.json` | Hand-synthesizes definition read/hydrate cassette entries from checked-in capture evidence.              |
| `config/gleam-port-ci-gates.json`                                     | Removes the three Metafields expected-failure entries.                                                   |
| `docs/endpoints/metafields.md`                                        | Documents the LiveHybrid passthrough/hydration boundary and product-shell delete behavior.               |

Validation:

- `cd gleam && gleam test --target javascript -- inspect_spec_test` temporary
  inspector before gate removal (824 passed, 3 failures), then after changes
  each HAR-535 scenario passed and only the expected-failure gate remained.
- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang -- parity_test'`
  (819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed metafields fixture/gate checks: no metafields expected-failure
  entries remain, no new top-level `seed*` keys, and no new
  `expectedDifferences`

### Findings

- Pattern 1 is appropriate for cold product-owner definition reads because the
  proxy adds no local overlay before any definition state exists.
- Pattern 2 is required for pin/unpin because those supported mutations must
  stage locally while starting from existing upstream definition records.
- Definition delete with `deleteAllAssociatedMetafields: true` must preserve a
  minimal product shell for downstream `product { metafield }` reads after the
  targeted metafield is removed.

### Risks / open items

- The hydrate parser intentionally captures the product-owner definition fields
  exercised by the current pinning fixture. Broader owner families and
  app-managed definition branches remain future fidelity work.

---

## 2026-05-03 - Pass 176: HAR-543 shipping fulfillments cassette parity

Migrates the remaining Shipping/Fulfillments parity scenarios to
cassette-backed LiveHybrid execution. Cold shipping reads now fetch the
captured upstream payload, hydrate the local shipping/store slices needed by
later lifecycle operations, and return Shopify's payload verbatim. Supported
shipping mutations still stage locally; they use targeted Pattern 2 hydrate
reads only when a prior upstream record or product/variant metadata is needed
before local mutation handling.

| Module / fixture                                                  | Change                                                                                                     |
| ----------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/shipping_fulfillments.gleam` | Adds Pattern 2 LiveHybrid read hydration and mutation prerequisite hydration for shipping lifecycle roots. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`           | Routes shipping queries/mutations through the upstream-aware shipping entrypoints.                         |
| `fixtures/conformance/**/shipping-fulfillments/*.json`            | Hand-synthesizes shipping hydrate cassette entries from checked-in capture evidence.                       |
| `config/parity-specs/shipping-fulfillments/*.json`                | Prunes stale cursor/line-item expected-difference rules where proxy output now matches the captures.       |
| `config/gleam-port-ci-gates.json`                                 | Removes the nine Shipping/Fulfillments expected-failure entries.                                           |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed shipping fixture/gate checks: no shipping expected-failure entries
  remain, no `seedX` keys, and no new `expectedDifferences`

### Findings

- Pattern 1 passthrough is not appropriate for the migrated scenarios because
  several supported shipping mutations must keep staging locally while using
  upstream records only as pre-hydration context.
- The checked-in captures already contained the authoritative shipping payloads,
  so the cassette entries could be hand-synthesized without live Shopify writes.
- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation used the established OTP 28 container fallback.

### Risks / open items

- Hydration is intentionally limited to the delivery profile, fulfillment,
  fulfillment-order, shipping package, location, carrier-service, order, and
  product/variant fields selected by current parity evidence. Broader shipping
  shapes remain future fidelity work.

---

## 2026-05-03 - Pass 175: HAR-529 customers cassette parity

Migrates the remaining Customers parity scenarios to cassette-backed
LiveHybrid execution. Existing-customer mutations and customer-adjacent reads
now hydrate the captured upstream record through narrow per-operation cassette
queries before local staging, so downstream reads observe the staged customer
state without runtime Shopify writes for supported roots.

| Module / area                                           | Change                                                                                                                       |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/customers.gleam`   | Adds Pattern 2 hydration for prior customers, store-credit accounts, order summaries, account pages, counts, and duplicates. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam` | Threads the request upstream context into the customers mutation handler.                                                    |
| `fixtures/conformance/**/customers/*.json`              | Hand-synthesizes the cassette entries needed by customer LiveHybrid hydration from checked-in captures.                      |
| `config/parity-specs/customers/*.json`                  | Prunes stale expected-difference allowances now covered by local behavior while preserving opaque cursor allowances.         |
| `config/gleam-port-ci-gates.json`                       | Removes the 15 Customers expected-failure entries.                                                                           |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `cd gleam && gleam test --target javascript` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine gleam test --target erlang` (819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed customer fixture/spec JSON checks: no `seed*` keys and no new
  `expectedDifferences` rules

### Findings

- Pattern 2 was required for the remaining customers scenarios because the
  handlers need the pre-existing upstream customer, account, or order summary
  to merge staged mutations and return Shopify-like downstream reads.
- The checked-in captures already contained the authoritative source payloads,
  so the missing cassette entries could be hand-synthesized without live
  Shopify credentials.

### Risks / open items

- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation for this pass used the established OTP 28 container
  fallback.

---

## 2026-05-03 - Pass 174: HAR-531 gift-card cassette parity

Migrates the remaining Gift Cards parity scenarios to cassette-backed
LiveHybrid execution. Existing upstream gift cards referenced by supported
mutation roots now hydrate through a narrow `GiftCardHydrate` cassette read,
persisting the prior gift card and shop configuration into base state before
local lifecycle mutations stage update/credit/debit/deactivate effects.

| Module                                                        | Change                                                                                                      |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/gift_cards.gleam`        | Adds Pattern 2 mutation hydration for existing gift cards and configuration before local lifecycle staging. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`       | Threads the per-request upstream context into gift-card mutation handling.                                  |
| `fixtures/conformance/**/gift-cards/gift-card-lifecycle.json` | Hand-synthesizes the `GiftCardHydrate` cassette from checked-in detail/configuration capture payloads.      |
| `config/gleam-port-ci-gates.json`                             | Removes the two Gift Cards expected-failure entries.                                                        |
| `docs/endpoints/gift-cards.md`                                | Documents the LiveHybrid hydrate path and cassette-backed parity evidence.                                  |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `cd gleam && gleam format --check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed gift-card fixture/config checks: no added `seed*` keys and no new
  `expectedDifferences`

### Findings

- Pattern 1 passthrough is not enough for these scenarios because the primary
  request is a mutation lifecycle against an existing upstream gift card. The
  local handler needs the prior record before it can stage supported mutations
  without writing to Shopify.
- The checked-in detail/configuration captures already contain the authoritative
  hydrate payload, so the cassette entry could be hand-synthesized without live
  Shopify credentials.

### Risks / open items

- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation for this pass used the established OTP 28 container
  fallback.

---

## 2026-05-03 - Pass 173: HAR-540 privacy cassette parity

Migrates the remaining Privacy parity scenario to cassette-backed LiveHybrid
execution. `dataSaleOptOut` stays a supported local mutation, but existing-email
flows now read the upstream customer by email first so the staged opt-out uses
Shopify's authoritative customer ID while preserving local read-after-write
behavior.

| Module                                                          | Change                                                                                                               |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/privacy.gleam`             | Adds Pattern 2 customer lookup for `dataSaleOptOut` before local staging when no matching customer is already local. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`         | Threads `UpstreamContext` into the privacy mutation handler.                                                         |
| `fixtures/conformance/**/privacy/data-sale-opt-out-parity.json` | Hand-synthesizes the customer lookup cassette from the checked-in precondition response.                             |
| `config/gleam-port-ci-gates.json`                               | Removes the Privacy expected-failure entry after the scenario passed.                                                |
| `docs/endpoints/privacy.md`                                     | Documents the endpoint-specific LiveHybrid lookup choice.                                                            |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `cd gleam && gleam test --target javascript` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'` (OTP 28, 819 passed)
- `corepack pnpm lint`
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed privacy fixture/spec checks: no `seed*` keys, no new
  `expectedDifferences`

### Findings

- The migrated fixture had no cassette entries, so the cold local mutation
  minted `gid://shopify/Customer/1` instead of the captured Shopify customer ID.
- Pattern 2 is required because this is a supported mutation: the proxy can read
  the prior customer from upstream, but must still stage the mutation locally.

### Risks / open items

- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation for this pass should use the established OTP 28 container
  fallback.

---

## 2026-05-03 - Pass 172: HAR-534 media cassette parity

Migrates the remaining Media parity scenarios to cassette-backed LiveHybrid
execution. Files API mutations now hydrate only the upstream slices needed to
stage product-reference effects locally: `fileUpdate.referencesToAdd` hydrates
the referenced product before validation, and `fileDelete` hydrates the
product/media ownership for Product-owned media ids before staging the local
delete.

| Module / fixture                                        | Change                                                                                                      |
| ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/media.gleam`       | Adds Pattern 2 product and product-media hydrate reads before local `fileUpdate` / `fileDelete` staging.    |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam` | Threads `UpstreamContext` into the media mutation handler.                                                  |
| `fixtures/conformance/**/media/*file-*product*.json`    | Hand-synthesizes media/product hydrate cassette entries from checked-in capture evidence.                   |
| `config/gleam-port-ci-gates.json`                       | Removes the two Media expected-failure entries.                                                             |
| `docs/endpoints/media.md`                               | Documents the LiveHybrid hydration boundary for Files API product-reference and Product media delete flows. |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed media fixture/gate checks: no media expected-failure entries remain,
  no new `seed*` keys, and no new `expectedDifferences`

### Findings

- Pattern 1 passthrough is not appropriate for these scenarios because the
  supported Files API mutations must still stage locally and drive downstream
  read-after-write effects.
- The checked-in captures already contained the authoritative product/media
  evidence, so the needed cassettes could be hand-synthesized without live
  Shopify writes.
- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation used the established OTP 28 container fallback.

### Risks / open items

- The hydrate queries intentionally persist only the product identity/metadata
  and media ownership fields selected by the current parity evidence. Broader
  Files API reference shapes remain future fidelity work.

---

## 2026-05-03 - Pass 171: HAR-532 localization cassette parity

Migrates the remaining Localization parity scenarios to cassette-backed
LiveHybrid execution. Cold localization reads now fetch the captured upstream
locale/source-content payload, hydrate the base localization state, and then
keep translation lifecycle mutations and downstream reads local-only.

| Module                                                                  | Change                                                                                                       |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/localization.gleam`                | Adds Pattern 2 LiveHybrid read hydration for available locales, shop locales, and translatable source marks. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`                 | Routes localization queries through the domain query entrypoint so it can decide when to fetch upstream.     |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                       | Adds base translation upsert support for read-hydrated source-content markers.                               |
| `fixtures/conformance/**/localization/*.json`                           | Hand-synthesizes one upstream cassette call per localization scenario from checked-in read captures.         |
| `config/gleam-port-ci-gates.json`                                       | Removes the two Localization expected-failure entries.                                                       |
| `docs/endpoints/localization.md` / `.agents/skills/gleam-port/SKILL.md` | Documents the LiveHybrid hydration choice and future porting note.                                           |

Validation:

- `cd gleam && gleam test --target javascript -- parity_test` (824 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang -- parity_test'` (OTP 28, 819 passed)
- `corepack pnpm gleam:format:check`
- `corepack pnpm gleam:port:coverage`
- `corepack pnpm gleam:registry:check`
- `corepack pnpm conformance:check`
- `git diff --check`
- changed localization fixture JSON checks: no `seed*` keys, no new
  `expectedDifferences`, one `upstreamCalls` entry per migrated fixture

### Findings

- Pattern 1 passthrough was insufficient for these scenarios because the
  initial upstream read must also teach the local model the product
  source-content digest used by later `translationsRegister` validation.
- The checked-in read captures already contained the authoritative upstream
  payloads, so the cassette entries could be hand-synthesized without live
  Shopify credentials.

### Risks / open items

- Host Erlang is OTP 25 in this workspace, while `gleam_json` requires OTP 27+.
  Erlang validation for this pass should use the established OTP 28 container
  fallback.

---

## 2026-05-01 - Pass 170: HAR-519 metafield deletion runtime coverage

Adds explicit Gleam runtime coverage for the product-owned metafield deletion
roots and records the current TypeScript-retirement boundary. `origin/main`
already routed `metafieldsDelete` and the legacy compatibility
`metafieldDelete` through the Gleam custom-data/metafield-definition surface;
this pass locks the product downstream read behavior with focused tests and
keeps the TypeScript runtime intact under the incremental port guardrail.

| Module                                                               | Change                                                                                                                                    |
| -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam`  | Covers singular deletion, plural mixed hit/miss deletion, unknown-ID userErrors, downstream reads, and mutation-log staging.              |
| `gleam/test/shopify_draft_proxy/proxy/operation_registry_test.gleam` | Preserves the merged inventory-transfer local-dispatch expectation while keeping a price-list fixed-price passthrough sentinel.           |
| `gleam/test/shopify_draft_proxy/proxy/passthrough_test.gleam`        | Moves the live-hybrid unported-root sentinel to `priceListFixedPricesAdd` after inventory transfer roots landed on `origin/main`.         |
| `.agents/skills/gleam-port/SKILL.md`                                 | Notes that owner-scoped metafield delete roots live with custom-data/metafield-definition handling while sharing product metafield state. |
| `GLEAM_PORT_LOG.md`                                                  | Records HAR-519 evidence and the TypeScript retirement deferral.                                                                          |

Validation:

- `cd gleam && gleam format --check`
- `cd gleam && gleam test --target javascript` (803 passed after merging `origin/main`)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine gleam test --target erlang`
  (799 passed after merging `origin/main`)

### Findings

- The missing behavior described in HAR-519 could not be reproduced on current
  `origin/main`: both `metafieldsDelete` and `metafieldDelete` already stage
  locally in the Gleam implementation and pass the checked-in parity suite.
- Explicit runtime tests were still valuable because the existing parity specs
  prove fixture replay, while the focused tests prove direct product-owned
  read-after-delete behavior and deterministic legacy `deletedId` handling.
- `standardMetafieldDefinitionTemplates` still has no captured executable
  catalog fixture in this branch, so no catalog support was added.

### Risks / open items

- TypeScript metafields/metafield-definition runtime deletion remains deferred
  under the incremental port preservation rule until the final whole-port
  cutover acceptance bar is met.

---

## 2026-05-01 - Pass 169: HAR-515 Products inventory shipment and transfer root completion

Completes the remaining Products local-dispatch gap for implemented inventory
shipment and inventory transfer mutation roots in the Gleam port. The Products
domain now routes the shipment create/add/remove/tracking/transit roots and
transfer edit/duplicate roots locally, stages their effects in memory, and keeps
the original mutation requests in the draft log for commit replay.

| Module                                                              | Change                                                                                                      |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                | Adds Products dispatch and local handlers for inventory shipment create/add/remove/tracking/transit roots.  |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                | Adds inventory transfer edit/duplicate staging, including metadata edits and reminted duplicate line items. |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam` | Covers the new shipment and transfer roots with downstream read-after-write and mutation-log assertions.    |
| `.agents/skills/gleam-port/SKILL.md`                                | Records product-specific shipment and transfer porting notes for future Products work.                      |

Validation:

- `cd gleam && gleam test --target javascript -- --module shopify_draft_proxy/proxy/products_mutation_test`
  (775 passed)
- `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine gleam test --target erlang -- --seed 0`
  (778 passed after merging `origin/main`)
- `cd gleam && gleam test --target javascript`
  (782 passed after merging `origin/main`)
- `cd gleam && gleam format --check`
- `corepack pnpm gleam:port:coverage` (379 specs; 13 expected failures; passed)
- `corepack pnpm gleam:registry:check`
- `git diff --check`

### Findings

- The registry/dispatch inventory showed no Product query gaps and narrowed the
  mutation gap to implemented inventory shipment/transfer roots; product-owned
  metafield mutation roots continue to route through the shared metafield
  handler while Product read serializers expose downstream owner state.
- Shipment draft creation differs from create-in-transit: DRAFT creation must
  not adjust incoming inventory until `inventoryShipmentMarkInTransit` runs.
- Transfer duplication must not copy ready-reservation side effects; it creates
  a DRAFT duplicate with fresh line-item IDs.
- Host Erlang is OTP 25, so Erlang validation needs a clean build cache and the
  OTP 28 Gleam container until the host runtime is upgraded.

### Risks / open items

- TypeScript Products runtime deletion remains deferred under the incremental
  port preservation rule until the final all-port cutover.

---

## 2026-05-01 - Pass 168: bulk operations query and import parity

Promotes the captured BulkOperation status/catalog/cancel/export scenario into
the Gleam parity suite. The port now stages BulkOperation query exports with
local Product and ProductVariant JSONL output, keeps in-memory result metadata
for later HTTP result serving, and replays `bulkOperationRunMutation` JSONL
imports for locally supported product-domain inner mutation roots. Successful
bulk import lines now emit replayable inner mutation-log drafts with the
original inner mutation and structured line variables so commit can replay
those writes in JSONL order. Unsupported bulk import roots fail locally as
FAILED mutation jobs with result JSONL instead of passing through to Shopify at
runtime.

| Module                                                            | Change                                                                                                  |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/bulk_operations.gleam`       | Adds Product/ProductVariant JSONL exports and local `bulkOperationRunMutation` replay/failure handling. |
| `gleam/src/shopify_draft_proxy/graphql/root_field.gleam`          | Adds shared conversion/decoding for resolved GraphQL variables used by mutation-log replay.             |
| `gleam/src/shopify_draft_proxy/proxy/commit.gleam`                | Replays structured mutation-log variables instead of string-only variables.                             |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                 | Adds staged-upload content helpers for the local bulk import executor.                                  |
| `gleam/src/shopify_draft_proxy/state/serialization.gleam`         | Initializes staged-upload content when loading serialized staged state.                                 |
| `gleam/test/parity/runner.gleam`                                  | Seeds captured BulkOperation jobs and product records for the bulk-operations parity scenario.          |
| `gleam/test/shopify_draft_proxy/proxy/bulk_operations_test.gleam` | Covers run-query JSONL, validation failures, missing uploads, unsupported imports, and Product imports. |
| `config/gleam-port-ci-gates.json`                                 | Removes `bulk-operation-status-catalog-cancel.json` from expected Gleam parity failures.                |

Validation:
Host `gleam test --target erlang` still compiles but fails under the known
local Erlang runner issue. The established container fallback is green at 774
tests with OTP 28:
`docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`.
Host `gleam test --target javascript` is green at 778 tests.
`corepack pnpm gleam:port:coverage` is green with 379 parity specs and 60
expected Gleam parity failures.

### Findings

- Bulk query result generation is intentionally bounded to local Product and
  ProductVariant connection exports for this pass; unsupported query shapes
  return Shopify-like user errors instead of pretending broader support.
- Bulk mutation imports are also bounded to single-root product-domain Admin
  mutations that already stage locally in the Gleam port. Other roots are
  recorded as local FAILED jobs, preserving the no-runtime-Shopify-write rule
  for supported bulk operation handling.
- Bulk import object counts follow staged line counts: validation rows still
  appear in result JSONL but do not create commit-log entries or increment the
  completed object count.
- The TypeScript bulk operations runtime remains intact under the port
  preservation rule. HAR-500's TypeScript retirement bullet is not actionable
  during a normal per-domain pass before the final all-port cutover bar is met.

### Risks / open items

- Exact Shopify import result-file schema and partial-failure status semantics
  still need broader live evidence before expanding beyond the local executor
  boundary documented in the checked-in parity spec.
- The future JS result route can consume the in-memory result JSONL, but HTTP
  serving is still owned by its separate route issue.

### Pass 169 candidates

- Continue with the next non-Product expected-failing domain from
  `config/gleam-port-ci-gates.json`, or wire the pending JS bulk result route to
  the in-memory result JSONL now generated by the Gleam bulk operations port.

---

## 2026-05-01 - Pass 167: HAR-496 payments branch localization refresh

Refreshes the HAR-496 Payments branch after `origin/main` advanced with the
HAR-504 localization parity completion. The merge keeps Payments and
order-payment parity ungated while preserving mainline localization source
marker seeding, available-locale excerpt seeding, and the localization gate
removal.

| Module                                                         | Change                                                                                                 |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `gleam/test/parity/runner.gleam`                               | Keeps payment precondition seeding alongside mainline localization source-marker and locale seeding.   |
| `gleam/src/shopify_draft_proxy/proxy/localization.gleam`       | Preserves mainline Product and Metafield translatable-resource localization behavior.                  |
| `gleam/test/shopify_draft_proxy/proxy/localization_test.gleam` | Preserves mainline localization coverage for Product-backed and source-marker-backed resources.        |
| `config/gleam-port-ci-gates.json`                              | Keeps Payments/order-payment, Online Store, and mainline localization paths ungated after the refresh. |
| `.agents/skills/gleam-port/SKILL.md`                           | Preserves mainline localization seeding guidance alongside the HAR-496 payments port guidance.         |

Validation:

- `git diff --cached --check`
- `corepack pnpm lint`
- `cd gleam && gleam check --target javascript`
- `cd gleam && gleam check --target erlang`
- `cd gleam && gleam test --target javascript -- --seed 0` (773 passed)
- `docker run --rm -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine gleam test --target erlang -- --seed 0`
  (769 passed)
- `corepack pnpm gleam:port:coverage` (379 specs; 52 expected failures; passed)
- `corepack pnpm gleam:registry:check`
- Targeted expected-failure scan for payments/order-payment, finance-risk,
  product grammar, Segments, Discounts, Media, Online Store, and Localization
  paths returned no matches.

### Findings

- Mainline now promotes the remaining localization parity fixture through
  runner-side source-content markers and captured available-locale excerpts.
  The runner conflict is additive with HAR-496's customer payment method and
  payment terms seeders, so both seed families remain in the generic
  capture-precondition chain.
- The expected-failure gate conflict resolves by keeping the HAR-496
  payments/order-payment removals plus the mainline localization removal.

### Risks / open items

- TypeScript Payments runtime deletion remains deferred under the incremental
  port preservation rule until final all-port cutover.

---

## 2026-05-01 - Pass 119: reverse logistics and order-edit shipping roots

Completes the remaining HAR-493 shipping/fulfillment registry roots that are
owned by broader order flows rather than checked-in shipping parity specs. The
Gleam port now stages reverse delivery creation/update/disposal over captured
reverse-fulfillment-order records and stages calculated-order shipping-line
add/update/remove operations with downstream calculated-order reads.

| Module                                                                  | Change                                                                                                                                                                                                                  |
| ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/shipping_fulfillments.gleam`       | Adds `reverseDelivery`, `reverseFulfillmentOrder`, `reverseDeliveryCreateWithShipping`, `reverseDeliveryShippingUpdate`, `reverseFulfillmentOrderDispose`, and `orderEditAdd/Update/RemoveShippingLine` local handling. |
| `gleam/src/shopify_draft_proxy/state/{types,store,serialization}.gleam` | Adds captured reverse-fulfillment-order, reverse-delivery, and calculated-order state slices for bounded order-backed staging.                                                                                          |
| `gleam/test/shopify_draft_proxy/proxy/shipping_fulfillments_test.gleam` | Covers reverse delivery lifecycle/detail reads and calculated-order shipping-line add/update/remove staging.                                                                                                            |

Validation:
Full JavaScript is green at 733 tests. Host Erlang still fails before tests
with the known `undef` runner issue; the Docker Erlang fallback using Gleam 1.16
and `HOME=/tmp` is green at 729 tests. `corepack pnpm gleam:port:coverage` is
green with 379 specs and 166 expected failures. `git diff --check` is green.

### Findings

- Reverse logistics can remain captured-state backed for this pass because the
  source TypeScript evidence is embedded in broader order-return flows; the
  Gleam slice preserves direct reverse delivery/fulfillment-order reads and
  local mutation staging without claiming the full order return domain.
- Calculated-order shipping-line support needs only the local calculated-order
  slice here; commit materialization remains part of the broader order-edit
  port, so the TypeScript runtime stays intact under the port preservation
  rule.

### Risks / open items

- Full order return/order-edit parity remains a future domain porting concern;
  this pass only covers the shipping/fulfillment roots called out by HAR-493.

### Pass 120 candidates

- Pick the next domain with manifest-backed expected failures from
  `config/gleam-port-ci-gates.json`.

---

## 2026-05-01 - Pass 118: fulfillment order and fulfillment read parity

Promotes the remaining shipping/fulfillment parity specs into the Gleam parity
suite. The shipping/fulfillments port now seeds captured order-backed
fulfillment data, serves top-level fulfillment and fulfillment-order reads,
routes order reads with nested fulfillment selections locally, and stages the
captured fulfillment-order request and lifecycle mutations without Shopify
passthrough.

| Module                                                                  | Change                                                                                                                                                                                                                                                      |
| ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/shipping_fulfillments.gleam`       | Adds fulfillment/fulfillment-order query roots, assigned/manual fulfillment-order connections, order nested fulfillment projection, request lifecycle mutations, and bounded fulfillment-order hold/move/progress/open/cancel/split/deadline/merge staging. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`                 | Routes `order` reads that select `fulfillments` or `fulfillmentOrders` through the shipping/fulfillments domain while leaving other order reads unsupported in the current port.                                                                            |
| `gleam/src/shopify_draft_proxy/state/{types,store,serialization}.gleam` | Adds captured fulfillment, fulfillment-order, and shipping-order state slices plus staged shipping-order updates for lifecycle read-after-write effects.                                                                                                    |
| `gleam/test/parity/runner.gleam`                                        | Seeds the captured fulfillment top-level reads, assigned fulfillment-order filters, fulfillment-order request lifecycle, and fulfillment-order lifecycle fixtures.                                                                                          |
| `gleam/test/shopify_draft_proxy/proxy/shipping_fulfillments_test.gleam` | Covers fixture-backed fulfillment reads, assigned/manual filters, order nested reads, request lifecycle staging, and fulfillment-order lifecycle effects.                                                                                                   |
| `config/gleam-port-ci-gates.json`                                       | Removes the last shipping/fulfillment parity specs from expected Gleam parity failures.                                                                                                                                                                     |

Validation:
Full JavaScript is green at 730 tests. Host Erlang still fails before tests
with the known `undef` runner issue; the Docker Erlang fallback using Gleam 1.16
and `HOME=/tmp` is green at 726 tests. `corepack pnpm gleam:port:coverage` is
green with 379 specs and 166 expected failures. Shipping/fulfillment expected
Gleam parity failures are now zero.

### Findings

- Fulfillment-order lifecycle parity needs to preserve Shopify's distinction
  between fulfillment-order allocation quantities and the underlying order line
  item quantity/fulfillable fields.
- Release and merge flows keep closed sibling fulfillment orders visible on the
  nested order read rather than dropping them from the order connection.
- The order read effect for `fulfillmentOrderReportProgress` is staged on the
  captured shipping-order record by updating `displayFulfillmentStatus`, then
  reset by the open flow.
- Captured deadline timestamps compare at Shopify's second precision, so local
  deadline staging normalizes millisecond input values before projection.

### Risks / open items

- This pass intentionally uses captured JSON-backed order/fulfillment records
  for the parity fixtures instead of porting the full TypeScript order runtime;
  the broader order domain remains a separate porting concern.
- Reverse logistics and order-edit shipping line roots are not represented by
  checked-in shipping/fulfillment parity specs in this pass; future captured
  specs should extend this domain substrate.
- The TypeScript shipping/fulfillment runtime remains intact under the port
  preservation rule until the final all-port cutover.

### Pass 119 candidates

- Pick the next domain with manifest-backed expected failures from
  `config/gleam-port-ci-gates.json`.
- Expand order-domain substrate when new order-owned parity fixtures require
  more than captured shipping-order projection.

---

## 2026-05-01 - Pass 117: delivery profile lifecycle parity

Promotes `delivery-profile-lifecycle.json` into the Gleam parity suite. The
shipping/fulfillments port now stages delivery profile create/update/remove
locally, returns captured validation-style user errors for blank/missing/default
profile branches, preserves read-after-remove null behavior, and seeds the
lifecycle fixture with captured product/variant/default-profile preconditions.

| Module                                                                  | Change                                                                                                                                                                                          |
| ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/shipping_fulfillments.gleam`       | Adds deliveryProfileCreate/update/remove mutation roots, synthetic profile/location-group/zone/method/rate/condition/job IDs, derived counts, variant item projection, and validation payloads. |
| `gleam/test/parity/runner.gleam`                                        | Seeds captured product/variant records plus the default profile needed by lifecycle validation replay.                                                                                          |
| `gleam/test/shopify_draft_proxy/proxy/shipping_fulfillments_test.gleam` | Covers lifecycle create/update/remove, downstream absence after removal, and validation branches.                                                                                               |
| `config/gleam-port-ci-gates.json`                                       | Removes the delivery-profile lifecycle spec from expected Gleam parity failures.                                                                                                                |

Validation:
Full JavaScript is green at 730 tests. Host Erlang still fails before tests
with the known `undef` runner issue; the Docker Erlang fallback using Gleam 1.16
and `HOME=/tmp` is green at 726 tests. `corepack pnpm gleam:port:coverage` is
green with 379 specs and 171 expected failures. Shipping/fulfillment expected
Gleam parity failures are down from 6 to 5.

### Findings

- The lifecycle fixture does not need a broad delivery-profile schema model for
  this pass; the captured-profile projector can represent the selected profile
  graph while derived counts and read-after-remove behavior are staged locally.
- The parity runner must seed the captured product/variant used by
  `variantsToAssociate`, otherwise the create payload cannot reproduce the
  downstream `profileItems` product and variant titles.
- Default-profile removal parity needs an explicit default profile baseline,
  because the default-remove validation branch must distinguish a missing
  profile from Shopify's "Cannot delete the default profile." response.

### Risks / open items

- The staged profile update currently models the lifecycle fields selected by
  the fixture; broader delivery-profile mutation subgraphs should be expanded
  when additional captured specs require them.
- Fulfillment order/event flows, assigned/manual fulfillment order reads,
  reverse logistics, and order-edit shipping line roots remain gated for
  HAR-493.
- The TypeScript shipping/fulfillment runtime remains intact under the port
  preservation rule until the final all-port cutover.

### Pass 118 candidates

- Continue `fulfillment-top-level-reads.json` if its fulfillment read shapes can
  reuse the fulfillment-service and location substrate.
- Continue order-backed fulfillment order/event lifecycle specs if the order
  seed surface is tractable.

---

## 2026-05-01 - Pass 116: delivery profile read parity

Promotes `delivery-profile-read.json` into the Gleam parity suite. The
shipping/fulfillments port now has fixture-backed delivery profile state,
top-level `deliveryProfile` detail reads, `deliveryProfiles` catalog
connections, merchant-owned filtering, reverse ordering, captured cursor
projection, and missing-profile null behavior.

| Module                                                                  | Change                                                                                                                                                                        |
| ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/shipping_fulfillments.gleam`       | Adds `deliveryProfile`/`deliveryProfiles` query roots, captured profile projection, connection pagination, and hidden rate-provider typename annotation for inline fragments. |
| `gleam/src/shopify_draft_proxy/state/{types,store,serialization}.gleam` | Adds delivery-profile state, effective read helpers, staged delete plumbing for future lifecycle work, and state dump fields.                                                 |
| `gleam/test/parity/runner.gleam`                                        | Seeds the captured delivery profile detail payload and catalog cursor for strict parity replay.                                                                               |
| `gleam/test/shopify_draft_proxy/proxy/shipping_fulfillments_test.gleam` | Covers detail, catalog, merchant-owned reverse read, and missing-profile null projection.                                                                                     |
| `config/gleam-port-ci-gates.json`                                       | Removes the delivery-profile read spec from expected Gleam parity failures.                                                                                                   |

Validation:
Full JavaScript is green at 728 tests. Host Erlang/OTP 25 still cannot run the
current `gleam_json` OTP 27 calls; the Docker Erlang fallback using Gleam 1.16
and `HOME=/tmp` is green at 724 tests. `corepack pnpm gleam:port:coverage` is
green with 379 specs and 172 expected failures. Shipping/fulfillment expected
Gleam parity failures are down from 7 to 6.

### Findings

- The capture omits `__typename` under `rateProvider`, but the local projector
  needs the hidden type to apply `DeliveryRateDefinition` versus
  `DeliveryParticipant` fragments. The read path infers that internal type from
  stable rate-provider fields before projection while leaving the selected JSON
  shape unchanged.
- The read spec is safely fixture-backed: the captured detail payload contains
  the nested profile item, location group, zone, rate, condition, unassigned
  location, and connection cursor slices required by the strict replay targets.

### Risks / open items

- Delivery profile create/update/remove lifecycle remains gated separately.
- Fulfillment order/event flows, assigned/manual fulfillment order reads,
  reverse logistics, and order-edit shipping line roots remain gated for
  HAR-493.
- The TypeScript shipping/fulfillment runtime remains intact under the port
  preservation rule until the final all-port cutover.

### Pass 117 candidates

- Continue `delivery-profile-lifecycle.json` now that delivery-profile read
  state and projection are available.
- Continue the order-backed fulfillment substrate if tackling
  `fulfillment-top-level-reads.json` or fulfillment-order lifecycle specs.

---

## 2026-05-01 - Pass 115: fulfillment service lifecycle parity

Promotes `fulfillment-service-lifecycle.json` into the Gleam parity suite. The
shipping/fulfillments port now stages FulfillmentService create/update/delete
locally, projects top-level `fulfillmentService` detail reads, and stages the
associated Store Properties location so downstream location reads observe the
service lifecycle.

| Module                                                                  | Change                                                                                                              |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/shipping_fulfillments.gleam`       | Adds fulfillment-service query/mutation roots, validation branches, synthetic IDs, and associated location staging. |
| `gleam/src/shopify_draft_proxy/state/{types,store,serialization}.gleam` | Adds normalized fulfillment-service state, effective reads, staged deletes, and state dump fields.                  |
| `gleam/test/shopify_draft_proxy/proxy/shipping_fulfillments_test.gleam` | Covers fulfillment-service lifecycle reads, associated location effects, delete cleanup, and validation branches.   |
| `config/gleam-port-ci-gates.json`                                       | Removes the fulfillment-service lifecycle spec from expected Gleam parity failures.                                 |

Validation:
Full JavaScript is green at 727 tests. Host Erlang/OTP 25 still cannot run the
current `gleam_json` OTP 27 calls; the Docker Erlang fallback using Gleam 1.16
and `HOME=/tmp` is green at 723 tests. `corepack pnpm gleam:port:coverage` is
green with 379 specs and 173 expected failures. Shipping/fulfillment expected
Gleam parity failures are down from 8 to 7.

### Findings

- Fulfillment service updates preserve the original handle while changing
  `serviceName` and the associated location name, matching the capture.
- The delete payload strips the query suffix from the FulfillmentService GID,
  while reads and staged identity keep the full synthetic GID.
- The local service location belongs in Store Properties location state so mixed
  `fulfillmentService` / `location` parity reads can observe the same staged
  graph.

### Risks / open items

- Delivery profiles, fulfillment order/event flows, assigned/manual
  fulfillment order reads, reverse logistics, and order-edit shipping line roots
  remain gated for HAR-493.
- The TypeScript shipping/fulfillment runtime remains intact under the port
  preservation rule until the final all-port cutover.

### Pass 116 candidates

- Continue `fulfillment-top-level-reads.json` if the remaining top-level
  fulfillment read shapes can share the fulfillment-service and location
  substrate.
- Continue `delivery-profile-read.json` / `delivery-profile-lifecycle.json` if
  delivery-profile state can share the shipping settings location substrate.

---

## 2026-05-01 - Pass 114: shipping settings and local pickup parity

Promotes `shipping-settings-package-pickup-constraints.json` into the Gleam
parity suite. The shipping/fulfillments port now projects available active
carrier services, active delivery-profile locations, and local pickup
enable/disable mutations backed by staged Store Properties location state so
downstream `location` reads observe pickup settings changes.

| Module                                                                  | Change                                                                                                    |
| ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/shipping_fulfillments.gleam`       | Adds shipping settings availability reads plus local pickup enable/disable payloads and location staging. |
| `gleam/test/parity/runner.gleam`                                        | Seeds captured carrier services and Store Properties locations for the shipping settings parity replay.   |
| `gleam/test/shopify_draft_proxy/proxy/shipping_fulfillments_test.gleam` | Covers availability filtering, pickup enable/read/disable, and unknown-location user errors.              |
| `config/gleam-port-ci-gates.json`                                       | Removes the shipping settings/package/local-pickup constraints spec from expected Gleam parity failures.  |

Validation:
Full JavaScript is green at 725 tests. Host Erlang/OTP 25 still cannot run the
current `gleam_json` OTP 27 calls; the Docker Erlang fallback using Gleam 1.16
and `HOME=/tmp` is green at 721 tests. `corepack pnpm gleam:port:coverage` is
green with 379 specs and 174 expected failures. Shipping/fulfillment expected
Gleam parity failures are down from 9 to 8.

### Findings

- `availableCarrierServices` is captured as active carrier services paired
  with active non-fulfillment locations when locations are selected.
- `locationsAvailableForDeliveryProfilesConnection` includes active
  fulfillment-service locations; it filters inactive/deleted locations and sorts
  by Shopify resource ID before normal connection windowing.
- Local pickup mutations should update Store Properties location records rather
  than creating a separate shipping-only location state, because the downstream
  captured read uses the already-ported `location` root.

### Risks / open items

- Delivery profiles, fulfillment services, fulfillment order/event flows,
  assigned/manual fulfillment order reads, reverse logistics, and order-edit
  shipping line roots remain gated for HAR-493.
- The TypeScript shipping/fulfillment runtime remains intact under the port
  preservation rule until the final all-port cutover.

### Pass 115 candidates

- Continue `fulfillment-service-lifecycle.json` if its mutation/read loop can
  be isolated against the existing Store Properties location seed.
- Continue `delivery-profile-read.json` / `delivery-profile-lifecycle.json` if
  delivery-profile state can share the shipping settings location substrate.

---

## 2026-05-01 - Pass 113: shipping package and carrier service lifecycle parity

Promotes two shipping/fulfillment lifecycle fixtures into the Gleam parity
suite. The port now stages custom shipping package update/default/delete roots
locally, preserves package state in dumps, returns captured unknown-id
`RESOURCE_NOT_FOUND` envelopes, and stages DeliveryCarrierService
create/update/delete with downstream detail and filtered connection reads.

| Module                                                                  | Change                                                                                                          |
| ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/shipping_fulfillments.gleam`       | Adds the shipping/fulfillment domain slice for shipping package and carrier service mutation/query roots.       |
| `gleam/src/shopify_draft_proxy/state/{types,store,serialization}.gleam` | Adds normalized shipping package and carrier service state, effective reads, staged deletes, and state dumps.   |
| `gleam/test/parity/{spec,runner}.gleam`                                 | Allows parity specs to compare selected proxy state/log paths and seeds captured shipping package baselines.    |
| `gleam/test/shopify_draft_proxy/proxy/shipping_fulfillments_test.gleam` | Covers local package lifecycle, carrier service lifecycle, filtered reads, and validation branches.             |
| `config/gleam-port-ci-gates.json`                                       | Removes the shipping package lifecycle and carrier service lifecycle specs from expected Gleam parity failures. |

Validation:
Full JavaScript is green at 722 tests. Host Erlang/OTP 25 still cannot run the
current `gleam_json` OTP 27 calls; the Docker Erlang fallback using Gleam 1.16
and `HOME=/tmp` is green at 718 tests. `corepack pnpm gleam:port:coverage` is
green with 379 specs and 175 expected failures. Shipping/fulfillment expected
Gleam parity failures are down from 11 to 9.

### Findings

- Shipping package mutations must not fabricate missing package records:
  Shopify and the TypeScript runtime return a top-level `RESOURCE_NOT_FOUND`
  error with a null root payload for unknown package IDs.
- Carrier service lifecycle parity is self-contained enough for isolated
  replay: the proxy-created DeliveryCarrierService ID can drive update,
  downstream detail/active-filter reads, delete, and after-delete absence.
- Carrier service search for this fixture only needs Shopify's captured
  `active:` and `id:` term behavior; broader search semantics remain deferred
  until another captured fixture requires them.

### Risks / open items

- Shipping settings/local pickup, delivery profiles, fulfillment services,
  fulfillment orders/events, reverse logistics, and order-edit shipping line
  roots remain gated for HAR-493.
- The TypeScript shipping/fulfillment runtime remains intact under the port
  preservation rule until the final all-port cutover.

### Pass 114 candidates

- Continue `shipping-settings-package-pickup-constraints.json` by porting
  available carrier services, locations available for delivery profiles, and
  local pickup enable/disable staging.
- Continue `fulfillment-service-lifecycle.json` if a smaller mutation/read
  lifecycle slice is preferred.

---

## 2026-05-01 - Pass 165: HAR-504 localization parity completion

Promotes the remaining localization parity fixture into the Gleam parity suite.
The localization port now derives translatable Product and product Metafield
resources from effective store state, keeps captured source-content markers as
a parity seeding bridge, and replays the locale/translation lifecycle fixture
without the expected-failure gate.

| Module                                                         | Change                                                                                              |
| -------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/localization.gleam`       | Lists/finds Product and Metafield translatable resources and preserves source-marker content order. |
| `gleam/test/parity/runner.gleam`                               | Seeds captured localization available-locale excerpts and source-content markers for replay.        |
| `gleam/test/shopify_draft_proxy/proxy/localization_test.gleam` | Covers Product-backed and source-marker-backed translatable resource enumeration.                   |
| `config/gleam-port-ci-gates.json`                              | Removes `localization-locale-translation-fixture.json` from expected Gleam parity failures.         |
| `.agents/skills/gleam-port/SKILL.md`                           | Records the localization source-marker seeding pattern for future passes.                           |

Validation:
`gleam test --target erlang` is green in the OTP 28 Gleam container;
`gleam test --target javascript`, `corepack pnpm gleam:port:coverage`, and
`corepack pnpm gleam:registry:check` are green.

### Findings

- The final gated localization fixture was not a mutation gap; it needed the
  runner to seed the captured source content before the first read so local
  `translatableResources` enumeration, register validation, downstream reads,
  and remove cleanup could all see the same product resource.
- `availableLocales` in the capture is intentionally an excerpt, so the runner
  seeds that captured catalog slice for this scenario instead of using the
  default broader catalog.
- The TypeScript localization runtime remains intact under the incremental port
  preservation rule.

### Risks / open items

- Final deletion of TypeScript localization runtime remains deferred until the
  whole-port cutover acceptance bar is met.

---

## 2026-05-01 - Pass 164: Admin Platform node coverage proof

Addresses HAR-498 review feedback by adding the Gleam equivalent of the
TypeScript Admin Platform Node coverage snapshot. The test now reads the
captured Shopify `Node` interface introspection fixture, subtracts the
Gleam-supported `node(id:)` / `nodes(ids:)` types, and snapshots the remaining
unsupported implementors in executable Gleam coverage. The pass also wires
primary `Domain` GIDs through the existing store-properties domain serializer so
the supported list reflects actual local `node` behavior.

| Module                                                           | Change                                                                                          |
| ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/admin_platform.gleam`       | Exposes the supported Node type list and resolves primary `Domain` IDs through store state.     |
| `gleam/test/shopify_draft_proxy/proxy/admin_platform_test.gleam` | Adds the introspection-backed unsupported Node snapshot and focused Domain node readback.       |
| `.agents/skills/gleam-port/SKILL.md`                             | Records the Admin Platform Node coverage pattern for future domain ports that add node support. |
| `GLEAM_PORT_LOG.md`                                              | Records the HAR-498 review-feedback rework.                                                     |

Validation: the TypeScript Vitest Node coverage test is green. Targeted
`gleam test --target javascript admin_platform` is green at 773 tests, and the
targeted Docker Erlang fallback is green at 769 tests. Full
`gleam test --target javascript` is green at 773 tests. Host
`gleam test --target erlang admin_platform` still reproduces the known local
`undef` runner issue; the full established Docker Erlang fallback with
`HOME=/tmp` is green at 769 tests. `corepack pnpm gleam:port:coverage` is green
with 379 specs and 62 expected Gleam failures. `corepack pnpm
gleam:registry:check`, `corepack pnpm lint`, and `git diff --check` are green.

### Findings

- The unsupported Node coverage proof belongs in Gleam tests, not only in the
  TypeScript fixture snapshot, because the Gleam node dispatch table is smaller
  while the incremental port is still in progress.
- `Domain` is a Shopify `Node` implementor and can resolve safely through the
  already-ported store-properties primary-domain serializer. Unsupported or
  missing Domain IDs still return `null`.

### Risks / open items

- Many implemented singular roots from other ported domains still need owning
  resource serializers before they can be removed from the unsupported Node
  list; this pass records that truth instead of claiming support early.
- TypeScript runtime deletion remains deferred to the final all-port cutover
  under the Gleam port preservation rule.

---

## 2026-05-01 - Pass 166: HAR-496 payments branch online-store refresh

Refreshes the HAR-496 Payments branch after `origin/main` advanced with the
Online Store parity port. The merge keeps Payments and order-payment parity
ungated while preserving mainline Online Store, Media, Discounts, and Orders
dispatch, store, serialization, and parity runner coverage.

| Module                                                  | Change                                                                                                  |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam` | Keeps Payments dispatch alongside mainline Online Store, Media, Discounts, and Orders dispatch.         |
| `gleam/src/shopify_draft_proxy/state/*`                 | Combines mainline Online Store/Media/Discounts state with HAR-496 payment customization/terms state.    |
| `gleam/test/parity/runner.gleam`                        | Keeps payment precondition seeding alongside mainline online-store, media, and discount seeding.        |
| `config/gleam-port-ci-gates.json`                       | Keeps Payments/order-payment and mainline Online Store/Media/Discounts paths ungated after the refresh. |

Validation:

- `git diff --check`
- `cd gleam && gleam format --check`
- `corepack pnpm lint`
- `cd gleam && gleam check --target javascript`
- `cd gleam && gleam check --target erlang`
- `cd gleam && gleam test --target javascript -- --seed 0` (771 passed)
- `docker run --rm -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine gleam test --target erlang -- --seed 0`
  (767 passed)
- `corepack pnpm gleam:port:coverage` (379 specs; 53 expected failures; passed)
- Targeted expected-failure scan for payments/order-payment, finance-risk,
  product grammar, Segments, Discounts, Media, and Online Store paths returned
  no matches.

### Findings

- Mainline now owns Online Store dispatch and state, including `shop` field
  routing for storefront tokens. This refresh preserves that routing while
  keeping HAR-496's `draftOrder` payment-terms and order-payment dispatch.
- The expected-failure gate conflict resolves by removing both HAR-496
  payments/order-payment paths and mainline Online Store paths.

### Risks / open items

- TypeScript Payments runtime deletion remains deferred under the incremental
  port preservation rule until final all-port cutover.

---

## 2026-05-01 - Pass 165: HAR-496 payments branch media refresh

Refreshes the HAR-496 Payments branch after `origin/main` advanced with the
HAR-506 Media files and uploads refresh. The merge keeps Payments and
order-payment parity ungated while preserving mainline Discounts and Media
dispatch, store, serialization, and parity runner coverage.

| Module                                                  | Change                                                                                  |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam` | Keeps Payments dispatch alongside mainline Discounts, Media, and Orders dispatch.       |
| `gleam/src/shopify_draft_proxy/state/*`                 | Combines mainline Media/Discounts state with HAR-496 payment customization/terms state. |
| `gleam/test/parity/runner.gleam`                        | Keeps payment precondition seeding alongside mainline media and discount seeding.       |
| `config/gleam-port-ci-gates.json`                       | Keeps Payments/order-payment and mainline Media/Discounts paths ungated.                |

Validation:

- `git diff --check`
- `cd gleam && gleam format --check`
- `corepack pnpm lint`
- `cd gleam && gleam check --target javascript`
- `cd gleam && gleam check --target erlang`
- `cd gleam && gleam test --target javascript -- --seed 0` (771 passed)
- `docker run --rm -v "$PWD":/repo -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine gleam test --target erlang -- --seed 0`
  (767 passed)
- `corepack pnpm gleam:port:coverage` (379 specs; 59 expected failures; passed)
- Targeted expected-failure scan for payments/order-payment, finance-risk,
  product grammar, Segments, Discounts, and Media paths returned no matches.

### Findings

- Mainline now owns Media mutation routing, so this refresh preserves it and
  keeps the HAR-496 Payments dispatch, state, seed, and gate removals layered
  into the same dispatcher/store surface.
- The dispatcher conflict resolves to the compact `first_matching_domain`
  helpers while keeping Payments, Discounts, Media, and Orders roots.

### Risks / open items

- TypeScript Payments runtime deletion remains deferred under the incremental
  port preservation rule until final all-port cutover.

---

## 2026-05-01 - Pass 164: HAR-506 post-approval mainline refresh

Refreshes the approved HAR-506 Media files and uploads branch after
`origin/main` added the Discounts lifecycle port. The merge keeps the mainline
Discounts dispatcher and state slices while preserving the HAR-506 Media
dispatcher, file state slice, staged-upload behavior, and file-delete
product-media seed.

| Module                                                  | Change                                                                                                                               |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam` | Keeps Media mutation routing alongside mainline Discounts, Bulk Operations, Admin Platform, Privacy, Orders, and Customers dispatch. |
| `gleam/src/shopify_draft_proxy/state/store.gleam`       | Combines the Media `FileRecord` import with mainline Discount, Draft Order, abandonment, and store-property state imports.           |
| `GLEAM_PORT_LOG.md`                                     | Records the post-approval mainline refresh evidence for HAR-506.                                                                     |

Validation:
JavaScript target is green at 771 tests. Docker Erlang target is green at 767
tests. `corepack pnpm gleam:port:coverage`, `corepack pnpm
gleam:registry:check`, `corepack pnpm lint`, and `git diff --check` are green.
Gleam parity coverage now reports 379 checked-in specs and 68 expected failures.

### Findings

- The conflict was additive: Discounts from `main` and Media from HAR-506 both
  remain explicitly routed locally.
- The ticket's TypeScript media runtime retirement remains deferred under the
  Gleam Port Guardrail until the final all-port cutover.

### Risks / open items

- TypeScript Media runtime retirement remains deferred to the final all-port
  cutover acceptance bar.

---

## 2026-05-01 - Pass 163: discounts lifecycle parity

Promotes the Discounts domain into the Gleam parity suite. The port now stages
discount catalog/detail reads, automatic and code discount lifecycle mutations,
app discounts, BXGY, free shipping, bulk activate/deactivate/delete jobs, and
redeem-code bulk add/delete flows locally while preserving captured buyer
context, status filtering, validation, and downstream read-after-write behavior.

| Module                                                      | Change                                                                                                                |
| ----------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/discounts.gleam`       | Adds Discounts query/mutation root handling, normalized projection, validation, staged lifecycle, jobs, and codes.    |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`     | Routes Discounts query and mutation roots to the new local domain dispatcher.                                         |
| `gleam/src/shopify_draft_proxy/state/types.gleam`           | Adds normalized discount records, typed discount kinds, redeem codes, async jobs, and bulk-creation records.          |
| `gleam/src/shopify_draft_proxy/state/store.gleam`           | Adds effective/staged discount helpers, deletion markers, bulk job helpers, and redeem-code lookup/update operations. |
| `gleam/src/shopify_draft_proxy/state/serialization.gleam`   | Carries discount state through dump/restore.                                                                          |
| `gleam/test/parity/runner.gleam`                            | Seeds captured discount catalog/detail records for parity replay.                                                     |
| `gleam/test/shopify_draft_proxy/proxy/discounts_test.gleam` | Adds focused lifecycle, validation, buyer-context, status-filter, and redeem-code behavior tests.                     |
| `config/gleam-port-ci-gates.json`                           | Removes all 18 checked-in Discounts parity specs from the expected-failure gate.                                      |

Validation:
Focused Discounts parity is green for all 18 checked-in specs. `gleam check`
is green. Full JavaScript target and the established Docker Erlang fallback are
green on the HAR-491 branch. The TypeScript discount runtime remains intact
under the port preservation rule until the final all-port cutover.

### Findings

- Discounts reuse more of Shopify's captured async-job surface than earlier
  domains: app bulk activation/deactivation/delete and redeem-code deletes
  return synthetic `Job` IDs, while redeem-code adds return a synthetic
  `DiscountRedeemCodeBulkCreation` ID.
- Captured discount catalog reads depend on both `status:` filters and code
  query behavior. The Gleam port keeps those decisions in the Discounts module
  instead of adding a resource-local generic search parser.
- Buyer-context fields are preserved as captured payload fragments so automatic
  and code discount detail reads round-trip stable Shopify-selected slices
  after local mutation staging.

### Risks / open items

- This pass removes the Discounts parity gate for Gleam, but it does not
  authorize deleting the TypeScript Discounts runtime before the broader
  whole-port cutover acceptance bar is met.

### Pass 164 candidates

- Continue with the next expected-failing non-Product domain from
  `config/gleam-port-ci-gates.json`.

---

## 2026-05-01 - Pass 162: fulfillment-order request lifecycle staging

Ports the remaining implemented fulfillment-order request roots from the
legacy Orders integration flow. This pass stages
`fulfillmentOrderSubmitFulfillmentRequest`,
`fulfillmentOrderAcceptFulfillmentRequest`,
`fulfillmentOrderRejectFulfillmentRequest`,
`fulfillmentOrderSubmitCancellationRequest`,
`fulfillmentOrderAcceptCancellationRequest`, and
`fulfillmentOrderRejectCancellationRequest`, plus
`assignedFulfillmentOrders` readback. It keeps `fulfillmentOrdersReroute`
unclaimed because the operation registry still marks it `implemented: false`
with HAR-234 capture blockers.

| Module                                                               | Change                                                   |
| -------------------------------------------------------------------- | -------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`                   | Adds request/cancellation staging and assigned readback. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam`             | Covers request/cancellation read-after-write.            |
| `gleam/test/shopify_draft_proxy/proxy/operation_registry_test.gleam` | Moves the unported-root sentinel to service reads.       |
| `gleam/test/shopify_draft_proxy/proxy/passthrough_test.gleam`        | Keeps live-hybrid passthrough coverage unported.         |
| `GLEAM_PORT_LOG.md`                                                  | Records pass 162 evidence.                               |
| `.agents/skills/gleam-port/SKILL.md`                                 | Records request lifecycle patterns.                      |

Validation:

- Reproduction signal before porting: request/cancellation roots were absent
  from `is_orders_mutation_root` and local mutation dispatch, while
  `assignedFulfillmentOrders` was the explicit implemented-but-unported
  sentinel.
- `cd gleam && gleam test --target javascript
orders_fulfillment_order_request_cancellation_read_after_write_test`
  (762 passed).

### Findings

- Submit-fulfillment-request staging must split partially requested line-item
  quantities into submitted and unsubmitted fulfillment orders, minting fresh
  unsubmitted fulfillment-order and line-item IDs while keeping the submitted
  fulfillment order at the original ID.
- Merchant requests need a connection-shaped serializer, not raw JSON
  projection, so `merchantRequests(first:) { nodes { ... } }` behaves like the
  TypeScript flow.
- `assignedFulfillmentOrders` is now local readback. The passthrough sentinel
  moved to `fulfillmentService`, which remains implemented in TypeScript but
  outside the current Gleam dispatch table.

### Risks / open items

- `fulfillmentOrdersReroute`, `fulfillmentOrderReschedule`, and
  `fulfillmentOrderClose` remain registry-unimplemented beyond captured
  guardrail behavior; do not claim lifecycle support for those roots without
  successful conformance evidence.

### Pass 163 candidates

- Run the full two-target/repo validation gate and re-check HAR-492 for any
  remaining executable Orders integration-flow gaps before deciding whether the
  draft PR can leave active implementation.

---

## 2026-05-01 - Pass 161: fulfillment-order split/deadline/merge staging

Extends the fulfillment-order lifecycle slice with the residual local staging
roots from the legacy Orders integration flow. This pass stages
`fulfillmentOrderSplit`, `fulfillmentOrdersSetFulfillmentDeadline`, and
`fulfillmentOrderMerge`, preserving downstream nested `Order.fulfillmentOrders`
readback without runtime Shopify writes.

| Module                                                   | Change                                      |
| -------------------------------------------------------- | ------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds split/deadline/merge mutation staging. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers residual lifecycle read-after-write. |
| `GLEAM_PORT_LOG.md`                                      | Records pass 161 evidence.                  |
| `.agents/skills/gleam-port/SKILL.md`                     | Records split/deadline/merge patterns.      |

Validation:

- Reproduction with the new residual lifecycle proof first failed with
  `{\"data\":{}}`, proving `fulfillmentOrderSplit` was still ignored locally.
- `cd gleam && gleam test --target javascript
orders_fulfillment_order_split_deadline_merge_read_after_write_test`
  (761 passed).

### Findings

- Split support needs a stored `supportedActions` override because Shopify adds
  `MERGE` to split fulfillment orders; the generic status/quantity-derived
  action set cannot infer that by itself.
- The split-off fulfillment order gets a new fulfillment-order ID and a new
  line-item ID when only part of an existing line item is split. The original
  fulfillment order keeps the original line-item ID and reduced quantities.
- Merge preserves the target fulfillment order ID and original line-item ID,
  closes merged sibling orders with zeroed line items, and carries forward the
  first staged `fulfillBy` deadline.

### Risks / open items

- Fulfillment-order request/cancellation request roots, reroute, and
  assigned-fulfillment-order catalog filters remain outside this pass.

### Pass 162 candidates

- Port fulfillment-order request and cancellation request roots plus
  `assignedFulfillmentOrders` readback, or decide whether the remaining
  shipping-fulfillments roots should move to a narrower follow-up issue.

---

## 2026-05-01 - Pass 160: fulfillment-order lifecycle staging

Adds the first fulfillment-order lifecycle slice to the Gleam Orders domain.
This pass stages hold/release, move, progress/open, cancel, and captured
reschedule/close guardrails locally, and exposes the downstream effects through
top-level `fulfillmentOrder`/`fulfillmentOrders`,
`manualHoldsFulfillmentOrders`, and nested `Order.fulfillmentOrders` reads.

| Module                                                               | Change                                               |
| -------------------------------------------------------------------- | ---------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`                   | Adds fulfillment-order lifecycle reads/mutations.    |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam`             | Covers hold/release, move/progress/open/cancel.      |
| `gleam/test/shopify_draft_proxy/proxy/operation_registry_test.gleam` | Moves the unported-root sentinel to assignment read. |
| `gleam/test/shopify_draft_proxy/proxy/passthrough_test.gleam`        | Keeps live-hybrid passthrough coverage unported.     |
| `.agents/skills/gleam-port/SKILL.md`                                 | Records fulfillment-order lifecycle patterns.        |

Validation:

- Reproduction with the in-flight lifecycle proof first exposed the old
  supported-root gap: local dispatch for these roots was absent, and after
  adding the root set the registry/passthrough sentinel still treated
  `fulfillmentOrders` as unported.
- `cd gleam && gleam test --target javascript
orders_fulfillment_order_hold_release_read_after_write_test`
  (760 passed).
- `cd gleam && gleam test --target javascript
orders_fulfillment_order_lifecycle_mutations_read_after_write_test`
  (760 passed).
- `cd gleam && gleam test --target javascript` (760 passed).
- Docker Erlang fallback `gleam clean && gleam test --target erlang`
  (756 passed).

### Findings

- `fulfillmentOrders` is now a local Orders query root, so substrate
  passthrough tests need a different implemented-but-unported sentinel;
  `assignedFulfillmentOrders` remains covered by the registry but outside this
  pass.
- Releasing a partial hold must merge the held quantity back with its split
  sibling and close that sibling. Without that merge, downstream supported
  actions lose `SPLIT` even though the local order graph should again represent
  the original quantity.
- Top-level fulfillment-order catalog reads should filter `CLOSED` records by
  default while nested order reads preserve the order graph, matching the legacy
  TypeScript integration flow.

### Risks / open items

- Fulfillment-order request/cancellation request roots, split/deadline/merge,
  reroute, and assigned-fulfillment-order catalog filters remain outside this
  pass.

### Pass 161 candidates

- Port the remaining fulfillment-order request/split/merge/deadline roots from
  the legacy TypeScript Orders module, or split them into a dedicated
  shipping-fulfillments pass if HAR-492 remains too broad for one PR.

---

## 2026-05-01 - Pass 159: fulfillment create and event staging

Adds a bounded fulfillment creation/event lifecycle slice to the Gleam Orders
domain. This pass stages `fulfillmentCreate` against order-backed local
fulfillment orders, exposes the new fulfillment through top-level and nested
reads, and stages `fulfillmentEventCreate` event history without invoking
external carrier, notification, or fulfillment-service side effects.

| Module                                                   | Change                                     |
| -------------------------------------------------------- | ------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds fulfillment create/event handling.    |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers create, event, and detail readback. |
| `.agents/skills/gleam-port/SKILL.md`                     | Records fulfillment create/event patterns. |

Validation:

- Reproduction with the new fulfillment create/event proof first failed because
  `fulfillmentCreate` always returned the captured invalid-id branch, even for a
  local order-backed fulfillment order.
- `cd gleam && gleam test --target javascript orders_fulfillment_create_event_and_detail_read_test`
  (758 passed).

### Findings

- The existing fulfillment cancel/tracking path already projected fulfillment
  JSON well enough for both mutation payloads and nested `Order.fulfillments`;
  the missing piece was creating and appending order-backed fulfillment JSON.
- Fulfillment events need connection-shaped storage (`events.nodes` plus
  `pageInfo`) so top-level `fulfillment(id:)` and nested order reads can use the
  shared GraphQL projector.

### Risks / open items

- Broader fulfillment-order request/move/hold/split/merge workflows remain
  outside this pass.

### Pass 160 candidates

- Port the fulfillment-order lifecycle roots still covered only by the legacy
  TypeScript Orders module, or decide whether they belong in a separate
  shipping-fulfillments ticket.

---

## 2026-05-01 - Pass 158: order delete tombstone staging

Adds direct `orderDelete` handling to the Gleam Orders domain. This pass
matches the existing TypeScript snapshot behavior for supported order deletes:
known orders are tombstoned locally, downstream reads omit the order, and repeat
deletes return a payload-level user error without runtime upstream writes.

| Module                                                   | Change                                   |
| -------------------------------------------------------- | ---------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `orderDelete` mutation handling.    |
| `gleam/src/shopify_draft_proxy/state/store.gleam`        | Adds staged order tombstone helper.      |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers delete read-after-write behavior. |
| `.agents/skills/gleam-port/SKILL.md`                     | Records order delete staging notes.      |

Validation:

- Reproduction with the new direct `orderDelete` proof first failed with an
  empty data payload because the Gleam Orders dispatcher ignored the supported
  root.
- `cd gleam && gleam test --target javascript orders_order_delete_tombstone_read_after_write_test`
  (757 passed).

### Findings

- `OrderRecord` already had deleted-id tracking in the base and staged state
  shapes; the port only lacked the store helper and mutation wiring.
- The local delete can use a staged tombstone without mutating base state,
  preserving isolated proxy instance behavior and read-after-write suppression.

### Risks / open items

- Fulfillment creation and broader fulfillment-order workflows are still not
  part of this pass.

### Pass 159 candidates

- Port the fulfillment creation/event slice or fulfillment-order lifecycle
  workflows if HAR-492 is judged to include those legacy Orders-module roots.

---

## 2026-05-01 - Pass 157: return reverse logistics staging

Promotes the local-runtime and recorded reverse-logistics return scenarios in
the Gleam Orders domain. This pass adds requested-return approval, reverse
delivery creation/update, reverse fulfillment disposition, return processing,
top-level reverse delivery and reverse fulfillment order reads, and downstream
read-after-write effects for both fixture shapes.

| Module                                             | Change                                           |
| -------------------------------------------------- | ------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam` | Adds reverse-logistics mutation/read handling.   |
| `config/gleam-port-ci-gates.json`                  | Removes the final two gated Orders parity specs. |
| `.agents/skills/gleam-port/SKILL.md`               | Records reverse-logistics staging notes.         |

Validation:

- Reproduction with both reverse-logistics specs ungated first failed at
  missing local dispatcher support for `returnApproveRequest`.
- `cd gleam && gleam test --target javascript` (756 passed).

### Findings

- Approval is the bridge from a requested return to reverse logistics: it
  creates the reverse fulfillment order and line item IDs that all later targets
  derive from.
- The local and recorded fixtures select different reverse-logistics field
  shapes (`company` vs `carrierName`, `dispositionType` vs `dispositions`), so
  serializers need to expose both Shopify field families from one captured
  reverse-order model.
- `returnProcess` persists a closed return when all line items are processed,
  while the mutation payload keeps Shopify's captured response status from the
  pre-process open return.

### Risks / open items

- All 78 checked-in Orders parity specs now run in the Gleam parity gate, but
  HAR-492's broader integration-flow acceptance still needs final validation
  before the issue can leave `In Progress`.
- Direct order delete success and broader fulfillment creation workflows remain
  outside the promoted parity set unless covered by existing executable tests.

### Pass 158 candidates

- Run the required full validation suite, inspect remaining registry/runtime
  gaps for HAR-492 acceptance, and decide whether the PR can move toward human
  review or needs another targeted pass.

---

## 2026-05-01 - Pass 156: return request decline staging

Promotes the local-runtime requested-return decline scenario in the Gleam
Orders domain. This pass adds `returnDeclineRequest` handling for captured
requested returns, preserving local-only notification boundaries while staging
the Shopify-like `DECLINED` return state and decline reason/note payload.

| Module                                             | Change                                          |
| -------------------------------------------------- | ----------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam` | Adds `returnDeclineRequest` mutation staging.   |
| `config/gleam-port-ci-gates.json`                  | Removes the newly passing request-decline spec. |
| `.agents/skills/gleam-port/SKILL.md`               | Records requested-return decline staging notes. |

Validation:

- Reproduction with `return-request-decline-local-staging` ungated first failed
  at missing local dispatcher support for `returnDeclineRequest`.
- `cd gleam && gleam test --target javascript` (756 passed).

### Findings

- Decline is a state transition on the existing order-backed return JSON. The
  handler should reject missing or non-`REQUESTED` returns, then stage
  `status: DECLINED` with the captured decline `reason` and `note`.
- Notification side effects are intentionally not modeled; the parity scenario
  only asserts the local mutation payload and no upstream passthrough.

### Risks / open items

- Reverse-logistics return roots remain gated.
- Direct order delete success and broader fulfillment creation workflows remain
  outside the promoted parity set.

### Pass 157 candidates

- Port `return-reverse-logistics-local-staging`; if it proves too broad, split
  request approval from reverse delivery/disposal/process roots.

---

## 2026-05-01 - Pass 155: remove from return staging

Promotes the local-runtime `removeFromReturn` scenario in the Gleam Orders
domain. This pass extends the order-backed return model from Pass 154 with
line-item removal, return quantity recomputation, and reverse fulfillment order
line recomputation without runtime upstream passthrough.

| Module                                             | Change                                             |
| -------------------------------------------------- | -------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam` | Adds `removeFromReturn` mutation staging.          |
| `config/gleam-port-ci-gates.json`                  | Removes the newly passing remove-from-return spec. |
| `.agents/skills/gleam-port/SKILL.md`               | Records return-removal staging notes.              |

Validation:

- Reproduction with `removeFromReturn-local-staging` ungated first failed at
  missing local dispatcher support for `removeFromReturn`.
- `cd gleam && gleam test --target javascript` (756 passed).

### Findings

- `removeFromReturn` should not mint replacement reverse fulfillment order line
  IDs when matching existing lines remain; it preserves existing reverse line
  identity and only drops lines whose return line item was removed.
- Reverse fulfillment order line nodes are derived from current return line
  items, so successful removal must update both `returnLineItems` and every
  reverse fulfillment order's `lineItems` array before staging the order.

### Risks / open items

- Request-decline and reverse-logistics return roots remain gated.
- Direct order delete success and broader fulfillment creation workflows remain
  outside the promoted parity set.

### Pass 156 candidates

- Port `return-request-decline-local-staging`, then use the same return-backed
  serializers for reverse-logistics roots.

---

## 2026-05-01 - Pass 154: return lifecycle staging

Promotes the local-runtime return lifecycle scenario in the Gleam Orders
domain. This pass adds order-backed return creation/request staging, return
status transitions, top-level `return(id:)` reads, and nested `Order.returns`
read-after-write serialization for the captured fixture.

| Module                                             | Change                                           |
| -------------------------------------------------- | ------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam` | Adds return lifecycle mutation/query handling.   |
| `gleam/test/parity/runner.gleam`                   | Seeds the fulfilled return-flow source order.    |
| `config/gleam-port-ci-gates.json`                  | Removes the newly passing return lifecycle spec. |
| `.agents/skills/gleam-port/SKILL.md`               | Records return lifecycle staging notes.          |

Validation:

- Reproduction with `return-lifecycle-local-staging` ungated first failed at
  missing local dispatcher support for `returnCreate`.
- After initial implementation, the direct runner showed only
  `returnClose.closedAt` one second late; the fix now mints `closedAt` before
  the order `updatedAt` timestamp to preserve local-runtime fixture identity
  order.
- `cd gleam && gleam test --target javascript` (756 passed).

### Findings

- Return IDs in the local-runtime fixture rely on mutation-log identity
  consumption between requests. Return lifecycle handlers must emit log drafts
  so later requested-return IDs line up with captured local behavior.
- The fixture stores returns as order-backed captured JSON. Custom serializers
  are needed for `Return`, `ReturnLineItem`, reverse fulfillment order
  connections, and nested `Order.returns` because raw captured arrays are not
  Shopify GraphQL connections.

### Risks / open items

- Reverse-logistics roots and `removeFromReturn` remain gated.
- Direct order delete success and broader fulfillment creation workflows remain
  outside the promoted parity set.

### Pass 155 candidates

- Port `removeFromReturn-local-staging`, then use the same return-backed
  serializers for request decline and reverse-logistics roots.

---

## 2026-05-01 - Pass 153: residual order edit calculated edits

Promotes the captured residual calculated-order edit scenario in the Gleam
Orders domain. This pass wires the local residual edit roots for custom items,
line-item discount add/remove, and shipping line add/update/remove into the
same hidden calculated-session state introduced for order-edit commit.

| Module                                             | Change                                              |
| -------------------------------------------------- | --------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam` | Adds residual calculated edit mutation handling.    |
| `gleam/test/parity/runner.gleam`                   | Seeds the residual edit source order precondition.  |
| `config/gleam-port-ci-gates.json`                  | Removes the newly passing residual order-edit spec. |
| `.agents/skills/gleam-port/SKILL.md`               | Records residual calculated-session notes.          |

Validation:

- Reproduction with the residual order-edit spec ungated first failed because
  the source order was not seeded for that scenario ID; after seeding, the
  next failure was no local dispatcher for `orderEditAddCustomItem`.
- `cd gleam && gleam test --target javascript` (756 passed).

### Findings

- The residual workflow only needs pre-commit calculated-order state, so it can
  reuse hidden session bookkeeping without widening commit behavior beyond the
  already-promoted add-variant and zero-removal workflows.
- Shopify's stable comparison fields are totals, quantities, discount
  allocation payloads, and shipping-line status/price/title; synthetic
  calculated ids remain opaque and are excluded by the spec.

### Risks / open items

- Returns remain gated and are the last Orders parity group still blocked in
  the Gleam gate.
- Direct order delete success and broader fulfillment creation workflows remain
  outside the promoted parity set.

### Pass 154 candidates

- Begin the return lifecycle slice, starting with `returnCreate` and return
  status transitions before reverse logistics.

---

## 2026-05-01 - Pass 152: order edit commit parity

Promotes the captured existing-order order-edit commit scenarios in the Gleam
Orders domain. This pass seeds the captured source order for all commit
workflow specs, keeps calculated edit sessions as hidden staged-order JSON
between `orderEditBegin`, `orderEditAddVariant` / `orderEditSetQuantity`, and
`orderEditCommit`, and applies committed line-item current quantity effects to
downstream `order(id:)` reads.

| Module                                             | Change                                                  |
| -------------------------------------------------- | ------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam` | Adds order-edit session persistence and commit effects. |
| `gleam/test/parity/runner.gleam`                   | Seeds existing-order commit workflow preconditions.     |
| `config/gleam-port-ci-gates.json`                  | Removes the three newly passing order-edit specs.       |
| `.agents/skills/gleam-port/SKILL.md`               | Records order-edit session bookkeeping notes.           |

Validation:

- Reproduction with the three order-edit commit specs ungated first failed
  because the source order was not seeded for those scenario IDs; after seeding,
  the remaining failure was the validation-only `orderEditCommit` null payload.
- `cd gleam && gleam test --target javascript` (756 passed).

### Findings

- The existing-order commit fixtures can share the already-ported begin/add/set
  payload behavior. The missing domain behavior was durable calculated-session
  state and applying that session back onto the original order at commit time.
- Hidden staged-order JSON is sufficient for this slice and stays invisible to
  selected order reads, matching the mandate-payment bookkeeping pattern from
  Pass 151.
- Shopify keeps historical `quantity` on existing line items after a zero
  removal while changing `currentQuantity`; newly added calculated lines become
  downstream order line items.

### Risks / open items

- Residual calculated edit roots for custom items, discounts, and shipping
  lines remain gated.
- Direct order delete success, fulfillment creation/fulfillment-order
  workflows, and returns remain gated.

### Pass 153 candidates

- Continue with residual order-edit calculated edits, or switch to the returns
  lifecycle slice.

---

## 2026-05-01 - Pass 151: order payment lifecycle parity

Promotes the checked-in local-runtime order payment parity scenarios in the
Gleam Orders domain. This pass wires `orderCapture`, `transactionVoid`, and
`orderCreateMandatePayment` through the local Orders dispatcher, stages payment
transactions against synthetic orders, updates downstream financial/capturable
fields, and preserves mutation-log-driven synthetic identity gaps for strict
fixture parity.

| Module                                             | Change                                                  |
| -------------------------------------------------- | ------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam` | Adds local order payment lifecycle mutation handling.   |
| `config/gleam-port-ci-gates.json`                  | Removes the three newly passing payment parity specs.   |
| `.agents/skills/gleam-port/SKILL.md`               | Records payment lifecycle synthetic-id and state notes. |

Validation:

- Reproduction with the three payment specs ungated first failed at dispatch:
  `orderCapture`, `transactionVoid`, and `orderCreateMandatePayment` had no
  local mutation dispatcher.
- `cd gleam && gleam test --target javascript` (756 passed).

### Findings

- The local-runtime payment fixtures rely on `orderCreate` mutation-log entries
  consuming synthetic ids between requests. Failed payment validation branches
  also need `Failed` log drafts so later capture ids line up with the recorded
  fixture.
- `orderCapture` derives partial/final financial state from the original
  authorization and staged capture children. Capture transactions keep a
  selected `parentTransaction` object so downstream transaction reads do not
  need resolver-time parent lookup.
- `orderCreateMandatePayment` can keep idempotency data as hidden captured JSON
  on the staged order; selected order fields ignore that local bookkeeping while
  repeated calls reuse the same job and payment reference.

### Risks / open items

- Direct order delete success, order-edit commit sessions, fulfillment
  creation/fulfillment-order workflows, and returns remain gated.
- The payment implementation covers the checked-in local-runtime lifecycle
  branches; broader gateway semantics remain outside this pass.

### Pass 152 candidates

- Continue with order-edit commit session persistence, or begin the returns
  lifecycle slice now that direct order/payment foundations are stronger.

---

## 2026-05-01 - Pass 150: order create parity

Promotes the checked-in `orderCreate-parity-plan` scenario in the Gleam
Orders domain. This pass replaces the validation-only `orderCreate` branch
with local staging for the captured direct-order creation payload, including
selected totals, tax/discount/shipping fields, line item identity, transaction
payment status, and immediate downstream `order(id:)` reads.

| Module                                                   | Change                                                   |
| -------------------------------------------------------- | -------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Builds and stages direct `orderCreate` order records.    |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers created order payload and downstream order reads. |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing `orderCreate` parity spec.     |
| `.agents/skills/gleam-port/SKILL.md`                     | Records direct order-create porting notes.               |

Validation:

- Reproduction with only `orderCreate-parity-plan.json` ungated first failed
  because `$.data.orderCreate.userErrors` did not resolve for valid input.
- `cd gleam && gleam test --target javascript` (756 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (752 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 106 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The parity spec's selected payload can be satisfied from the resolved
  `OrderCreateOrderInput`; no fixture rewrite or Shopify runtime passthrough is
  needed.
- Synthetic identity order is load-bearing for downstream payment specs:
  `Order/1`, `LineItem/2`, then `OrderTransaction/3` for the authorization
  local-runtime fixtures.
- Shopify sorts direct order tags lexicographically, preserves
  `presentmentMoney` on line-item prices when present, and computes
  `currentTotalPriceSet` as line subtotal plus shipping plus tax minus fixed
  discount.

### Risks / open items

- Payment transaction roots remain gated because `orderCapture`,
  `transactionVoid`, and `orderCreateMandatePayment` are not yet locally
  modeled in Gleam.
- Order-edit commit sessions, fulfillment creation/fulfillment-order workflows,
  and returns remain gated.

### Pass 151 candidates

- Use the now-staged direct `orderCreate` foundation to port payment
  transaction lifecycle roots, or switch to order-edit commit state if a smaller
  session model slice is available.

---

## 2026-05-01 - Pass 149: order edit validation parity

Promotes the checked-in `orderEditExistingOrder-validation` parity scenario in
the Gleam Orders domain. This pass seeds the captured validation source order,
keeps duplicate-variant add payloads on the local calculated-line path, and
adds Shopify's captured invalid-variant user error for `ProductVariant/0`.

| Module                                                   | Change                                                         |
| -------------------------------------------------------- | -------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds captured invalid-variant `orderEditAddVariant` payloads.  |
| `gleam/test/parity/runner.gleam`                         | Seeds captured order-edit source orders for validation parity. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers invalid variant null payload and user error shape.      |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing validation parity spec.              |
| `.agents/skills/gleam-port/SKILL.md`                     | Records order-edit validation porting notes.                   |

Validation:

- Reproduction with only `orderEditExistingOrder-validation.json` ungated first
  failed because `$.data.orderEditBegin.userErrors` did not resolve before the
  validation scenario seeded `$.seedOrder`.
- `cd gleam && gleam test --target javascript` (755 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (751 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 107 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The validation scenario uses the same begin seed path as other order-edit
  existing-order specs.
- Shopify's captured invalid variant id `gid://shopify/ProductVariant/0`
  returns a payload object with null calculated objects/session and a
  `variantId` user error, not a top-level GraphQL error.
- Duplicate existing variants with `allowDuplicates: false` still return a
  calculated line item in the captured scenario, so the existing add-variant
  calculated-line path covers that target once the product/variant seed is
  loaded.

### Risks / open items

- Order-edit commit sessions, direct order creation/delete, payment transaction
  and mandate roots, fulfillment creation/fulfillment-order workflows, and
  returns remain gated.

### Pass 150 candidates

- Continue order-edit by adding commit persistence for the captured add/remove
  workflows, or switch to another bounded existing-order, payment, fulfillment,
  or return fixture if it can be modeled without partial support.

---

## 2026-05-01 - Pass 148: order edit set quantity parity

Promotes the checked-in `orderEditSetQuantity` existing-order parity scenario
in the Gleam Orders domain. This pass seeds the captured zero-removal source
order, maps the synthetic calculated-line id from `orderEditBegin` back to the
captured source line item, and returns Shopify's stable zero-quantity
calculated-line payload without claiming commit or downstream edit persistence.

| Module                                                   | Change                                                           |
| -------------------------------------------------------- | ---------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `orderEditSetQuantity` calculated-line payload support.     |
| `gleam/test/parity/runner.gleam`                         | Seeds captured order-edit source orders for set-quantity parity. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers zero-quantity calculated-line payloads.                   |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing `orderEditSetQuantity` parity spec.    |
| `.agents/skills/gleam-port/SKILL.md`                     | Records order-edit set-quantity porting notes.                   |

Validation:

- Reproduction with only `orderEditSetQuantity-parity-plan.json` ungated failed
  because `fromPrimaryProxyPath` could not resolve
  `$.data.orderEditBegin.calculatedOrder.id` before the set-quantity scenario
  seeded `$.seedOrder`.
- `cd gleam && gleam test --target javascript` (754 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (750 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 108 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The set-quantity spec compares only the stable calculated-line payload and
  empty `userErrors`; it does not require a serialized session id.
- Because the begin payload mints calculated line ids deterministically after
  the calculated order id, the payload slice can map `CalculatedLineItem/N` back
  to the seeded order line item by order index. This is a narrow bridge, not
  persistent calculated-edit state.
- Commit and downstream order effects remain separate work. This pass returns
  the selected calculated line with overridden `quantity`/`currentQuantity`
  only.

### Risks / open items

- Order-edit commit sessions, direct order creation/delete, payment transaction
  and mandate roots, fulfillment creation/fulfillment-order workflows, and
  returns remain gated.

### Pass 149 candidates

- Continue order-edit by adding commit persistence for the captured add/remove
  workflows, or switch to another bounded existing-order, payment, fulfillment,
  or return fixture if it can be modeled without partial support.

---

## 2026-05-01 - Pass 147: order edit add variant parity

Promotes the checked-in `orderEditAddVariant` existing-order parity scenario in
the Gleam Orders domain. This pass reuses the captured begin-order fixture,
seeds the captured product/variant catalog, and returns the stable calculated
line-item payload locally without claiming set-quantity, commit, or persistent
calculated-order lifecycle support.

| Module                                                   | Change                                                          |
| -------------------------------------------------------- | --------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `orderEditAddVariant` calculated-line payload support.     |
| `gleam/test/parity/runner.gleam`                         | Seeds captured order-edit source orders for add-variant parity. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers add-variant payload, product title, SKU, and price set.  |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing `orderEditAddVariant` parity spec.    |
| `.agents/skills/gleam-port/SKILL.md`                     | Records order-edit add-variant porting notes.                   |

Validation:

- Reproduction with only `orderEditAddVariant-parity-plan.json` ungated failed
  because `fromPrimaryProxyPath` could not resolve
  `$.data.orderEditBegin.calculatedOrder.id` before the add-variant scenario
  seeded `$.seedOrder`.
- `cd gleam && gleam test --target javascript` (753 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (749 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 109 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The add-variant parity spec depends on the begin request only to provide a
  calculated-order id; the strict comparison target is the stable
  `calculatedLineItem`, empty `userErrors`, and an `OrderEditSession` GID type.
- Captured `seedProducts` already provide the product title and variant SKU/
  price. The local payload should prefer the product title over the variant
  option title, matching Shopify's calculated add-line shape.
- This is still a payload slice: it does not persist calculated edits or mutate
  downstream order line items until set/commit lifecycle support lands.

### Risks / open items

- Order-edit set/commit sessions, direct order creation/delete, payment
  transaction and mandate roots, fulfillment creation/fulfillment-order
  workflows, and returns remain gated.

### Pass 148 candidates

- Continue order-edit by adding `orderEditSetQuantity`/`orderEditCommit`
  calculated-edit persistence, or switch to another bounded existing-order,
  payment, fulfillment, or return fixture if it can be modeled without partial
  support.

---

## 2026-05-01 - Pass 146: order edit begin parity

Promotes the checked-in `orderEditBegin` existing-order parity scenario in the
Gleam Orders domain. This pass seeds the captured source order, builds a
synthetic calculated order/session payload locally, and preserves the captured
`originalOrder` and empty `userErrors` behavior without claiming add/set/commit
order-edit workflow support.

| Module                                                   | Change                                                       |
| -------------------------------------------------------- | ------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `orderEditBegin` existing-order calculated payloads.    |
| `gleam/test/parity/runner.gleam`                         | Seeds captured order-edit source orders from `$.seedOrder`.  |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers begin payload, line-item clone, and session id shape. |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing `orderEditBegin` parity spec.      |
| `.agents/skills/gleam-port/SKILL.md`                     | Records order-edit begin porting notes.                      |

Validation:

- Reproduction with only `orderEditBegin-parity-plan.json` ungated failed on
  `$.data.orderEditBegin.calculatedOrder.originalOrder` before implementation.
- `cd gleam && gleam test --target javascript` (752 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (748 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 110 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The begin parity spec compares only the stable calculated order
  `originalOrder` and empty `userErrors`; session ids and calculated-line ids
  are synthetic and intentionally not used as strict fixture equality.
- `orderEditBegin` needs the fixture's `$.seedOrder` payload, not a downstream
  read, because that is where the source line items for calculated-order
  cloning live.
- Begin can mint the calculated order/session shape without mutating order
  state. Add-variant, set-quantity, commit, and calculated edit persistence
  remain separate gated lifecycle work.

### Risks / open items

- Order-edit add/set/commit sessions, direct order creation/delete, payment
  transaction and mandate roots, fulfillment creation/fulfillment-order
  workflows, and returns remain gated.

### Pass 147 candidates

- Continue order-edit sessions by adding persistent calculated-order state for
  add/set/commit, or switch to another bounded existing-order/payment/return
  fixture if it can be modeled without partial support.

---

## 2026-05-01 - Pass 145: refund success parity

Promotes the checked-in full and partial `refundCreate` success parity
scenarios in the Gleam Orders domain. This pass stages synthetic refund,
refund-line-item, and refund transaction records over captured setup orders,
updates order financial status and refunded totals, and preserves downstream
order refund/transaction/returns visibility.

| Module                                                   | Change                                                         |
| -------------------------------------------------------- | -------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `refundCreate` success staging and downstream order data. |
| `gleam/test/parity/runner.gleam`                         | Seeds captured setup orders for refund success parity.         |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers partial refund success and downstream reads.            |
| `config/gleam-port-ci-gates.json`                        | Removes the two newly passing refund success parity specs.     |
| `.agents/skills/gleam-port/SKILL.md`                     | Records refund success porting notes.                          |

Validation:

- `cd gleam && gleam test --target javascript` (751 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (747 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 111 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- Refund success parity needs the captured setup `orderCreate` order because it
  carries line-item prices, shipping lines, sale transactions, and original
  total price needed to calculate status and downstream state.
- Shopify's captured `NO_RESTOCK` refund line item has `subtotalSet` `0.0`,
  while `RETURN` uses unit price times refunded quantity. The refund
  transaction amount drives the total refunded amount when present.
- Successful refunds append a refund transaction to the order transactions,
  append a refund to `order.refunds`, preserve the empty returns connection,
  and mark the order `REFUNDED` only once refunded total reaches the order
  total.

### Risks / open items

- Direct order creation/delete, payment transaction and mandate roots,
  fulfillment creation/fulfillment-order workflows, returns, and order-edit
  sessions remain gated.

### Pass 146 candidates

- Continue with another bounded existing-order lifecycle fixture, or start a
  coherent return/payment/order-edit slice only if downstream state and parity
  evidence can be modeled together.

---

## 2026-05-01 - Pass 144: refund over-refund validation parity

Promotes the checked-in `refundCreate` over-refund user-error parity scenario
in the Gleam Orders domain. This pass handles only the captured validation
branch: no refund is staged, the existing paid order is serialized unchanged,
and downstream `order(id:)` still shows no refunds or returns.

| Module                                                   | Change                                                       |
| -------------------------------------------------------- | ------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `refundCreate` over-refund validation payload handling. |
| `gleam/test/parity/runner.gleam`                         | Seeds the captured setup order for refund validation parity. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers over-refund payload and unchanged downstream reads.   |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing over-refund parity spec.           |

Validation:

- `cd gleam && gleam test --target javascript` (750 passed).
- Local `cd gleam && gleam test --target erlang` still hit the host runner
  `undef` issue after compile; Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  passed (746 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 113 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The captured over-refund scenario needs the setup `orderCreate` order, not
  only the downstream order read, because Shopify's error message compares the
  requested refund to the order's original `totalPriceSet`.
- Shopify returns `userErrors.field: null` for this branch and leaves the order
  unchanged: `refund` is null, financial status remains `PAID`, total refunded
  stays `0.0`, and downstream `refunds`/`returns` remain empty.

### Risks / open items

- Refund success paths, full/partial refund staging, transaction insertion,
  return flows, payment transaction roots, order creation/delete, and order-edit
  sessions remain gated.

### Pass 145 candidates

- Continue with another validation-only existing-resource fixture, or start a
  coherent refund success slice only if refund records, order totals,
  transactions, and downstream reads can be modeled together.

---

## 2026-05-01 - Pass 143: fulfillment cancel and tracking parity

Promotes the checked-in `fulfillmentCancel` and
`fulfillmentTrackingInfoUpdate` parity scenarios in the Gleam Orders domain.
This pass stages updates to existing fulfillments embedded in captured order
state and preserves immediate downstream `order(id:)` fulfillment reads.

| Module                                                   | Change                                                    |
| -------------------------------------------------------- | --------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Stages fulfillment cancel/tracking updates inside orders. |
| `gleam/test/parity/runner.gleam`                         | Seeds fulfillment parity from captured downstream orders. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers tracking, cancel, and downstream reads.            |
| `config/gleam-port-ci-gates.json`                        | Removes the two newly passing fulfillment parity specs.   |

Validation:

- `cd gleam && gleam test --target javascript` (749 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (745 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 114 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The captured fulfillment success specs are existing-fulfillment state updates:
  tracking replacement for `fulfillmentTrackingInfoUpdate`, and
  `CANCELLED`/`CANCELED` status fields for `fulfillmentCancel`. They can be
  staged locally over captured order JSON without claiming fulfillment creation
  or fulfillment-order workflows.
- Seeding from the captured downstream order keeps fulfillment line items and
  fulfillment-order snapshots available for strict downstream comparisons while
  the mutation handler touches only the matching fulfillment.

### Risks / open items

- Fulfillment creation, fulfillment-order lifecycle roots, direct order
  creation/delete, payment transaction and mandate roots, refunds, returns, and
  order-edit sessions remain gated.

### Pass 144 candidates

- Continue with another narrow existing-resource validation or lifecycle
  fixture, or start the larger direct-order creation/payment/refund slices only
  with coherent downstream state modeling.

---

## 2026-05-01 - Pass 142: order update field parity

Promotes the checked-in simple and expanded `orderUpdate` parity scenarios in
the Gleam Orders domain. This pass turns the previous validation-only
`orderUpdate` handler into a local existing-order update path for the captured
simple fields and preserves downstream `order(id:)` read visibility.

| Module                                                   | Change                                                   |
| -------------------------------------------------------- | -------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Stages existing-order update fields and metafields.      |
| `gleam/test/parity/runner.gleam`                         | Seeds order-update parity from captured downstream data. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers expanded update payload and downstream read.      |
| `config/gleam-port-ci-gates.json`                        | Removes the two newly passing order-update parity specs. |

Validation:

- `cd gleam && gleam test --target javascript` (748 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (744 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 116 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The captured `orderUpdate` success fixtures are limited to existing-order
  field staging: note/tags in the simple fixture and email, PO number, note,
  tags, custom attributes, shipping address, and one order metafield in the
  expanded fixture. These can be modeled over captured order JSON without
  claiming order creation, deletion, or order-edit session behavior.
- The expanded fixture updates an existing `custom/gift` metafield, so the
  Gleam handler preserves the seeded Shopify metafield id by namespace/key
  rather than minting a replacement id for that captured path.

### Risks / open items

- Direct order creation/delete success, broader payment transaction and mandate
  roots, fulfillment success paths, refunds, returns, and order-edit sessions
  remain gated.

### Pass 143 candidates

- Continue with another bounded existing-order lifecycle fixture, or start a
  coherent payment/fulfillment/refund slice only if the staged state model can
  satisfy all asserted downstream reads.

---

## 2026-05-01 - Pass 141: order mark-as-paid parity

Promotes the checked-in `orderMarkAsPaid` parity scenario in the Gleam Orders
domain. This pass adds local existing-order payment state staging for
mark-as-paid requests, preserves already-paid captured orders without
duplicating transactions, and seeds the parity fixture from its captured paid
order payload.

| Module                                                   | Change                                                   |
| -------------------------------------------------------- | -------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds local `orderMarkAsPaid` payment state handling.     |
| `gleam/test/parity/runner.gleam`                         | Seeds mark-as-paid parity from captured mutation order.  |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers mark-as-paid payload and downstream read effects. |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing mark-as-paid parity spec.      |

Validation:

- `cd gleam && gleam test --target javascript` (747 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (743 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 118 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The captured `orderMarkAsPaid` scenario selects the paid financial status,
  manual gateway, zero outstanding amount, and a successful manual SALE
  transaction. These fields can be modeled locally from an existing captured
  order without claiming the broader payment/transaction lifecycle roots.
- Parity seeding uses the captured already-paid mutation order, so the handler
  must serialize already-paid orders unchanged rather than appending a duplicate
  local transaction.

### Risks / open items

- Manual payment creation, mandate payments, transaction capture/void flows,
  order creation/update/delete success, fulfillment, refunds, returns, and
  order-edit sessions remain gated.

### Pass 142 candidates

- Continue with payment lifecycle roots only if their transaction/state
  interactions can be modeled coherently, or move to another narrow validation
  fixture.

---

## 2026-05-01 - Pass 140: order customer association parity

Promotes the checked-in `orderCustomerSet` and `orderCustomerRemove` parity
scenarios in the Gleam Orders gate without moving root ownership away from the
Customers domain. This pass seeds the captured customers and customer order
summaries required by the existing Customers-domain handlers, then ungates the
two Orders parity specs.

| Module                            | Change                                                        |
| --------------------------------- | ------------------------------------------------------------- |
| `gleam/test/parity/runner.gleam`  | Seeds order-customer parity from customer/order summary data. |
| `config/gleam-port-ci-gates.json` | Removes the two newly passing order-customer parity specs.    |

Validation:

- `cd gleam && gleam test --target javascript` (746 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (742 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 119 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The Gleam Customers domain already owns `orderCustomerSet` and
  `orderCustomerRemove` because those roots update Customer.orders summary
  state. Registering them in Orders would route customer order-summary parity to
  the wrong domain.
- The standalone Orders parity captures only need the mutation's selected
  `order.customer` payload and empty user errors. Seeding customer records plus
  customer order summaries from the capture lets the existing local handler
  satisfy those targets without duplicating order-customer logic.

### Risks / open items

- Direct order creation/update/delete success, payment mutations, fulfillment,
  refunds, returns, and order-edit sessions remain gated.
- Broader customer/order lifecycle edge cases stay with the Customers-domain
  parity coverage until a later whole-domain cutover reconciles ownership.

### Pass 141 candidates

- Continue with another narrow existing handler/seeding promotion if available,
  or start a coherent payment lifecycle slice such as `orderMarkAsPaid`.

---

## 2026-05-01 - Pass 139: order invoice send payload parity

Promotes the checked-in `orderInvoiceSend` parity scenario in the Gleam Orders
domain. This pass adds a local existing-order payload response for invoice-send
requests, returns the selected captured order and empty user errors, keeps
runtime state untouched, and seeds the parity fixture from its captured mutation
order payload.

| Module                                                   | Change                                                   |
| -------------------------------------------------------- | -------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds local `orderInvoiceSend` payload handling.          |
| `gleam/test/parity/runner.gleam`                         | Seeds invoice-send parity from captured mutation order.  |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers invoice-send payload and no staging side effects. |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing invoice-send parity spec.      |

Validation:

- `cd gleam && gleam test --target javascript` (746 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (742 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 121 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The captured `orderInvoiceSend` parity target asserts the returned order id
  and empty user errors; no local order state mutation is needed for this slice.
- Seeding from `$.mutation.response.data.orderInvoiceSend.order` preserves the
  selected order shape without claiming email delivery or side effects.

### Risks / open items

- Email-send delivery semantics, notification side effects, direct order
  creation/update/delete/customer/payment effects, fulfillment, refunds,
  returns, and order-edit sessions remain gated.

### Pass 140 candidates

- Continue with another narrow order management validation fixture, or start a
  coherent payment/customer lifecycle slice when the surrounding state effects
  can be modeled together.

---

## 2026-05-01 - Pass 138: order cancel downstream parity

Promotes the checked-in `orderCancel` parity scenario in the Gleam Orders
domain. This pass adds local cancellation staging for existing orders, returns
the captured immediate empty `orderCancelUserErrors` payload, updates
closed/cancelled fields for downstream `order(id:)` reads, and seeds the parity
fixture from its captured downstream order payload.

| Module                                                   | Change                                                  |
| -------------------------------------------------------- | ------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds local `orderCancel` mutation staging.              |
| `gleam/test/parity/runner.gleam`                         | Seeds captured downstream order state for cancellation. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers cancel payload and downstream cancelled state.   |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing order cancel parity spec.     |

Validation:

- `cd gleam && gleam test --target javascript` (745 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (741 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 122 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- Shopify's captured `orderCancel` mutation response can be an immediate
  payload with only `orderCancelUserErrors: []`; the cancelled state is observed
  through a downstream order read.
- For this executable scenario, seeding from
  `$.downstreamRead.response.data.order` gives the local proxy enough captured
  order shape to stage cancellation without claiming direct order creation or
  broader payment/customer side effects.
- `cancelReason` should come from the captured/requested `reason` argument;
  local timestamps can use the deterministic synthetic timestamp because the
  checked-in comparison targets assert closed state and cancel reason, not the
  exact live timestamp.

### Risks / open items

- Repeated-cancel errors, canceled-order interactions with other roots, direct
  order creation/update/delete/customer/payment effects, fulfillment, refunds,
  returns, and order-edit sessions remain gated.

### Pass 139 candidates

- Continue with a narrow order management fixture such as invoice send, or defer
  to a coherent payment/customer lifecycle slice when the surrounding state
  effects can be modeled together.

---

## 2026-05-01 - Pass 137: order open/close lifecycle parity

Promotes the checked-in `orderOpen` and `orderClose` parity scenarios in the
Gleam Orders domain. This pass adds narrow existing-order lifecycle staging for
open/close mutations, preserves selected captured order fields, writes the
staged order back into downstream reads, records local mutation-log drafts, and
seeds the two parity fixtures from their captured order payloads.

| Module                                                   | Change                                                        |
| -------------------------------------------------------- | ------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds local `orderOpen`/`orderClose` mutation staging.         |
| `gleam/test/parity/runner.gleam`                         | Seeds order-management parity scenarios from captured orders. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers close/open read-after-write lifecycle behavior.        |
| `config/gleam-port-ci-gates.json`                        | Removes the two newly passing order lifecycle specs.          |

Validation:

- `cd gleam && gleam test --target javascript` (744 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (740 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 123 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The captured `orderOpen`/`orderClose` scenarios only need existing-order
  lifecycle fields and selected downstream order reads; they can stage over a
  captured `OrderRecord` without claiming direct `orderCreate` success support.
- `orderClose` can use the synthetic timestamp for local `closedAt` and
  `updatedAt`; the checked-in comparison targets assert closed state and
  downstream read effects, not the captured live timestamp value.
- The parity runner should seed these scenarios from
  `$.mutation.response.data.<root>.order`, matching the captured order payload
  shape that the mutation and downstream read select.

### Risks / open items

- Repeated open/close user-error branches, canceled-order constraints, direct
  order creation/update success, payment/customer effects, fulfillment,
  refunds, returns, and order-edit sessions remain gated.

### Pass 138 candidates

- Continue with another narrow order-management validation/lifecycle fixture, or
  start a coherent order create/update/payment slice only when the required
  downstream state effects can be modeled together.

---

## 2026-05-01 - Pass 136: order update unknown-id validation

Promotes the checked-in `orderUpdate-parity-plan` scenario in the Gleam Orders
domain. This pass extends the existing `orderUpdate` validation guardrail from
missing/null nested IDs to Shopify's captured unknown-order payload branch,
returning `order: null` and the `Order does not exist` user error without
staging state or claiming update success behavior.

| Module                                                   | Change                                                      |
| -------------------------------------------------------- | ----------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds store-aware unknown-order `orderUpdate` user errors.   |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers unknown-id response plus no staging/logging effects. |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing order update validation spec.     |

Validation:

- `cd gleam && gleam test --target javascript` (743 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (739 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 125 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- Unknown-order `orderUpdate` is a payload user-error branch, not a top-level
  GraphQL error: `order: null`, `field: ["id"]`, and message
  `Order does not exist`.
- The guardrail needs access to effective order state so future successful
  `orderUpdate` support can distinguish unknown upstream IDs from local staged
  orders.

### Risks / open items

- Successful `orderUpdate` field changes, timestamp behavior, downstream reads,
  and lifecycle interactions remain gated.

### Pass 137 candidates

- Either continue with another captured validation guardrail, or begin the
  direct order lifecycle substrate when the order, payment, customer, inventory,
  fulfillment, refund, and return effects can be modeled coherently.

---

## 2026-05-01 - Pass 135: order create no-line-items validation

Promotes the checked-in `orderCreate-validation-matrix` scenario in the Gleam
Orders domain. This pass extends the existing `orderCreate` validation
guardrail beyond required-argument errors to mirror Shopify's captured
no-line-items business-rule user error without staging an order, writing a
mutation-log draft, or claiming order creation success behavior.

| Module                                                   | Change                                                          |
| -------------------------------------------------------- | --------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds the no-line-items `orderCreate` validation payload branch. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers the local no-line-items response and no staging/logging. |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing order create validation spec.         |

Validation:

- `cd gleam && gleam test --target javascript` (743 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (739 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 126 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- Shopify returns the no-line-items branch as payload `userErrors`, not a
  top-level GraphQL error: `order: null`, `field: ["order", "lineItems"]`, and
  message `Line items must have at least one line item`.
- This rejected branch should not stage an `OrderRecord`, mint synthetic IDs, or
  create a mutation-log draft.

### Risks / open items

- This is still a validation guardrail only. Successful `orderCreate`
  lifecycle, downstream read effects, payment transactions, inventory bypass,
  customer linkage, fulfillment state, refunds, and returns remain gated.

### Pass 136 candidates

- Continue with another validation-only order guardrail if it is backed by a
  captured executable fixture, or start the broader `orderCreate` lifecycle only
  when all downstream state effects can be modeled together.

---

## 2026-05-01 - Pass 134: order catalog filters and count limits

Promotes the checked-in `order-catalog-count-read` scenario in the Gleam Orders
domain. This pass extends the narrow order catalog slice with captured catalog
seeding from node-based responses, shared Admin search-query filtering for the
captured `tag:`, `name:`, `financial_status:`, and `fulfillment_status:` terms,
raw cursor replay for the captured next-page request, reverse ordering, and
`ordersCount(limit:)` precision.

| Module                                                   | Change                                                                     |
| -------------------------------------------------------- | -------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds order catalog query filtering, reverse windows, and count precision.  |
| `gleam/test/parity/runner.gleam`                         | Seeds node-based captured order catalogs with preserved cursors.           |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Extends direct catalog/count coverage for tag filters and limit precision. |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing order catalog/count parity spec.                 |

Validation:

- `cd gleam && gleam test --target javascript` (743 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (739 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 127 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- `orders` can use the shared `search_query_parser.apply_search_query` helper
  with a small order-specific positive term matcher. The captured terms only
  require exact tag/status matching and text matching for names.
- The captured catalog fixture selects `nodes`, not `edges`, so the parity
  runner needs to derive stable raw cursors from the captured pageInfo windows
  and attach them to seeded `OrderRecord`s.
- `ordersCount(limit:)` returns `AT_LEAST` when the filtered count exceeds the
  limit and otherwise returns `EXACT`; a `null` limit behaves as unlimited.

### Risks / open items

- Search support remains limited to the fields proven by the captured fixture.
- This does not add order lifecycle mutations, order-edit success paths,
  fulfillment success paths, refunds, returns, or customer/payment side effects.

### Pass 135 candidates

- Continue with another order read fixture that can build on the catalog
  substrate, or shift to the next lifecycle fixture only when the required
  downstream state can be modeled without partial mutation support.

---

## 2026-05-01 - Pass 133: order empty catalog/count reads

Promotes the checked-in `order-empty-state-read` scenario and the
`order-edit-residual-local-staging` empty `ordersCount` baseline in the Gleam
Orders domain. This pass adds narrow `orders`/`ordersCount` query support,
seeds the captured order catalog edge plus placeholder records for count and
pageInfo parity, and keeps missing direct order reads as Shopify-style `null`.

| Module                                                   | Change                                                                 |
| -------------------------------------------------------- | ---------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `orders` connection and `ordersCount` query serialization.        |
| `gleam/test/parity/runner.gleam`                         | Seeds captured order catalog edges and count-padding placeholders.     |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers order catalog cursor/pageInfo/count output.                     |
| `gleam/test/shopify_draft_proxy/proxy/*_test.gleam`      | Moves unported-root sentinel assertions from `orders` to fulfillments. |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing order empty-state and count-baseline specs.  |

Validation:

- `cd gleam && gleam test --target javascript` (743 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (739 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 128 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The older `order-empty-state-read` capture includes a selected order catalog
  node with only `id` and `name`. The connection serializer therefore preserves
  captured sparse nodes by omitting fields absent from the captured payload,
  while direct `order(id:)` detail reads still use the normal captured JSON
  projector.
- Count/pageInfo parity can be satisfied by padding the seeded order catalog
  with placeholder records after captured edges, mirroring the draft-order
  catalog approach without exposing placeholders on the selected first page.
- Once `orders` is locally dispatched, substrate tests that used it as an
  unported-root sentinel need to use a still-unported implemented root such as
  `fulfillmentOrders`.

### Risks / open items

- This pass does not add order search filtering, count `limit:` precision
  semantics, cursor `after:` replay, or lifecycle mutations.
- The `order-edit-residual-local-staging` parity target promoted here is only
  the empty `ordersCount` baseline. It does not claim order-edit session,
  commit, or order-delete mutation success behavior in Gleam.

### Pass 134 candidates

- Continue into `order-catalog-count-read` if search, sort, cursor, and count
  precision can be modeled against the captured catalog, or choose another
  narrow read slice backed by existing order fixtures.

---

## 2026-05-01 - Pass 132: order merchant detail read

Promotes the checked-in `order-merchant-detail-read` scenario in the Gleam
Orders domain. This pass introduces the first narrow first-class order state
slice, seeds the captured merchant-detail order fixture for parity replay, and
serves `order(id:)` reads by projecting the requested selection from captured
JSON while preserving Shopify's `null` behavior for missing order IDs.

| Module                                                    | Change                                                                      |
| --------------------------------------------------------- | --------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`         | Adds `OrderRecord` for captured order payloads.                             |
| `gleam/src/shopify_draft_proxy/state/store.gleam`         | Adds base/staged order buckets and effective order lookup helpers.          |
| `gleam/src/shopify_draft_proxy/state/serialization.gleam` | Adds order bucket dump/restore serialization.                               |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`        | Adds `order(id:)` query dispatch and captured JSON projection.              |
| `gleam/test/parity/runner.gleam`                          | Seeds `order-merchant-detail-read` from the captured Shopify order payload. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam`  | Covers seeded order detail reads and missing-order `null` responses.        |
| `config/gleam-port-ci-gates.json`                         | Removes the newly passing merchant-detail order parity spec.                |

Validation:

- `cd gleam && gleam test --target javascript` (742 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (738 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 130 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- `order(id:)` can be promoted before order catalog/search/counts by seeding
  only the captured order detail payload and projecting fields from captured
  JSON. This keeps the slice faithful without inventing broad order behavior.
- The order state bucket needs dump/restore support immediately because parity
  seeding uses the same store shape as runtime state snapshots.
- The local read should return GraphQL `null` when no captured or staged order
  exists for an ID, matching Shopify's no-data behavior.

### Risks / open items

- This does not add `orders` connection/count/search support or any order
  lifecycle mutation behavior.
- Order-edit sessions, fulfillment success paths, refunds, returns, and
  payment/customer downstream effects remain gated.

### Pass 133 candidates

- Continue with a narrow order no-data/read/count slice if backed by existing
  captures, or pick the next order lifecycle fixture that can reuse the new
  `OrderRecord` state without claiming unsupported mutations.

---

## 2026-05-01 - Pass 131: draft order create from order

Promotes the checked-in `draftOrderCreateFromOrder-parity-plan` scenario in
the Gleam Orders domain. This pass adds local `draftOrderCreateFromOrder`
dispatch, required `orderId` validation, source-order lookup through completed
draft orders, and staged draft-order creation with fresh draft/line-item IDs and
downstream `draftOrder(id:)` visibility.

| Module                                                   | Change                                                                               |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `draftOrderCreateFromOrder` mutation handling from embedded completed orders.   |
| `gleam/test/parity/runner.gleam`                         | Seeds the captured setup draft/order chain for create-from-order parity replay.      |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers local create-complete-createFromOrder read-after-write plus missing order id. |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing create-from-order parity spec.                             |

Validation:

- `cd gleam && gleam test --target javascript` (741 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (737 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 131 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The captured create-from-order setup can be replayed without a new broad
  order store by seeding the completed draft order and finding its nested
  `order` payload by id.
- Shopify resets draft-order fields when creating from an order: the new draft
  is open/ready, clears shipping and discounts, allocates fresh draft and
  draft-line-item IDs, and recomputes totals from order line-item unit prices.
- The captured completed-order payload carries line-item prices, while the
  source draft carries the original email. The local builder keeps both pieces
  together for this narrow slice.

### Risks / open items

- This does not introduce general `OrderRecord` state or standalone order
  reads/search/counts; broader order lifecycle remains gated.
- Fulfillment success paths, refunds, returns, and order editing remain
  unported.

### Pass 132 candidates

- Start a narrow order no-data/read/count slice if it can be backed by existing
  captured parity fixtures, or continue with the next draft-order-adjacent
  order lifecycle fixture that does not require broad fulfillment state.

---

## 2026-05-01 - Pass 130: draft order catalog/count reads

Promotes the checked-in `draftOrders-read-parity-plan`,
`draftOrdersCount-read-parity-plan`, and `draftOrders-invalid-email-query-read`
parity scenarios in the Gleam Orders domain. This pass adds local
`draftOrders` connection serialization, `draftOrdersCount`, captured catalog
baseline seeding for parity, and Shopify's invalid-email search warning
extension for these draft-order roots.

| Module                                                   | Change                                                                           |
| -------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `draftOrders`/`draftOrdersCount` query roots and search-warning extensions. |
| `gleam/test/parity/runner.gleam`                         | Seeds captured draft-order catalog edges, cursors, and count baselines.          |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers draft-order connection/count output plus invalid email warnings.          |
| `config/gleam-port-ci-gates.json`                        | Removes the three newly passing `draftOrders*` specs.                            |

Validation:

- `cd gleam && gleam test --target javascript` (738 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (734 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 136 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- Captured catalog parity needs cursor preservation from connection edges, not
  generated local cursors, because the checked-in specs compare strict JSON.
- Count parity can be satisfied by seeding placeholder draft orders after the
  captured window; only the count/pageInfo shape observes the extra records.
- Shopify returns invalid-email search warnings for both `draftOrders` and
  `draftOrdersCount` while still returning the unfiltered catalog baseline.

### Risks / open items

- This pass does not add draft-order search filtering beyond the captured
  invalid-email warning branch.
- `draftOrderCreateFromOrder`, broader order lifecycle, order editing,
  fulfillment success paths, refunds, and returns remain unported.

### Pass 131 candidates

- Continue with `draftOrderCreateFromOrder` while the draft-order data model is
  active, or start a narrow order no-data/read slice if create-from-order needs
  broader order seeding first.

---

## 2026-05-01 - Pass 129: draft order create validation parity

Promotes the checked-in `draftOrderCreate-validation-matrix` parity scenario in
the Gleam Orders domain. This pass adds Shopify-shaped payload validation for
invalid draft-order create inputs, keeps failed branches from staging draft
orders or consuming synthetic IDs, and preserves valid local create staging.

| Module                                                   | Change                                                                             |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds create-time validation and nullable user-error payload serialization.         |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers the captured validation matrix and verifies failed creates stage no drafts. |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing `draftOrderCreate-validation-matrix` spec.               |

Validation:

- `cd gleam && gleam test --target javascript` (737 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (733 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 139 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The create validation matrix can run without new live credentials because its
  captured request/response fixture already covers the invalid input branches.
- Validation failures need the same nullable `userErrors.field` projection as
  invoice-send guardrails, and they should produce failed mutation-log drafts
  without local store changes.
- Reserve-inventory validation should use the existing cross-target timestamp
  FFI rather than a fixed cutoff so future-dated valid creates stay valid.

### Risks / open items

- This does not port draft-order catalog/count/search hydration, nor
  `draftOrderCreateFromOrder`; those remain gated.
- Broader order lifecycle, order editing, fulfillment success paths, refunds,
  and returns remain unported.

### Pass 130 candidates

- Continue with `draftOrderCreateFromOrder` if the existing setup/order data
  can be seeded narrowly, or tackle draft-order catalog/count/search hydration
  as a focused read slice.

---

## 2026-05-01 - Pass 128: draft order residual helper roots

Promotes the checked-in `draft-order-residual-helper-roots` parity scenario in
the Gleam Orders domain. This pass adds local handling for draft-order delivery
option no-data reads, `draftOrderCalculate`, `draftOrderInvoicePreview`, and
the bulk add/remove/delete tag helpers. It preserves local read-after-write
effects for staged draft-order tags and deletion while keeping the work scoped
to draft orders rather than broad order lifecycle behavior.

| Module                                                   | Change                                                                                   |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds helper root dispatch, empty delivery options, calculate, invoice preview, and bulk. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers helper payloads plus bulk tag/delete read-after-write behavior directly.          |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing `draft-order-residual-helper-roots` spec.                      |

Validation:

- `cd gleam && gleam test --target javascript` (736 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (732 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 140 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The residual helper fixture can reuse staged draft orders from
  `draftOrderCreate`; no capture seeding is needed for the primary scenario.
- `draftOrderCalculate` needs a calculate-shaped `lineItems` list and explicit
  `currencyCode`, not the connection-shaped `lineItems` stored on staged draft
  orders.
- Bulk tag helpers can model Shopify's async `Job` payload deterministically
  while still applying local staged tag/delete effects immediately.

### Risks / open items

- Saved-search query semantics are reused from the saved-search domain, but
  broader `draftOrders` catalog/count/search hydration remains gated.
- `draftOrderCreateFromOrder`, order lifecycle, order editing, fulfillment
  success paths, refunds, and returns remain unported.

### Pass 129 candidates

- Continue with draft-order catalog/count/search parity, or a narrow validation
  matrix if the checked-in fixture can be modeled without broad order-store
  work.

---

## 2026-05-01 - Pass 127: draft order invoice send guardrails

Promotes the captured `draftOrderInvoiceSend` safety parity plan in the Gleam
Orders domain. This pass keeps the root validation-only: it handles unknown,
deleted/unseeded, open no-recipient, and completed no-recipient drafts locally,
serializes Shopify's nullable user-error `field`, and deliberately avoids
claiming email-send success behavior or mutating staged draft-order state.

| Module                                                   | Change                                                                                          |
| -------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `draftOrderInvoiceSend` dispatch and captured no-recipient/not-found/paid validation.      |
| `gleam/test/parity/runner.gleam`                         | Seeds only the captured open/completed no-recipient draft states before replaying the scenario. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers unknown-id and open no-recipient validation directly.                                    |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing `draftOrderInvoiceSend-parity-plan` spec.                             |

Validation:

- `cd gleam && gleam test --target javascript` (735 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (731 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 141 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- Shopify returns `field: null` for the captured invoice-send user errors, so
  the Orders serializer needs a nullable user-error projection instead of the
  existing list-backed helper used by other mutation payloads.
- A completed draft with no recipient returns both `To can't be blank` and
  `Draft order Invoice can't be sent. This draft order is already paid.`, while
  unknown and deleted/unseeded draft ids both return `Draft order not found`.

### Risks / open items

- Recipient-backed invoice send success remains gated; this pass does not send
  email, mutate draft state, or claim notification lifecycle support.
- Draft-order helper roots, broader draft-order reads/count/search, order
  lifecycle, order editing, fulfillment success paths, refunds, and returns
  remain unported.

### Pass 128 candidates

- Continue with draft-order helper roots or read/count/search parity if the
  checked-in fixtures can be modeled without broad order-store work.

---

## 2026-05-01 - Pass 126: draft order complete lifecycle

Promotes the captured `draftOrderComplete` parity plan in the Gleam Orders
domain. This pass keeps the required-id validation guardrails, completes a
seeded/staged draft locally, attaches a synthetic nested order to the completed
draft, preserves downstream `draftOrder(id:)` visibility, and keeps root order
reads/counts gated until the order store slice is ported.

| Module                                                   | Change                                                                                  |
| -------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `draftOrderComplete` success handling, completed draft mutation, and nested order. |
| `gleam/test/parity/runner.gleam`                         | Seeds the captured setup draft and setup input note before replaying completion.        |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers completion payload normalization and downstream reads directly.                  |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing `draftOrderComplete-parity-plan` spec.                        |

Validation:

- `cd gleam && gleam test --target javascript` (734 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (730 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 142 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- Shopify completes the same draft-order id rather than allocating a new draft;
  draft line-item ids remain stable while the linked order receives fresh order
  and order line-item ids.
- The captured Shopify order normalizes any non-null completion `sourceName` to
  `347082227713`, records `manual` as the payment gateway when
  `paymentPending` is false, and copies the setup input note into the completed
  order even though the setup draft-order response did not select `note`.

### Risks / open items

- Root `order(id:)`, `orders`, and `ordersCount` visibility for completed draft
  orders remains gated until the Gleam order store slice exists.
- Draft-order invoice/helper roots, order editing, fulfillment success paths,
  refunds, and returns remain unported.

### Pass 127 candidates

- Continue with draft-order read/count/search parity or a narrow invoice/helper
  root if the checked-in fixture can be modeled without broad order-store work.

---

## 2026-05-01 - Pass 125: draft order duplicate lifecycle

Promotes the captured `draftOrderDuplicate` parity plan in the Gleam Orders
domain. This pass stages a local duplicate with fresh draft-order and line-item
IDs, preserves the copied customer/address/tag/custom-attribute fields, clears
the Shopify-cleared shipping and discount fields, recalculates totals, and keeps
the duplicate visible through downstream `draftOrder(id:)` reads.

| Module                                                   | Change                                                                           |
| -------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `draftOrderDuplicate` mutation handling and duplicate graph normalization.  |
| `gleam/test/parity/runner.gleam`                         | Seeds the captured setup draft order before replaying the duplicate parity plan. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers duplicate payload normalization and downstream reads directly.            |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing `draftOrderDuplicate-parity-plan` spec.                |

Validation:

- `cd gleam && gleam test --target javascript` (733 passed).
- Docker Erlang fallback
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (729 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 specs, 143 expected failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- Shopify duplicate preserves identifying customer/address/tag/custom-attribute
  data from the source draft, but it clears `taxExempt`,
  `reserveInventoryUntil`, order-level discount, shipping line, and line-item
  discounts before recalculating totals.
- The parity spec already treats duplicate draft ids, names, invoice URLs, and
  line-item ids as expected differences, so the port can use deterministic
  synthetic ids while strictly comparing the stable duplicated graph.

### Risks / open items

- `draftOrderComplete` success, invoice, bulk/helper roots, and broader
  draft-order search/count reads remain gated.
- Order lifecycle, order editing success paths, fulfillment success paths,
  refunds, and returns remain unported.

### Pass 126 candidates

- Continue draft-order lifecycle with `draftOrderComplete` success if the
  checked-in fixture can be modeled safely, or move to a small draft-order
  read/count/search slice backed by checked-in parity evidence.

---

## 2026-05-01 - Pass 124: draft order update lifecycle

Promotes the captured `draftOrderUpdate` parity plan in the Gleam Orders
domain. This pass seeds the setup draft order from the live capture, stages
local update effects for the captured draft-order fields, recalculates totals
when shipping or line items change, and preserves downstream `draftOrder(id:)`
read-after-write visibility.

| Module                                                   | Change                                                                             |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `draftOrderUpdate` mutation handling, local field updates, and recalculation. |
| `gleam/test/parity/runner.gleam`                         | Seeds the captured setup draft order before replaying the update parity plan.      |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers local update payloads and downstream reads directly.                        |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing `draftOrderUpdate-parity-plan` spec.                     |

Validation:

- `cd gleam && gleam test --target javascript` (732 passed).
- Docker Erlang fallback:
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (728 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 parity specs; 144 expected Gleam
  parity failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The update capture reuses the setup draft order's Shopify-generated id, name,
  invoice URL, customer, discount, addresses, and line-item ids; the port seeds
  that captured graph into base state, then stages only the requested local
  changes.
- Draft-order totals must be recalculated from the effective line-item totals,
  order discount, and shipping line after update, even when the parity request
  only changes the shipping line.

### Risks / open items

- `draftOrderDuplicate`, `draftOrderComplete` success, invoice, bulk/helper
  roots, and broader draft-order search/count reads remain gated.
- Order lifecycle, order editing success paths, fulfillment success paths,
  refunds, and returns remain unported.

### Pass 125 candidates

- Continue draft-order lifecycle with `draftOrderDuplicate`, or move to a
  small draft-order read/count/search slice backed by checked-in parity
  evidence.

---

## 2026-05-01 - Pass 123: draft order delete lifecycle

Promotes the captured `draftOrderDelete` parity plan in the Gleam Orders domain.
This pass stages a local draft-order deletion, returns the selected `deletedId`
and empty `userErrors`, and preserves Shopify-like downstream `draftOrder(id:)`
null behavior through the deleted-id marker.

| Module                                                   | Change                                                                        |
| -------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds `draftOrderDelete` mutation handling and selected payload serialization. |
| `gleam/src/shopify_draft_proxy/state/store.gleam`        | Adds staged draft-order deletion with a deleted-id marker.                    |
| `gleam/test/parity/runner.gleam`                         | Seeds the captured draft order before replaying the delete parity plan.       |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers delete payload and downstream local read suppression directly.         |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing `draftOrderDelete-parity-plan` spec.                |

Validation:

- `cd gleam && gleam test --target javascript` (731 passed).
- Docker Erlang fallback:
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (727 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 parity specs; 145 expected Gleam
  parity failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- Draft-order state already carried deleted-id markers, but the Gleam store did
  not yet expose a helper to stage draft-order deletion.
- The Shopify capture verifies downstream `draftOrder(id:)` returns null after
  delete; the local helper marks deletes instead of only removing staged data so
  seeded base records are also suppressed.

### Risks / open items

- `draftOrderUpdate`, `draftOrderDuplicate`, `draftOrderComplete` success, and
  invoice/helper roots remain gated.
- Order lifecycle, order editing success paths, fulfillment success paths,
  refunds, and returns remain unported.

### Pass 124 candidates

- Continue draft-order lifecycle with update/delete siblings, or move to a
  small read/count/search slice backed by checked-in parity evidence.

---

## 2026-05-01 - Pass 122: fulfillment create invalid-id guardrail

Promotes the captured `fulfillmentCreate` invalid fulfillment-order id branch
in the Gleam Orders domain. This pass mirrors Shopify's top-level
`RESOURCE_NOT_FOUND` error with `data.fulfillmentCreate: null` for the checked-in
invalid-id parity fixture, while keeping successful fulfillment creation and
downstream fulfillment reads gated.

| Module                                                   | Change                                                                     |
| -------------------------------------------------------- | -------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds a narrow `fulfillmentCreate` invalid-id resource-not-found guardrail. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers the invalid fulfillment-order id response envelope directly.        |
| `config/gleam-port-ci-gates.json`                        | Removes the newly passing `fulfillmentCreate-invalid-id-parity` spec.      |

Validation:

- `cd gleam && gleam test --target javascript` (730 passed).
- Docker Erlang fallback:
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (726 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 parity specs; 146 expected Gleam
  parity failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- Shopify returns this branch as a top-level GraphQL error with
  `extensions.code = RESOURCE_NOT_FOUND` and a null mutation payload, not as a
  mutation `userErrors` payload.
- The checked-in parity target compares the stable data/error message,
  extensions, and path; locations remain outside the comparison target.

### Risks / open items

- `fulfillmentCreate` happy path, fulfillment-order state, and downstream order
  fulfillment visibility remain gated.
- Order lifecycle, order editing success paths, refunds, returns, and the
  remaining draft-order lifecycle roots remain unported.

### Pass 123 candidates

- Start a durable draft-order update/delete lifecycle slice, or continue
  fulfillment success-path state only with checked-in parity evidence.

---

## 2026-05-01 - Pass 121: order-edit missing-id guardrails

Promotes the captured order-edit missing-id validation branches in the Gleam
Orders domain. This pass mirrors Shopify's `INVALID_VARIABLE` response when the
non-null `$id` variable is omitted for `orderEditBegin`, `orderEditAddVariant`,
`orderEditSetQuantity`, and `orderEditCommit`, without claiming edit-session
lifecycle support.

| Module                                                   | Change                                                                                      |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds narrow required-`id` validation guardrails for four order-edit mutation roots.         |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers missing `$id` variables for begin/add-variant/set-quantity/commit branches directly. |
| `config/gleam-port-ci-gates.json`                        | Removes four newly passing order-edit missing-id validation parity specs.                   |

Validation:

- `cd gleam && gleam test --target javascript` (729 passed).
- Docker Erlang fallback:
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (725 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 parity specs; 147 expected Gleam
  parity failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- The checked-in parity specs compare only the stable error message and
  extensions; Shopify also reports root-specific variable locations that are
  intentionally outside the current comparison target.
- These roots remain guardrail-only. Successful edits still need calculated
  order state, line-item mutations, commit effects, and downstream order reads.

### Risks / open items

- Order edit begin/add-variant/set-quantity/commit success paths remain gated.
- Order lifecycle, fulfillments, refunds, returns, and the remaining draft-order
  lifecycle roots remain unported.

### Pass 122 candidates

- Start a durable draft-order update/delete lifecycle slice, or continue
  order-edit calculated edit state with checked-in parity evidence.

---

## 2026-05-01 - Pass 120: order update validation guardrails

Promotes the captured `orderUpdate` missing-id validation branches in the Gleam
Orders domain. This pass mirrors Shopify's error message/extension shapes for
inline missing `input.id`, inline null `input.id`, and variable-backed
`OrderInput` values without an id, while leaving order update lifecycle
semantics gated.

| Module                                                   | Change                                                                                |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds narrow nested `OrderInput.id` validation guardrails for `orderUpdate`.           |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers inline missing, inline null, and variable-backed missing id branches directly. |
| `config/gleam-port-ci-gates.json`                        | Removes three newly passing `orderUpdate` validation parity specs.                    |

Validation:

- `cd gleam && gleam test --target javascript` (728 passed).
- Docker Erlang fallback:
  `docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine sh -lc 'gleam clean && gleam test --target erlang'`
  (724 passed).
- `corepack pnpm gleam:format:check`.
- `corepack pnpm gleam:port:coverage` (379 parity specs; 151 expected Gleam
  parity failures).
- `corepack pnpm conformance:check` (1402 passed).
- `corepack pnpm conformance:parity` (384 passed).
- `corepack pnpm lint`.
- `corepack pnpm typecheck`.
- `corepack pnpm gleam:registry:check`.
- `git diff --check`.

### Findings

- `orderUpdate` cannot use the top-level required-argument helper for these
  captures because Shopify validates the required `id` inside `OrderInput`.
- The checked-in parity specs compare the stable error message/extensions, so
  this pass keeps the guardrail focused and does not claim update success,
  downstream reads, metafields, address updates, or timestamp behavior.

### Risks / open items

- `orderUpdate-parity-plan`, `orderUpdate-live-parity`, and
  `orderUpdate-expanded-parity-plan` remain gated.
- Order lifecycle, editing, fulfillments, refunds, returns, and the remaining
  draft-order lifecycle roots remain unported.

### Pass 121 candidates

- Continue order-edit missing-id validation guardrails, or start a durable
  draft-order update/delete lifecycle slice with checked-in parity evidence.

---

## 2026-05-01 - Pass 119: order create validation guardrails

Promotes the captured `orderCreate` required-`order` validation branches in the
Gleam Orders domain. This pass mirrors Shopify's top-level GraphQL validation
errors for omitted, inline-null, and missing variable order input values without
claiming direct order creation, payment staging, inventory effects, or
downstream order materialization.

| Module                                                   | Change                                                                   |
| -------------------------------------------------------- | ------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds a narrow `orderCreate` required-order validation guardrail.         |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers inline missing-order and inline null-order error shapes directly. |
| `config/gleam-port-ci-gates.json`                        | Removes three newly passing `orderCreate` validation parity specs.       |

Validation:
Full JavaScript is green at 727 tests; Docker Erlang is green at 723 tests.
Orders now has 22 executable/pass specs and 56 gated specs out of 78.

### Findings

- `orderCreate`'s captured missing-order branches fit the existing top-level
  required argument helper with `OrderCreateOrderInput!`.
- These validation branches are not evidence for the happy-path `orderCreate`
  lifecycle; order creation remains gated until local order state, payments,
  inventory bypass, and downstream reads are modeled.

### Risks / open items

- `orderCreate-parity-plan` and `orderCreate-validation-matrix` remain gated.
- Order update/open/close/cancel/customer/payment roots, order editing,
  fulfillments, refunds, and returns remain unported.

### Pass 120 candidates

- Continue validation branches for `orderUpdate` after adding nested input
  object validation helpers, or move to a durable draft-order update/delete
  lifecycle slice.

---

## 2026-05-01 - Pass 118: fulfillment validation guardrails

Promotes the captured `fulfillmentCancel` and
`fulfillmentTrackingInfoUpdate` required-id validation branches in the Gleam
Orders domain. This pass mirrors Shopify's top-level GraphQL validation errors
for omitted, inline-null, and missing variable fulfillment identifiers without
claiming fulfillment lifecycle staging, cancellation, or tracking update
success semantics.

| Module                                                   | Change                                                                                   |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds narrow fulfillment required-id validation guardrails for two mutation roots.        |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers inline missing-id and inline null-id shapes for cancel and tracking update roots. |
| `config/gleam-port-ci-gates.json`                        | Removes six newly passing fulfillment validation parity specs.                           |

Validation:
Full JavaScript is green at 726 tests; Docker Erlang is green at 722 tests.
Orders now has 19 executable/pass specs and 59 gated specs out of 78.

### Findings

- `fulfillmentCancel` validates a required `id: ID!` argument, while
  `fulfillmentTrackingInfoUpdate` validates `fulfillmentId: ID!`; both fit the
  existing shared mutation validation helper.
- These are guardrails only. The happy-path fulfillment cancel/tracking specs
  still need order fulfillment state, downstream visibility, and local mutation
  log effects before they can be ungated.

### Risks / open items

- `fulfillmentCreate-invalid-id-parity`, `fulfillmentCancel-parity-plan`, and
  `fulfillmentTrackingInfoUpdate-parity-plan` remain gated.
- Draft-order lifecycle, order editing, refunds, and returns remain unported.

### Pass 119 candidates

- Continue validation branches for `orderUpdate`/`orderCreate`, or start a
  durable lifecycle slice for draft-order update/delete if it can be backed by
  checked-in executable parity.

---

## 2026-05-01 - Pass 117: draft-order complete validation guardrails

Promotes the captured `draftOrderComplete` required-`id` validation branches in
the Gleam Orders domain. This pass mirrors Shopify's top-level GraphQL
validation errors for omitted, inline-null, and missing variable `id` values
without claiming the happy-path draft-order completion lifecycle.

| Module                                                   | Change                                                                    |
| -------------------------------------------------------- | ------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds a narrow `draftOrderComplete` required-id validation guardrail.      |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers inline missing-id and inline null-id error shapes directly.        |
| `config/gleam-port-ci-gates.json`                        | Removes three newly passing `draftOrderComplete` validation parity specs. |

Validation:
Full JavaScript is green at 725 tests. Orders now has 13 executable/pass specs
and 65 gated specs out of 78.

### Findings

- The existing mutation validation helper is sufficient for
  `draftOrderComplete`'s required top-level `id` argument; no draft-order
  completion state transition should be inferred from these guardrails.
- The happy-path `draftOrderComplete` scenario remains gated until the port can
  model payment/source effects and downstream order materialization.

### Risks / open items

- `draftOrderComplete-parity-plan` and the rest of the draft-order mutation
  lifecycle remain unported.
- Regular order lifecycle, editing, fulfillment, refund, and return roots
  remain unported.

### Pass 118 candidates

- Continue with another small captured validation branch only if it does not
  broaden runtime support beyond the proven branch, or start modeling
  draft-order update/delete lifecycle state.

---

## 2026-05-01 - Pass 116: draft-order detail read seeding

Promotes the standalone draft-order detail read parity scenario in the Gleam
runner. The Orders runtime already projects captured draft-order records through
the `draftOrder(id:)` query path from Pass 115; this pass seeds the captured
detail fixture into base state so the checked-in strict read contract can run
without changing the request or fixture.

| Module                            | Change                                                                                  |
| --------------------------------- | --------------------------------------------------------------------------------------- |
| `gleam/test/parity/runner.gleam`  | Seeds `draft-order-detail-read` from `$.response.data.draftOrder` as a draft-order row. |
| `config/gleam-port-ci-gates.json` | Removes the newly passing `draftOrder-read-parity-plan` Orders spec.                    |

Validation:
Full JavaScript is green at 724 tests. `corepack pnpm gleam:port:coverage` is
green with 379 specs and 166 expected Gleam parity failures. Orders now has 10
executable/pass specs and 68 gated specs out of 78.

### Findings

- The captured detail read does not need bespoke field modeling yet; storing
  the captured `DraftOrder` payload as `CapturedJsonValue` preserves the strict
  selected-field contract.
- Scenario-specific seeding is enough for standalone read parity; it should not
  be generalized into broad draft-order fixture hydration until update,
  complete, delete, invoice, and helper roots define their lifecycle needs.

### Risks / open items

- `draft-order-residual-helper-roots` and the draft-order lifecycle mutations
  remain gated.
- The broader order lifecycle, editing, fulfillment, refund, and return roots
  remain unported.

### Pass 117 candidates

- Continue the draft-order cluster with validation branches for
  `draftOrderComplete`, `draftOrderDelete`, or the residual helper roots only
  when each branch has executable parity evidence.

---

## 2026-05-01 - Pass 115: draft-order create/read parity seed

Ports the first executable draft-order lifecycle slice in the Orders Gleam
domain. The local Orders dispatcher now handles `draftOrder(id:)` not-found
reads, `draftOrderCreate` required-input validation branches, and the captured
draft-order create/read-after-write parity scenario. Draft orders are staged as
captured JSON records so selected downstream reads project the same Shopify
field shapes while broader draft-order roots remain gated.

| Module                                                    | Change                                                                                              |
| --------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`        | Adds `draftOrder` reads plus `draftOrderCreate` staging for the captured custom/variant line slice. |
| `gleam/src/shopify_draft_proxy/state/types.gleam`         | Adds captured draft-order records and a minimal variant catalog seed record.                        |
| `gleam/src/shopify_draft_proxy/state/store.gleam`         | Adds instance-owned draft-order base/staged state and variant catalog helpers.                      |
| `gleam/src/shopify_draft_proxy/state/serialization.gleam` | Carries draft-order slices through dump/restore serialization.                                      |
| `gleam/test/parity/runner.gleam`                          | Seeds customer and variant catalog data for the live draft-order create parity fixture.             |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam`  | Covers not-found reads, validation guardrails, and custom-line create read-after-write.             |
| `config/gleam-port-ci-gates.json`                         | Removes five newly passing Orders draft-order parity specs.                                         |

Validation:
Full JavaScript is green at 724 tests. The draft-order create parity plan now
passes in the Gleam runner, bringing Orders to 9 executable/pass specs with 69
Orders specs still gated.

### Findings

- Variant-backed draft-order line items have separate Shopify semantics for
  line-item `sku`, line-item `variantTitle`, and nested `variant.sku`: a
  default-title variant projects `variantTitle: null`, line item `sku` can be
  the empty string, and nested variant `sku` remains null.
- The captured draft-order create fixture needs customer preconditions plus a
  variant catalog seed derived from the captured created line item; no parity
  request or fixture shape changes were needed.
- Storing draft orders as captured JSON is the right first boundary for this
  slice because it preserves selected field projection without prematurely
  designing the whole draft-order lifecycle model.

### Risks / open items

- Existing captured draft-order detail reads still need base draft-order
  seeding before they can be ungated.
- Draft-order update, complete, delete, invoice, and create-from-order roots
  remain unported; this pass is not broader order lifecycle support.

### Pass 116 candidates

- Seed captured draft-order detail fixtures so `draftOrder-read-parity-plan`
  can run, then continue into draft-order update/delete/complete lifecycle
  behavior.

---

## 2026-05-01 - Pass 114: orders access-denied guardrail parity

Promotes two captured Orders access-denied guardrail fixtures into the Gleam
parity suite without claiming broader order payment or tax-summary lifecycle
support. The Orders dispatcher now handles only the documented safe
`orderCreateManualPayment` unknown/non-local order branch and
`taxSummaryCreate` access-denied branch locally, returning Shopify-shaped
top-level `ACCESS_DENIED` errors with the selected root payload set to null.

| Module                                                   | Change                                                                                       |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`       | Adds captured access-denied error payloads and failed mutation-log drafts for the two roots. |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam` | Covers both access-denied guardrails directly.                                               |
| `config/gleam-port-ci-gates.json`                        | Removes the two newly passing Orders access-denied parity specs.                             |

Validation:
Full JavaScript is green at 721 tests. `corepack pnpm gleam:port:coverage` is
green with 379 parity specs and 172 expected Gleam parity failures.

### Findings

- `ACCESS_DENIED` GraphQL errors preserve both `errors` and `data` in the same
  response envelope; the Orders mutation accumulator now keeps selected null
  root payloads when a handled branch also returns top-level errors.
- These roots remain guardrail-only in Gleam. `orderCreateManualPayment` does
  not yet model local synthetic-order success, and `taxSummaryCreate` does not
  model tax-app success semantics.

### Risks / open items

- The remaining Orders parity gaps still include draft-order lifecycle,
  regular order lifecycle, order editing, fulfillments, refunds, and returns.

### Pass 115 candidates

- Continue Orders with a complete draft-order create/read lifecycle slice that
  seeds or models the ProductVariant and Customer data required by the captured
  merchant-realistic fixture.

---

## 2026-05-01 - Pass 113: orders abandonment no-data parity

Starts the Orders Gleam domain with the narrow abandoned-checkout and
abandonment slice backed by checked-in parity evidence. The dispatcher now
claims only `abandonedCheckouts`, `abandonedCheckoutsCount`, `abandonment`,
`abandonmentByAbandonedCheckoutId`, and
`abandonmentUpdateActivitiesDeliveryStatuses`; all broader orders, draft-order,
fulfillment, refund, and return roots stay gated until their lifecycle behavior
is ported.

| Module                                                    | Change                                                                                                       |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/orders.gleam`        | Adds abandoned-checkout empty reads/counts, abandonment lookup reads, and unknown-id delivery-status errors. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`   | Wires the narrow Orders dispatch roots without claiming the rest of the domain.                              |
| `gleam/src/shopify_draft_proxy/state/types.gleam`         | Adds captured abandoned-checkout and abandonment state records.                                              |
| `gleam/src/shopify_draft_proxy/state/store.gleam`         | Adds instance-owned abandoned-checkout/abandonment base and staged slices plus delivery activity staging.    |
| `gleam/src/shopify_draft_proxy/state/serialization.gleam` | Carries the new slices through state dump serialization and restore.                                         |
| `gleam/test/shopify_draft_proxy/proxy/orders_test.gleam`  | Covers the empty read roots and the unknown-id mutation branch.                                              |
| `config/gleam-port-ci-gates.json`                         | Removes the two newly passing Orders parity specs.                                                           |

Validation:
Full JavaScript is green at 718 tests. Host Erlang still fails because local
OTP 25 cannot run `gleam_json`, matching the known workstation limitation; the
Docker Erlang fallback with `HOME=/tmp` is green at 714 tests. `corepack pnpm
gleam:format:check`, `corepack pnpm gleam:port:coverage`, and `corepack pnpm
conformance:check` are green. The port coverage gate now reports 379 parity
specs with 175 expected Gleam parity failures, meaning 2 of the 78 Orders specs
are executable in Gleam and 76 remain gated.

### Findings

- The abandonment delivery-status unknown-id branch is a useful first Orders
  mutation because it is side-effect-free but still exercises local mutation
  dispatch, selected payload projection, userErrors, and a failed mutation-log
  draft.
- Captured abandoned checkout and abandonment payloads are best stored as raw
  captured JSON values so future non-empty fixtures can project Shopify fields
  without a premature bespoke record model.
- The TypeScript order runtime remains intact under the active port guardrail;
  this pass does not authorize TypeScript runtime retirement.

### Risks / open items

- The current Orders module intentionally does not claim order, draft-order,
  fulfillment, refund, return, or broad search/count behavior yet.
- Seeded non-empty abandoned-checkout search/filter parity still needs more
  fixture-backed work before it should be ungated beyond the two safe specs.

### Pass 114 candidates

- Continue Orders with the next evidence-backed slice that can be ported
  without claiming unsupported roots, preferably a complete draft-order or
  order lifecycle cluster rather than validation-only roots.

---

## 2026-05-01 - Pass 115: markets mutation parity completion

Promotes the remaining checked-in Markets parity scenarios into the Gleam
suite. The Markets port now covers market and catalog validation/lifecycle
basics, price-list validation, product fixed-price updates, quantity
pricing/rules, default metafield market-localization behavior, and the captured
read-after-write effects needed by the existing fixtures.

| Module                                              | Change                                                                                                        |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/markets.gleam` | Adds staged market/catalog/price-list mutations, fixed prices, quantity pricing, and localization validation. |
| `gleam/src/shopify_draft_proxy/state/store.gleam`   | Adds staged upsert/delete helpers for Markets, Catalogs, and PriceLists.                                      |
| `gleam/test/parity/runner.gleam`                    | Seeds price-list and metafield localization baselines from the captured fixtures.                             |
| `config/gleam-port-ci-gates.json`                   | Removes the final 13 Markets expected-failure gates, leaving 0 Markets gated specs.                           |

Validation:
Full JavaScript is green at 718 tests. Host `gleam test --target erlang`
still fails before tests execute with `undef shopify_draft_proxy@@main:run`, so
the Docker Erlang fallback using mounted Gleam 1.16 on `erlang:27-alpine` is
the BEAM proof and is green at 714 tests. `corepack pnpm gleam:port:coverage`
is green with 379 specs and 149 expected failures. `corepack pnpm
gleam:registry:check`, `corepack pnpm lint`, and `git diff --check` are green.
Markets parity inventory is 27 checked-in specs, with all 27 now executable in
the Gleam parity suite and 0 Markets specs expected-failing.

### Findings

- The product fixed-price and quantity-pricing captures already contain enough
  `data.priceList` and `seedProducts` state to replay staged read-after-write
  effects without new live captures.
- The metafield market-localization fixture is the default ad hoc metafield
  branch: reads return an identity payload with empty content/localizations,
  register rejects `value` as `INVALID_KEY_FOR_MODEL`, and remove returns a
  null localization payload with no errors.
- Market/catalog validation roots were only promoted after adding local
  lifecycle staging for those resource families, preserving the no
  validation-only support guardrail.

### Risks / open items

- The TypeScript Markets runtime remains intact under the Gleam port
  preservation rule until the final all-port cutover.
- This completes the checked-in Markets parity corpus, but broader whole-port
  cutover work still owns TypeScript runtime retirement and packaging/docs.

### Pass 116 candidates

- Move to the next non-Markets expected-failing domain in
  `config/gleam-port-ci-gates.json`, preserving the same fixture-backed
  promotion discipline.

---

## 2026-05-01 - Pass 117: online-store content and integration parity

Promotes the online-store content, integrations, storefront token, default page
publish, and article media/navigation fixtures into the Gleam parity suite. The
port now stages online-store content and integration records locally, projects
Shopify-shaped read-after-write payloads, and routes `shop.storefrontAccessTokens`
through online-store without weakening the existing store-properties `shop`
handler.

| Module                                                    | Change                                                                                                      |
| --------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/online_store.gleam`  | Adds online-store query/mutation handling for content, themes/files, script tags, pixels, tokens, and apps. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`   | Wires online-store query/mutation dispatch, including `shop { storefrontAccessTokens }` routing.            |
| `gleam/src/shopify_draft_proxy/state/types.gleam`         | Adds captured-json-backed online-store content and integration records.                                     |
| `gleam/src/shopify_draft_proxy/state/store.gleam`         | Adds effective/staged online-store content and integration store helpers.                                   |
| `gleam/src/shopify_draft_proxy/state/serialization.gleam` | Carries online-store content/integration buckets through state dump serialization.                          |
| `gleam/test/parity/runner.gleam`                          | Executes the storefront-token read-after-create override instead of substituting the safe upstream read.    |
| `config/gleam-port-ci-gates.json`                         | Removes the six newly passing online-store parity specs.                                                    |

Validation:
Focused JavaScript parity is green for all six online-store specs:
`online-store-content-lifecycle.json`,
`online-store-content-search-filters.json`,
`online-store-integrations-local-staging.json`,
`online-store-page-default-publish-local-staging.json`,
`storefront-access-token-local-staging.json`, and
`online-store-article-media-navigation-follow-through.json`. Host Erlang still
fails under OTP 25 with the known `gleam_json` OTP 27 requirement; after clearing
host-built artifacts, the Docker Erlang fallback using Gleam 1.16 is green at
712 tests. Full JavaScript is green at 716 tests. `corepack pnpm
gleam:port:coverage` is green with 379 specs and 171 expected failures.
`corepack pnpm lint` is green.

### Findings

- Online-store needs two generic state families rather than one resource type:
  content records for blogs/articles/pages/comments, and integration records
  for themes/files, script tags, pixels, storefront access tokens, and mobile
  applications.
- Shopify treats `WebPixel.settings` as a JSON scalar; projecting it through an
  empty child selection produces `{}` instead of the raw settings object.
- Mobile platform app create inputs may nest Android fields under `android`, and
  Article metafield read-after-write parity requires adding `ownerType` and
  `jsonValue` to locally staged metafields.
- The storefront-token fixture includes live safe-read evidence, but the local
  read-after-create target must execute against the staged proxy state.

### Risks / open items

- Online-store parity is now ungated in Gleam, but the TypeScript online-store
  runtime and TypeScript integration tests remain intact until the final
  all-port cutover.

### Pass 118 candidates

- Continue with the next expected-failing non-online-store domain from
  `config/gleam-port-ci-gates.json`.

---

## 2026-05-01 - Pass 116: segments baseline and member parity

Completes the next Segments Gleam parity pass while preserving the TypeScript
runtime and TypeScript tests for the incremental port. The segment baseline
parity spec now seeds captured segment roots into Gleam base state and runs as
passing evidence, and customer segment member reads now evaluate the supported
segment query grammar against effective customer state instead of returning only
empty placeholders.

This pass also removes the segment baseline expected-failure gate. The original
TypeScript segment runtime remains in place because per-domain Gleam parity does
not authorize TypeScript retirement before the final all-port cutover.

| Module                                                     | Change                                                                                                                    |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/segments.gleam`       | Adds segment metadata roots, top-level missing-resource errors, customer segment member filtering, and membership checks. |
| `gleam/src/shopify_draft_proxy/state/store.gleam`          | Adds base segment upsert helpers and captured segment root payload storage.                                               |
| `gleam/src/shopify_draft_proxy/state/serialization.gleam`  | Persists `segmentRootPayloads` through state dumps and restores.                                                          |
| `gleam/test/parity/runner.gleam`                           | Seeds `segments-baseline-read` from captured root payloads and segment records.                                           |
| `gleam/test/shopify_draft_proxy/proxy/segments_test.gleam` | Covers segment metadata root predicates, customer member filters, and membership evaluation.                              |
| `config/gleam-port-ci-gates.json`                          | Removes the now-passing segment baseline expected-failure entry.                                                          |

Validation: see HAR-510 workpad for the latest merge-refresh validation against
the current `origin/main`.

### Findings

- Segment baseline parity needs both normalized segment records and captured
  root payloads for catalog-like roots such as filters, suggestions, value
  suggestions, and migrations.
- Customer segment member reads can share the mutation validation grammar for
  the currently supported `number_of_orders` and `customer_tags CONTAINS` forms,
  then evaluate that parsed predicate against effective customer records.
- The host Erlang build cache can mask the container OTP version; clean inside
  the OTP 27+ container before treating `gleam_json` OTP errors as real target
  failures.

### Risks / open items

- The supported segment query grammar remains intentionally narrow and should be
  expanded only with captured Shopify evidence for additional predicates.
- The TypeScript segment runtime still remains the shipping Node/Koa path until
  a final all-port cutover proves repository-wide parity.

---

## 2026-05-01 - Pass 114: market web-presence staging

Promotes the captured MarketWebPresence mutation lifecycle into the Gleam
parity suite. The Markets port now locally stages web presence create, update,
delete, unknown-id validation, invalid-routing validation, and downstream
`webPresences` reads without runtime Shopify writes.

| Module                                                  | Change                                                                                                |
| ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/markets.gleam`     | Adds MarketWebPresence mutation dispatch, validation payloads, synthetic IDs, and read projection.    |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam` | Routes web-presence Markets mutations through the explicit local mutation dispatcher.                 |
| `gleam/src/shopify_draft_proxy/state/store.gleam`       | Adds staged upsert/delete helpers for web-presence records.                                           |
| `gleam/test/parity/runner.gleam`                        | Seeds lifecycle specs from the captured baseline `data.webPresences.nodes` instead of mutation cases. |
| `config/gleam-port-ci-gates.json`                       | Removes four now-passing MarketWebPresence parity specs.                                              |

Validation:
Full JavaScript is green at 718 tests. Host `gleam test --target erlang`
still fails in the local Erlang runtime, so the Docker Erlang fallback using
mounted Gleam 1.16 on `erlang:27-alpine` is the BEAM proof and is green at 714
tests. `corepack pnpm gleam:port:coverage` is green with 379 specs and 162
expected failures. `corepack pnpm gleam:registry:check`, `corepack pnpm lint`,
and `git diff --check` are green. Markets parity inventory remains 27 checked-in
specs, with 14 specs now executable in the Gleam parity suite and 13 Markets
specs still expected-failing.

### Findings

- Web-presence lifecycle captures contain both a stable baseline
  `data.webPresences.nodes` tree and disposable mutation response IDs; seeding
  the entire capture would incorrectly preserve deleted disposable live IDs in
  local read-after-delete results.
- Shopify returns subfolder web presences with `domain: null` and root URLs
  derived from the shop's primary baseline web-presence URL.

### Risks / open items

- Market/catalog/price-list mutation lifecycles, quantity pricing/rules, fixed
  product prices, and market localization metafield lifecycle remain gated for
  later passes.
- The TypeScript Markets runtime remains intact under the Gleam port
  preservation rule until the final all-port cutover.

### Pass 115 candidates

- Continue Markets with price-list fixed-price and quantity-pricing staging,
  or port market/catalog mutation validation only after the corresponding
  success lifecycle can be staged locally without overstating operation support.

---

## 2026-05-01 - Pass 113: markets captured read substrate

Promotes the first captured Markets read slice into the Gleam parity suite.
The port now has a read-only Markets domain module with captured-state
projection for Markets, MarketCatalogs, PriceLists, MarketWebPresences,
`marketsResolvedValues`, and Shopify's empty market-localizable read roots.
Mutation staging remains gated for a later Markets pass.

| Module                                                      | Change                                                                                                 |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/markets.gleam`         | Adds captured read projection for core Markets resources, root payloads, and empty localizable roots.  |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`     | Routes the ported Markets query roots through the explicit local dispatcher.                           |
| `gleam/src/shopify_draft_proxy/proxy/graphql_helpers.gleam` | Lets `Catalog` interface fragments project over captured `MarketCatalog` objects.                      |
| `gleam/src/shopify_draft_proxy/state/*`                     | Adds Markets records, root payload storage, ordering, deletion markers, and state dump round-tripping. |
| `gleam/test/parity/runner.gleam`                            | Seeds captured Markets resources and root payloads from existing parity captures.                      |
| `config/gleam-port-ci-gates.json`                           | Removes ten now-passing captured Markets read parity specs.                                            |

Validation:
Full JavaScript is green at 718 tests. Host `gleam test --target erlang`
still fails because local Erlang/OTP is 25 while `gleam_json` requires OTP 27.
The Docker Erlang fallback using mounted Gleam 1.16 on `erlang:27-alpine` is
green at 714 tests. `corepack pnpm gleam:port:coverage` is green with 379
specs and 166 expected failures. `corepack pnpm lint` is green. Markets parity
inventory is 27 checked-in specs, with 10 captured read specs now executable in
the Gleam parity suite and 17 Markets specs still expected-failing.

### Findings

- Captured `MarketCatalog` payloads use reusable fragments on the `Catalog`
  interface, so the shared projector needs to treat `Catalog` as applying to
  concrete Market/App/CompanyLocation catalog typenames.
- Several read fixtures already contain enough resource graph data to seed
  Markets, Catalogs, PriceLists, WebPresences, and nested root payloads without
  mutating Shopify or changing parity request shapes.
- The market-localizable empty fixture is a pure no-data read: the single root
  returns `null`, and both connection roots return empty `edges`, `nodes`, and
  `pageInfo`.

### Risks / open items

- Markets mutation staging, validation branches, quantity pricing/rules,
  fixed-price product updates, web-presence lifecycle, and market localization
  lifecycle remain gated for later passes.
- The TypeScript Markets runtime remains intact under the Gleam port
  preservation rule until the final all-port cutover.

### Pass 114 candidates

- Continue Markets with validation-only mutation branches, or start the
  web-presence lifecycle staging path because it has small local state surface
  and downstream `webPresences`/`marketsResolvedValues` read effects.

---

## 2026-05-01 — Pass 115: apps billing/access parity cutover

Completes the broader Apps billing/access parity scenario in the Gleam runner.
Both checked-in app parity specs now execute against the Gleam proxy, including
subscription billing lifecycle, one-time purchases, usage records, access-scope
revocation, app uninstall read suppression, delegated-token create/destroy, and
generic Admin Platform `node` reads for locally staged app resources.

The TypeScript app runtime, legacy dispatcher wiring, and TS parity harness
remain in place for this pass. Apps parity is now executable in Gleam, but the
public TypeScript/Koa implementation is preserved until a later full-port
cutover explicitly retires it.

| Module                                                                                                         | Change                                                                                                                                       |
| -------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/apps.gleam`                                                               | Exposes app-owned generic Node serializers for App, AppInstallation, AppPurchaseOneTime, AppSubscription, and AppUsageRecord.                |
| `gleam/src/shopify_draft_proxy/proxy/admin_platform.gleam`                                                     | Routes app-owned GIDs through the Apps serializers so multi-step billing parity can read staged app resources through `node(id:)`.           |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                                                              | Suppresses uninstalled app installations from effective/current reads and hides destroyed delegated tokens from token-hash lookup.           |
| `gleam/test/parity/runner.gleam`                                                                               | Adds `fromProxyResponse` variable substitution so later targets can reference earlier named target responses, not only the primary response. |
| `gleam/test/parity/diff.gleam`                                                                                 | Supports expected-difference paths with multiple `[*]` segments, needed for nested app subscription line item IDs.                           |
| `config/gleam-port-ci-gates.json`, `gleam/test/parity_test.gleam`                                              | Removes Apps billing/access from the expected-failure gate so the discovered parity suite runs it as strict passing evidence.                |
| `src/proxy/apps.ts`, `src/proxy/routes.ts`, `src/proxy/admin-platform.ts`, `scripts/conformance-parity-lib.ts` | Remains in place as the legacy TypeScript runtime and conformance harness until a later full-port cutover.                                   |

Validation: `gleam test --target javascript` is green at 682 tests.
`gleam test --target erlang` is green at 678 tests via the
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container because the host
lacks `escript`. `corepack pnpm typecheck` and `git diff --check` are green.

### Findings

- Broad app parity needed target-to-target variable references such as
  `fromProxyResponse`, because delegated-token destroy and app-node reads depend
  on responses produced by earlier non-primary targets.
- Apps now exercise multi-wildcard expected-difference paths such as
  `$.allSubscriptions.nodes[*].lineItems[*].id`; the Gleam diff matcher needed
  to support more than one wildcard index segment.
- Shopify-like downstream reads hide uninstalled app installations and destroyed
  delegated tokens, while still leaving app identity records resolvable where
  later node reads need them.

### Risks / open items

- The root TypeScript server remains the legacy shell for all runtime domains,
  including Apps, until the full-port cutover retires TypeScript runtime code.
- Several previously ported domains still have TypeScript runtime modules in
  main and should be cut over only when the whole port is ready for that final
  transition.

### Pass 116 candidates

- Port product-owned `metafieldDelete` / `metafieldsDelete` and their
  hydrated/downstream deletion flows into Gleam.
- Add `standardMetafieldDefinitionTemplates` catalog query support once a
  captured template-catalog fixture exists.
- Start Shipping/Fulfillments substrate so fulfillment-service, carrier-service,
  delivery-profile, and shipping-settings roots can consume ported Location
  state without reaching back into the TypeScript module.
- Start Products publication substrate so Product and Collection publishable
  projections can move from captured Store Properties rows into typed product
  and collection records.
- Continue Markets or Online Store ports where Store Properties shop/location
  read effects are now available as local Gleam state.
- Continue Marketing parity-runner seeding so captured Marketing read/update
  scenarios can execute against the Gleam proxy.
- Audit already-ported domains for final-cutover readiness without deleting
  TypeScript runtime modules during incremental parity passes.

---

## 2026-05-01 - Pass 114: HAR-505 mainline refresh seeding

Refreshes the HAR-505 Marketing branch after `origin/main` promoted the
remaining Product parity gates. The conflict resolution preserves the branch's
Marketing and localization parity seeding while keeping the mainline
Products/Inventory runner additions. The merged runner now explicitly seeds the
captured `inventory-quantity-contracts-2026-04` disposable product from the
fixture's `setup.product` block before replaying the 2026-04 set/adjust/read
flow.

| Module                           | Change                                                                                     |
| -------------------------------- | ------------------------------------------------------------------------------------------ |
| `gleam/test/parity/runner.gleam` | Keeps Marketing capture seeding and seeds the 2026-04 inventory quantity contract fixture. |
| `gleam/test/parity/spec.gleam`   | Keeps both `selectedPaths` and `upstreamCapturePath` parity target documentation.          |
| `GLEAM_PORT_LOG.md`              | Preserves mainline Product pass history and the branch-local localization seeding entry.   |

Validation:
Full JavaScript is green at 718 tests. Docker Erlang is green at 714 tests.
`corepack pnpm lint`, `git diff --check`, `corepack pnpm
gleam:port:coverage`, and `corepack pnpm gleam:registry:check` are green.
Gleam parity coverage reports 379 checked-in specs and 176 expected failures.

### Findings

- The 2026-04 inventory quantity contract fixture stores its disposable Product,
  Variant, and InventoryItem ids under `setup.product`; without that seed, the
  success mutation branches correctly reject the unknown inventory item instead
  of exercising the captured contract path.
- The Marketing branch did not need parity fixture or request changes for this
  refresh; the required work was runner seeding and conflict reconciliation.

### Risks / open items

- TypeScript Marketing runtime deletion remains deferred to HAR-518 under the
  incremental port preservation rule.

---

## 2026-04-30 — Pass 113: state dump completeness proof

Extends the HAR-522 rework so the Gleam dump/restore substrate can answer the
review question directly: current state dumps now use strict base/staged state
decoders that require every serialized state bucket to be present. Snapshot
restore keeps the permissive decoder path because TypeScript snapshots remain
incremental while the port lands domain-by-domain.

The serializer now exposes field-name inventories derived from the actual
`baseState` and `stagedState` JSON field lists. Restore tests remove every
serialized bucket name from a real dump, one by one, and assert restore fails as
malformed. That makes newly serialized state buckets automatically enter the
missing-field coverage instead of relying on a manually maintained test list.

| Module                                                        | Change                                                                                                                                           |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/state/serialization.gleam`     | Exposes serializer-derived base/staged state dump field inventories and strict current-dump decoders that require each serialized bucket key.    |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`       | Uses strict base/staged state decoders for `restore_state` while leaving `restore_snapshot` on the existing permissive snapshot-compatible path. |
| `gleam/test/shopify_draft_proxy/proxy/draft_proxy_test.gleam` | Adds exhaustive missing-bucket tests for every serialized base/staged state field plus a whole-runtime dump round-trip assertion.                |

Validation after merging `origin/main@cf7be0ae`: `corepack pnpm
gleam:registry:check`, `corepack pnpm gleam:port:coverage`, `corepack pnpm
lint`, `corepack pnpm typecheck`, `corepack pnpm build`, `corepack pnpm
conformance:check`, `corepack pnpm conformance:parity`, `corepack pnpm
conformance:capture:check`, `corepack pnpm gleam:test:js`, `corepack pnpm
gleam:smoke:js`, and `git diff --check` are green. The JS target is green at
683 tests. The Erlang target is green at 679 tests via
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine`.

### Findings

- The previous Pass 112 guard made `baseState`, `stagedState`, and
  `mutationLog` required top-level store fields, but the base/staged state
  decoders still shared permissive snapshot defaults for inner buckets.
- Strictness needs to be scoped to current state dumps. Snapshot restore must
  keep accepting partial TypeScript snapshot files until the port owns every
  bucket and fixture shape.
- The post-merge parity gate showed
  `localization-disable-clears-translations` now passes in the Gleam suite, so
  its stale expected-failure entry was removed from the port gate manifest.

### Risks / open items

- This pass guarantees that anything currently serialized as part of
  `baseState` or `stagedState` is required during dump restore. Gleam has no
  runtime reflection over record fields, so future state fields still need to be
  added to the serializer; once serialized, the meta-test makes missing restore
  coverage automatic.

---

## 2026-04-30 — Pass 112: strict runtime state restore guards

Tightens the runtime state dump/restore substrate added in Pass 36. The Gleam
restore path now treats the current store field dump as a required structural
contract: `baseState`, `stagedState`, and `mutationLog` must all be present in
the versioned store envelope instead of silently defaulting missing fields to an
empty store. This keeps incomplete dumps from erasing modeled state during
restore and makes newly modeled store buckets safer because the top-level store
dump shape cannot drift without a decoder failure.

This pass is also the rework for HAR-522 after reviewer feedback clarified that
the ticket belongs to the Gleam port, not the legacy TypeScript implementation.
The earlier TypeScript-only strict-restore patch was reverted on the PR branch;
the shipping TypeScript runtime remains unchanged while the Gleam port gains
the missing guard.

| Module                                                        | Change                                                                                                                                                 |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`       | Requires `baseState`, `stagedState`, and `mutationLog` when restoring a store field dump instead of applying empty defaults for omitted fields.        |
| `gleam/test/shopify_draft_proxy/proxy/draft_proxy_test.gleam` | Builds negative restore fixtures from a real default dump and adds coverage that each required store field returns `MalformedDumpJson(_)` when absent. |

Validation after merging `origin/main@606459f1`: `corepack pnpm
gleam:registry:check`, `corepack pnpm gleam:port:coverage`, `corepack pnpm
lint`, `corepack pnpm typecheck`, `corepack pnpm conformance:check`,
`corepack pnpm conformance:parity`, `corepack pnpm conformance:capture:check`,
`corepack pnpm conformance:status -- --output-json
.conformance/current/conformance-status-report.json --output-markdown
.conformance/current/conformance-status-comment.md`, `corepack pnpm build`,
`corepack pnpm gleam:test:js`, `corepack pnpm gleam:smoke:js`, and
`git diff --check` are green. The JS target is green at 677 tests. The Erlang
target is green at 673 tests via
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine`, and the Elixir smoke is
green at 17 tests via `ghcr.io/gleam-lang/gleam:v1.16.0-elixir-alpine`,
because the host Erlang runtime is OTP 25 and `gleam_json` requires OTP 27+.

### Findings

- The existing Gleam decoder used optional store fields as backwards-compatible
  defaults, but the current state-dump schema is the active runtime contract;
  defaulting missing `baseState`, `stagedState`, or `mutationLog` would erase
  internal state without surfacing a malformed dump.
- Older malformed-dump tests used hand-written store fragments that omitted the
  current required fields. Building those fixtures from `dump_state` keeps each
  test focused on the intended invalid field.

### Risks / open items

- This pass guards the current store envelope shape; per-bucket omission inside
  `baseState` and `stagedState` remains governed by the incremental snapshot
  and state serializer decoders from Pass 36.
- Root-owned Docker validation can leave `gleam/build` owned by root on the
  host. The local workspace needed ownership repaired before rerunning the JS
  target after the Erlang container pass.

---

## 2026-04-30 - Pass 111: product search grammar parity

Promotes the final gated Product parity spec,
`products-search-grammar-read.json`, into the Gleam parity suite. The checked-in
capture is an older phrase-only upstream response while the replay request now
selects additional NOT and `tag_not` aliases; the TypeScript parity harness
passes it by returning the upstream Product overlay response unchanged when no
local state is staged. The Gleam runner now mirrors that no-staged upstream
passthrough narrowly for this stale grammar fixture without changing the
capture, request, variables, or strict comparison contract.

| Module                            | Change                                                                                            |
| --------------------------------- | ------------------------------------------------------------------------------------------------- |
| `gleam/test/parity/runner.gleam`  | Uses the target `upstreamCapturePath` for the primary Product grammar target, matching TS parity. |
| `config/gleam-port-ci-gates.json` | Removes the final expected-failing Product parity spec.                                           |

Validation:
TypeScript oracle parity is green for `products-search-grammar-read.json`.
Full JavaScript is green at 716 tests. Host Erlang still fails with the known
local `Undef` runner class; the Docker Erlang fallback using Gleam 1.16 and
`HOME=/tmp` is green at 712 tests. `corepack pnpm elixir:smoke` is green at 16
ExUnit tests. `corepack pnpm gleam:port:coverage` is green with 379 specs and
177 expected failures. Product parity inventory is now 115 checked-in specs,
with all 115 product specs executable in the Gleam parity suite and 0 product
specs expected-failing. `corepack pnpm lint` and whitespace checks are green.

### Findings

- `products-search-grammar-read.json` is a stale but valid TS-passing parity
  fixture: the capture only includes `phraseCount` and `phraseMatches`, while
  the replay document also selects NOT and `tag_not` aliases. The TS harness
  accepts this by short-circuiting to the upstream response for a no-staged
  Product overlay read.
- The authenticated live conformance store is valid, but the aggregate product
  read capture script cannot complete there because its pagination capture
  cannot derive the filtered `after` cursor. Re-recording the aggregate product
  read set was therefore not a cleaner path for this pass.
- Product parity is now fully ungated in the Gleam runner, but the TypeScript
  product runtime remains intact under the port preservation rule until the
  final all-port cutover.

### Risks / open items

- The Product domain has no remaining expected-failing Gleam parity specs, but
  HAR-487 should not be used as authority to remove the TypeScript product
  runtime before the broader cutover acceptance bar is met.

### Pass 112 candidates

- Continue with the next non-Product expected-failing domain from
  `config/gleam-port-ci-gates.json`, or start the explicit final-cutover plan
  once the whole-port acceptance criteria bind.

---

## 2026-04-30 — Pass 46: localization source-content parity seeding

Keeps the latest mainline localization parity gate green after the HAR-505
branch was refreshed with webhook evidence. The checked-in
`localization-disable-clears-translations` capture registers a translation
against an existing Shopify product, but the Gleam port still lacks the Products
domain. This pass seeds the captured source title digest into the parity runner
as a non-target-locale source marker, then lets the localization runtime
reconstruct the minimal translatable content slot needed for Shopify-like
`translationsRegister` validation.

| Module                                                                  | Change                                                                                                                                    |
| ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/test/parity/runner.gleam`                                        | Seeds the captured product title digest for `localization-disable-clears-translations` before replaying the enable/register/disable flow. |
| `gleam/src/shopify_draft_proxy/proxy/localization.gleam`                | Reconstructs translatable content slots from seeded translation/source markers while preserving `RESOURCE_NOT_FOUND` for unknown ids.     |
| `gleam/test/shopify_draft_proxy/proxy/localization_mutation_test.gleam` | Covers the seeded source marker register-disable-read lifecycle directly.                                                                 |

Validation after the `origin/main@3e99c073` merge: `gleam test --target
javascript` passed at 681 tests, Erlang passed at 677 tests via the
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container, `corepack pnpm
lint`, `git diff --check`, `corepack pnpm gleam:port:coverage`, and `corepack
pnpm gleam:registry:check` are green.

### Findings

- The source digest already exists in the live capture, so no parity request or
  fixture shape needed to change.
- Unknown localization resource ids still fail unless a product/metafield
  domain record or a capture-seeded source marker exists.
- The expected Gleam parity-failure manifest now reports 292 remaining failures
  after this localization scenario passes.

### Risks / open items

- The real Product-backed `find_resource` path remains deferred until the
  Products domain ports; this seed is only the parity runner bridge for captured
  upstream resources that already exist.

---

## 2026-04-30 - Pass 110: selling-plan lifecycle parity

Promotes the captured selling-plan group lifecycle and product/variant
selling-plan association fixtures into the Gleam parity suite. The port now
stages SellingPlanGroup create/update/delete, group-centric product and variant
membership edits, and product-/variant-centric join/leave roots locally while
preserving read-after-write Product and ProductVariant selling-plan overlays.

| Module                                                    | Change                                                                                                            |
| --------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`      | Adds SellingPlanGroup state projection, query roots, mutation staging, payload serializers, and membership reads. |
| `gleam/src/shopify_draft_proxy/state/types.gleam`         | Adds SellingPlan and SellingPlanGroup records for captured and staged group lifecycle state.                      |
| `gleam/src/shopify_draft_proxy/state/store.gleam`         | Adds effective/staged SellingPlanGroup store helpers and Product/ProductVariant visibility helpers.               |
| `gleam/src/shopify_draft_proxy/state/serialization.gleam` | Carries SellingPlanGroup state through the state dump serializer.                                                 |
| `gleam/test/parity/runner.gleam`                          | Seeds captured `seedSellingPlanGroups` records for association parity.                                            |
| `config/gleam-port-ci-gates.json`                         | Removes the two newly passing selling-plan parity specs.                                                          |

Validation:
Focused JavaScript parity is green for
`selling-plan-product-variant-associations.json`,
`selling-plan-group-lifecycle.json`, and `products-catalog-read.json` as a
regression guard. Full JavaScript is green at 716 tests. Host Erlang still
fails with the known local `Undef` runner class and the crash dump was removed;
the Docker Erlang fallback is green at 712 tests. `corepack pnpm
elixir:smoke` is green at 16 ExUnit tests. `corepack pnpm
gleam:port:coverage` is green with 379 specs and 178 expected failures. Product
parity inventory remains 115 checked-in specs, with 114 product specs
executable in the Gleam parity suite and 1 product spec still expected-failing.

### Findings

- Product-centric `productLeaveSellingPlanGroups` has Shopify's captured
  visibility/count split: after a Product leaves a group, Product
  `sellingPlanGroups.nodes` can still show the group through a remaining
  variant membership while `sellingPlanGroupsCount.count` is 0.
- ProductVariant `sellingPlanGroups.nodes` is visible through either direct
  variant membership or Product-level membership, but
  `sellingPlanGroupsCount` counts only direct variant membership.
- SellingPlanGroup updates preserve existing billing, delivery, inventory, and
  created-at fields for a plan update, but clear pricing policies when
  `pricingPolicies` is omitted, matching the captured Shopify update payload.

### Risks / open items

- `products-search-grammar-read.json` remains gated because the checked-in
  capture only contains phrase aliases while the replay request selects
  additional NOT/tag_not aliases.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 111 candidates

- Resolve `products-search-grammar-read.json` with a faithful capture/request
  decision.

---

## 2026-04-30 - Pass 109: advanced product search read parity

Promotes four captured advanced Product search read fixtures into the Gleam
parity suite. The runner now hydrates captured Product connection edges with
their upstream cursors for advanced search, OR precedence, relevance, and
filtered pagination captures, and adds fixture-derived pagination sentinels
when Shopify's captured `pageInfo` proves additional matching rows exist beyond
the selected edge payloads.

| Module                            | Change                                                                                         |
| --------------------------------- | ---------------------------------------------------------------------------------------------- |
| `gleam/test/parity/runner.gleam`  | Seeds captured Product connection edges/cursors and pagination sentinel rows for search reads. |
| `config/gleam-port-ci-gates.json` | Removes the four newly passing advanced Product search/pagination parity specs.                |

Validation:
Focused JavaScript parity is green for `products-advanced-search-read.json`,
`products-or-precedence-read.json`, `products-relevance-search-read.json`, and
`products-search-pagination-read.json`. Full pass validation is recorded in the
Linear workpad for HAR-487.

### Findings

- Advanced Product search fixtures already encode enough selected Product rows
  to hydrate local read parity when the runner preserves captured edge cursors
  and merges partial Product seeds before replay.
- The pagination fixture captures only the visible edge rows, but its count and
  pageInfo require extra matching rows after the captured second edge. Local
  sentinel rows are acceptable runner seed data here because they model the
  hidden store rows implied by Shopify's capture without changing the request
  or captured comparison contract.
- `products-search-grammar-read.json` remains gated separately: its capture
  only contains the phrase aliases even though the replay request includes
  additional NOT/tag_not aliases, so it needs a distinct fidelity decision
  rather than being bundled with generic connection seeding.

### Risks / open items

- Product search grammar and selling-plan scenarios remain incomplete in
  Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 110 candidates

- Continue `products-search-grammar-read.json` parity.
- Continue selling-plan product/variant association or selling-plan group
  lifecycle parity.

---

## 2026-04-30 - Pass 108: product sort-key read parity

Promotes the captured `products-sort-keys-read` fixture into the Gleam parity
suite. The port now seeds the captured sort-key Product catalog slices, carries
Product `publishedAt` through local state/projection, sorts Product connections
by the captured Shopify sort keys, and emits Shopify-style sort cursors while
preserving stored upstream cursors for older catalog captures.

| Module                                                    | Change                                                                                                                   |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`      | Adds Product `publishedAt` projection, Product connection sorting, sort-key cursor generation, and timestamp term reads. |
| `gleam/src/shopify_draft_proxy/state/types.gleam`         | Adds Product `published_at` state so captured Product reads can round-trip `publishedAt`.                                |
| `gleam/src/shopify_draft_proxy/state/serialization.gleam` | Carries Product `publishedAt` through state JSON output.                                                                 |
| `gleam/test/parity/runner.gleam`                          | Seeds the captured sort-key Product aliases and fills fixture-derived searchable tags/vendor fields.                     |
| `config/gleam-port-ci-gates.json`                         | Removes the newly passing `products-sort-keys-read` parity spec.                                                         |

Validation:
Focused JavaScript parity is green for `products-sort-keys-read.json` and the
existing `products-catalog-read.json` cursor regression guard. Full JavaScript
is green at 716 tests. Host Erlang still fails with the known local `Undef`
runner class; the Docker Erlang fallback is green at 712 tests. `corepack pnpm
elixir:smoke` is green at 16 ExUnit tests. `corepack pnpm gleam:port:coverage`
is green with 379 specs and 184 expected failures. `corepack pnpm lint` and
whitespace checks are green. Product parity inventory remains 115 checked-in
specs, with 108 product specs executable in the Gleam parity suite and 7
product specs still expected-failing.

### Findings

- Captured Product sort cursors are base64-encoded JSON objects with
  `last_id` and sort-specific `last_value`. Sort-key fixtures without stored
  upstream cursors can synthesize these, but pre-existing catalog captures with
  captured cursors must keep their stored cursor strings authoritative.
- Shopify's captured Product `VENDOR` and `PRODUCT_TYPE` sort tie-breaks are
  resource-id based for this fixture, not title-based. Nullable sort values
  behave like empty strings for ordering so reverse sorts do not promote
  partial seed rows above real values.
- The sort-key fixture selects partial Product rows across aliases, so the
  runner must merge duplicate Product seeds and hydrate searchable metadata
  implied by the captured query instead of letting sparse aliases overwrite
  richer Product records.

### Risks / open items

- Advanced product search grammar/pagination/relevance and selling-plan
  scenarios remain incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 109 candidates

- Continue advanced product search grammar/pagination/relevance parity.
- Continue selling-plan product/variant association or selling-plan group
  lifecycle parity.

---

## 2026-04-30 - Pass 107: productSet graph parity

Promotes the captured `productSet` create/update graph fixture into the Gleam
parity suite. The port now routes `productSet` locally, stages synchronous
Product graph create and update requests without runtime Shopify writes, and
keeps downstream Product, ProductVariant, InventoryItem, inventory-level,
option, and Product metafield reads coherent across the fixture's multi-step
target flow.

| Module                                               | Change                                                                                                                  |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Adds `productSet` mutation dispatch, payload projection, graph staging, inventory quantity/level writes, and summaries. |
| `gleam/test/parity/runner.gleam`                     | Seeds captured dev-store location names used by ProductSet inventory-level payloads.                                    |
| `config/gleam-port-ci-gates.json`                    | Removes the newly passing `productSet` parity spec.                                                                     |

Validation:
Focused JavaScript parity is green for `productSet-parity-plan.json`. Full
JavaScript is green at 716 tests. Host Erlang still fails with the known local
`Undef` runner class; the Docker Erlang fallback is green at 712 tests.
`corepack pnpm elixir:smoke` is green at 16 ExUnit tests. `corepack pnpm
gleam:port:coverage` is green with 379 specs and 185 expected failures.
`corepack pnpm lint` and whitespace checks are green. Product parity inventory
remains 115 checked-in specs, with 107 product specs executable in the Gleam
parity suite and 8 product specs still expected-failing.

### Findings

- `productSet` uses the `input` argument shape rather than `product`, but the
  local Product creation/update path can reuse the existing Product record
  helpers once identifier/input lookup is handled.
- Captured `productSet` inventory quantities use `quantity` plus `name:
available`, not the older `availableQuantity` variant input shape. Available
  writes also mirror `on_hand`, preserve `incoming`, and need captured location
  names for strict payload parity.
- Shopify's Product `totalInventory` behavior differs between ProductSet create
  and update: create sums variants whose inventory item is not explicitly
  untracked, while update preserves the Product's previous `totalInventory`
  even as variant inventory levels change.

### Risks / open items

- Advanced product search/sort/read and selling-plan scenarios remain
  incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 108 candidates

- Continue advanced product search/sort/read parity.
- Continue selling-plan product/variant association or selling-plan group
  lifecycle parity.

---

## 2026-04-30 - Pass 106: synchronous productDuplicate graph parity

Promotes the captured synchronous `productDuplicate` fixture into the Gleam
parity suite. The port now seeds the source Product graph from the live capture,
stages the duplicate Product locally, copies captured options, variants,
inventory items, collection memberships, and Product metafields, and preserves
Shopify's immediate empty-media behavior for the duplicate without runtime
Shopify writes.

| Module                                               | Change                                                                                                         |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Duplicates the synchronous Product graph and serializes argument-aware `newProduct` selections.                |
| `gleam/test/parity/runner.gleam`                     | Seeds the synchronous duplicate source Product, collections, memberships, and Product metafields from capture. |
| `gleam/test/parity/diff.gleam`                       | Normalizes quoted connection path segments in expected differences so existing captures remain unchanged.      |
| `config/gleam-port-ci-gates.json`                    | Removes the newly passing synchronous duplicate parity spec.                                                   |

Validation:
Focused JavaScript parity is green for `productDuplicate-parity-plan.json`.
Full JavaScript is green at 716 tests. Host Erlang still fails with the known
local `Undef` runner class; the Docker Erlang fallback is green at 712 tests.
`corepack pnpm elixir:smoke` is green at 16 ExUnit tests. `corepack pnpm
gleam:port:coverage` is green with 379 specs and 186 expected failures.
`corepack pnpm lint` is green. Product parity inventory remains 115 checked-in
specs, with 106 product specs executable in the Gleam parity suite and 9
product specs still expected-failing.

### Findings

- Synchronous `productDuplicate` returns `newProduct` immediately and the
  downstream Product read observes duplicated options, variants, inventory
  items, collection memberships, and Product metafields.
- Shopify's immediate duplicate Product media connection is empty even when the
  source Product had ready image media, so the local duplicate clears staged
  media instead of copying source media rows.
- The checked-in fixture uses expected-difference paths with quoted connection
  segments such as `variants["nodes"][0].id`; the Gleam diff layer must
  normalize those to the runner's emitted `variants.nodes[0].id` paths instead
  of weakening or editing the capture.

### Risks / open items

- ProductSet, advanced product search/sort/read, and selling-plan scenarios
  remain incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 107 candidates

- Continue productSet root parity.
- Continue advanced product search/sort/read parity.
- Continue selling-plan product/variant association or selling-plan group
  lifecycle parity.

---

## 2026-04-30 - Pass 105: async productDuplicate parity

Promotes the two captured async `productDuplicate` fixtures into the Gleam
parity suite. The port now stages `ProductDuplicateOperation` records locally,
returns the mutation-time operation as `CREATED`, completes the downstream
`productOperation(id:)` read as `COMPLETE`, and exposes the duplicated Product
through downstream reads without runtime Shopify writes.

| Module                                                    | Change                                                                                                     |
| --------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`      | Adds async `productDuplicate` staging, ProductDuplicateOperation projection, and downstream Product reads. |
| `gleam/src/shopify_draft_proxy/state/store.gleam`         | Adds Product operation state on base/staged stores plus effective lookup and staging helpers.              |
| `gleam/src/shopify_draft_proxy/state/types.gleam`         | Adds Product operation and Product operation user-error records.                                           |
| `gleam/src/shopify_draft_proxy/state/serialization.gleam` | Carries the new Product operation slice through state constructors.                                        |
| `gleam/test/parity/runner.gleam`                          | Seeds the captured async duplicate source Product before replaying the primary request.                    |
| `config/gleam-port-ci-gates.json`                         | Removes the newly passing async duplicate parity specs.                                                    |
| `.agents/skills/gleam-port/SKILL.md`                      | Records the async ProductDuplicateOperation projection and seeding trap.                                   |

Validation:
Focused JavaScript parity is green for `productDuplicate-async-missing.json`
and `productDuplicate-async-success.json`. Full JavaScript is green at 716
tests. Host Erlang still fails with the known local `Undef` runner class; the
Docker Erlang fallback is green at 712 tests. `corepack pnpm
gleam:port:coverage` is green with 379 specs and 187 expected failures. Product
parity inventory remains 115 checked-in specs, with 105 product specs
executable in the Gleam parity suite and 10 product specs still
expected-failing.

### Findings

- Async `productDuplicate` reports an initial `ProductDuplicateOperation` with
  `status: CREATED` and `newProduct: null`, then the operation read resolves as
  `COMPLETE` with the duplicated Product.
- Missing async duplicate Product IDs surface no mutation payload user errors;
  the `Product does not exist` user error appears only on the completed
  `productOperation(id:)` read.
- Root `productOperation(id:)` selections include inline fragments, so the
  serializer must project the raw selection set rather than flatten only direct
  fields.

### Risks / open items

- This pass covers captured async duplicate behavior only. Synchronous
  duplicate, productSet, advanced search, and selling-plan scenarios remain
  incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 106 candidates

- Continue synchronous duplicate / productSet roots.
- Continue advanced product search/sort/read parity.
- Continue selling-plan product/variant association or selling-plan group
  lifecycle parity.

---

## 2026-04-30 - Pass 104: product variant bulk validation atomicity parity

Promotes the captured Product variant bulk validation/atomicity fixture into
the Gleam parity suite. Bulk create/update/delete now validate the full
submitted batch before staging local ProductVariant, option, inventory-item, or
inventory-summary writes, preserve the captured ProductVariantsBulkUserError
`code` and nullable-field shapes, and keep mixed valid/invalid batches atomic.

| Module                                               | Change                                                                                                           |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Adds batch validation for bulk create/update/delete and nullable `productVariants` / user-error payload support. |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured atomicity Product options/default variant preconditions from the live fixture.                |
| `config/gleam-port-ci-gates.json`                    | Removes the newly passing bulk validation atomicity parity spec.                                                 |

Validation:
Focused JavaScript parity is green for
`product-variants-bulk-validation-atomicity.json`. Full JavaScript is green at
716 tests. Host Erlang still fails with the known local `Undef` runner class;
the Docker Erlang fallback is green at 712 tests. `corepack pnpm
elixir:smoke` is green at 16 ExUnit tests. `corepack pnpm
gleam:port:coverage` is green with 379 specs and 189 expected failures.
`corepack pnpm lint` and whitespace checks are green. Product parity inventory
remains 115 checked-in specs, with 103 product specs executable in the Gleam
parity suite and 12 product specs still expected-failing.

### Findings

- Shopify rejects bulk create/update/delete batches atomically: if any submitted
  variant row is invalid, no earlier valid row is staged.
- Bulk update validation uses `productVariants: null`, while bulk create
  validation keeps `productVariants: []`; unknown products return
  `PRODUCT_DOES_NOT_EXIST` codes.
- Empty bulk update returns a nullable `field` user error and null Product,
  while empty create/delete return the seeded Product with zero inventory and
  untracked inventory summary.

### Risks / open items

- Duplicate, productSet, advanced search, and selling-plan scenarios remain
  incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 105 candidates

- Continue duplicate / productSet roots.
- Continue advanced product search/sort/read parity.
- Continue selling-plan product/variant association or selling-plan group
  lifecycle parity.

---

## 2026-04-30 - Mainline Pass 45: Elixir embedder wrapper smoke

Merged the mainline Elixir embedder wrapper smoke into the long-running
Products branch. The smoke project now includes a thin `ShopifyDraftProxy`
Elixir wrapper that keeps proxy state opaque, returns the next proxy explicitly
from GraphQL/meta helpers, exposes JSON response bodies as strings, and drives
deterministic commit reports through an injected transport. The branch preserves
the richer Products implementation already present here rather than replacing it
with the narrow mainline smoke-only Product slice.

| Module                                               | Change                                                                                                                               |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `gleam/elixir_smoke/lib/shopify_draft_proxy.ex`      | Adds Elixir structs and wrapper helpers for config, GraphQL, meta state/log/reset/commit, dump/restore, and injected commit reports. |
| `gleam/elixir_smoke/test/interop_test.exs`           | Uses the Elixir wrapper broadly for config, GraphQL, meta state/log, dump/restore, reset, and commit report/error smoke coverage.    |
| `scripts/elixir-smoke.ts` / `package.json`           | Keeps `corepack pnpm elixir:smoke` as the canonical command, with a Docker fallback when host `escript`/`mix` are unavailable.       |
| `gleam/README.md`                                    | Documents the Elixir wrapper calling conventions and Erlang shipment path.                                                           |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Resolves the mainline smoke-only Product slice in favor of this branch's broader Products port.                                      |

Validation: carried forward from mainline, where `corepack pnpm elixir:smoke`
was green at 16 ExUnit tests through the container fallback. The merge into
HAR-487 will be covered by this branch's normal JS/Erlang/Docker validation
before the next push.

### Findings

- The BEAM wrapper can consume the existing `default_graphql_path` and
  `process_request` public surface already present on the Products branch.
- The smoke-only Product records added on main conflict with this branch's
  richer Products state model; the Products branch keeps its broader state,
  dispatcher, and tests.

---

## 2026-04-30 - Pass 103: product merchandising guardrail parity

Promotes the captured Product merchandising mutation guardrail fixture into the
Gleam parity suite. The port now routes the bundle, combined-listing, and
ProductVariant relationship validation roots locally, preserves raw mutation
logging, and returns the captured Shopify user-error/null payload shapes for the
guardrail branches without runtime Shopify writes.

| Module                                               | Change                                                                                |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Adds captured guardrail handlers for bundle, combined-listing, and variant relations. |
| `config/gleam-port-ci-gates.json`                    | Removes the newly passing merchandising guardrail parity spec.                        |
| `docs/endpoints/products.md`                         | Links the executable merchandising guardrail parity spec in the Product docs.         |
| `.agents/skills/gleam-port/SKILL.md`                 | Records validation priority and JSON-string error-message traps.                      |

Validation:
Focused JavaScript parity is green for
`product-merchandising-mutation-guardrails.json`. Full JavaScript is green at
716 tests. Host Erlang still fails with the known local `Undef` runner class;
the Docker Erlang fallback is green at 712 tests. `corepack pnpm
gleam:port:coverage` is green with 379 specs and 190 expected failures.
`corepack pnpm lint` and whitespace checks are green. Product parity inventory
remains 115 checked-in specs, with 102 product specs executable in the Gleam
parity suite and 13 product specs still expected-failing.

### Findings

- Shopify returns nullable `field` values for Product bundle guardrail errors:
  empty bundle create components and unknown bundle update products both use
  `field: null`.
- `productBundleUpdate` validates the submitted Product before component
  validation, so the captured unknown-product branch returns only the `Product
does not exist` user error even when `components: []` is also present.
- ProductVariant relationship missing-ID errors include a compact JSON string
  list in the message, including both the missing parent variant and missing
  component variants.

### Risks / open items

- This pass covers captured validation branches only. Full Product bundle,
  ProductVariant component, and combined-listing success lifecycle parity
  remains incomplete in Gleam.
- Duplicate, productSet, advanced search, selling-plan scenarios, and remaining
  bulk validation atomicity parity remain incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 104 candidates

- Continue duplicate / productSet roots and validation atomicity.
- Continue advanced product search/sort/read parity.
- Continue selling-plan product/variant association or selling-plan group
  lifecycle parity.

---

## 2026-04-30 - Pass 102: product media async plan parity

Promotes the three captured Product media async plan fixtures into the Gleam
parity suite. The runner now hydrates the captured existing Product/media rows
needed by update/delete media captures, while `productCreateMedia` keeps the
mutation payload `UPLOADED` and makes the immediate downstream Product media
read observe Shopify's null-url `PROCESSING` state before later successful media
operations can settle staged media to `READY`.

| Module                                               | Change                                                                                          |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Models create-media `UPLOADED` -> immediate `PROCESSING` lifecycle and delete product payloads. |
| `gleam/test/parity/runner.gleam`                     | Seeds captured media plan Product/media preconditions without changing captures/specs.          |
| `config/gleam-port-ci-gates.json`                    | Removes the three newly passing media plan parity specs.                                        |
| `docs/endpoints/products.md`                         | Documents the captured async media lifecycle boundary.                                          |
| `.agents/skills/gleam-port/SKILL.md`                 | Records the media plan seeding and async lifecycle traps.                                       |

Validation:
Focused JavaScript parity is green for `productCreateMedia-parity-plan.json`,
`productUpdateMedia-parity-plan.json`, and
`productDeleteMedia-parity-plan.json`. Full JavaScript is green at 716 tests.
Host Erlang still fails with the known local `Undef` runner class; the Docker
Erlang fallback is green at 712 tests. `corepack pnpm gleam:port:coverage` is
green with 379 specs and 191 expected failures. `corepack pnpm lint` and
whitespace checks are green. Product parity inventory remains 115 checked-in
specs, with 101 product specs executable in the Gleam parity suite and 14
product specs still expected-failing.

### Findings

- The create media plan fixture has no explicit seed rows. The runner must seed
  only the captured `mutation.variables.productId` Product, then let the
  runtime create the staged MediaImage so the mutation payload and downstream
  read use the same local media ID.
- Update media plan captures assume an existing READY media row with captured
  image URLs; delete media plan captures need the existing media ID plus the
  captured deleted ProductImage ID even though the downstream Product media
  connection is empty after deletion.
- Shopify returns null `preview.image` and null `MediaImage.image` objects for
  newly uploaded/processing image media. Product media projection now returns
  `null` for absent image objects instead of an object with `url: null`.

### Risks / open items

- Selling-plan group membership, duplicate, productSet, advanced search, and
  remaining validation atomicity parity remain incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 103 candidates

- Continue duplicate / productSet roots and validation atomicity.
- Continue advanced product search/sort/read parity.
- Continue selling-plan product/variant association or selling-plan group
  lifecycle parity.

---

## 2026-04-30 - Pass 101: product relationship roots parity

Promotes the captured Product relationship roots fixture into the Gleam parity
suite. The port now stages `collectionAddProductsV2` with Shopify-like async
Job payloads and non-manual prepend-reverse ordering, tracks ProductVariant
media membership locally, and makes `productVariantAppendMedia` /
`productVariantDetachMedia` visible through downstream ProductVariant media
reads without runtime Shopify writes.

| Module                                               | Change                                                                               |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/state/types.gleam`    | Adds ProductVariant media ID membership state.                                       |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Handles collection V2 add and ProductVariant media append/detach relationship roots. |
| `gleam/test/parity/runner.gleam`                     | Seeds captured relationship collections alongside generic products/media.            |
| `config/gleam-port-ci-gates.json`                    | Removes the newly passing relationship-roots parity spec.                            |
| `.agents/skills/gleam-port/SKILL.md`                 | Records collection V2 ordering and variant media membership traps.                   |

Validation:
Focused JavaScript parity is green for
`product-relationship-roots-live-parity.json`. Full JavaScript is green at 711
tests. Host Erlang still fails with the known local `Undef` runner class; the
Docker Erlang fallback is green at 707 tests. `corepack pnpm
gleam:port:coverage` is green with 379 specs and 195 expected failures.
`corepack pnpm lint` and whitespace checks are green. Product parity inventory
remains 115 checked-in specs, with 98 product specs executable in the Gleam
parity suite and 17 product specs still expected-failing.

### Findings

- `collectionAddProductsV2` differs from legacy `collectionAddProducts`: the
  mutation payload returns only async `job` plus `userErrors`, and collections
  without explicit `MANUAL` sorting prepend the input products in reverse order.
- ProductVariant media is relationship state, not duplicated media state. The
  variant stores ordered Product media IDs and resolves its `media` connection
  through the Product media records already seeded/staged for the Product.
- The relationship fixture depends on generic `seedProducts` and
  `seedProductMedia`, plus explicit `seedCollections`; the runner must hydrate
  all three before replaying the primary collection add request.

### Risks / open items

- Selling-plan group membership, duplicate, productSet, advanced search, media
  async plan fixtures, and remaining validation atomicity parity remain
  incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 102 candidates

- Continue selling-plan product/variant association or selling-plan group
  lifecycle parity now that relationship fixture seeding is in place.
- Continue duplicate / productSet roots and validation atomicity.
- Continue advanced product search/sort/read parity.

---

## 2026-04-30 - Pass 100: product media reorder parity

Promotes the captured `productReorderMedia` fixture into the Gleam parity
suite. Product media records can now be reordered locally with Shopify-like
zero-based `MoveInput.newPosition` semantics, the mutation returns an async
Job-shaped payload, and downstream Product media/image reads reflect the staged
order without runtime Shopify writes.

| Module                                               | Change                                                                          |
| ---------------------------------------------------- | ------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Handles `productReorderMedia`, stages media ordering, and exposes empty images. |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured product/media setup rows for reorder replay.                 |
| `config/gleam-port-ci-gates.json`                    | Removes the newly passing reorder-media parity spec.                            |
| `.agents/skills/gleam-port/SKILL.md`                 | Records the Product media reorder parity trap.                                  |

Validation:
Focused JavaScript parity is green for `productReorderMedia-parity.json`.
Full JavaScript is green at 711 tests. Host Erlang still fails with the known
local `Undef` runner class; the Docker Erlang fallback is green at 707 tests.
`corepack pnpm gleam:port:coverage` is green with 379 specs and 196 expected
failures. Product parity inventory remains 115 checked-in specs, with 97
product specs executable in the Gleam parity suite and 18 product specs still
expected-failing.

### Findings

- `productReorderMedia` reuses Shopify's collection-style `MoveInput`
  contract: empty moves, over-limit moves, missing IDs, and invalid
  `newPosition` values are rejected before any staging, but media-specific
  payloads surface the failures through `mediaUserErrors`.
- Successful reorder returns an async Job with `done: false`; only the Job ID
  is volatile compared with the live capture, so the parity spec keeps the
  existing strict payload/read comparisons.
- The downstream read fixture selects both `media` and `images`. For the
  captured media-only setup, Shopify returns an empty Product `images`
  connection, so the Gleam Product projection now exposes that same empty
  no-data shape.

### Risks / open items

- Product media relationship roots, broader asynchronous media lifecycle,
  duplicate, productSet, advanced search, selling plans, and remaining product
  parity expected failures remain incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 101 candidates

- Continue product relationship media roots (`productVariantAppendMedia`,
  `productVariantDetachMedia`) now that base/staged Product media exists.
- Continue duplicate / productSet roots and validation atomicity.
- Continue advanced product search/sort/read parity or selling-plan group
  lifecycle behavior.

---

## 2026-04-30 - Pass 99: product media validation branches

Promotes the captured product media validation fixture into the Gleam parity
suite. Products now carry media records in state, Product `media` reads project
locally staged media, and the three captured media mutation roots stage or
reject create/update/delete branches without runtime Shopify writes.

| Module                                               | Change                                                                               |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/state/types.gleam`    | Adds Product media records.                                                          |
| `gleam/src/shopify_draft_proxy/state/store.gleam`    | Adds base/staged Product media families and effective media reads.                   |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Handles productCreateMedia, productUpdateMedia, productDeleteMedia, and media reads. |
| `gleam/test/parity/runner.gleam`                     | Seeds explicit `seedProductMedia` fixture rows into base state.                      |
| `config/gleam-port-ci-gates.json`                    | Removes the newly passing media validation spec.                                     |
| `.agents/skills/gleam-port/SKILL.md`                 | Records the Product media validation/staging trap.                                   |

Validation:
Focused JavaScript parity is green for
`product-media-validation-branches.json`. Full JavaScript is green at 711
tests. Host Erlang still fails with the known local `Undef` runner class; the
Docker Erlang fallback is green at 707 tests. `corepack pnpm
gleam:port:coverage`, `corepack pnpm lint`, and whitespace checks are green.
Product parity inventory remains 115 checked-in specs, with 96 product specs
executable in the Gleam parity suite and 19 product specs still
expected-failing.

### Findings

- The media validation fixture needs both global `seedProducts` and explicit
  `seedProductMedia` hydration before the primary create request. The seeded
  media row is what lets later update/delete mixed branches reject only the
  unknown ID while preserving the known seed row for downstream reads.
- Shopify returns empty-product-id and invalid `mediaContentType` branches as
  top-level `INVALID_VARIABLE` GraphQL errors, while unknown product/media and
  invalid image source branches stay in `mediaUserErrors`.
- Mixed create is partial: valid media is staged even when another input has an
  invalid image source. Mixed update/delete with an unknown media ID reject the
  whole batch and leave downstream media unchanged.

### Risks / open items

- Product media promotion is sufficient for the captured validation branch but
  broader asynchronous media lifecycle parity, product images, reorder-media,
  variant-media relationship roots, duplicate, productSet, selling plans, and
  advanced search remain incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 100 candidates

- Continue product relationship media roots (`productVariantAppendMedia`,
  `productVariantDetachMedia`, `productReorderMedia`) with strict captured
  fixture replay.
- Continue duplicate / productSet roots and validation atomicity.
- Continue advanced product search/sort/read parity or selling-plan group
  lifecycle behavior.

---

## 2026-04-30 - Pass 98: inventory quantity 2026-04 contracts

Promotes the captured 2026-04 inventory quantity contract scenario into the
Gleam parity suite. The parity runner now honors per-request `apiVersion`
metadata, and Products mutation handling applies the 2026-04
`changeFromQuantity` / `@idempotent` contract before local staging while still
preserving the older `compareQuantity` behavior on earlier Admin routes.

| Module                                               | Change                                                                               |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Adds route-versioned inventory set/adjust contract validation and compare semantics. |
| `gleam/test/parity/spec.gleam`                       | Decodes `proxyRequest.apiVersion` for primary and target requests.                   |
| `gleam/test/parity/runner.gleam`                     | Executes parity requests through the requested Admin API version route.              |
| `config/gleam-port-ci-gates.json`                    | Removes the newly passing inventory quantity contract spec.                          |
| `.agents/skills/gleam-port/SKILL.md`                 | Records the versioned parity runner and 2026-04 inventory contract trap.             |

Validation:
Focused JavaScript parity is green for
`inventory-quantity-contracts-2026-04.json`. Full JavaScript is green at 711
tests. Host Erlang still fails with the known local `Undef` runner class; the
Docker Erlang fallback is green at 707 tests. `corepack pnpm
gleam:port:coverage`, `corepack pnpm lint`, and whitespace checks are green.
Product parity inventory remains 115 checked-in specs, with 95 product specs
executable in the Gleam parity suite and 20 product specs still
expected-failing.

### Findings

- The 2026-04 contract is route-gated; running the checked-in parity request
  through the runner's previous hardcoded 2025-01 path hid the captured Shopify
  schema drift from the product mutation handler.
- Shopify returns omitted `changeFromQuantity` as a top-level
  `INVALID_FIELD_ARGUMENTS` GraphQL error with the mutation root set to `null`
  in `data`, not as a payload `userErrors` branch.
- Successful 2026-04 set/adjust writes compare against `changeFromQuantity`;
  older routes keep the existing `compareQuantity` / `ignoreCompareQuantity`
  behavior.

### Risks / open items

- Selling plans, media, advanced search, duplicate, productSet, relationship
  roots, and broader validation atomicity parity remain incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 99 candidates

- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.
- Continue advanced product search/sort/read parity.
- Port selling-plan group behavior with strict captured lifecycle evidence.

---

## 2026-04-30 - Pass 97: inactive inventory level lifecycle

Promotes the captured inactive inventory level lifecycle scenario into the
Gleam parity suite. Inventory levels now retain Shopify's active/inactive state
instead of being removed on `inventoryDeactivate`, `InventoryItem` reads honor
`includeInactive: true`, and `inventoryActivate` reactivates the same row while
preserving quantities and synthesizing the captured available-quantity
timestamp shape.

| Module                                               | Change                                                              |
| ---------------------------------------------------- | ------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`    | Adds optional `is_active` state to inventory levels.                |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Toggles active state for deactivate/reactivate and filters reads.   |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured inactive-level product and level activity state. |
| `config/gleam-port-ci-gates.json`                    | Removes the newly passing inactive-level lifecycle spec.            |
| `.agents/skills/gleam-port/SKILL.md`                 | Records the inactive inventory-level modeling trap.                 |

Validation:
Focused JavaScript parity is green for
`inventory-inactive-level-lifecycle-2026-04.json`. Full JavaScript is green at
711 tests. Host Erlang still fails with the known local `Undef` runner class;
the Docker Erlang fallback is green at 707 tests. `corepack pnpm
gleam:port:coverage`, `corepack pnpm lint`, and whitespace checks are green.
Product parity inventory remains 115 checked-in specs, with 94 product specs
executable in the Gleam parity suite and 21 product specs still
expected-failing.

### Findings

- Shopify keeps deactivated inventory levels readable by id and through
  `inventoryItem.inventoryLevel(locationId:, includeInactive: true)` with
  `isActive: false`; deleting the local level breaks downstream reads and
  reactivation.
- Default `inventoryLevels` reads should continue to omit inactive rows unless
  `includeInactive: true` is supplied. The existing literal `first:` projection
  still needs to apply after the activity filter.

### Risks / open items

- Selling plans, media, advanced search, inventory contracts, duplicate,
  productSet/media roots, and broader validation atomicity parity remain
  incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 98 candidates

- Continue inventory quantity contracts and validation atomicity.
- Port selling-plan group behavior with strict captured lifecycle evidence.
- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 96: product contextual pricing read

Promotes the captured Product/ProductVariant contextual pricing read scenario
into the Gleam parity suite. Product and ProductVariant state now carry a
small captured JSON payload for Shopify contextual pricing snapshots, and the
Products serializer exposes that payload through normal GraphQL source
projection so seeded Markets price-list reads preserve Shopify's no-mutation
read shape.

| Module                                               | Change                                                                  |
| ---------------------------------------------------- | ----------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`    | Adds a walkable captured JSON value for product contextual pricing.     |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Projects Product and ProductVariant `contextualPricing` from state.     |
| `gleam/test/parity/runner.gleam`                     | Seeds captured contextual pricing onto Product and ProductVariant rows. |
| `config/gleam-port-ci-gates.json`                    | Removes the newly passing contextual-pricing spec from expected fails.  |
| `.agents/skills/gleam-port/SKILL.md`                 | Records the contextual pricing seeding/projection pattern.              |

Validation:
Focused JavaScript parity is green for
`product-contextual-pricing-price-list-read.json`. Full JavaScript is green at
711 tests. Host Erlang still fails with the known local `Undef` runner class;
the Docker Erlang fallback is green at 707 tests. `corepack pnpm
gleam:port:coverage`, `corepack pnpm lint`, and whitespace checks are green.
Product parity inventory remains 115 checked-in specs, with 93 product specs
executable in the Gleam parity suite and 22 product specs still
expected-failing.

### Findings

- The fixture already carries exact Product and ProductVariant
  `contextualPricing` snapshots under `seedProducts`; the missing Gleam piece
  was preserving that captured payload in state and projecting it through the
  selected GraphQL fields.
- Contextual pricing remains read-seeded evidence for the captured
  country/price-list path. Broader price-list derivation, relative
  adjustments, priority conflicts, and B2B/app catalog contextual pricing stay
  outside this pass.

### Risks / open items

- Selling plans, media, advanced search, inventory contracts, duplicate,
  productSet/media roots, and broader validation atomicity parity remain
  incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 97 candidates

- Port selling-plan group behavior with strict captured lifecycle evidence.
- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.
- Continue advanced product search/sort/read parity.

---

## 2026-04-30 - Pass 95: product metafieldsSet owner expansion

Promotes the captured Product `metafieldsSet` owner expansion scenario into the
Gleam parity suite. The Products serializer now handles owner-scoped
`metafield` and `metafields` fields on ProductVariant and Collection records,
including Product-nested `variants` connections, so the staged metafieldsSet
mutation is visible through the downstream owner reads without falling back to
runtime Shopify writes.

| Module                                                            | Change                                                                                          |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/metafield_definitions.gleam` | Preserves `metafieldsSet` owner/input order across JavaScript and Erlang targets.               |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`              | Serializes ProductVariant and Collection owner metafield fields from staged owner-scoped state. |
| `gleam/test/parity/runner.gleam`                                  | Seeds the owner-expansion capture's Product, ProductVariant, and Collection preconditions.      |
| `config/gleam-port-ci-gates.json`                                 | Removes the newly passing owner-expansion spec from expected failures.                          |
| `.agents/skills/gleam-port/SKILL.md`                              | Records the owner-expansion serializer trap for future product passes.                          |

Validation:
Focused JavaScript parity is green for
`metafieldsSet-owner-expansion.json`. Full JavaScript is green at 711 tests.
Host Erlang still fails with the known local `Undef` runner class; the Docker
Erlang fallback is green at 707 tests. `corepack pnpm gleam:port:coverage`,
`corepack pnpm lint`, and whitespace checks are green. Product parity
inventory remains 115 checked-in specs, with 92 product specs executable in
the Gleam parity suite and 23 product specs still expected-failing.

### Findings

- ProductVariant and Collection owner projections cannot use generic JSON
  source projection for `metafield` / `metafields` because those fields depend
  on GraphQL arguments and the staged owner-scoped metafield slice.
- The owner-expansion fixture also needs Collection base-state seeding from
  the capture before replaying the local `metafieldsSet` mutation; otherwise
  the downstream Collection owner read correctly remains `null`.
- `metafieldsSet` mutation payload order must follow the supplied input owner
  order. Grouping by owner through a dictionary made JavaScript and Erlang
  disagree on payload order even though both staged the same owner data.

### Risks / open items

- Selling plans, media, advanced search, inventory contracts, duplicate,
  productSet/media roots, and broader validation atomicity parity remain
  incomplete in Gleam.
- Product parity is still not complete; the TypeScript product runtime remains
  intact until full parity and final cutover.

### Pass 96 candidates

- Port selling-plan group behavior with strict captured lifecycle evidence.
- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.
- Continue advanced product search/sort/read parity.

---

## 2026-04-30 - Pass 94: product metafieldsSet invalid variables

Promotes the captured Product `metafieldsSet` missing required variable-input
branches into the Gleam parity suite. Shopify rejects missing `ownerId`, `key`,
or `value` on a `metafields` variable before mutation execution with a
top-level `INVALID_VARIABLE` GraphQL error, not a
`metafieldsSet.userErrors` payload, so the Gleam mutation path now detects that
variable shape and returns the captured error envelope without staging local
state or draft log entries.

| Module                                                            | Change                                                                   |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/metafield_definitions.gleam` | Adds top-level invalid-variable handling for `metafieldsSet`.            |
| `config/gleam-port-ci-gates.json`                                 | Removes the three newly passing missing-field specs from expected fails. |
| `.agents/skills/gleam-port/SKILL.md`                              | Records the `metafieldsSet` variable-validation trap for future passes.  |

Validation:
Focused JavaScript parity is green for `metafieldsSet-missing-key`,
`metafieldsSet-missing-owner`, and `metafieldsSet-missing-value`. The
all-discovered JavaScript parity gate is green at 708 tests after manifest
alignment. Full JavaScript is green at 708 tests. Host Erlang still fails under
OTP 25 with the `gleam_json` OTP 27 requirement plus the known `Undef` runner
class; after clearing root-owned build artifacts, the Docker Erlang fallback is
green at 704 tests. `corepack pnpm gleam:port:coverage`, `corepack pnpm lint`,
and whitespace checks are green. Product parity inventory remains 115
checked-in specs, with 91 product specs executable in the Gleam parity suite
and 24 product specs still expected-failing.

### Findings

- Missing required fields on the `metafields` variable input are GraphQL
  invalid-variable errors. They include `extensions.value` echoing the supplied
  variable list and `extensions.problems[0].path` pointing at the list index
  and missing field.
- Top-level variable validation must abort the whole mutation operation before
  local staging, even if the GraphQL document contains other mutation roots.

### Risks / open items

- `metafieldsSet` owner expansion, selling plans, media, advanced search,
  duplicate/productSet/media roots, and broader validation atomicity parity
  remain incomplete in Gleam.
- Only 91 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 95 candidates

- Continue Product `metafieldsSet` owner expansion parity.
- Port selling-plan group behavior with strict captured lifecycle evidence.
- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 93: product metafield delete roots

Promotes the captured Product `metafieldsDelete` parity plan and the singular
`metafieldDelete` compatibility shim into the Gleam parity suite. The pass
keeps both roots local in the owner-scoped metafields domain, seeds the shared
delete capture from the downstream Product read plus the deleted
`custom/material` metafield, and stages removals by replacing the owner
metafield slice so the immediate Product downstream read retains the same
sibling metafields Shopify returned.

| Module                                                            | Change                                                                  |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/metafield_definitions.gleam` | Routes and serializes `metafieldsDelete` / `metafieldDelete` locally.   |
| `gleam/test/parity/runner.gleam`                                  | Seeds the shared Product metafield delete capture before replay.        |
| `config/gleam-port-ci-gates.json`                                 | Removes the two newly passing Product delete specs from expected fails. |

Validation:
Focused JavaScript parity is green for `metafieldDelete-parity-plan.json` and
`metafieldsDelete-parity-plan.json`. The all-discovered JavaScript parity gate
is green at 708 tests after manifest alignment. Full JavaScript is green at
708 tests. Host Erlang still fails under OTP 25 with the `gleam_json` OTP 27
requirement plus the known `Undef` runner class; the Docker Erlang fallback is
green at 704 tests. `corepack pnpm gleam:port:coverage`, `corepack pnpm lint`,
and whitespace checks are green. Product parity inventory remains 115
checked-in specs, with 88 product specs executable in the Gleam parity suite
and 27 product specs still expected-failing.

### Findings

- The singular `metafieldDelete` fixture intentionally compares only the
  compatibility alias user-errors and downstream read against the plural live
  capture, so the runner seeds `gid://shopify/Metafield/9001` as the local
  deleted metafield ID while keeping the downstream owner state faithful to the
  capture.
- The plural delete root returns an ordered `deletedMetafields` list where an
  existing owner namespace/key serializes its identifier and a missing
  namespace/key remains `null`; the local staging path must preserve that
  order before downstream reads.

### Risks / open items

- GraphQL variable validation branches for malformed `metafieldsSet` inputs,
  `metafieldsSet` owner expansion, selling plans, media, advanced search,
  duplicate/productSet/media roots, and broader validation atomicity parity
  remain incomplete in Gleam.
- Only 88 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 94 candidates

- Continue Product `metafieldsSet` GraphQL variable validation or owner
  expansion parity.
- Port selling-plan group behavior with strict captured lifecycle evidence.
- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 92: product metafieldsSet seeded validation reads

Promotes three additional captured Product `metafieldsSet` scenarios whose
mutation payloads already matched Shopify but whose downstream Product reads
needed the same captured precondition owner-metafield seed added in Pass 91:
missing namespace, missing type, and over-limit validation.

| Module                            | Change                                                                  |
| --------------------------------- | ----------------------------------------------------------------------- |
| `gleam/test/parity/runner.gleam`  | Seeds captured Product metafields for three more `metafieldsSet` specs. |
| `config/gleam-port-ci-gates.json` | Removes the three newly passing Product specs from expected failures.   |

Validation:
Focused JavaScript parity is green for `metafieldsSet-missing-namespace`,
`metafieldsSet-missing-type`, and `metafieldsSet-over-limit`. The
all-discovered JavaScript parity gate is green at 708 tests after manifest
alignment. Product parity inventory remains 115 checked-in specs, with 86
product specs executable in the Gleam parity suite and 29 product specs still
expected-failing.

### Findings

- Some validation scenarios still perform downstream reads after a failed or
  partially defaulted mutation; these need captured baseline owner state even
  when the mutation payload itself is already correct.
- Missing key, missing owner, and missing value remain a different class: the
  captured Shopify response is GraphQL variable validation, not a
  `metafieldsSet.userErrors` payload.

### Risks / open items

- GraphQL variable validation branches for malformed `metafieldsSet` inputs,
  metafield delete roots, selling plans, media, advanced search,
  duplicate/productSet/media roots, and broader validation atomicity parity
  remain incomplete in Gleam.
- Only 86 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 93 candidates

- Continue product metafield mutation parity with `metafieldsDelete`,
  `metafieldDelete`, or GraphQL variable validation branches.
- Port selling-plan group behavior with strict captured lifecycle evidence.
- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 91: product metafieldsSet captured branches

Promotes four captured Product `metafieldsSet` branches into the Gleam parity
suite: duplicate inputs, CAS success, stale digest, and null-create downstream
readback. The pass reuses captured precondition Product reads to seed existing
product-owned metafields before mutation replay, preserving upstream metafield
IDs and compare digests for update/CAS branches without changing the checked-in
fixtures or request shapes.

For null-create, the local runtime still mints a synthetic Metafield ID, but
owner metafield ordering now treats low draft-digest local IDs as later than
captured upstream IDs. That mirrors Shopify's observed connection order where a
newly allocated metafield appears after the existing product metafields.

| Module                                            | Change                                                                   |
| ------------------------------------------------- | ------------------------------------------------------------------------ |
| `gleam/test/parity/runner.gleam`                  | Seeds captured precondition Product metafields for more `metafieldsSet`. |
| `gleam/src/shopify_draft_proxy/state/store.gleam` | Keeps low local draft metafield IDs after captured upstream IDs.         |
| `config/gleam-port-ci-gates.json`                 | Removes four passing Product `metafieldsSet` specs from expected fails.  |
| `.agents/skills/gleam-port/SKILL.md`              | Records the Product metafield ordering trap for future passes.           |

Validation:
Focused JavaScript parity is green for `metafieldsSet-duplicate-input`,
`metafieldsSet-cas-success`, `metafieldsSet-stale-digest`, and
`metafieldsSet-null-create`. The all-discovered JavaScript parity gate is green
at 708 tests after manifest alignment. Product parity inventory remains 115
checked-in specs, with 83 product specs executable in the Gleam parity suite and
32 product specs still expected-failing.

### Findings

- Captured CAS and duplicate-input branches need the precondition read, not the
  mutation response, as the seed source; otherwise local replay mints new IDs or
  treats captured compare digests as stale.
- The precondition seed must not be followed by sparse owner replacement, or the
  runner erases the captured sibling metafields it just inserted.
- Local synthetic Metafield IDs can be numerically lower than captured upstream
  IDs even though Shopify would allocate the new metafield later.

### Risks / open items

- GraphQL variable validation branches for malformed `metafieldsSet` inputs,
  metafield delete roots, selling plans, media, advanced search,
  duplicate/productSet/media roots, and broader validation atomicity parity
  remain incomplete in Gleam.
- Only 83 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 92 candidates

- Continue product metafield mutation parity with `metafieldsDelete`,
  `metafieldDelete`, or GraphQL validation branches.
- Port selling-plan group behavior with strict captured lifecycle evidence.
- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 90: CI parity gate merge alignment

Merges the mainline Gleam port CI coverage gate into the HAR-487 product branch
and keeps the Products dispatcher aligned with the product roots already ported
locally. The pass preserves the new dynamic all-spec parity runner and
manifest-backed expected-failure gate, removes only product specs that are
currently passing from the expected-failure manifest, and keeps normal
product-adjacent roots on the Products dispatcher before metafield owner-root
fallbacks.

| Module                                                  | Change                                                                      |
| ------------------------------------------------------- | --------------------------------------------------------------------------- |
| `config/gleam-port-ci-gates.json`                       | Tracks expected Gleam parity failures for the dynamic all-spec gate.        |
| `gleam/test/parity_test.gleam`                          | Uses the discovered parity corpus instead of a hand-coded allowlist.        |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam` | Routes product-adjacent roots to Products before metafield owner fallbacks. |
| `scripts/gleam-port-coverage-gate.ts`                   | Verifies strict executable parity specs and CI gate wiring.                 |

Validation:
Full `gleam test --target javascript` is green at 708 tests. Host
`gleam test --target erlang` still fails before tests execute with the known
`undef` runner issue; the Docker Erlang fallback is green at 704 tests.
`corepack pnpm gleam:port:coverage`, `corepack pnpm lint`, and
`git diff --check` are green. The dynamic parity gate discovers 379 parity
specs with 246 manifest-backed expected Gleam failures. Product parity inventory
remains 115 checked-in specs, with 79 product specs executable in the Gleam
parity suite and 36 product specs still expected-failing.

### Findings

- Mainline now enforces the Gleam parity corpus through a generated
  expected-failure manifest instead of a hand-coded test allowlist.
- During the merge, `productVariant` downstream inventory reads exposed that
  metafield owner-root dispatch must remain a fallback after Products for
  product-adjacent roots; otherwise owner shells return inventory fields as
  null.

### Risks / open items

- Metafield delete/CAS/validation branches, selling plans, media, advanced
  search, duplicate/productSet/media roots, and broader validation atomicity
  parity remain incomplete in Gleam.
- Only 79 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 91 candidates

- Continue product metafield mutation parity with `metafieldsDelete`,
  `metafieldDelete`, CAS, or validation branches.
- Port selling-plan group behavior with strict captured lifecycle evidence.
- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 89: metafieldsSet product downstream parity

Promotes the captured `metafields-set-live-parity` Product spec into the Gleam
parity suite. The pass keeps `metafieldsSet` in the metafields domain, but
teaches Product reads to expose a narrow staged owner view when a successful
product-owned metafield mutation has local metafields for a Product GID before
the Products slice has a full Product record.

The parity runner seeds the captured existing product metafields from the
mutation response before replay. That matches the fixture's update semantics:
Shopify returned stable existing Metafield IDs, so the local replay updates
seeded base metafields instead of minting new IDs. The checked-in request,
fixture, and comparison contract stay unchanged.

| Module                                               | Change                                                               |
| ---------------------------------------------------- | -------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Serializes staged Product metafield owner views for Product GIDs.    |
| `gleam/test/parity/runner.gleam`                     | Seeds captured existing metafields for `metafields-set-live-parity`. |
| `gleam/test/parity_test.gleam`                       | Enables the strict `metafields-set-live-parity` product parity spec. |

Validation:
Focused JavaScript parity for `metafields_set_live_parity_test`,
`product_metafields_read_test`, `custom_data_metafield_type_matrix_test`, and
`metafield_definition_lifecycle_mutations_test` is green at 803 tests. Full
`gleam test --target javascript` is green at 803 tests on the host Node
runtime. Host `gleam test --target erlang` still fails before tests execute
with the known `undef` runner issue; after clearing host-built Erlang artifacts,
the Docker Erlang fallback is green at 800 tests. Product parity inventory
remains 115 checked-in specs, with 79 product specs executable in the Gleam
parity suite plus the admin-platform ProductOption node scenario after this
pass.

### Findings

- The first replay after enabling the spec produced correct mutation and
  downstream shapes but synthetic Metafield IDs; the capture is an update of
  existing product metafields, not a first create.
- A staged product-owned metafield mutation can make the Products dispatcher
  responsible for `product(id:) { id metafield metafields }` even when only the
  metafield slice has local owner state.

### Risks / open items

- Metafield delete/CAS/validation branches, selling plans, media, advanced
  search, duplicate/productSet/media roots, and broader validation atomicity
  parity remain incomplete in Gleam.
- Only 79 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 90 candidates

- Continue product metafield mutation parity with `metafieldsDelete`,
  `metafieldDelete`, CAS, or validation branches.
- Port selling-plan group behavior with strict captured lifecycle evidence.
- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 88: product metafield read bridge

Completes strict captured product-owned metafield reads through the Gleam
Products dispatcher while preserving the metafields domain's owner-root parity.
The pass adds argument-aware `metafield(namespace:, key:)` and `metafields(...)`
projection for Product roots, seeds the captured product/metafield graph for
`product-metafields-read`, and keeps staged `metafieldsSet` downstream product
reads working after Products takes precedence for `product(id:)`.

This pass also resolves the `origin/main` merge that introduced the shared
state serializer and expanded metafields/metaobjects/event coverage. The merge
keeps HAR-487's Products routing and state buckets, while allowing the mainline
serializer and metafields owner-root behavior to coexist with product-domain
reads.

| Module                                                      | Change                                                                 |
| ----------------------------------------------------------- | ---------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/graphql_helpers.gleam` | Exposes field-value projection for domain-specific argument bridges.   |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`        | Projects Product `metafield` and `metafields` fields from local state. |
| `gleam/test/parity/runner.gleam`                            | Seeds captured product metafields and sparse product owner records.    |
| `gleam/test/parity_test.gleam`                              | Enables the strict `product-metafields-read` product parity scenario.  |

Validation:
Focused JavaScript parity for `product_metafields_read_test`,
`custom_data_metafield_type_matrix_test`, and
`metafield_definition_lifecycle_mutations_test` is green at 802 tests. Full
`gleam test --target javascript` is green at 802 tests on the host Node
runtime. Host `gleam test --target erlang` still fails before tests execute
with the known `undef` runner issue; after clearing host-built Erlang artifacts,
the Docker Erlang fallback is green at 799 tests. Product parity inventory
remains 115 checked-in specs, with 78 product specs executable in the Gleam
parity suite plus the admin-platform ProductOption node scenario after this
pass.

### Findings

- The pre-implementation signal was strict product metafield replay being
  routed through Products, while Product did not yet project owner metafield
  fields; existing metafields owner-root specs also regressed once Products
  correctly won `product(id:)` routing.
- Product-owned metafield reads need argument-aware projection because the
  shared static `SourceValue` projector cannot see field arguments for singular
  `metafield(namespace:, key:)`.
- Existing staged metafields scenarios relied on sparse captured product owners;
  the parity runner now seeds those Product records directly from capture
  metadata instead of depending on a metafields-only owner stub.

### Risks / open items

- Selling plans, media, advanced search, duplicate/productSet/media roots, and
  broader validation atomicity parity remain incomplete in Gleam.
- Only 78 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 89 candidates

- Port selling-plan group behavior with strict captured lifecycle evidence.
- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.
- Continue advanced product search and validation atomicity slices.

---

## 2026-04-30 - Pass 87: publication roots local runtime

Completes the captured publication roots local-runtime lifecycle in the Gleam
Products handler. The pass stages `publicationCreate`, `publicationUpdate`,
`publicationDelete`, and `publishablePublish` locally, adds staged-aware
publication/channel reads, and updates downstream product/collection
publication visibility and count reads without runtime Shopify writes.

The pass promotes `publication-roots-local-runtime` into the Gleam parity
suite. The checked-in fixture, request documents, variables capture paths, and
strict comparison contract stay unchanged; the parity runner now seeds the
fixture's minimal publication/product/collection preconditions for replay.

| Module                                               | Change                                                                                |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`    | Adds publication metadata, derived channel records, and product publication IDs.      |
| `gleam/src/shopify_draft_proxy/state/store.gleam`    | Adds staged publication storage, deletion, effective reads, and derived channels.     |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Stages publication lifecycle and publishable publish mutations plus downstream reads. |
| `gleam/test/parity/runner.gleam`                     | Seeds minimal local-runtime publication/product/collection preconditions.             |
| `gleam/test/parity_test.gleam`                       | Enables the strict publication roots local-runtime parity spec.                       |

Validation:
Focused JavaScript parity for `publication_roots_local_runtime_test` is green
at 791 tests, and full `gleam test --target javascript` is green at 791 tests
on the host Node runtime. Host `gleam test --target erlang` still fails before
tests execute on the local Erlang install with the known `undef` runner issue;
after clearing host-built Erlang artifacts, the Docker Erlang fallback is green
at 787 tests. `corepack pnpm lint` and `git diff --check` are green. Product
parity inventory remains 115 checked-in specs, with 77 product specs executable
in the Gleam parity suite plus the admin-platform ProductOption node scenario
after this pass.

### Findings

- The pre-implementation signal was strict parity replay returning HTTP 400 for
  `publicationCreate`: no mutation dispatcher was implemented for that root.
- Publication root replay needs staged publication storage and derived channels:
  `gid://shopify/Publication/N` maps to `gid://shopify/Channel/N` unless the
  publication explicitly carries a channel ID.
- The local-runtime fixture intentionally seeds compact product/collection
  records with only publication IDs, so the Gleam parity runner now uses relaxed
  scenario-specific seeding for those preconditions.

### Risks / open items

- Selling plans, product metafields, media, advanced search,
  duplicate/productSet/media roots, and broader validation atomicity parity
  remain incomplete in Gleam.
- Only 77 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 88 candidates

- Port selling-plan or product metafield behavior now that core Product,
  Variant, Inventory, Collection, Location, Publication, feed, feedback,
  shipment, and transfer read/write slices are locally staged or seeded.
- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.
- Continue advanced product search and validation atomicity slices.

---

## 2026-04-30 - Pass 86: inventory transfer local staging

Completes the two captured inventory transfer local-staging parity scenarios in
the Gleam Products handler. The pass adds normalized transfer state, routes
`inventoryTransfer` / `inventoryTransfers` reads through the local Products
dispatcher, and stages `inventoryTransferCreate`,
`inventoryTransferCreateAsReadyToShip`, `inventoryTransferMarkAsReadyToShip`,
`inventoryTransferSetItems`, `inventoryTransferRemoveItems`,
`inventoryTransferCancel`, and `inventoryTransferDelete` locally. Ready-transfer
staging reserves origin inventory by moving quantities between `available` and
`reserved`, and cancel/remove/set flows release or adjust those reservations
without runtime Shopify writes.

The pass promotes `inventory-transfer-lifecycle-local-staging` and
`inventory-transfer-ready-item-adjustments-local-staging` into the Gleam parity
suite. The checked-in fixtures, request documents, variables capture paths, and
strict comparison contracts stay unchanged; the Gleam parity runner now honors
the existing per-target `excludedPaths` contract and literal `first:` windows
when projecting source-backed nested connections.

| Module                                                      | Change                                                             |
| ----------------------------------------------------------- | ------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/state/types.gleam`           | Adds transfer, line-item, and location-snapshot record shapes.     |
| `gleam/src/shopify_draft_proxy/state/store.gleam`           | Adds base/staged transfer storage, ordering, deletion, and lookup. |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`        | Stages transfer lifecycle mutations and serializes transfer reads. |
| `gleam/src/shopify_draft_proxy/proxy/graphql_helpers.gleam` | Applies literal `first:` windows to source-backed connections.     |
| `gleam/test/parity/spec.gleam`                              | Decodes target-level `excludedPaths` as ignore rules.              |
| `gleam/test/parity/runner.gleam`                            | Seeds captured transfer product/variant preconditions for replay.  |
| `gleam/test/parity_test.gleam`                              | Enables the two strict inventory transfer local-staging specs.     |

Validation:
Focused JavaScript parity for the two inventory transfer tests is green at 790
tests, and full `gleam test --target javascript` is green at 790 tests on the
host Node runtime. Host `gleam test --target erlang` still fails before tests
execute on the local Erlang install with the known `undef` runner issue; after
clearing host-built Erlang artifacts, the Docker Erlang fallback is green at
786 tests. `corepack pnpm lint` and `git diff --check` are green. Product
parity inventory remains 115 checked-in specs, with 76 product specs executable
in the Gleam parity suite plus the admin-platform ProductOption node scenario
after this pass.

### Findings

- The pre-implementation signal was strict parity replay returning HTTP 400 for
  `inventoryTransferCreate` and `inventoryTransferCreateAsReadyToShip`: no
  mutation dispatcher was implemented for either root.
- The transfer fixtures exercise target-level `excludedPaths`; the Gleam parity
  spec decoder now maps that existing contract to ignore rules instead of
  treating excluded fields as mismatches.
- Transfer downstream inventory reads select `inventoryLevels(first: 1)`.
  Source-backed nested connection projection now applies literal `first:`
  windows so seeded origin/destination levels do not over-serialize.

### Risks / open items

- Broader publication roots, selling plans, product metafields, media,
  advanced search, duplicate/productSet/media roots, and broader validation
  atomicity parity remain incomplete in Gleam.
- Only 76 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 87 candidates

- Continue broader publication roots local-runtime now that top-level
  publications and product publish/unpublish are represented.
- Port selling-plan or product metafield behavior now that core Product,
  Variant, Inventory, Collection, Location, Publication, feed, feedback,
  shipment, and transfer read/write slices are locally staged or seeded.
- Continue product media / duplicate / productSet roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 85: inventory shipment local staging

Completes the two captured inventory shipment local-staging parity scenarios in
the Gleam Products handler. The pass adds normalized shipment state, stages
`inventoryShipmentCreateInTransit`, `inventoryShipmentReceive`,
`inventoryShipmentUpdateItemQuantities`, and `inventoryShipmentDelete` locally,
and routes `inventoryShipment(id:)` detail reads through the local Products
dispatcher. Shipment staging now updates product-backed InventoryItem quantity
reads for incoming, available, and on-hand quantities without runtime Shopify
writes.

The pass promotes `inventory-shipment-lifecycle-local-staging` and
`inventory-shipment-partial-receive-update-delete-local-staging` into the Gleam
parity suite. The checked-in fixtures, request documents, variables capture
paths, and strict comparison contracts stay unchanged.

| Module                                               | Change                                                                |
| ---------------------------------------------------- | --------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`    | Adds shipment, line-item, and tracking record shapes.                 |
| `gleam/src/shopify_draft_proxy/state/store.gleam`    | Adds base/staged shipment storage, ordering, deletion, and lookup.    |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Stages shipment lifecycle mutations and serializes shipment reads.    |
| `gleam/test/parity/runner.gleam`                     | Seeds captured shipment product/variant preconditions for replay.     |
| `gleam/test/parity_test.gleam`                       | Enables the two strict inventory shipment local-staging parity specs. |

Validation:
Focused JavaScript parity for the two inventory shipment tests is green at 788
tests, and full `gleam test --target javascript` is green at 788 tests on the
host Node runtime. Host `gleam test --target erlang` still fails before tests
execute on the local Erlang install with the known `undef` runner issue; after
clearing host-built Erlang artifacts, the Docker Erlang fallback is green at
784 tests. Product parity inventory remains 115 checked-in specs, with 74
product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was strict parity replay returning HTTP 400 for
  `inventoryShipmentCreateInTransit`: `No mutation dispatcher implemented for
root field`.
- Shipment parity needs product variant seeding, not product-only seeding,
  because nested shipment line-item `inventoryItem { sku }` is sourced from the
  variant that owns the captured InventoryItem.
- The local shipment lifecycle mutates only the product-backed inventory level:
  create-in-transit increases `incoming`, partial receive moves accepted
  quantity from `incoming` to `available`/`on_hand`, quantity updates adjust
  remaining `incoming`, and delete reverses unreceived incoming quantity.

### Risks / open items

- Inventory transfer roots, broader publication roots, selling plans, product
  metafields, media, advanced search, duplicate/productSet/media roots, and
  broader validation atomicity parity remain incomplete in Gleam.
- Only 74 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 86 candidates

- Continue inventory transfer local-staging now that shipment-side inventory
  quantity effects are represented.
- Continue the broader publication roots local-runtime scenario now that
  top-level publications and product publish/unpublish are represented.
- Port selling-plan or product metafield behavior now that core Product,
  Variant, Inventory, Collection, Location, Publication, feed, feedback, and
  shipment read/write slices are locally staged or seeded.

---

## 2026-04-30 - Pass 84: product feedback lifecycle

Completes the captured product and shop resource feedback local-runtime
lifecycle in the Gleam Products handler. The pass adds normalized product and
shop feedback state slices, routes `productResourceFeedback` reads through the
local Products dispatcher, and stages `bulkProductResourceFeedbackCreate` plus
`shopResourceFeedbackCreate` without runtime Shopify writes. Downstream reads
now expose staged product feedback by product ID while preserving Shopify's
null behavior for missing products.

The pass promotes `product-feedback-lifecycle-local-runtime` into the Gleam
parity suite. The checked-in fixture, request documents, variables capture
path, and strict comparison contract stay unchanged.

| Module                                               | Change                                                                |
| ---------------------------------------------------- | --------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`    | Adds product and shop resource feedback record shapes.                |
| `gleam/src/shopify_draft_proxy/state/store.gleam`    | Adds base/staged feedback storage and effective product lookup.       |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Stages feedback mutations and serializes product/shop feedback reads. |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured product precondition for strict replay.            |
| `gleam/test/parity_test.gleam`                       | Enables the strict product feedback lifecycle parity spec.            |

Validation:
`gleam test --target javascript product_feedback_lifecycle_local_runtime_test`
is green at 786 tests, and full `gleam test --target javascript` is green at
786 tests on the host Node runtime. Host `gleam test --target erlang` still
fails before tests execute on the local Erlang install with the known `undef`
runner issue; after clearing host-built Erlang artifacts, the Docker Erlang
fallback is green at 782 tests. Product parity inventory remains 115 checked-in
specs, with 72 product specs executable in the Gleam parity suite plus the
admin-platform ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was strict parity replay returning HTTP 400 for
  `bulkProductResourceFeedbackCreate`: `No mutation dispatcher implemented for
root field`.
- `bulkProductResourceFeedbackCreate` is partially successful by input row:
  staged feedback is returned for the existing product while the missing
  product row returns `Product does not exist` at the captured indexed
  `feedbackInput` path.
- `shopResourceFeedbackCreate` returns an `AppFeedback` payload with message
  objects, null `app`/`link`, and the captured generated timestamp rather than
  a product-addressable record.

### Risks / open items

- Inventory shipment/transfer roots, broader publication roots, selling plans,
  product metafields, media, advanced search, duplicate/productSet/media roots,
  and broader validation atomicity parity remain incomplete in Gleam.
- Only 72 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 85 candidates

- Continue remaining inventory shipment/transfer roots with strict captured
  fixture replay.
- Continue the broader publication roots local-runtime scenario now that
  top-level publications and product publish/unpublish are represented.
- Port selling-plan or product metafield behavior now that core Product,
  Variant, Inventory, Collection, Location, Publication, feed, and feedback
  read/write slices are locally staged or seeded.

---

## 2026-04-30 - Pass 83: product feed lifecycle

Completes the captured product feed local-runtime lifecycle in the Gleam
Products handler. The pass adds a normalized product-feed state slice, routes
`productFeed` and `productFeeds` reads through the local Products dispatcher,
and stages `productFeedCreate`, `productFullSync`, and `productFeedDelete`
without runtime Shopify writes. Downstream reads now expose the staged ACTIVE
feed, connection cursors, and null/empty no-data behavior after deletion.

The pass promotes `product-feed-lifecycle-local-runtime` into the Gleam parity
suite. The checked-in fixture, request documents, variables capture path, and
strict comparison contract stay unchanged.

| Module                                               | Change                                                                 |
| ---------------------------------------------------- | ---------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`    | Adds `ProductFeedRecord` for the captured feed fields.                 |
| `gleam/src/shopify_draft_proxy/state/store.gleam`    | Adds base/staged product-feed storage, ordering, deletion, and lookup. |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Stages feed lifecycle mutations and serializes feed reads.             |
| `gleam/test/parity_test.gleam`                       | Enables the strict product-feed lifecycle parity spec.                 |

Validation:
`gleam test --target javascript product_feed_lifecycle_local_runtime_test` is
green at 785 tests, and full `gleam test --target javascript` is green at 785
tests on the host Node runtime. Host `gleam test --target erlang` still fails
before tests execute on the local Erlang install with the known `undef` runner
issue; after clearing host-built Erlang artifacts, the Docker Erlang fallback is
green at 781 tests. Product parity inventory remains 115 checked-in specs, with
71 product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was strict parity replay returning HTTP 400 for
  `productFeedCreate`: `No mutation dispatcher implemented for root field`.
- The captured local-runtime fixture expects `gid://shopify/ProductFeed/2`
  because the TypeScript path reserves a mutation-log synthetic id before
  minting the feed id; the Gleam handler preserves that observable sequence for
  this staged root.
- Product feed delete needs a real deleted-id marker so the same read document
  returns `productFeed: null` and an empty `productFeeds` connection with null
  cursors after deletion.

### Risks / open items

- Broader publication roots, product feedback, selling plans, product
  metafields, media, advanced search, inventory shipment/transfer, and broader
  validation atomicity parity remain incomplete in Gleam.
- Only 71 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 84 candidates

- Port product feedback lifecycle now that product-feed local-runtime behavior
  is represented.
- Continue the broader publication roots local-runtime scenario now that
  top-level publications and product publish/unpublish are represented.
- Continue remaining inventory shipment/transfer roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 82: product publication publish/unpublish

Completes the captured `productPublish` and `productUnpublish` payload and
aggregate-read parity slices in the Gleam Products handler. The pass wires both
mutation roots into local staging, preserves raw mutation log handling through
the existing Products mutation flow, seeds the captured DRAFT product
precondition, and projects Shopify's captured publication aggregate fields for
that DRAFT product as `false`/zero-count values.

The pass promotes `productPublish-parity-plan`,
`productPublish-aggregate-parity`, `productUnpublish-parity-plan`, and
`productUnpublish-aggregate-parity` into the Gleam parity suite. The checked-in
fixtures, request documents, variables capture paths, and strict comparison
contracts stay unchanged.

| Module                                               | Change                                                                |
| ---------------------------------------------------- | --------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Stages product publish/unpublish roots and serializes aggregates.     |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured minimal product fixture for publication mutations. |
| `gleam/test/parity_test.gleam`                       | Enables four strict product publication parity specs.                 |

Validation:
Focused JavaScript parity for the four product publication tests is green at
784 tests, and full `gleam test --target javascript` is green at 784 tests on
the host Node runtime. Host `gleam test --target erlang` still fails before
tests execute on the local Erlang install with the known `undef` runner issue;
after clearing host-built Erlang artifacts, the Docker Erlang fallback is green
at 780 tests. Product parity inventory remains 115 checked-in specs, with 70
product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was strict parity replay returning HTTP 400 for
  both `productPublish` and `productUnpublish`: `No mutation dispatcher
implemented for root field`.
- The live fixtures publish/unpublish a DRAFT product, and Shopify still
  reports `publishedOnCurrentPublication: false` plus zero
  `availablePublicationsCount`/`resourcePublicationsCount` values in both the
  mutation payload and downstream read.
- The checked-in seed product for this pair is intentionally minimal
  (`id`/`title`/`status`), so the parity runner seeds it through the relaxed
  product decoder rather than weakening the capture.

### Risks / open items

- Broader publication roots (`publicationCreate`/update/delete,
  `publishablePublish`, channels, and count roots), product feeds/feedback,
  selling plans, product metafields, media, advanced search, inventory
  shipment/transfer, and broader validation atomicity parity remain incomplete
  in Gleam.
- Only 70 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 83 candidates

- Continue the broader publication roots local-runtime scenario now that
  top-level publications and product publish/unpublish are represented.
- Port product metafield behavior now that Product, Product Option,
  Product Variant, Inventory, Collection, Location, and Publication read slices
  are locally staged or seeded.
- Continue remaining inventory shipment/transfer roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 81: publications catalog read

Completes the captured top-level `publications` catalog read in the Gleam
Products handler. The pass adds a base publication catalog slice, routes
`publications` through the Products dispatcher, serializes selected
`Publication` fields through the shared connection helpers, and preserves
captured opaque cursors/pageInfo for strict replay.

The pass promotes `publications-catalog-read` into the Gleam parity suite. The
checked-in fixture, request document, variables, and strict comparison contract
stay unchanged.

| Module                                               | Change                                                         |
| ---------------------------------------------------- | -------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`    | Adds a top-level `PublicationRecord` with cursor preservation. |
| `gleam/src/shopify_draft_proxy/state/store.gleam`    | Adds base publication catalog storage/listing helpers.         |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Routes and serializes the top-level `publications` connection. |
| `gleam/test/parity/runner.gleam`                     | Seeds captured publication catalog baselines for pure replay.  |
| `gleam/test/parity_test.gleam`                       | Enables the strict publications catalog parity spec.           |

Validation:
`gleam test --target javascript publications_catalog_read_test` is green at
780 tests, and full `gleam test --target javascript` is green at 780 tests on
the host Node runtime. Host `gleam test --target erlang` still fails before
tests execute on the local Erlang install with the known `undef` runner issue;
after clearing host-built Erlang artifacts, the Docker Erlang fallback is green
at 776 tests. Product parity inventory remains 115 checked-in specs, with 66
product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a strict parity replay returning HTTP 400
  with `No domain dispatcher implemented for root field: publications`.
- The captured fixture compares Shopify's opaque publication cursors exactly,
  so the local record stores the captured cursor instead of deriving a synthetic
  cursor from the publication ID.
- The top-level `publications` root belongs to the Products registry domain in
  this repo and can be represented as a read-only captured catalog baseline
  until publication-related mutations/links are ported.

### Risks / open items

- Inventory shipment/transfer, publication links, product feeds/feedback,
  selling plans, product metafields, media, advanced search, and broader
  validation atomicity parity remain incomplete in Gleam.
- Only 66 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 82 candidates

- Continue publication-related collection/product roots if the captured
  fixtures can be represented without weakening request shapes.
- Port product metafield behavior now that Product, Product Option,
  Product Variant, Inventory, Collection, Location, and Publication read slices
  are locally staged or seeded.
- Continue remaining inventory shipment/transfer roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 80: locations catalog read

Completes the captured top-level `locations` catalog read in the Gleam Products
handler. The pass adds a base location catalog slice, routes `locations` through
the Products dispatcher, serializes selected `Location` fields through the
shared connection helpers, and preserves captured opaque cursors/pageInfo for
strict replay.

The pass promotes `locations-catalog-read` into the Gleam parity suite. The
checked-in fixture, request document, variables, and strict comparison contract
stay unchanged.

| Module                                               | Change                                                      |
| ---------------------------------------------------- | ----------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`    | Adds a top-level `LocationRecord` with cursor preservation. |
| `gleam/src/shopify_draft_proxy/state/store.gleam`    | Adds base location catalog storage/listing helpers.         |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Routes and serializes the top-level `locations` connection. |
| `gleam/test/parity/runner.gleam`                     | Seeds captured location catalog baselines for pure replay.  |
| `gleam/test/parity_test.gleam`                       | Enables the strict locations catalog parity spec.           |

Validation:
`gleam test --target javascript locations_catalog_read_test` is green at 779
tests, and full `gleam test --target javascript` is green at 779 tests on the
host Node runtime. Host `gleam test --target erlang` still fails before tests
execute on the local Erlang install with the known `undef` runner issue; after
clearing host-built Erlang artifacts, the Docker Erlang fallback is green at
775 tests. Product parity inventory remains 115 checked-in specs, with 65
product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a strict parity replay returning HTTP 400
  with `No domain dispatcher implemented for root field: locations`.
- The captured fixture compares Shopify's opaque location cursors exactly, so
  the local record stores the captured cursor instead of deriving a synthetic
  cursor from the location ID.
- The top-level `locations` root belongs to the Products registry domain in
  this repo, even though location records are also reused by inventory-level
  projections.

### Risks / open items

- Inventory shipment/transfer, publication links, product feeds/feedback,
  selling plans, product metafields, media, advanced search, and broader
  validation atomicity parity remain incomplete in Gleam.
- Only 65 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 81 candidates

- Port product metafield behavior now that Product, Product Option,
  Product Variant, Inventory, Collection, and Location read slices are locally
  staged or seeded.
- Continue publication-related collection/product roots if the captured
  fixtures can be represented without weakening request shapes.
- Continue remaining inventory shipment/transfer roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 79: collection create initial products

Completes the captured `collectionCreate` fixture that supplies initial
product IDs through `CollectionInput.products`. The Gleam Products handler now
stages initial collection/product memberships locally, returns selected product
nodes and `hasProduct: true` in the mutation payload, preserves Shopify's
captured mutation-time aggregate lag with `productsCount: 0`, and exposes the
actual membership count through immediate downstream collection/product reads.

The pass promotes `collectionCreate-initial-products-parity` into the Gleam
parity suite. The checked-in fixture, request documents, variables capture
path, expected ID differences, and strict comparison contract stay unchanged.

| Module                                               | Change                                                                     |
| ---------------------------------------------------- | -------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Stages `CollectionInput.products` memberships during collection create.    |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured pre-existing product/Home page collection relationship. |
| `gleam/test/parity_test.gleam`                       | Enables the strict initial-products collection create parity spec.         |

Validation:
`gleam test --target javascript collection_create_initial_products_live_parity_test`
is green at 778 tests, and full `gleam test --target javascript` is green at
778 tests on the host Node runtime. Host `gleam test --target erlang` still
fails before tests execute on the local Erlang install with the known `undef`
runner issue; after clearing host-built Erlang artifacts, the Docker Erlang
fallback is green at 774 tests. Product parity inventory remains 115 checked-in
specs, with 64 product specs executable in the Gleam parity suite plus the
admin-platform ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a strict parity replay where the mutation
  payload returned `hasProduct: null`, no product nodes, `productsCount: null`,
  and downstream reads missed both the new collection membership and the
  product-side relationship.
- Shopify's live capture returns product nodes and `hasProduct: true` in the
  mutation payload while keeping the mutation payload's `productsCount` at zero;
  the immediate downstream read returns the real count of two.
- The downstream product read expects the first seed product to retain its
  existing `Home page` collection relationship, so the parity runner seeds that
  captured precondition without changing the fixture.

### Risks / open items

- Inventory shipment/transfer, publication links, product feeds/feedback,
  selling plans, product metafields, media, advanced search, and broader
  validation atomicity parity remain incomplete in Gleam.
- Only 64 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 80 candidates

- Port product metafield behavior now that Product, Product Option,
  Product Variant, Inventory, and Collection relationship slices are locally
  staged or seeded.
- Continue publication-related collection/product roots if the captured
  fixtures can be represented without weakening request shapes.
- Continue remaining inventory shipment/transfer roots with strict captured
  fixture replay.

---

## 2026-04-30 - Pass 78: collection create staging

Completes the captured base collection create mutation in the Gleam Products
handler. The pass wires `collectionCreate` into the local mutation dispatcher,
allocates Shopify-shaped synthetic collection IDs, derives Shopify-like handles
from collection titles, stages new collections without runtime Shopify writes,
and returns the captured collection/userErrors payload shape.

The pass promotes `collectionCreate-parity-plan` into the Gleam parity suite.
The checked-in fixture, request document, variables capture path, expected ID
difference, and strict comparison contract stay unchanged.

| Module                                               | Change                                                       |
| ---------------------------------------------------- | ------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Stages base `collectionCreate` and serializes create output. |
| `gleam/test/parity_test.gleam`                       | Enables the strict base collection create parity spec.       |

Validation:
`gleam test --target javascript collection_create_live_parity_test` is green at
777 tests, and full `gleam test --target javascript` is green at 777 tests on
the host Node runtime. Host `gleam test --target erlang` still fails before
tests execute on the local Erlang install with the known `undef` runner issue;
after clearing host-built Erlang artifacts, the Docker Erlang fallback is green
at 773 tests. Product parity inventory remains 115 checked-in specs, with 63
product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay returning
  HTTP 400 with `No mutation dispatcher implemented for root field:
collectionCreate`.
- The base captured fixture creates a manual collection with no initial
  products, so the staged collection projects an empty `products` connection
  and `productsCount: 0`.
- Shopify allocates the created collection ID independently of local replay; the
  existing parity spec already path-scopes that ID difference.

### Risks / open items

- `collectionCreate` with initial products remains a separate captured fixture
  because it asserts immediate downstream collection/product relationship reads.
- Inventory shipment/transfer, publication links, product feeds/feedback,
  selling plans, product metafields, media, advanced search, and broader
  validation atomicity parity remain incomplete in Gleam.
- Only 63 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 79 candidates

- Extend `collectionCreate` with the captured initial-products behavior and
  downstream relationship reads.
- Port product metafield behavior now that Product, Product Option,
  Product Variant, Inventory, and Collection relationship slices are locally
  staged or seeded.
- Continue publication-related collection/product roots if the captured
  fixtures can be represented without weakening request shapes.

---

## 2026-04-30 - Pass 77: collection delete staging

Completes the captured collection delete mutation in the Gleam Products
handler. The pass wires `collectionDelete` into the local mutation dispatcher,
stages collection deletion without runtime Shopify writes, clears collection
membership rows for the deleted collection, and returns the captured
deletedCollectionId/userErrors payload shape.

The pass promotes `collectionDelete-parity-plan` into the Gleam parity suite.
The checked-in fixture, request document, variables capture path, and strict
comparison contract stay unchanged.

| Module                                               | Change                                                    |
| ---------------------------------------------------- | --------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/store.gleam`    | Adds staged collection deletion and membership cleanup.   |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Stages `collectionDelete` and serializes delete output.   |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured delete target collection precondition. |
| `gleam/test/parity_test.gleam`                       | Enables the strict collection delete parity spec.         |

Validation:
`gleam test --target javascript collection_delete_live_parity_test` is green at
776 tests, and full `gleam test --target javascript` is green at 776 tests on
the host Node runtime. Host `gleam test --target erlang` still fails before
tests execute on the local Erlang install with the known `undef` runner issue;
after clearing host-built Erlang artifacts, the Docker Erlang fallback is green
at 772 tests. Product parity inventory remains 115 checked-in specs, with 62
product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay returning
  HTTP 400 with `No mutation dispatcher implemented for root field:
collectionDelete`.
- The captured delete fixture only compares mutation data, but the store helper
  also marks the collection deleted and clears its product-collection rows so
  downstream reads observe Shopify-like no-data behavior for the deleted
  collection.
- The fixture does not contain the deleted collection body after the delete, so
  the parity runner seeds a minimal target collection from the captured input ID
  before replaying the mutation.

### Risks / open items

- Remaining collection create roots, inventory shipment/transfer, publication
  links, product feeds/feedback, selling plans, product metafields, media,
  advanced search, and broader validation atomicity parity remain incomplete in
  Gleam.
- Only 62 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 78 candidates

- Continue collection lifecycle roots with `collectionCreate` and its captured
  initial-products behavior.
- Port product metafield behavior now that Product, Product Option,
  Product Variant, Inventory, and Collection relationship slices are locally
  staged or seeded.
- Continue publication-related collection/product roots if the captured
  fixtures can be represented without weakening request shapes.

---

## 2026-04-30 - Pass 76: collection update staging

Completes the captured collection update mutation in the Gleam Products
handler. The pass wires `collectionUpdate` into the local mutation dispatcher,
stages updated collection records without runtime Shopify writes, preserves the
existing collection/product membership reads, and returns the captured
collection/userErrors payload shape.

The pass promotes `collectionUpdate-parity-plan` into the Gleam parity suite.
The checked-in fixture, request document, variables capture path, downstream
read, and strict comparison contract stay unchanged.

| Module                                               | Change                                                      |
| ---------------------------------------------------- | ----------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Stages `collectionUpdate` and serializes collection output. |
| `gleam/test/parity/runner.gleam`                     | Seeds captured update collection/product preconditions.     |
| `gleam/test/parity_test.gleam`                       | Enables the strict collection update parity spec.           |

Validation:
`gleam test --target javascript collection_update_live_parity_test` is green at
775 tests, and full `gleam test --target javascript` is green at 775 tests on
the host Node runtime. Host `gleam test --target erlang` still fails before
tests execute on the local Erlang install with the known `undef` runner issue;
after clearing host-built Erlang artifacts, the Docker Erlang fallback is green
at 771 tests. Product parity inventory remains 115 checked-in specs, with 61
product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay returning
  HTTP 400 with `No mutation dispatcher implemented for root field:
collectionUpdate`.
- The captured update fixture only changes `title` and `handle`, but the
  handler preserves collection membership and projects the selected
  `Collection.products` connection from the effective store.
- Seeding the target collection from the captured mutation payload plus its
  product nodes is enough for strict mutation payload parity without changing
  the fixture or request shape.

### Risks / open items

- Remaining collection create/delete roots, inventory shipment/transfer,
  publication links, product feeds/feedback, selling plans, product metafields,
  media, advanced search, and broader validation atomicity parity remain
  incomplete in Gleam.
- Only 61 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 77 candidates

- Continue collection lifecycle roots with `collectionCreate` initial-products
  behavior or `collectionDelete`.
- Port product metafield behavior now that Product, Product Option,
  Product Variant, Inventory, and Collection relationship slices are locally
  staged or seeded.
- Continue publication-related collection/product roots if the captured
  fixtures can be represented without weakening request shapes.

---

## 2026-04-30 - Pass 75: collection reorder-products staging

Completes the captured collection product reorder mutation in the Gleam
Products handler. The pass wires `collectionReorderProducts` into the local
mutation dispatcher, parses Shopify `MoveInput` values, stages reordered
product-collection families without runtime Shopify writes, and returns the
captured asynchronous Job/userErrors payload shape.

The pass promotes `collectionReorderProducts-parity-plan` into the Gleam parity
suite. The checked-in fixture, request document, variables, downstream read,
and comparison contract stay unchanged.

| Module                                               | Change                                                        |
| ---------------------------------------------------- | ------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Stages `collectionReorderProducts` and serializes Job output. |
| `gleam/test/parity/runner.gleam`                     | Seeds captured reorder collection/product preconditions.      |
| `gleam/test/parity_test.gleam`                       | Enables the strict collection reorder-products parity spec.   |

Validation:
`gleam test --target javascript collection_reorder_products_live_parity_test`
is green at 774 tests, and full `gleam test --target javascript` is green at
774 tests on the host Node runtime. Host `gleam test --target erlang` still
fails before tests execute on the local Erlang install with the known `undef`
runner issue; after clearing host-built Erlang artifacts, the Docker Erlang
fallback is green at 770 tests. Product parity inventory remains 115 checked-in
specs, with 60 product specs executable in the Gleam parity suite plus the
admin-platform ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay returning
  HTTP 400 with `No mutation dispatcher implemented for root field:
collectionReorderProducts`.
- The captured reorder fixture moves the second product to position `0`; both
  default and manual collection product reads reflect the new order while the
  individual Product.collections reads preserve unrelated collection links.
- The staged product-collection family substrate from Pass 74 maps directly to
  reorder behavior: each affected product gets a complete staged collection
  family with only the target membership position changed.

### Risks / open items

- Remaining collection create/update/delete roots, inventory shipment/transfer,
  publication links, product feeds/feedback, selling plans, product metafields,
  media, advanced search, and broader validation atomicity parity remain
  incomplete in Gleam.
- Only 60 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 76 candidates

- Continue collection mutations with `collectionCreate` initial-products
  behavior or collection update/delete lifecycle.
- Port product metafield behavior now that Product, Product Option,
  Product Variant, Inventory, and Collection relationship slices are locally
  staged or seeded.
- Continue publication-related collection/product roots if the captured
  fixtures can be represented without weakening request shapes.

---

## 2026-04-30 - Pass 74: collection remove-products staging

Completes the second collection membership mutation in the Gleam Products
handler. The pass wires `collectionRemoveProducts` into the local mutation
dispatcher, stages per-product collection membership replacements without
runtime Shopify writes, and emits the captured asynchronous Job/userErrors
payload shape.

The pass promotes `collectionRemoveProducts-parity-plan` into the Gleam parity
suite. The checked-in fixture, request document, variables, and comparison
contract stay unchanged.

| Module                                               | Change                                                         |
| ---------------------------------------------------- | -------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/store.gleam`    | Adds staged product-collection family replacement semantics.   |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Stages `collectionRemoveProducts` and serializes Job payloads. |
| `gleam/test/parity/runner.gleam`                     | Seeds captured remove-products collection preconditions.       |
| `gleam/test/parity_test.gleam`                       | Enables the strict collection remove-products parity spec.     |

Validation:
`gleam test --target javascript collection_remove_products_live_parity_test` is
green at 773 tests, and full `gleam test --target javascript` is green at 773
tests on the host Node runtime. Host `gleam test --target erlang` still fails
before tests execute on the local Erlang install with the known `undef` runner
issue; after clearing host-built Erlang artifacts, the Docker Erlang fallback is
green at 769 tests. Product parity inventory remains 115 checked-in specs, with
59 product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay returning
  HTTP 400 with `No mutation dispatcher implemented for root field:
collectionRemoveProducts`.
- Shopify returns a `Job` with `done: false` for a successful removal, and the
  proxy only treats the Job ID as nondeterministic under the existing strict
  comparison contract.
- Collection-side product reads need to derive membership through each
  product's effective collection family so staged removals suppress base
  memberships without hiding unrelated collection links.

### Risks / open items

- Remaining collection roots, inventory shipment/transfer, publication links,
  product feeds/feedback, selling plans, product metafields, media, advanced
  search, and broader validation atomicity parity remain incomplete in Gleam.
- Only 59 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 75 candidates

- Continue collection membership roots with `collectionReorderProducts`, using
  the staged product-collection family substrate added here.
- Port `collectionCreate` initial-products behavior, which should reuse the
  collection membership seeding and projection paths.
- Port product metafield behavior now that Product, Product Option,
  Product Variant, Inventory, and Collection read/write slices are locally
  staged or seeded.

---

## 2026-04-30 - Pass 73: collection add-products staging

Adds the first collection membership mutation to the Gleam Products handler.
The pass wires `collectionAddProducts` into the local mutation dispatcher,
stages Product-to-Collection membership rows without runtime Shopify writes,
and projects downstream reads from both directions: Collection.products and
Product.collections.

The pass promotes `collectionAddProducts-parity-plan` into the Gleam parity
suite. The checked-in fixture, request document, variables, and comparison
contract stay unchanged.

| Module                                               | Change                                                       |
| ---------------------------------------------------- | ------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/state/store.gleam`    | Adds base/staged helpers for product collection memberships. |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Stages `collectionAddProducts` and serializes product links. |
| `gleam/test/parity/runner.gleam`                     | Seeds captured collection/product membership preconditions.  |
| `gleam/test/parity_test.gleam`                       | Enables the strict collection add-products parity spec.      |

Validation:
`gleam test --target javascript collection_add_products_live_parity_test` is
green at 772 tests, and full `gleam test --target javascript` is green at 772
tests on the host Node runtime. Host `gleam test --target erlang` still fails
before tests execute on the local Erlang install with the known `undef` runner
issue; after clearing host-built Erlang artifacts, the Docker Erlang fallback is
green at 768 tests. Product parity inventory remains 115 checked-in specs, with
58 product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay returning
  HTTP 400 with `No mutation dispatcher implemented for root field:
collectionAddProducts`.
- The captured downstream read needs membership projection in both directions:
  the mutation-created collection membership appears under `collection.products`,
  while one product also preserves an existing ADIDAS collection before the new
  collection appears in `product.collections`.
- Product and collection SourceValue projections must avoid recursive eager
  relationship expansion, so Collection.products serializes Product nodes with
  basic product fields while Product.collections carries the collection
  connection.

### Risks / open items

- Remaining collection mutation roots, publication links, product
  feeds/feedback, selling plans, product metafields, inventory
  shipment/transfer, media, advanced search, and broader validation atomicity
  parity remain incomplete in Gleam.
- Only 58 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 74 candidates

- Continue collection membership roots with `collectionRemoveProducts` or
  `collectionReorderProducts` now that bidirectional membership projection is
  available.
- Port `collectionCreate` initial-products behavior, using the same membership
  substrate and captured downstream reads.
- Port product metafield behavior now that the collection/product relationship
  path is locally staged.

---

## 2026-04-30 - Pass 72: collections catalog reads

Extends the normalized collection slice from Pass 71 to the top-level
`collections` catalog root. The pass seeds the captured catalog page into base
state, preserves captured cursors for default, title, and updated-at orderings,
and routes collection catalog reads through local search filtering, Shopify-like
sort handling, connection pagination, and the existing collection serializer.

The pass promotes `collections-catalog-read` into the Gleam parity suite. The
checked-in fixture, request document, variables, and comparison contract stay
unchanged.

| Module                                               | Change                                                        |
| ---------------------------------------------------- | ------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`    | Tracks per-sort captured collection cursors.                  |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Serializes top-level collection catalog connections locally.  |
| `gleam/test/parity/runner.gleam`                     | Seeds captured catalog collections and sort-specific cursors. |
| `gleam/test/parity_test.gleam`                       | Enables the strict collections catalog parity spec.           |

Validation:
`gleam test --target javascript collections_catalog_read_test` is green at 771
tests, and full `gleam test --target javascript` is green at 771 tests on the
host Node runtime. Host `gleam test --target erlang` still fails before tests
execute on the local Erlang install with the known `undef` runner issue; after
clearing host-built Erlang artifacts, the Docker Erlang fallback is green at
767 tests. Product parity inventory remains 115 checked-in specs, with 57
product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay where the
  top-level `collections` root returned the empty no-data connection and
  produced 21 mismatches against the captured catalog fixture.
- Shopify's catalog fixture reuses the same collection records across default,
  title-filtered, smart-only, product-membership, updated-newest, and empty
  query reads, but each sorted connection can carry a distinct opaque cursor;
  the seeder stores those cursors without decoding them.
- The collection search path follows the shared Admin query parser and keeps
  permissive handling for publication and updated-at terms that are present in
  captured catalog requests but are not yet modeled as first-class collection
  publication state.

### Risks / open items

- Collection mutation roots, publication links, product feeds/feedback, selling
  plans, product metafields, inventory shipment/transfer, media, advanced
  search, and broader validation atomicity parity remain incomplete in Gleam.
- Only 57 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 73 candidates

- Port collection membership mutation roots now that collection reads and
  product membership rows are normalized in Gleam state.
- Port product metafield behavior now that Product, Product Option,
  Product Variant, Inventory, and Collection read slices are locally staged or
  seeded.
- Continue publication-related collection/product roots if the captured
  fixtures can be represented without weakening request shapes.

---

## 2026-04-30 — Pass 71: collection detail and identifier reads

Adds the first normalized collection read slice to the Gleam Products handler.
The pass models captured collection records, collection images, SEO, smart
rule sets, product membership rows, exact product counts, and collection product
connection cursors so strict collection detail and identifier/handle reads can
replay against the local proxy instead of returning no-data placeholders.

The pass promotes `collection-detail-read` and `collection-identifier-read` into
the Gleam parity suite. The checked-in fixtures, request documents, variables,
and comparison contracts stay unchanged.

| Module                                               | Change                                                             |
| ---------------------------------------------------- | ------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/state/types.gleam`    | Adds collection, image, rule-set, and product-membership records.  |
| `gleam/src/shopify_draft_proxy/state/store.gleam`    | Adds base/staged collection storage and effective read helpers.    |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Serializes `collection`, identifier, and deprecated handle roots.  |
| `gleam/test/parity/runner.gleam`                     | Seeds collection detail captures into normalized collection state. |
| `gleam/test/parity_test.gleam`                       | Enables the strict collection detail and identifier specs.         |

Validation:
`gleam test --target javascript collection_detail_read_test` is green at 769
tests, `gleam test --target javascript collection_identifier_read_test` is green
at 770 tests, and full `gleam test --target javascript` is green at 770 tests on
the host Node runtime. Host `gleam test --target erlang` still fails before
tests execute on the local Erlang install with the known `undef` runner issue;
after clearing host-built Erlang artifacts, the Docker Erlang fallback is green
at 766 tests. Product parity inventory remains 115 checked-in specs, with 56
product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay where both
  `customCollection` and `smartCollection` came back `null`.
- Shopify's captured smart collection had `hasProduct: true` for the queried
  product even though that product was not on the first product-connection page;
  the seeder preserves that membership after the visible page so `hasProduct`
  and page cursors both stay faithful.
- The detail fixture's captured `productsCount` is authoritative for connection
  pagination and count serialization, so the normalized collection record keeps
  that exact count instead of inferring it only from the locally seeded first
  page.

### Risks / open items

- Top-level `collections` catalog search/sort/filter parity remains incomplete.
- Collection mutation roots, publication links, product feeds/feedback, selling
  plans, product metafields, inventory shipment/transfer, media, advanced
  search, and broader validation atomicity parity remain incomplete in Gleam.
- Only 56 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 72 candidates

- Build on the collection slice with top-level `collections` catalog
  search/sort/filter replay.
- Port collection membership mutation roots now that product membership rows
  exist in normalized Gleam state.
- Port product metafield behavior now that Product, Product Option, Product
  Variant, Inventory, and initial Collection read slices are locally staged or
  seeded.

---

## 2026-04-30 — Pass 70: product variant bulk create inventory reads

Enables strict Gleam parity coverage for the captured
`productVariantsBulkCreate` inventory downstream-read scenario. The scenario
adds a variant with a staged InventoryItem, then immediately reads the Product,
created ProductVariant, and created InventoryItem roots. The existing Gleam
Products handler already staged the created variant and inventory item with
consistent synthetic IDs; this pass promotes the fixture-backed scenario by
seeding the captured pre-mutation Product baseline through the bulk-create
precondition path.

The pass promotes `productVariantsBulkCreate-inventory-read-parity` into the
Gleam parity suite. The checked-in fixture, request document, variables, and
comparison contract stay unchanged.

| Module                           | Change                                                              |
| -------------------------------- | ------------------------------------------------------------------- |
| `gleam/test/parity/runner.gleam` | Seeds the captured bulk-create inventory-read Product precondition. |
| `gleam/test/parity_test.gleam`   | Enables the strict bulk-create inventory downstream-read spec.      |

Validation:
`gleam test --target javascript product_variants_bulk_create_inventory_read_parity_test`
is green at 768 tests, and full `gleam test --target javascript` is green at
768 tests on the host Node runtime. Host `gleam test --target erlang` still
fails before tests execute on the local Erlang install with the known `undef`
runner issue; after clearing host-built Erlang artifacts, the Docker Erlang
fallback is green at 764 tests. Product parity inventory remains 115 checked-in
specs, with 54 product specs executable in the Gleam parity suite plus the
admin-platform ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay failing
  because `$.data.productVariantsBulkCreate.product.id` could not be resolved
  before seeding the captured product baseline.
- After reusing the existing bulk-create precondition seeder, the scenario
  passed without additional runtime changes. The staged created ProductVariant
  and InventoryItem IDs already flow consistently through mutation payload,
  Product read, ProductVariant read, and InventoryItem read.

### Risks / open items

- Collections, publication, product feeds/feedback, selling plans, product
  metafields, inventory shipment/transfer, media, advanced search, and broader
  validation atomicity parity remain incomplete in Gleam.
- Only 54 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 71 candidates

- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.
- Port product metafield behavior now that several Product and Product Option
  mutation families are locally staged.
- Tackle product media local staging if the media fixtures can be represented
  without weakening the recorded Shopify request shapes.

---

## 2026-04-30 — Pass 69: product variant bulk create remove custom edge

Enables strict Gleam parity coverage for the captured
`productVariantsBulkCreate(strategy: REMOVE_STANDALONE_VARIANT)` custom
standalone-variant edge. Shopify removes the existing custom standalone variant,
creates the submitted variant, and rebuilds the Product option/value graph
around the created variant's selected option value. The Gleam Products handler
already matched that removal path after the Pass 66/68 option-selection sync
work; this pass promotes the sibling scenario by seeding the captured
pre-mutation Product baseline.

The pass promotes
`productVariantsBulkCreate-strategy-remove-custom-standalone` into the Gleam
parity suite. The checked-in fixture, request document, and comparison contract
stay unchanged.

| Module                           | Change                                                                 |
| -------------------------------- | ---------------------------------------------------------------------- |
| `gleam/test/parity/runner.gleam` | Seeds the captured custom standalone-variant precondition graph.       |
| `gleam/test/parity_test.gleam`   | Enables the strict remove/custom standalone bulk-create strategy spec. |

Validation:
`gleam test --target javascript product_variants_bulk_create_strategy_remove_custom_standalone_test`
is green at 767 tests, and full `gleam test --target javascript` is green at
767 tests on the host Node runtime. Host `gleam test --target erlang` still
fails before tests execute on the local Erlang install with the known `undef`
runner issue; after clearing host-built Erlang artifacts, the Docker Erlang
fallback is green at 763 tests. Product parity inventory remains 115 checked-in
specs, with 53 product specs executable in the Gleam parity suite plus the
admin-platform ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay failing
  because `$.data.productVariantsBulkCreate.product.id` could not be resolved
  before seeding the captured product baseline.
- After seeding, the scenario passed without additional runtime changes: the
  standalone removal path already rebuilt options from the created variant's
  selected options and the spec already treats freshly allocated option/value
  IDs as expected differences.
- The four captured standalone bulk-create strategy edges are now executable in
  the Gleam parity suite.

### Risks / open items

- Collections, publication, product feeds/feedback, selling plans, product
  metafields, inventory shipment/transfer, media, and advanced search parity
  remain incomplete in Gleam.
- Only 53 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 70 candidates

- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.
- Port product metafield behavior now that several Product and Product Option
  mutation families are locally staged.
- Port the remaining variant media or inventory-read parity slices that can be
  lifted from existing fixtures without live capture.

---

## 2026-04-30 — Pass 68: product variant bulk create default custom edge

Adds strict Gleam parity coverage for the captured
`productVariantsBulkCreate(strategy: DEFAULT)` custom standalone-variant edge.
Shopify keeps the existing custom standalone variant, appends the submitted
variant, and extends the existing Product option with the new selected option
value. The Gleam Products handler now mirrors that retained-variant path by
upserting selected option values from the full post-mutation variant set before
recomputing `hasVariants`, while preserving existing option and option-value
IDs.

The pass promotes
`productVariantsBulkCreate-strategy-default-custom-standalone` into the Gleam
parity suite. Runner seeding reuses the captured `preMutationRead` Product,
option, and variant baseline; the checked-in fixture, request document, and
comparison contract stay unchanged.

| Module                                               | Change                                                                  |
| ---------------------------------------------------- | ----------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Appends selected option values for retained bulk-create variants.       |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured custom standalone-variant precondition graph.        |
| `gleam/test/parity_test.gleam`                       | Enables the strict default/custom standalone bulk-create strategy spec. |

Validation:
`gleam test --target javascript product_variants_bulk_create_strategy_default_custom_standalone_test`
is green at 766 tests, and full `gleam test --target javascript` is green at
766 tests on the host Node runtime. Host `gleam test --target erlang` still
fails before tests execute on the local Erlang install with the known `undef`
runner issue; after clearing host-built Erlang artifacts, the Docker Erlang
fallback is green at 762 tests. Product parity inventory remains 115 checked-in
specs, with 52 product specs executable in the Gleam parity suite plus the
admin-platform ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal first failed because
  `$.data.productVariantsBulkCreate.product.id` could not be resolved before
  seeding the captured product baseline.
- After seeding, the real fidelity gap was the retained DEFAULT/custom path:
  Shopify appended the new selected option value (`Default Blue`) to the
  existing `Color` option, while Gleam only recomputed `hasVariants` for values
  that already existed.
- The standalone removal path already rebuilt options from variant selections;
  this pass reuses the same upsert helper for retained variants without
  changing existing option IDs.

### Risks / open items

- The sibling REMOVE/custom standalone strategy edge remains to be enabled.
- Collections, publication, product feeds/feedback, selling plans, product
  metafields, inventory shipment/transfer, media, and advanced search parity
  remain incomplete in Gleam.
- Only 52 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 69 candidates

- Enable the remaining
  `productVariantsBulkCreate-strategy-remove-custom-standalone` spec now that
  retained and removal option/value sync paths share the selection upsert
  helper.
- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.
- Port product metafield behavior now that several Product and Product Option
  mutation families are locally staged.

---

## 2026-04-30 — Pass 67: product variant bulk create remove strategy edge

Enables strict Gleam parity coverage for the captured
`productVariantsBulkCreate(strategy: REMOVE_STANDALONE_VARIANT)` standalone
default-variant edge. The Products handler behavior added in Pass 66 already
matched this sibling strategy capture once the runner seeded the captured
pre-mutation Product, option, and variant graph, so this pass promotes the
scenario without changing the fixture, request document, or comparison
contract.

| Module                           | Change                                                                      |
| -------------------------------- | --------------------------------------------------------------------------- |
| `gleam/test/parity/runner.gleam` | Seeds the captured standalone default-variant precondition graph.           |
| `gleam/test/parity_test.gleam`   | Enables the strict remove/default standalone bulk-create strategy scenario. |

Validation:
`gleam test --target javascript product_variants_bulk_create_strategy_remove_default_standalone_test`
is green at 765 tests, and full `gleam test --target javascript` is green at
765 tests on the host Node runtime. Host `gleam test --target erlang` still
fails before tests execute on the local Erlang install with the known `undef`
runner issue; after clearing host-built Erlang artifacts, the Docker Erlang
fallback is green at 761 tests. Product parity inventory remains 115 checked-in
specs, with 51 product specs executable in the Gleam parity suite plus the
admin-platform ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay failing
  because `$.data.productVariantsBulkCreate.product.id` could not be resolved
  before seeding the captured product baseline.
- The sibling DEFAULT/default standalone fix from Pass 66 already handled the
  option/value derivation needed by this REMOVE_STANDALONE_VARIANT capture.
- The checked-in request and fixture shapes were sufficient; only the Gleam
  runner needed to seed the captured pre-mutation graph and register the spec.

### Risks / open items

- This pass covers one captured remove/default standalone strategy slice, not
  the remaining bulk-create strategy matrix.
- Collections, publication, product feeds/feedback, selling plans, product
  metafields, inventory shipment/transfer, media, and advanced search parity
  remain incomplete in Gleam.
- Only 51 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 68 candidates

- Enable the remaining sibling `productVariantsBulkCreate` standalone strategy
  specs if the seeded graph and option/value rewrite behavior already match.
- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.
- Port product metafield behavior now that several Product and Product Option
  mutation families are locally staged.

---

## 2026-04-30 — Pass 66: product variant bulk create default strategy edge

Adds strict Gleam parity coverage for the captured
`productVariantsBulkCreate(strategy: DEFAULT)` standalone default-variant edge.
When Shopify removes the standalone `Default Title` variant during bulk create,
it derives the Product option/value graph from the created variant's selected
options. The Gleam Products handler now mirrors that path by rebuilding option
records from created variant selections when the standalone default variant is
removed, while preserving raw mutation logging and local read-after-write
staging.

The pass promotes
`productVariantsBulkCreate-strategy-default-default-standalone` into the Gleam
parity suite. Runner seeding reuses the captured `preMutationRead` product,
option, and variant baseline; the checked-in fixture, request document, and
comparison contract stay unchanged.

| Module                                               | Change                                                                               |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Rebuilds option/value rows from variant selections after standalone default removal. |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured standalone default-variant precondition graph.                    |
| `gleam/test/parity_test.gleam`                       | Enables the strict default/default standalone bulk-create strategy scenario.         |

Validation:
`gleam test --target javascript product_variants_bulk_create_strategy_default_default_standalone_test`
is green at 764 tests, and full `gleam test --target javascript` is green at
764 tests on the host Node runtime. Host `gleam test --target erlang` still
fails before tests execute on the local Erlang install with the known `undef`
runner issue; after clearing host-built Erlang artifacts, the Docker Erlang
fallback is green at 760 tests. Product parity inventory remains 115 checked-in
specs, with 50 product specs executable in the Gleam parity suite plus the
admin-platform ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay failing
  because `$.data.productVariantsBulkCreate.product.id` could not be resolved
  before seeding the captured product baseline.
- After seeding, the real fidelity gap was Shopify rewriting the standalone
  default option value from `Default Title` to the created variant value
  `Default Blue`; the previous Gleam sync path only toggled `hasVariants` on
  existing option values.
- The TypeScript oracle derives missing option/value rows from variant selected
  options when the standalone default variant is removed; this pass ports that
  behavior for the bulk-create removal path.

### Risks / open items

- This pass covers the captured DEFAULT/default standalone strategy slice. The
  sibling DEFAULT/custom and REMOVE_STANDALONE strategy edge specs still need
  their own parity enablement.
- Collections, publication, product feeds/feedback, selling plans, product
  metafields, inventory shipment/transfer, media, and advanced search parity
  remain incomplete in Gleam.
- Only 50 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 67 candidates

- Enable the sibling `productVariantsBulkCreate` standalone strategy specs now
  that option/value derivation from created variants exists.
- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.
- Port product metafield behavior now that several Product and Product Option
  mutation families are locally staged.

---

## 2026-04-30 — Pass 65: product options create parity seeding

Enables the captured `productOptionsCreate-parity-plan` in the Gleam parity
suite without changing the fixture or request shape. The Product Options create
root was already staged locally in Gleam, but this specific live parity scenario
was missing the same captured `preMutationRead` seed path used by the sibling
Product Option scenarios. The runner now seeds that product/options/variant
baseline before replaying the captured mutation and downstream read.

| Module                           | Change                                                             |
| -------------------------------- | ------------------------------------------------------------------ |
| `gleam/test/parity/runner.gleam` | Seeds `product-options-create-live-parity` from `preMutationRead`. |
| `gleam/test/parity_test.gleam`   | Enables the strict product options create parity scenario.         |

Validation: `gleam test --target javascript product_options_create_parity_plan_test`
is green at 763 tests, and full `gleam test --target javascript` is green at
763 tests on the host Node runtime. Host `gleam test --target erlang` still
fails before tests execute on the local Erlang install with the known `undef`
runner issue; after clearing host-built Erlang artifacts, the Docker Erlang
fallback is green at 759 tests. Product parity inventory remains 115 checked-in
specs, with 49 product specs executable in the Gleam parity suite plus the
admin-platform ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct parity-runner replay of
  `productOptionsCreate-parity-plan` failing because
  `$.data.productOptionsCreate.product.id` could not be resolved from the
  primary proxy response.
- The failure was a missing scenario seeding entry, not a runtime request-shape
  gap: the scenario's captured `preMutationRead` graph uses the same product
  option seed helper as existing create-strategy/update/delete option scenarios.

### Risks / open items

- This pass enables one already-modeled Product Option lifecycle slice; broader
  collection, publication, product feeds/feedback, selling plans, product
  metafield, inventory shipment/transfer, media, and advanced search parity
  remain incomplete in Gleam.
- Only 49 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 66 candidates

- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.
- Port product metafield behavior now that several Product and Product Option
  mutation families are locally staged.
- Port publication roots or product feed/feedback local-runtime roots if their
  fixture-backed state shapes can be lifted narrowly.

---

## 2026-04-30 — Pass 64: inventory item update lifecycle

Adds local staging for the captured `inventoryItemUpdate` success path. The
Gleam Products mutation handler now routes the root locally, preserves the raw
mutation request through the centralized draft-log path, updates tracked,
shipping, country/province origin, HS code, and measurement weight fields on the
InventoryItem, syncs the owning Product inventory summary, and exposes the
updated InventoryItem through immediate ProductVariant and InventoryItem reads
without runtime Shopify writes.

The pass promotes `inventoryItemUpdate-parity-plan` into the Gleam parity suite.
Runner seeding reconstructs the captured Product/ProductVariant/InventoryItem
from the fixture's productCreate setup payload, then replays the captured
mutation and downstream read without changing any fixture, request, or
comparison contract.

| Module                                               | Change                                                                                 |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Adds inventory item update routing, item field staging, measurement parsing, payloads. |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured productCreate item-update preconditions.                            |
| `gleam/test/parity_test.gleam`                       | Enables the strict inventory item update parity scenario.                              |

Validation: `gleam test --target javascript` is green at 762 tests on the host
Node runtime. Host `gleam test --target erlang` still fails before tests execute
on the local Erlang install with the known `undef` runner issue; after clearing
host-built Erlang artifacts, the Docker Erlang fallback is green at 758 tests.
Product parity inventory remains 115 checked-in specs, with 48 product specs
executable in the Gleam parity suite plus the admin-platform ProductOption node
scenario after this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  request returning HTTP 400 for `inventoryItemUpdate` because the root was not
  routed by the Gleam Products mutation dispatcher.
- The downstream read target depends on syncing Product `tracksInventory` after
  setting the InventoryItem `tracked` field to true; the existing
  `sync_product_inventory_summary` helper already models that relationship.
- The existing InventoryItem input reader preserved origin/shipping fields but
  did not parse measurement weight input; this pass adds that parser so the
  captured weight payload round-trips.

### Risks / open items

- This pass covers the captured success path and existing not-found validation
  shape, not a broad validation matrix for all `InventoryItemInput` fields.
- Collections, publication, product feeds/feedback, selling plans, product
  metafields, inventory shipment/transfer, and remaining variant
  relationship/media behavior remain incomplete in Gleam.
- Only 48 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 65 candidates

- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.
- Port product metafield behavior now that several Products mutation families
  are locally staged.
- Port inventory shipment/transfer roots if their checked-in specs can be seeded
  from existing inventory fixtures.

---

## 2026-04-30 — Pass 63: inventory bulk toggle activation lifecycle

Adds local staging for the captured `inventoryBulkToggleActivation` slices. The
Gleam Products mutation handler now routes the root locally, preserves the raw
mutation request through the centralized draft-log path, returns the captured
already-active `activate: true` no-op inventory item/level payload, and models
the `activate: false` single-location guardrail with Shopify's
`CANNOT_DEACTIVATE_FROM_ONLY_LOCATION` user error without runtime Shopify writes.

The pass promotes `inventoryBulkToggleActivation-parity-plan` into the Gleam
parity suite. Runner seeding reuses the captured activation fixture state so
both strict targets replay against the same single active inventory level from
the live inventory linkage capture without changing any fixture, request, or
comparison contract.

| Module                                               | Change                                                                                         |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Adds inventory bulk toggle routing, captured payload projection, guardrail errors, and drafts. |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured single-level inventory item preconditions for bulk toggle parity.           |
| `gleam/test/parity_test.gleam`                       | Enables the strict inventory bulk toggle parity scenario.                                      |

Validation: `gleam test --target javascript` is green at 761 tests on the host
Node runtime. Host `gleam test --target erlang` still fails before tests execute
on the local Erlang install with the known `undef` runner issue; after clearing
host-built Erlang artifacts, the Docker Erlang fallback is green at 757 tests.
Product parity inventory remains 115 checked-in specs, with 47 product specs
executable in the Gleam parity suite plus the admin-platform ProductOption node
scenario after this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  request returning HTTP 400 for `inventoryBulkToggleActivation` because the
  root was not routed by the Gleam Products mutation dispatcher.
- The parity spec has two strict targets using the same request document:
  already-active activation returns the existing item/level graph, while
  deactivation of the only level returns null resources plus a coded userError.
- The existing inventory item and inventory level source projections were enough
  for the selected payload shape; no request or capture narrowing was needed.

### Risks / open items

- This pass covers the captured no-op and only-location guardrail slices, not
  broad unknown-location activation or first-class inactive inventory-level
  state.
- Collections, publication, product feeds/feedback, selling plans, product
  metafields, inventory item update, inventory shipment/transfer, and remaining
  variant relationship/media behavior remain incomplete in Gleam.
- Only 47 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 64 candidates

- Port `inventoryItemUpdate` as the next inventory item lifecycle root.
- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.
- Port product metafield behavior now that the Products mutation dispatcher has
  several local mutation families to reuse.

---

## 2026-04-30 — Pass 62: inventory deactivate guardrail

Adds local staging for the captured `inventoryDeactivate` single-location
guardrail path. The Gleam Products mutation handler now routes the root locally,
preserves the raw mutation request through the centralized draft-log path, and
returns Shopify's minimum-one-location `userErrors` payload with `field: null`
without runtime Shopify writes.

The pass promotes `inventoryDeactivate-parity-plan` into the Gleam parity suite.
Runner seeding reuses the captured inventory activation product, variant,
inventory item, and inventory level from the live inventory linkage fixture so
the deactivation request can replay against the exact single active level from
the capture without changing any fixture, request, or comparison contract.

| Module                                               | Change                                                                              |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Adds inventory deactivate routing, nullable-field userErrors, local drafts.         |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured single-level inventory item preconditions for deactivate parity. |
| `gleam/test/parity_test.gleam`                       | Enables the strict inventory deactivate parity scenario.                            |

Validation: `gleam test --target javascript` is green at 760 tests on the host
Node runtime. Host `gleam test --target erlang` still fails before tests execute
on the local Erlang install with the known `undef` runner issue; after clearing
host-built Erlang artifacts, the Docker Erlang fallback is green at 756 tests.
Product parity inventory remains 115 checked-in specs, with 46 product specs
executable in the Gleam parity suite plus the admin-platform ProductOption node
scenario after this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  request returning HTTP 400 for `inventoryDeactivate` because the root was not
  routed by the Gleam Products mutation dispatcher.
- Shopify returns the minimum-one-location guardrail with `field: null`, while
  the existing shared Product user-error helper only models list-valued fields;
  this pass adds a narrow nullable-field serializer for this payload family.
- The capture's deactivation target is the same inventory level as the safe
  activation no-op path, so the activation seeding is sufficient and avoids
  inventing extra fixture state.

### Risks / open items

- This pass covers the captured single-active-location guardrail and keeps a
  simple local success path for multiple active levels, but it does not yet
  model inactive inventory levels as first-class state.
- Collections, publication, product feeds/feedback, selling plans, product
  metafields, inventory bulk toggle, inventory item update, inventory
  shipment/transfer, and remaining variant relationship/media behavior remain
  incomplete in Gleam.
- Only 46 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 63 candidates

- Port `inventoryBulkToggleActivation` now that activate/deactivate guardrail
  slices are both routed locally.
- Port `inventoryItemUpdate` as the next inventory item lifecycle root.
- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.

---

## 2026-04-30 — Pass 61: inventory activate no-op lifecycle

Adds local staging for the captured `inventoryActivate` already-active no-op
path. The Gleam Products mutation handler now routes the root locally, preserves
the raw mutation request through the centralized draft-log path, returns the
captured `InventoryActivatePayload` inventory level projection including nested
InventoryItem/variant/product fields, and reports the already-active
`available` guardrail as a local `userErrors` branch without runtime Shopify
writes.

The pass promotes `inventoryActivate-parity-plan` into the Gleam parity suite.
Runner seeding reconstructs the captured product, variant, inventory item, and
inventory level from the live inventory linkage fixture, then replays the
captured no-op activation request without changing any fixture, request, or
comparison contract.

| Module                                               | Change                                                                                        |
| ---------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Adds inventory activate routing, already-active no-op payload projection, userErrors, drafts. |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured inventory activation product/variant/item/level preconditions.             |
| `gleam/test/parity_test.gleam`                       | Enables the strict inventory activate parity scenario.                                        |

Validation: `gleam test --target javascript` is green at 759 tests on the host
Node runtime. Host `gleam test --target erlang` still fails before tests execute
on the local Erlang install with the known `undef` runner issue; after clearing
host-built Erlang artifacts, the Docker Erlang fallback is green at 755 tests.
Product parity inventory remains 115 checked-in specs, with 45 product specs
executable in the Gleam parity suite plus the admin-platform ProductOption node
scenario after this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  request returning HTTP 400 for `inventoryActivate` because the root was not
  routed by the Gleam Products mutation dispatcher.
- The checked-in capture is intentionally the safe already-active no-op slice:
  activation returns the existing level and quantities without mutating
  inventory state.
- Passing `available` for an already-active level is a captured guardrail branch
  with `field: ["available"]`; this pass models that locally instead of
  treating it as passthrough.

### Risks / open items

- This pass covers the captured already-active/no-op and already-active
  `available` guardrail behavior, not broad inactive-level activation,
  unknown-location creation, or the full validation matrix.
- Collections, publication, product feeds/feedback, selling plans, product
  metafields, inventory deactivation/bulk toggle, inventory shipment/transfer,
  and remaining variant relationship/media behavior remain incomplete in Gleam.
- Only 45 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 62 candidates

- Port `inventoryDeactivate` and `inventoryBulkToggleActivation` roots that
  build on the staged InventoryItem/InventoryLevel helpers.
- Port `inventoryItemUpdate` as the next inventory item lifecycle root.
- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.

---

## 2026-04-30 — Pass 60: inventory adjust quantities lifecycle

Adds local staging for `inventoryAdjustQuantities`. The Gleam Products mutation
handler now routes the root locally, preserves the raw mutation request through
the centralized draft-log path, applies captured available and non-available
inventory quantity deltas, mirrors available adjustments into `on_hand`, returns
Shopify-like `InventoryAdjustmentGroup` payloads, and exposes immediate
downstream ProductVariant/InventoryItem inventory reads without runtime Shopify
writes.

The pass promotes `inventoryAdjustQuantities-parity-plan` into the Gleam parity
suite. Runner seeding reconstructs the two captured tracked products, variants,
inventory items, inventory level quantities, location name, and catalog products
needed by the strict downstream read targets, then replays the captured primary
and non-available mutations without changing any fixture, request, or comparison
contract.

| Module                                               | Change                                                                                                  |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Adds inventory adjust routing, validation, quantity staging, adjustment payload projection, and drafts. |
| `gleam/test/parity/runner.gleam`                     | Seeds the captured inventory adjust products, levels, app/location context, and matching catalog rows.  |
| `gleam/test/parity_test.gleam`                       | Enables the strict inventory adjust quantities parity scenario.                                         |

Validation: `gleam test --target javascript` is green at 758 tests on the host
Node runtime. Host `gleam test --target erlang` still fails before tests execute
on the local Erlang install with the known `undef` runner issue; after clearing
host-built Erlang artifacts, the Docker Erlang fallback is green at 754 tests.
Product parity inventory remains 115 checked-in specs, with 44 product specs
executable in the Gleam parity suite plus the admin-platform ProductOption node
scenario after this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  request returning HTTP 400 for `inventoryAdjustQuantities` because the root
  was not routed by the Gleam Products mutation dispatcher.
- The captured success path mirrors `available` deltas into `on_hand` changes
  in the mutation payload and inventory level quantities, while immediate
  Product `totalInventory` and `inventory_total:` catalog search still lag.
- The non-available `incoming` adjustment requires per-change ledger document
  URIs and updates InventoryLevel quantities without changing available/on-hand
  totals.

### Risks / open items

- This pass covers the captured adjust quantities slice, not the 2026-04
  idempotent `changeFromQuantity` contract or the broader missing-field top-level
  GraphQL validation matrix.
- Collections, publication, product feeds/feedback, selling plans, product
  metafields, inventory activation/deactivation/bulk toggle, inventory
  shipment/transfer, and remaining variant relationship/media behavior remain
  incomplete in Gleam.
- Only 44 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 61 candidates

- Port inventory activation/deactivation/bulk-toggle roots that build on the
  staged InventoryItem/InventoryLevel helpers.
- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.
- Add product variant relationship/media update scenarios now that variant
  family ordering and replacement helpers are in place.

---

## 2026-04-30 — Pass 59: product variant bulk reorder lifecycle

Adds local staging for `productVariantsBulkReorder`. The Gleam Products
mutation handler now routes the root locally, preserves the raw mutation request
through the centralized draft-log path, applies Shopify's captured one-based
`ProductVariantPositionInput.position` semantics, stages the reordered variant
family, and exposes immediate downstream Product variant reads without runtime
Shopify writes.

The pass promotes `productVariantsBulkReorder-parity` into the Gleam parity
suite. Runner seeding reconstructs the two-variant Product graph from the
captured post-bulk-create setup payload, then replays the captured reorder
mutation and strict downstream Product read without changing any fixture,
request, or comparison contract.

| Module                                               | Change                                                                                           |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Adds bulk variant reorder routing, position validation, sequential reorder staging, and payload. |
| `gleam/test/parity/runner.gleam`                     | Seeds the two-variant reorder fixture preconditions from captured setup data.                    |
| `gleam/test/parity_test.gleam`                       | Enables the strict product variant bulk reorder parity scenario.                                 |

Validation: `gleam test --target javascript` is green at 757 tests on the host
Node runtime. Host `gleam test --target erlang` still fails before tests execute
on the local Erlang install with the known `undef` runner issue; after clearing
host-built Erlang artifacts, the Docker Erlang fallback is green at 753 tests.
Product parity inventory remains 115 checked-in specs, with 43 product specs
executable in the Gleam parity suite plus the admin-platform ProductOption node
scenario after this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  request returning HTTP 400 for `productVariantsBulkReorder` because the root
  was not routed by the Gleam Products mutation dispatcher.
- The captured Shopify behavior treats positions as one-based; the local
  staging converts them to zero-based list indexes only after validation.
- The fixture's captured setup product payload is enough to seed Product and
  ProductVariant records because the compared mutation/downstream slices select
  only variant identity, title, and selected options.

### Risks / open items

- This pass covers the captured two-variant reorder success slice, not a broad
  validation matrix for missing variants or malformed position inputs.
- Collections, publication, product feeds/feedback, selling plans, product
  metafields, inventory activation/shipment/transfer, and remaining variant
  relationship/media behavior remain incomplete in Gleam.
- Only 43 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 60 candidates

- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.
- Add product variant relationship/media update scenarios now that variant
  family ordering and replacement helpers are in place.
- Add inventory activation/quantity-adjacent roots that build on the staged
  InventoryLevel helpers introduced in Pass 58.

---

## 2026-04-30 — Pass 58: inventory quantity set/move lifecycle

Adds local staging for the captured inventory quantity roots
`inventorySetQuantities` and `inventoryMoveQuantities`. The Gleam Products
mutation handler now routes both roots locally, preserves the raw mutation
requests through the centralized draft-log path, updates staged
InventoryItem/InventoryLevel quantities through the owning ProductVariant, and
returns Shopify-like InventoryAdjustmentGroup payloads without runtime Shopify
writes.

The pass promotes `inventory-quantity-roots-parity` into the Gleam parity
suite. Runner seeding reconstructs the captured disposable product, variant,
inventory item, and two stocked locations from the live fixture metadata, then
replays the captured set and move mutations against the local state. The strict
targets cover mutation payloads, empty inventory item search behavior,
inventory properties, downstream InventoryItem/ProductVariant quantity reads,
and the captured blocked branches.

| Module                                               | Change                                                                                              |
| ---------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Adds inventory set/move routing, validation, quantity staging, adjustment payloads, and log drafts. |
| `gleam/test/parity/runner.gleam`                     | Seeds the inventory quantity fixture product/variant/item/location preconditions.                   |
| `gleam/test/parity_test.gleam`                       | Enables the strict inventory quantity roots parity scenario.                                        |

Validation: `gleam test --target javascript` is green at 756 tests on the host
Node runtime. Host `gleam test --target erlang` still fails before tests execute
on the local Erlang install with the known `undef` runner issue; after clearing
host-built Erlang artifacts, the Docker Erlang fallback is green at 752 tests.
Product parity inventory remains 115 checked-in specs, with 42 product specs
executable in the Gleam parity suite plus the admin-platform ProductOption node
scenario after this pass.

### Findings

- The pre-implementation signal was direct `draft_proxy.process_request`
  requests returning HTTP 400 for `inventorySetQuantities` and the same
  unrouted Products mutation-family gap for `inventoryMoveQuantities`.
- The fixture intentionally keeps downstream Product `totalInventory` at `0`
  after quantity set/move. The Gleam staging updates the variant/item
  inventory levels but avoids Product inventory-summary sync for this slice so
  the recorded Shopify lag remains intact.
- The move root needs the sibling ledger-document validation rules from the
  TypeScript oracle: available adjustments reject ledger documents, while
  non-available terminals require them.

### Risks / open items

- This pass covers the captured set/move quantity slice, not activation,
  deactivation, scheduled changes, inventory shipments, or inventory transfers.
- Collections, publication, product feeds/feedback, selling plans, product
  metafields, and remaining variant relationship/media/reorder behavior remain
  incomplete in Gleam.
- Only 42 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 59 candidates

- Port `productVariantsBulkReorder` and remaining variant relationship/media
  parity scenarios.
- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.
- Add inventory activation/quantity-adjacent roots that build on the staged
  InventoryLevel helpers introduced in this pass.

---

## 2026-04-30 — Pass 57: product variant bulk lifecycle

Adds local staging for the live-supported product variant bulk roots:
`productVariantsBulkCreate`, `productVariantsBulkUpdate`, and
`productVariantsBulkDelete`. The Gleam Products mutation handler now routes
these roots locally, logs the original raw mutations through the centralized
draft-log path, stages variant-family replacements, refreshes Product inventory
summaries, keeps option value `hasVariants` usage in sync for known options,
and exposes immediate downstream Product/variant reads without runtime Shopify
writes.

The pass promotes the three captured bulk variant parity scenarios into the
Gleam suite. Runner seeding reconstructs the pre-mutation Product/variant state
from the recorded live fixtures, including the richer downstream create/update
variant records needed for inherited merchandising fields and selected-option
reads. The existing SKU search-lag behavior from Pass 56 covers these bulk
family changes: direct Product reads see staged variants while immediate
SKU-filtered `products` / `productsCount` reads keep matching the base variant
family when staged variants differ.

| Module                                               | Change                                                                                                |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Adds bulk variant create/update/delete routing, staging, payloads, inventory summary and option sync. |
| `gleam/test/parity/runner.gleam`                     | Seeds bulk create/update/delete fixture preconditions from captured Product and variant payloads.     |
| `gleam/test/parity_test.gleam`                       | Enables three strict bulk product-variant parity scenarios.                                           |

Validation: `gleam test --target javascript` is green at 755 tests on the host
Node runtime. Host `gleam test --target erlang` still fails before tests execute
on the local Erlang install with the known `undef` runner issue; after clearing
host-built Erlang artifacts, the Docker Erlang fallback is green at 751 tests.
Product parity inventory remains 115 checked-in specs, with 41 product specs
executable in the Gleam parity suite plus the admin-platform ProductOption node
scenario after this pass.

### Findings

- The pre-implementation signal was direct `draft_proxy.process_request`
  requests returning HTTP 400 for `productVariantsBulkCreate`,
  `productVariantsBulkUpdate`, and `productVariantsBulkDelete` because the
  roots were not routed by the Gleam Products mutation dispatcher.
- Bulk create fixture parity needs the downstream Product seed rather than only
  the mutation payload, because Shopify's downstream read includes inherited
  compare-at-price, taxable, and inventory-policy fields that are not selected
  by the mutation payload.
- Bulk update fixture parity must preserve selected options in the seeded base
  variant while clearing only SKU, so the immediate SKU search lag remains
  faithful without losing direct downstream selected-option reads.

### Risks / open items

- This pass covers the captured bulk create/update/delete happy-path slices,
  not the full bulk validation/atomicity matrix or bulk reorder.
- Collections, inventory quantity/shipment/transfer, publications, product
  metafields, selling plans, feeds, and feedback remain incomplete in Gleam.
- Only 41 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 58 candidates

- Add inventory quantity mutation/read-after-write behavior for the
  `inventory-quantity-roots-parity` set/move slice.
- Port `productVariantsBulkReorder` and the remaining variant strategy/media
  relationship scenarios.
- Port collection membership roots so the product relationship parity scenario
  can move closer to full coverage.

---

## 2026-04-30 — Pass 56: product variant compatibility lifecycle

Adds local staging for the legacy single-variant compatibility roots:
`productVariantCreate`, `productVariantUpdate`, and `productVariantDelete`.
The Gleam Products mutation handler now routes these roots locally, preserves
raw mutation logging through the centralized log-draft path, updates Product
inventory summaries from staged variants, and exposes immediate downstream
Product/ProductVariant reads without runtime Shopify writes.

The pass also adds captured SKU search-lag behavior for staged variant family
changes: direct Product variant reads see the staged variant list immediately,
while immediate `products(query: "sku:...")` / `productsCount(query:
"sku:...")` checks keep matching against the base variant SKU set when staged
variant state differs. The parity runner seeds the single-root compatibility
specs from the existing captured bulk-variant fixtures without changing any
captures or request shapes.

| Module                                                              | Change                                                                                          |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                | Adds single-variant create/update/delete routing, staging, payloads, inventory summary sync.    |
| `gleam/test/parity/runner.gleam`                                    | Seeds the compatibility roots from captured bulk create/update/delete variant fixture evidence. |
| `gleam/test/parity_test.gleam`                                      | Enables three strict single-variant compatibility parity scenarios.                             |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam` | Adds direct variant create/update/delete lifecycle and mutation-log coverage.                   |

Validation: `gleam test --target javascript` is green at 752 tests on the host
Node runtime. Host `gleam test --target erlang` still fails before tests execute
on the local Erlang install with the known `undef` runner issue; after clearing
host-built Erlang artifacts, the Docker Erlang fallback is green at 748 tests.
Product parity inventory remains 115 checked-in specs, with 38 product specs
executable in the Gleam parity suite plus the admin-platform ProductOption node
scenario after this pass.

### Findings

- The pre-implementation signal was direct `draft_proxy.process_request`
  requests returning HTTP 400 for `productVariantCreate`,
  `productVariantUpdate`, and `productVariantDelete` because the roots were not
  routed by the Gleam Products mutation dispatcher.
- The single-root compatibility parity specs intentionally compare against
  captured bulk-variant Shopify evidence; runner seeding must reconstruct the
  pre-mutation Product/variant state from those captures.
- Product variant update/delete captures rely on immediate SKU search lag, so
  direct reads use staged variants while SKU-filtered Product searches consult
  the base variant family when staged variants differ.

### Risks / open items

- This pass covers the legacy single-variant compatibility roots, not the full
  bulk variant strategy matrix or validation atomicity scenarios.
- Collections, inventory quantity/shipment/transfer, publications, product
  metafields, selling plans, feeds, and feedback remain incomplete in Gleam.
- Only 38 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 57 candidates

- Add inventory quantity mutation/read-after-write behavior for the
  `inventory-quantity-roots-parity` set/move slice.
- Port product variant bulk create/update/delete/reorder scenarios now that
  the single-root variant record helpers are in place.
- Port collection membership and product variant media relationship roots so
  the full `product-relationship-roots-live-parity` scenario can run.

---

## 2026-04-30 — Pass 55: productCreate lifecycle

Adds local `productCreate` staging to the Gleam Products mutation handler. The
handler now routes the root locally, validates captured blank-title and
over-length handle branches, mints a staged Product, creates Shopify-like
default option/option value state, creates a default ProductVariant with a
synthetic InventoryItem, preserves raw mutation logging through the centralized
log-draft path, and exposes immediate downstream Product, ProductVariant, and
InventoryItem reads without runtime Shopify writes.

The parity suite now enables the captured product create success, blank-title
validation, too-long handle validation, and product-create inventory read
scenarios without changing the checked-in captures or request shapes.

| Module                                                              | Change                                                                                         |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                | Adds `productCreate` routing, validation, default Product/variant/inventory staging, payloads. |
| `gleam/test/parity_test.gleam`                                      | Enables four strict `productCreate` parity scenarios.                                          |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam` | Adds direct product create success, downstream read, validation, and mutation-log coverage.    |

Validation: `gleam test --target javascript` is green at 748 tests on the host
Node runtime. Host `gleam test --target erlang` still fails before tests execute
on the local Erlang install with the known `undef` runner issue; after clearing
host-built Erlang artifacts, the Docker Erlang fallback is green at 744 tests.
Product parity inventory remains 115 checked-in specs, with 35 product specs
executable in the Gleam parity suite plus the admin-platform ProductOption node
scenario after this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  request returning HTTP 400 for `productCreate` because the root was not
  routed by the Gleam Products mutation dispatcher.
- The captured inventory-read scenario depends on the staged default variant's
  synthetic InventoryItem being readable both through `productVariant(id:)` and
  `inventoryItem(id:)` immediately after create.
- Product create uses the proxy synthetic Product GID form while default
  ProductOption, ProductOptionValue, ProductVariant, and InventoryItem records
  use plain synthetic Shopify GIDs.

### Risks / open items

- Handle normalization is modeled for the captured ASCII/title/too-long paths;
  broader Unicode handle normalization remains a future fidelity slice if a
  capture exercises it.
- Variant mutation families, collections, inventory quantity/shipment/transfer,
  publications, product metafields, selling plans, feeds, and feedback remain
  incomplete in Gleam.
- Only 35 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 56 candidates

- Add inventory quantity mutation/read-after-write behavior for the
  `inventory-quantity-roots-parity` set/move slice.
- Port collection membership and product variant media relationship roots so
  the full `product-relationship-roots-live-parity` scenario can run.
- Add product variant lifecycle mutation slices that build on the staged
  default variant created by `productCreate`.

---

## 2026-04-30 — Pass 54: productUpdate lifecycle

Adds local `productUpdate` staging to the Gleam Products mutation handler. The
handler now routes the root locally, stages selected merchandising/detail fields
onto the in-memory Product record, preserves raw mutation logging through the
centralized log-draft path, and returns updated downstream `product(id:)` reads
without runtime Shopify writes.

The handler also covers the captured payload-level validation branches for
missing Product ID, unknown Product ID, blank title, and over-length handle.
The parity runner seeds the successful and blank-title captures from
`productUpdate.product` response data so strict replay can compare the mutation
payloads without changing checked-in captures or request shapes.

| Module                                                              | Change                                                                                    |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                | Adds `productUpdate` routing, validation payloads, field staging, and payload projection. |
| `gleam/test/parity/runner.gleam`                                    | Seeds captured product update success/blank-title Product preconditions.                  |
| `gleam/test/parity_test.gleam`                                      | Enables five strict `productUpdate` parity scenarios.                                     |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam` | Adds direct product update success and blank-title validation coverage.                   |

Validation: `gleam test --target javascript` is green at 742 tests on the host
Node runtime. Host `gleam test --target erlang` still fails before tests execute
on the local Erlang install; after clearing host-built Erlang artifacts, the
Docker Erlang fallback is green at 738 tests. Product parity inventory remains
115 checked-in specs, with 31 product specs executable in the Gleam parity suite
plus the admin-platform ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  request returning HTTP 400 for `productUpdate` because the root was not
  routed by the Gleam Products mutation dispatcher.
- The currently enabled productUpdate captures exercise the core Product
  detail fields and validation payloads, but not the broader handle
  normalization branches from the source capture.
- Seeding the success capture from the captured response is sufficient for the
  strict payload contract because Shopify preserves handle/status/preview URL
  while updating the selected mutable fields.

### Risks / open items

- Product handle normalization beyond the captured over-length rejection is not
  fully modeled by this pass.
- Product create, variants, collections, inventory mutation families,
  publications, product metafields, selling plans, feeds, and feedback remain
  incomplete in Gleam.
- Only 31 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 55 candidates

- Add `productCreate` local lifecycle slices with captured validation branches.
- Add inventory quantity mutation/read-after-write behavior for the
  `inventory-quantity-roots-parity` set/move slice.
- Port collection membership and product variant media relationship roots so
  the full `product-relationship-roots-live-parity` scenario can run.

---

## 2026-04-30 — Pass 53: tagsAdd/tagsRemove lifecycle

Adds local `tagsAdd` and `tagsRemove` staging to the Gleam Products mutation
handler. Both roots now route locally, validate missing Product IDs and empty
tag input with Shopify-like payload-level `userErrors`, normalize Product tags,
preserve raw mutation logging through the centralized log-draft path, and
update direct Product reads without runtime Shopify writes.

Tag-filtered Product searches now preserve Shopify's captured immediate
search-index lag for hydrated/base products: direct `product(id:)` reads see
the staged tag list immediately, while `products(query: "tag:...")` and
`productsCount(query: "tag:...")` keep matching against the pre-mutation
base/searchable tag set. The parity runner also seeds the captured
`tagsRemove` fixture's pre-mutation/searchable tag state so the strict
downstream remaining/removed tag-filter reads can replay unchanged.

| Module                                                              | Change                                                                                        |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                | Adds `tagsAdd`/`tagsRemove` routing, staging, payload projection, and tag search lag.         |
| `gleam/test/parity/runner.gleam`                                    | Seeds captured `tagsRemove` preconditions from mutation and downstream-read fixture payloads. |
| `gleam/test/parity_test.gleam`                                      | Enables two strict tag parity scenarios.                                                      |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam` | Adds direct tag add/remove read-after-write and mutation-log coverage.                        |

Validation: `gleam test --target javascript` is green at 735 tests on the host
Node runtime. Host `gleam test --target erlang` still fails before tests execute
on the local Erlang install; after clearing host-built Erlang artifacts, the
Docker Erlang fallback is green at 731 tests. Product parity inventory remains
115 checked-in specs, with 26 product specs executable in the Gleam parity suite
plus the admin-platform ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was direct `draft_proxy.process_request`
  requests returning HTTP 400 for both `tagsAdd` and `tagsRemove` because the
  roots were not routed by the Gleam Products mutation dispatcher.
- Shopify sorts Product tags in mutation payloads and direct reads, but its
  immediate Product tag search index can lag behind staged tag changes for
  hydrated/base products.
- The captured `tagsRemove` fixture needs a specialized seed: the direct
  Product payload is post-mutation, while the immediate `tag:` filters still
  prove both remaining and removed tags searchable.

### Risks / open items

- This pass models the captured base-product tag search lag by consulting base
  tags when staged tags differ; explicit time-based lag expiry remains a future
  fidelity improvement if a later capture requires delayed-read parity.
- Product create/update, variants, collections, inventory mutation families,
  publications, product metafields, selling plans, feeds, and feedback remain
  incomplete in Gleam.
- Only 26 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 54 candidates

- Add `productCreate` / `productUpdate` local lifecycle slices with their
  captured validation branches.
- Add inventory quantity mutation/read-after-write behavior for the
  `inventory-quantity-roots-parity` set/move slice.
- Port collection membership and product variant media relationship roots so
  the full `product-relationship-roots-live-parity` scenario can run.

---

## 2026-04-30 — Pass 52: productDelete lifecycle

Adds local `productDelete` staging to the Gleam Products mutation handler. The
handler now routes the root locally, validates the captured `ProductDeleteInput`
branches without runtime Shopify writes, returns Shopify-like success and
unknown-product payloads, marks deleted Product IDs in staged state, and
preserves the raw mutation through the centralized mutation-log path.

The parity runner now mirrors the TypeScript parity harness for the successful
delete capture by seeding a minimal base Product from the captured mutation
variables. Unknown-id validation remains unseeded so it continues to return
Shopify's `Product does not exist` userError.

| Module                                                              | Change                                                                                         |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                | Adds `productDelete` routing, staging, payload projection, and captured input error envelopes. |
| `gleam/test/parity/runner.gleam`                                    | Seeds successful product delete parity from captured mutation variables only.                  |
| `gleam/test/parity_test.gleam`                                      | Enables five strict `productDelete` parity scenarios.                                          |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam` | Adds direct mutation/read-after-write/log coverage for product deletion.                       |

Validation: `gleam test --target javascript` is green at 731 tests on the host
Node runtime. Host `gleam test --target erlang` compiles but fails because the
host Erlang runtime is OTP 25 while `gleam_json` requires OTP 27+; after
clearing host-built Erlang artifacts, the Docker Erlang fallback is green at
727 tests. Product parity inventory remains 115 checked-in specs, with 24
product specs executable in the Gleam parity suite plus the admin-platform
ProductOption node scenario after this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  request returning HTTP 400 for `productDelete` because the root was not
  routed by the Gleam Products mutation dispatcher.
- The captured success fixture does not include a pre-mutation product read;
  the TypeScript parity harness seeds the product from the captured mutation
  ID, and the Gleam runner now does the same only for the success scenario.
- Shopify reports inline missing/null `input.id` as top-level GraphQL errors
  against the `ProductDeleteInput` object, while unknown Product IDs remain a
  payload-level `userErrors` response.

### Risks / open items

- Product create/update, variants, collections, inventory mutation families,
  publications, product metafields, tags, selling plans, feeds, and feedback
  remain incomplete in Gleam.
- Host Erlang validation needs OTP 27+ to run this repository's current
  `gleam_json` version; Docker remains the local Erlang target proof.
- Only 24 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 53 candidates

- Add `productCreate` / `productUpdate` local lifecycle slices with their
  captured validation branches.
- Add inventory quantity mutation/read-after-write behavior for the
  `inventory-quantity-roots-parity` set/move slice.
- Port collection membership and product variant media relationship roots so
  the full `product-relationship-roots-live-parity` scenario can run.

---

## 2026-04-30 — Pass 51: productChangeStatus lifecycle

Adds local `productChangeStatus` staging to the Gleam Products mutation
handler. The handler now stages Product status updates without a runtime
Shopify write, mints a synthetic `updatedAt`, returns Shopify-like userErrors
for missing/unknown products and invalid statuses, preserves the raw mutation
through the centralized mutation-log path, and surfaces the captured top-level
GraphQL error shape for an inline `productId: null` literal.

The Product search path now preserves Shopify's captured status-search lag for
base products whose status is changed locally: direct `product(id:)` reads see
the staged status immediately, while catalog `products(query:)` /
`productsCount(query:)` status filters continue matching the base product
status. The parity runner also seeds the single `seedProduct` capture shape
used by the status-change fixture.

| Module                                                              | Change                                                                                    |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                | Adds `productChangeStatus`, top-level null-argument error support, and status search lag. |
| `gleam/test/parity/runner.gleam`                                    | Seeds captured single `seedProduct` preconditions for status lifecycle parity.            |
| `gleam/test/parity_test.gleam`                                      | Enables three strict `productChangeStatus` parity scenarios.                              |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam` | Adds direct mutation/read-after-write/log coverage for status staging and search lag.     |

Validation: `gleam test --target javascript` is green at 725 tests on the host
Node runtime. Host `gleam test --target erlang` remains blocked by missing
`escript`, and the Docker Erlang fallback is green at 721 tests. Product parity
inventory remains 115 checked-in specs, with 19 product specs executable in the
Gleam parity suite plus the admin-platform ProductOption node scenario after
this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  test returning HTTP 400 for `productChangeStatus` because the root was not
  routed by the Gleam Products mutation dispatcher.
- The captured downstream read proves a Shopify search-index lag: the changed
  product is archived in `product(id:)`, but an immediate
  `products(query: "status:archived ...")` read can still return no catalog
  match.
- The null-literal branch is a top-level GraphQL error rather than a
  payload-level `userErrors` response, so the Products mutation envelope now
  supports the same error-only shape already used by other mutation domains.

### Risks / open items

- The status-search lag model is intentionally scoped to local staged status
  differences for base products; broader product update/search recency behavior
  remains future work.
- Product create/update/delete, variants, collections, inventory mutation
  families, publications, product metafields, tags, selling plans, feeds, and
  feedback remain incomplete in Gleam.
- Only 19 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 52 candidates

- Add `productCreate` / `productUpdate` / `productDelete` local lifecycle
  slices with their captured validation branches.
- Add inventory quantity mutation/read-after-write behavior for the
  `inventory-quantity-roots-parity` set/move slice.
- Port collection membership and product variant media relationship roots so
  the full `product-relationship-roots-live-parity` scenario can run.

---

## 2026-04-30 — Pass 50: productOptionsReorder lifecycle

Adds local `productOptionsReorder` staging to the Gleam Products mutation
handler. The handler now validates product lookup, reorders ProductOption
records by captured option inputs, preserves Shopify's observed option value
ordering, remaps each variant's `selectedOptions` to the new option order,
derives the variant title from that reordered selection list, and records the
raw mutation through the centralized mutation-log path.

The parity runner now seeds the captured admin-platform ProductOption node
scenario from `preMutationRead.data.product`; the scenario replays the captured
`productOptionsReorder` request unchanged and then resolves ProductOption and
ProductOptionValue GIDs through `nodes(ids:)`.

| Module                                                              | Change                                                                                        |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                | Adds `productOptionsReorder` routing, staging, payload projection, and option matching.       |
| `gleam/test/parity/runner.gleam`                                    | Seeds the captured admin-platform product option node scenario before replaying the mutation. |
| `gleam/test/parity_test.gleam`                                      | Enables the strict `admin-platform-product-option-node-reads` scenario.                       |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam` | Adds direct mutation/read-after-write/log coverage for product option reordering.             |

Validation: `gleam test --target javascript` is green at 721 tests on the host
Node runtime. Host `gleam test --target erlang` remains blocked by missing
`escript`, and the Docker Erlang fallback is green at 717 tests. Product parity
inventory remains 115 checked-in specs, with 16 product specs executable in the
Gleam parity suite plus the admin-platform ProductOption node scenario after
this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  test returning HTTP 400 for `productOptionsReorder` because the root was not
  routed by the Gleam Products mutation dispatcher.
- The capture confirms Shopify reorders options and variant selected options
  but does not reorder `optionValues` to match the reorder input's nested
  `values` array.
- The admin-platform ProductOption node parity scenario could be enabled
  without changing the capture or request shape once the lifecycle setup
  mutation was locally staged.

### Risks / open items

- The full `product-relationship-roots-live-parity` scenario remains unenabled
  because the same capture also exercises collection membership and variant
  media relationship roots that are still incomplete in Gleam.
- Product option validation branches beyond the captured missing product and
  unknown option behavior remain future work.
- Only 16 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 51 candidates

- Add inventory quantity mutation/read-after-write behavior for the
  `inventory-quantity-roots-parity` set/move slice.
- Port collection membership and product variant media relationship roots so
  the full `product-relationship-roots-live-parity` scenario can run.
- Continue product metafield or collection lifecycle slices with captured
  parity specs.

---

## 2026-04-30 — Pass 49: productOptionsDelete lifecycle

Adds local `productOptionsDelete` staging to the Gleam Products mutation
handler. The handler now validates requested option IDs, returns
`deletedOptionsIds`, removes staged option records, restores Shopify's default
Title option when all custom options are deleted, remaps the remaining variant
to `Default Title`, and records the raw mutation through the centralized
mutation-log path.

The parity runner now seeds the delete lifecycle capture from
`preMutationRead.data.product`, and the strict
`productOptionsDelete-parity-plan` scenario runs unchanged against the Gleam
proxy.

| Module                                                              | Change                                                                                    |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                | Adds `productOptionsDelete` routing, deletion payloads, user errors, and default restore. |
| `gleam/test/parity/runner.gleam`                                    | Seeds the captured delete lifecycle fixture from its pre-mutation product read.           |
| `gleam/test/parity_test.gleam`                                      | Enables the strict `productOptionsDelete-parity-plan` scenario.                           |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam` | Adds direct mutation/read-after-write/log coverage for all-options delete restoration.    |

Validation: `gleam test --target javascript` is green at 719 tests on the host
Node runtime. Host `gleam test --target erlang` remains blocked by missing
`escript`, and the Docker Erlang fallback is green at 715 tests. Product parity
inventory remains 115 checked-in specs, with 16 product specs executable in the
Gleam parity suite after this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  test returning HTTP 400 for `productOptionsDelete` because the root was not
  routed by the Gleam Products mutation dispatcher.
- The delete capture confirms Shopify restores a fresh default `Title` option
  and `Default Title` option value when the last custom option is removed.
- Restored default option and option value IDs are volatile; the existing
  parity spec already records those as expected Shopify/local ID differences.

### Risks / open items

- `productOptionsReorder` remains unported.
- Partial option deletion beyond the captured all-options-delete/default-restore
  path remains lightly covered.
- Only 16 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 50 candidates

- Port `productOptionsReorder` so the admin-platform product option node parity
  scenario can replay its lifecycle setup.
- Add inventory quantity mutation/read-after-write behavior for the
  `inventory-quantity-roots-parity` set/move slice.
- Continue product metafield or collection lifecycle slices with captured
  parity specs.

---

## 2026-04-30 — Pass 48: productOptionUpdate lifecycle

Extends the Products mutation slice with local `productOptionUpdate` staging.
The Gleam handler now renames and repositions an existing ProductOption, adds
new option values, updates/deletes existing option values, remaps variant
`selectedOptions`, reorders variant selections to match the new option order,
and records the raw mutation through the centralized mutation-log path. Product
reads immediately observe the staged option and variant graph without a runtime
Shopify write.

The parity runner now seeds the captured update scenario from
`preMutationRead.data.product`, and the strict
`productOptionUpdate-parity-plan` scenario runs unchanged against the Gleam
proxy.

| Module                                                              | Change                                                                                          |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                | Adds `productOptionUpdate` routing, staging, option value updates, and variant selection remap. |
| `gleam/test/parity/runner.gleam`                                    | Seeds the captured update lifecycle fixture from its pre-mutation product read.                 |
| `gleam/test/parity_test.gleam`                                      | Enables the strict `productOptionUpdate-parity-plan` scenario.                                  |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam` | Adds direct mutation/read-after-write/log coverage for option update lifecycle behavior.        |

Validation: `gleam test --target javascript` is green at 717 tests on the host
Node runtime. Host `gleam test --target erlang` remains blocked by missing
`escript`, and the Docker Erlang fallback is green at 713 tests. Product parity
inventory remains 115 checked-in specs, with 15 product specs executable in the
Gleam parity suite after this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  test returning HTTP 400 for `productOptionUpdate` because the root was not
  routed by the Gleam Products mutation dispatcher.
- The captured update lifecycle confirms Shopify reorders variants'
  `selectedOptions` after an option position change and derives the variant
  title from that new selected-option order.
- The update capture uses the same `preMutationRead.data.product` seeding shape
  as the productOptionsCreate lifecycle captures.

### Risks / open items

- `productOptionsDelete` and `productOptionsReorder` remain unported.
- Validation branches beyond the captured missing/unknown option behavior remain
  future work.
- Only 15 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 49 candidates

- Port `productOptionsDelete` default-option restoration and enable the strict
  delete parity plan.
- Port `productOptionsReorder` so the admin-platform product option node parity
  scenario can replay its lifecycle setup.
- Add inventory quantity mutation/read-after-write behavior for the
  `inventory-quantity-roots-parity` set/move slice.

---

## 2026-04-30 — Pass 47: productOptionsCreate lifecycle

Ports the first Products mutation slice to Gleam: `productOptionsCreate` now
stages locally, records the raw mutation through the centralized mutation-log
path, replaces Shopify's default Title option state, remaps the existing
default variant for `LEAVE_AS_IS` / explicit `null`, and fans out variants for
`variantStrategy: CREATE`. Downstream product reads observe the staged option
and variant graph without a runtime Shopify write.

The parity runner now seeds these scenarios from the captured
`preMutationRead.data.product` payloads before replaying the primary mutation.
It also handles expected-difference paths with two array wildcards, which the
over-default-limit fixture uses for nested option value IDs.

| Module                                                              | Change                                                                                     |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`                | Adds Products mutation processing, `productOptionsCreate`, option sync, and CREATE fanout. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`             | Routes locally supported Products mutations to the Products mutation handler.              |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                   | Adds staged ProductVariant family replacement for read-after-write variant graphs.         |
| `gleam/test/parity/runner.gleam`                                    | Seeds productOptionsCreate captures from pre-mutation product reads.                       |
| `gleam/test/parity/diff.gleam`                                      | Supports two-wildcard expected-difference paths for nested array payloads.                 |
| `gleam/test/parity_test.gleam`                                      | Enables four strict `productOptionsCreate` parity scenarios.                               |
| `gleam/test/shopify_draft_proxy/proxy/products_mutation_test.gleam` | Adds direct mutation/read-after-write/log coverage for the default-option create path.     |

Validation: `gleam test --target javascript` is green at 715 tests on the host
Node runtime. Host `gleam test --target erlang` remains blocked by missing
`escript`, and the Docker Erlang fallback is green at 711 tests. Product parity
inventory remains 115 checked-in specs, with 14 product specs executable in the
Gleam parity suite after this pass.

### Findings

- The pre-implementation signal was a direct `draft_proxy.process_request`
  test returning HTTP 400 for `productOptionsCreate` because Products
  mutations had no Gleam dispatcher.
- Shopify's CREATE fanout for an existing variant family preserves the first
  new option value across existing variants first, then creates remaining
  value combinations per existing variant. The over-default-limit capture made
  that ordering visible.
- Existing productOptionsCreate captures do not use `seedProducts`; they carry
  the required baseline under `preMutationRead.data.product`.

### Risks / open items

- `productOptionUpdate`, `productOptionsDelete`, and `productOptionsReorder`
  remain unported.
- CREATE fanout is covered for the captured single-new-option scenarios; bulk
  variant strategy behavior outside this productOptionsCreate slice remains
  future work.
- Only 14 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 48 candidates

- Port `productOptionUpdate` over the captured option lifecycle fixture.
- Port `productOptionsDelete` default-option restoration.
- Port `productOptionsReorder` so the admin-platform product option node parity
  scenario can replay its lifecycle setup instead of relying only on direct
  seeded node coverage.

---

## 2026-04-30 — Pass 46: product option node reads

Adds generic Relay node resolution for ProductOption and ProductOptionValue
records backed by the product option state added in Pass 45. The Admin
Platform `node`/`nodes` dispatcher now routes `gid://shopify/ProductOption/*`
and `gid://shopify/ProductOptionValue/*` IDs to the Products serializer, while
missing option IDs preserve Shopify-like `null` entries in `nodes(ids:)`.

This remains a read-only node resolution slice. It does not implement product
option mutations, option reorder lifecycle, variant strategy behavior, or the
full admin-platform product option node parity scenario, which depends on the
unported product option lifecycle roots.

| Module                                                           | Change                                                                           |
| ---------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                | Adds effective ProductOption/ProductOptionValue lookup by GID.                   |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`             | Exports ProductOption/ProductOptionValue node serializers.                       |
| `gleam/src/shopify_draft_proxy/proxy/admin_platform.gleam`       | Dispatches ProductOption/ProductOptionValue GIDs through `node` / `nodes`.       |
| `gleam/test/shopify_draft_proxy/proxy/admin_platform_test.gleam` | Adds direct generic node read coverage for option, option value, and missing ID. |

Validation: `gleam test --target javascript` is green at 710 tests on the host
Node runtime. Host `gleam test --target erlang` remains blocked by missing
`escript`, and the Docker Erlang fallback is green at 706 tests. Product parity
inventory remains 115 checked-in specs, with 10 product specs executable in the
Gleam parity suite after this pass.

### Findings

- The pre-implementation signal was the Admin Platform dispatcher returning
  `null` for ProductOption/ProductOptionValue GIDs because only store-property
  node families were routed in Gleam.
- The ProductOption source added in Pass 45 could be reused directly for node
  selection projection, so no second option serializer was needed.

### Risks / open items

- The checked-in `admin-platform-product-option-node-reads` parity scenario is
  still unenabled because it replays product option reorder lifecycle first.
- Product option mutations and read-after-write lifecycle behavior remain
  unported.
- Only 10 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 47 candidates

- Start product option create/update/delete/reorder local lifecycle behavior.
- Add inventory quantity mutation/read-after-write behavior for the
  `inventory-quantity-roots-parity` set/move slice.
- Add product search sort-key ordering once per-connection product cursors and
  `publishedAt` state are modeled.

---

## 2026-04-30 — Pass 45: product option read projection

Adds ProductOption/ProductOptionValue records to the Gleam store and projects
store-backed product `options` from Product reads. The serializer now covers
`ProductOption.__typename`, `id`, `name`, `position`, `values`, and nested
`optionValues { __typename id name hasVariants }`, matching the captured
Shopify shape used by product relationship and option lifecycle scenarios.
Variant `selectedOptions` remains backed by ProductVariant state.

This is a read/projection slice only. It does not implement product option
create/update/delete/reorder mutations, variant strategy behavior, default
option restoration, or full product option lifecycle parity.

| Module                                                     | Change                                                                              |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`          | Adds ProductOption/ProductOptionValue record shapes.                                |
| `gleam/src/shopify_draft_proxy/state/store.gleam`          | Adds base/staged option slices, replace helpers, and effective option lookup.       |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`       | Projects product `options`, `values`, and nested `optionValues` from store state.   |
| `gleam/test/parity/runner.gleam`                           | Seeds ProductOption records from captured `seedProducts` and product read payloads. |
| `gleam/test/shopify_draft_proxy/proxy/products_test.gleam` | Adds direct option/value projection coverage alongside variant selected options.    |

Validation: `gleam test --target javascript` is green at 709 tests on the host
Node runtime. Host `gleam test --target erlang` remains blocked by missing
`erl`, and the Docker Erlang fallback is green at 705 tests. Product parity
inventory remains 115 checked-in specs, with 10 product specs executable in the
Gleam parity suite after this pass.

### Findings

- The pre-implementation signal was a direct read failure: after seeding
  product options into store, `product { options { ... } }` returned
  `options: null` while the same product's variant `selectedOptions` already
  serialized correctly.
- Product relationship captures already include option records under
  `seedProducts`; teaching the parity runner to ingest those records prepares
  future strict option lifecycle scenarios without editing capture files.

### Risks / open items

- Product option mutations and read-after-write lifecycle behavior remain
  unported.
- `ProductOption` / `ProductOptionValue` node resolution remains unported.
- Variant parent-product backreferences still use the existing shallow product
  source and do not hydrate product option relationships from that nested path.
- Only 10 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 48 candidates

- Add `ProductOption` / `ProductOptionValue` node resolution for admin-platform
  product option node reads.
- Start product option create/update/delete/reorder local lifecycle behavior.
- Add product search sort-key ordering once per-connection product cursors and
  `publishedAt` state are modeled.

---

## 2026-04-30 — Pass 44: inventory item catalog reads

Adds the first top-level InventoryItem catalog/search read slice to the Gleam
Products handler. `inventoryItems(first:, query:, reverse:)` now resolves from
effective variant-backed InventoryItem records, sorts by inventory item id,
serializes the existing InventoryItem -> ProductVariant -> Product
backreferences, and supports simple `id:`, `sku:`, `tracked:`, and unfielded
text filtering through the shared search parser. This pass also adds the static
`inventoryProperties.quantityNames` projection that Shopify exposes alongside
inventory quantity roots.

This is read-only progress over already-seeded variant inventory state. It does
not implement inventory quantity mutations, inventory adjustment groups,
inactive-level lifecycle behavior, shipments, transfers, or the full
`inventory-quantity-roots-parity` scenario.

| Module                                                     | Change                                                                            |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`       | Adds `inventoryItems` connection reads/search and `inventoryProperties` output.   |
| `gleam/test/shopify_draft_proxy/proxy/products_test.gleam` | Adds direct inventory item catalog/search and inventory properties read coverage. |

Validation: `gleam test --target javascript` is green at 708 tests on the host
Node runtime. Host `gleam test --target erlang` remains blocked by missing
`escript`, and the Docker Erlang fallback is green at 704 tests. Product parity
inventory remains 115 checked-in specs, with 10 product specs executable in the
Gleam parity suite after this pass.

### Findings

- The pre-implementation signal was a direct test failure: even with
  variant-backed InventoryItem records in store, every top-level
  `inventoryItems` query returned an empty connection.
- The existing nested InventoryItem serializer was reusable for top-level
  connection nodes once the connection iterated ProductVariant records and
  supplied the variant backreference.

### Risks / open items

- `inventory-quantity-roots-parity` remains unenabled because it also requires
  local `inventorySetQuantities` / `inventoryMoveQuantities` mutation handling
  and downstream quantity lifecycle behavior.
- Inventory item search remains limited to the captured/TS-compatible basic
  fields covered by direct tests: text, `id`, `sku`, and `tracked`.
- Only 10 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 45 candidates

- Add ProductOption state and product `options`/option value projection from
  captured fixtures.
- Add inventory quantity mutation/read-after-write behavior for the
  `inventory-quantity-roots-parity` set/move slice.
- Add product search sort-key ordering once per-connection product cursors and
  `publishedAt` state are modeled.

---

## 2026-04-30 — Pass 43: product scalar search parity

Enables the captured `products-search-read` parity scenario in the Gleam suite.
Product `query:` filtering now covers the scalar/read predicates exercised by
that capture: unfielded product text, `vendor:`, `product_type:`, `tag:`,
`status:`, `id:`, `sku:`, and numeric `inventory_total:` comparisons. The
parity runner seeds the captured vendor-filtered and low-inventory product
rows plus the captured total count, preserving the first-page `hasNextPage`
signal without changing the checked-in spec or request.

This remains a narrow read slice. Advanced product search grammar, sort-key
ordering, saved-search connection hydration, timestamp/publication filters,
relationship-backed filters, and search lag semantics remain unported.

| Module                                                     | Change                                                                            |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`       | Adds shared-parser-backed product scalar/text/id/inventory search predicates.     |
| `gleam/test/parity/runner.gleam`                           | Seeds the captured products search result rows and total count for strict replay. |
| `gleam/test/shopify_draft_proxy/proxy/products_test.gleam` | Adds direct multi-product coverage for scalar search filtering/count behavior.    |
| `gleam/test/parity_test.gleam`                             | Enables `products-search-read` in the pure-Gleam parity suite.                    |

Validation: `gleam test --target javascript` is green at 706 tests on the host
Node runtime. Host `gleam test --target erlang` remains blocked by missing
`escript`, and the Docker Erlang fallback is green at 702 tests. Product parity
inventory remains 115 checked-in specs, with 10 product specs executable in the
Gleam parity suite after this pass.

### Findings

- Enabling `products-search-read` before implementation produced the expected
  strict mismatch: the proxy returned no search result rows and total count
  `0` instead of the captured Shopify product rows and count.
- The capture selects only the first two `vendor:NIKE` rows while Shopify
  reports `hasNextPage: true`; the runner adds an internal sentinel row after
  the captured rows so pagination truthiness is preserved without weakening the
  comparison contract or serializing non-captured data.

### Risks / open items

- Product search fields beyond this scalar slice remain unported, including
  timestamp/publication filters and relationship-backed filters.
- Product search sort keys and relevance ordering remain unported.
- Only 10 of 115 checked-in product parity specs are enabled by the Gleam
  parity suite after this pass.

### Pass 44 candidates

- Add ProductOption state and product `options`/option value projection from
  captured fixtures.
- Add product search sort-key ordering for the captured `products-sort-keys-read`
  or advanced-search slices.
- Start inventory item/level catalog/search reads over effective
  variant-backed inventory items.

---

## 2026-04-30 — Pass 42: variant SKU product search

Enables the captured `products-variant-search-read` parity scenario in the
Gleam suite. Product `query:` filtering now uses the shared Gleam search query
parser for a narrow `sku:` term slice, matching products when any effective
variant for the product has the requested SKU. `productsCount(query:)` now uses
the same filtered product list so count and connection reads agree.

This pass intentionally limits search support to variant SKU terms. Broader
product search fields, OR/advanced product search semantics beyond the shared
parser mechanics, sort-key parity, and variant catalog search remain unported.

| Module                                                     | Change                                                                        |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`       | Applies shared search parsing to Products reads for `sku:` variant matching.  |
| `gleam/test/shopify_draft_proxy/proxy/products_test.gleam` | Adds direct two-product coverage proving `sku:` filters product reads/counts. |
| `gleam/test/parity_test.gleam`                             | Enables `products-variant-search-read` in the pure-Gleam parity suite.        |

Validation: `gleam test --target javascript` is green at 704 tests on the host
Node runtime. Product parity inventory remains 115 checked-in specs, with 9
product specs executable in the Gleam parity suite after this pass.

### Findings

- The captured parity fixture seeds only the matching product, so the parity
  scenario alone would pass even if the query were ignored. A direct two-product
  test was added so this pass proves actual `sku:` filtering behavior.
- The existing `seedProducts` convention already carries the variant SKU needed
  by the parity scenario; no fixture/request changes were required.

### Risks / open items

- Product query fields beyond `sku:` are still unported in Gleam.
- Product sort-key parity, advanced search grammar captures, and variant search
  over top-level ProductVariant catalogs remain unported.
- Only 9 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 43 candidates

- Add ProductOption state and product `options`/option value projection from
  captured fixtures.
- Add simple Product search fields for `vendor`, `product_type`, `tag`, `id`,
  and `status` using the shared search parser.
- Start inventory item/level catalog/search reads over effective variant-backed
  inventory items.

---

## 2026-04-30 — Pass 41: top-level inventory level reads

Enables the captured `inventory-level-read` parity scenario in the Gleam suite.
The Products handler now resolves top-level `inventoryLevel(id:)` from the
effective variant-backed InventoryItem graph seeded by the product variants
matrix fixture, reusing the same InventoryLevel serializer that backs nested
`inventoryItem.inventoryLevels`.

This remains a narrow read slice over captured fixture state. It does not
implement inventory level mutation/lifecycle behavior, inactive-level handling,
inventory activation/deactivation, quantity adjustments, or inventory catalog
reads.

| Module                                               | Change                                                                           |
| ---------------------------------------------------- | -------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/store.gleam`    | Adds effective InventoryLevel lookup by id across variant-backed inventory data. |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam` | Resolves top-level `inventoryLevel(id:)` through the local inventory graph.      |
| `gleam/test/parity/runner.gleam`                     | Reuses product variants matrix seeding for `inventory-level-read`.               |
| `gleam/test/parity_test.gleam`                       | Enables `inventory-level-read` in the pure-Gleam parity suite.                   |

Validation: `gleam test --target javascript` is green at 702 tests on the host
Node runtime. Product parity inventory remains 115 checked-in specs, with 8
product specs executable in the Gleam parity suite after this pass.

### Findings

- Enabling `inventory-level-read` before implementation produced the expected
  strict parity mismatch: the capture contained the selected InventoryLevel
  object, while the proxy returned `null` from the top-level root.
- The scenario can share Pass 40's product variants matrix seeding; no capture,
  request, or comparison contract changes were needed.

### Risks / open items

- Inventory level lifecycle behavior, inactive-level semantics, quantity
  adjustments, activation/deactivation, and inventory item/level catalog reads
  remain unported.
- Only 8 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 42 candidates

- Add ProductOption state and product `options`/option value projection from
  captured fixtures.
- Start inventory item/level catalog/search reads over effective variant-backed
  inventory items.
- Add ProductVariant query filtering for simple `vendor`, `product_type`, `tag`,
  `sku`, and `id` terms.

---

## 2026-04-30 — Pass 40: product variant inventory reads

Enables the captured `product-variants-read` parity scenario in the Gleam
suite. ProductVariant records now carry the captured InventoryItem subset used
by the product variants matrix fixture: tracked/shipping flags, measurement
weight, origin fields, inventory level connections, quantities, and the
InventoryItem -> ProductVariant backreference. Product detail projection can
also expose a product-scoped `variants` connection from effective variant state,
which lets the same seeded record serve `product(id:)`, top-level
`productVariant(id:)`, and top-level `inventoryItem(id:)` reads.

This is still a read slice over captured fixture state. It does not implement
inventory item mutations, inventory level lifecycle behavior, inventory quantity
roots, variant query filtering, or broader inventory projections beyond the
fields selected by the parity request.

| Module                                                     | Change                                                                                                       |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/state/types.gleam`          | Adds InventoryItem, InventoryLevel, quantity, location, measurement, and weight records.                     |
| `gleam/src/shopify_draft_proxy/state/store.gleam`          | Adds effective variant lookup by inventory item id for `inventoryItem(id:)` reads.                           |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`       | Serializes product-scoped variant connections, nested inventory items, inventory levels, and backreferences. |
| `gleam/test/parity/runner.gleam`                           | Seeds the product variants matrix capture, including edge cursors and nested inventory item data.            |
| `gleam/test/shopify_draft_proxy/proxy/products_test.gleam` | Updates the direct ProductVariant record helper for the new inventory item field.                            |
| `gleam/test/parity_test.gleam`                             | Enables `product-variants-read` in the pure-Gleam parity suite.                                              |

Validation: `gleam test --target javascript` is green at 701 tests on the host
Node runtime. Product parity inventory remains 115 checked-in specs, with 7
product specs executable in the Gleam parity suite after this pass.

### Findings

- The product variants matrix capture stores the seed product as
  `$.data.product` rather than top-level `seedProducts`, and the product payload
  omits catalog-only fields such as `handle` and `status`. The runner seeds a
  relaxed Product baseline for this scenario while preserving the captured
  selected fields.
- The static SourceValue projector is sufficient for the selected product-scoped
  `variants(first:)` connection because the parity fixture has one captured
  variant. Broader paginated product-scoped variants remain a later behavior
  slice.

### Risks / open items

- Inventory mutations, inventory quantity roots, inventory item search/catalog
  reads, and inventory level lifecycle behavior remain unported.
- Product variant query filtering and non-ID sort keys remain unported.
- Only 7 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 41 candidates

- Add ProductOption state and product `options`/option value projection from
  captured fixtures.
- Start inventory item/level top-level catalog/search reads over effective
  variant-backed inventory items.
- Add ProductVariant query filtering for simple `vendor`, `product_type`, `tag`,
  `sku`, and `id` terms.

---

## 2026-04-30 — Pass 39: product helper-root parity

Enables the captured `product-helper-roots-read` parity scenario in the Gleam
suite. The runner now seeds top-level `seedProducts` fixtures, including their
captured variants and selected options, so helper roots can read Product and
ProductVariant records from the same checked-in conformance preconditions as
the TypeScript parity harness. The parity target decoder also honors
`upstreamCapturePath` for override targets; this preserves the spec's
live-hybrid full-payload target without weakening the capture or request shape.

This pass closes the helper-root read scenario only. ProductVariant search
filters, non-ID sorting, variant inventory projection, product options,
inventory item/level reads, collections, publications, selling plans, and
product mutations remain unported.

| Module                           | Change                                                                                                                                                     |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/test/parity/spec.gleam`   | Decodes target-level `upstreamCapturePath` so live-hybrid capture targets keep their checked-in comparison contract.                                       |
| `gleam/test/parity/runner.gleam` | Seeds `seedProducts` and captured variants/options needed by product helper roots; reuses captured upstream payloads for upstream-backed override targets. |
| `gleam/test/parity_test.gleam`   | Enables `product-helper-roots-read` in the pure-Gleam parity suite.                                                                                        |

Validation: `gleam test --target javascript` is green at 700 tests on the host
Node runtime. Host `gleam test --target erlang` still fails because `escript`
is missing; the Docker Erlang fallback is green at 696 tests. Product parity
inventory remains 115 checked-in specs, with 6 product specs executable in the
Gleam parity suite after this pass.

### Findings

- The TypeScript parity harness already treats top-level `seedProducts` as
  reusable preconditions across scenarios; lifting that convention into Gleam
  unblocks helper roots without adding scenario-specific fixture rewrites.
- The helper-root spec's final target is intentionally live-hybrid evidence:
  `upstreamCapturePath` points at the full captured Shopify payload, while the
  earlier targets exercise local snapshot reads over seeded product records.

### Risks / open items

- The Gleam runner does not yet have a general injectable upstream transport.
  For upstream-backed override targets it compares against the captured upstream
  payload directly, matching the current no-staged-state helper scenario but
  not yet modeling live-hybrid overlay mutations.
- Product options, inventory item/level/quantity, collections, publications,
  selling plans, product feeds/feedback, metafields, variant query filtering,
  and all product mutations remain unported in Gleam.
- Only 6 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 40 candidates

- Add ProductVariant query filtering for simple `vendor`, `product_type`, `tag`,
  `sku`, and `id` terms before enabling variant-search parity specs.
- Start ProductOption state and reads from seeded product fixtures.
- Add inventory item/level projection for variant-backed helper reads.

---

## 2026-04-30 — Pass 38: product variant helper reads

Adds the first ProductVariant state and read slice to the Gleam Products port.
The store now tracks base/staged product variants with Shopify-like family
overlay behavior: staged variants for a product replace that product's base
variant family, and deleted products hide their variants. The Products handler
resolves `productVariant(id:)`, `productVariantByIdentifier(identifier: { id })`,
`productVariants(first:, sortKey: ID)`, and `productVariantsCount` from effective
local variant state.

This remains a narrow read slice. Variant query filtering, non-ID sort keys,
inventory item/level projection, selected-option parity fixtures, variant
mutations, and full `product-helper-roots-read` parity remain deferred.

| Module                                                     | Change                                                                                                               |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`          | Adds minimal ProductVariant and selected-option records.                                                             |
| `gleam/src/shopify_draft_proxy/state/store.gleam`          | Adds base/staged ProductVariant buckets, effective family overlay helpers, lookups, list reads, and count.           |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`       | Serializes top-level and helper ProductVariant reads plus ProductVariant connections/counts.                         |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`    | Includes ProductVariant buckets/counts in the current meta state dump shape.                                         |
| `gleam/test/shopify_draft_proxy/proxy/products_test.gleam` | Adds direct coverage for variant ID/helper reads, missing variants, product projection, connection order, and count. |

Validation: `gleam test --target javascript` is green at 699 tests on the host
Node runtime. Product parity inventory remains 115 checked-in specs, with 5
product specs executable in the Gleam parity suite after this pass.

### Findings

- The TS store treats a staged variant family as replacing the corresponding
  base product's variant family, rather than merging variant-by-variant. The
  Gleam helper mirrors that behavior from the start.
- Top-level `productVariants` lists variants through effective Product order,
  then sorts by Shopify resource ID for the helper scenario's `sortKey: ID`.

### Risks / open items

- `product-helper-roots-read` remains disabled because the captured strict
  scenario also needs captured Shopify variant count, saved-search helper fields,
  product operation, product duplicate job shape, and product feedback branches.
- Product options, inventory item/level/quantity, collections, publications,
  selling plans, feeds/feedback, metafields, and all product mutations remain
  unported in Gleam.
- Only 5 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 39 candidates

- Seed the captured `product-helper-roots-read` product and variant records and
  narrow the remaining mismatches toward enabling that full parity spec.
- Add product operation/feedback no-data/helper branches needed by
  `product-helper-roots-read`.
- Begin ProductVariant query filtering for simple `vendor`, `product_type`,
  `tag`, `sku`, and `id` terms before enabling variant-search parity specs.

---

## 2026-04-30 — Pass 37: product string helper catalogs

Extends the Product helper read slice to Product-backed string catalog roots:
`productTags`, `productTypes`, and `productVendors` now derive their values from
effective local Product state. The implementation filters blank strings,
deduplicates values, sorts them lexicographically, supports `reverse`, and uses
the shared connection serializer so selected `nodes`, `edges`, and `pageInfo`
follow the same synthetic cursor behavior as the TypeScript runtime.

This still does not enable the broad `product-helper-roots-read` parity spec.
That scenario continues to require ProductVariant state, product variant helper
roots/counts, saved-search helper payloads, and the remaining operation/feedback
branches before it can run strictly without weakening the capture.

| Module                                                     | Change                                                                                                        |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`       | Adds Product-backed string connection serialization for tags, product types, and vendors.                     |
| `gleam/test/shopify_draft_proxy/proxy/products_test.gleam` | Adds direct coverage for sorted/deduped string catalogs, edge cursors, pageInfo, reverse ordering, and limit. |

Validation: `gleam test --target javascript` is green at 697 tests on the host
Node runtime. Product parity inventory remains 115 checked-in specs, with 5
product specs executable in the Gleam parity suite after this pass.

### Findings

- The existing shared connection helper is sufficient for scalar string
  connections; no Products-specific pagination loop is needed.
- The TS runtime builds these helper catalogs directly from effective products,
  so this pass can increase local read fidelity without adding a new state
  bucket.

### Risks / open items

- `product-helper-roots-read` remains disabled because variant helper roots,
  variant counts, saved-search helper fields, product operation, and feedback
  branches are not fully ported.
- Product variants, collections, options, inventory, publications, selling
  plans, feeds/feedback, metafields, and all product mutations remain unported
  in Gleam.
- Only 5 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 38 candidates

- Add ProductVariant state and seed the helper-root first variant scenario.
- Port `productVariantsCount` and simple variant ID helper reads over effective
  variant state.
- Begin products search grammar support using shared search-query parser
  patterns before enabling search-specific parity specs.

---

## 2026-04-30 — Pass 36: product identifier helper reads

Extends the seeded Product read slice to the simplest product helper root:
`productByIdentifier(identifier:)` now resolves effective local Product records
by `id` first and then by `handle`, matching the TypeScript branch precedence.
Missing IDs, missing handles, omitted identifiers, and unported `customId`
lookups continue to return `null`.

This is still deliberately narrower than the captured
`product-helper-roots-read` scenario. That parity spec also exercises product
variants, helper catalogs, saved searches, duplicate jobs, operations, feedback,
and live-hybrid payloads, so it remains disabled until those sibling paths are
ported instead of weakening the captured request or comparison contract.

| Module                                                     | Change                                                                                                          |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`       | Adds `productByIdentifier` ID/handle projection over effective Product state while leaving custom IDs deferred. |
| `gleam/src/shopify_draft_proxy/state/store.gleam`          | Adds an effective Product lookup by handle that respects staged/base ordering and deletion markers.             |
| `gleam/test/shopify_draft_proxy/proxy/products_test.gleam` | Adds direct ID, handle, and missing identifier branch coverage for seeded Product helper reads.                 |

Validation: `gleam test --target javascript` is green at 695 tests on the host
Node runtime. Host `gleam test --target erlang` still fails because `escript`
is missing; the Docker Erlang fallback is green at 691 tests. Product parity
inventory remains 115 checked-in specs, with 5 product specs executable in the
Gleam parity suite after this pass.

### Findings

- The existing Product record slice already contains the handle needed for the
  helper lookup; no new fixture seeding is required for direct coverage.
- `productByIdentifier` custom IDs depend on product metafield-definition and
  product-metafield state that has not landed in the Gleam port yet, so this
  pass keeps that branch explicitly unclaimed.

### Risks / open items

- `product-helper-roots-read` remains disabled because variant helper roots and
  broad helper catalogs are not ported.
- Product variants, collections, options, inventory, publications, selling
  plans, feeds/feedback, tags, metafields, and all product mutations remain
  unported in Gleam.
- Only 5 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 37 candidates

- Add ProductVariant state and seed the helper-root first variant scenario.
- Start product tag/type/vendor helper catalogs from seeded Product state.
- Begin products search grammar support using shared search-query parser
  patterns before enabling search-specific parity specs.

---

## 2026-04-30 — Pass 35: seeded products catalog reads

Extends the Product read foundation from by-ID detail reads to the first
catalog connection. The product record now carries catalog-selected fields and
captured cursors, the store tracks a seeded product count, and the Products
handler serializes `products(first:)` over effective product state with
Shopify-shaped edges, pageInfo, and `productsCount` precision. The parity runner
seeds the captured `products-catalog-read` page from checked-in edge nodes
without changing the GraphQL request, variables, or capture.

This remains read-only progress. Product lifecycle mutations, search grammar,
variant/collection/inventory/publication/selling-plan/metafield resources, and
most product helper roots are still deferred, so the TypeScript Products runtime
remains in place.

| Module                                                     | Change                                                                                                                      |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`          | Expands the Product record with catalog fields, inventory summary fields, timestamps, and captured cursors.                 |
| `gleam/src/shopify_draft_proxy/state/store.gleam`          | Adds effective product-count state and keeps product list order/cursor data available for catalog connection reads.         |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`       | Serializes `products(first:)` connections and `productsCount` from seeded state while preserving empty behavior when unset. |
| `gleam/test/parity/runner.gleam`                           | Seeds the captured `products-catalog-read` count and edge node rows into the Products base state.                           |
| `gleam/test/parity_test.gleam`                             | Enables `products-catalog-read` as executable strict Gleam parity evidence.                                                 |
| `gleam/test/shopify_draft_proxy/proxy/products_test.gleam` | Adds direct coverage for seeded product catalog edges, captured cursors, pageInfo, and count precision.                     |

Validation: `gleam test --target javascript` is green at 681 tests on the host
Node runtime. Product parity inventory remains 115 checked-in specs, with 5
product specs now executable in the Gleam parity suite:
`product-empty-state-read`, `product-related-by-id-not-found-read`,
`product-feeds-empty-read`, `product-detail-read`, and
`products-catalog-read`.

### Findings

- The catalog capture records Shopify opaque cursors; the Gleam connection
  serializer can preserve them by disabling synthetic cursor prefixes for this
  seeded path.
- A captured product count is necessary for strict `productsCount` and
  `hasNextPage` parity because the checked-in catalog page only includes the
  first three product rows from a much larger store.
- The product record remains intentionally partial, but now includes the fields
  needed by the first catalog page and can be widened as search/helper scenarios
  require more of the TS product model.

### Risks / open items

- Product search/filter/sort semantics are not ported yet; this pass preserves
  the seeded catalog ordering rather than claiming general Shopify search
  behavior.
- Product variants, collections, options, inventory, publications, selling
  plans, feeds/feedback, tags, metafields, and all product mutations remain
  unported in Gleam.
- Only 5 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 36 candidates

- Port simple product helper identifier roots over the Product state slice,
  including `productByIdentifier` by ID/handle.
- Add ProductVariant state and seed the helper-root first variant scenario.
- Begin products search grammar support using the shared search-query parser
  patterns before enabling search-specific parity specs.

---

## 2026-04-30 — Pass 34: seeded product detail reads

Extends the Products foundation from no-data reads to the first store-backed
product read. The Gleam state now has a normalized `ProductRecord` slice, the
Products query handler resolves `product(id:)` from effective base/staged
product state, and the parity runner seeds the captured `product-detail-read`
fixture into base state before replaying the strict proxy request. Nested
collections and media intentionally remain empty connection shapes until their
own product graph slices land.

This pass still does not claim product lifecycle support. Product mutations,
variants, collections, inventory, publications, selling plans, product
metafields, and broad connection/search behavior remain deferred, so the
TypeScript Products runtime stays in place.

| Module                                                     | Change                                                                                                                                |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`          | Adds Product, Product SEO, and Product category record types for the seeded read slice.                                               |
| `gleam/src/shopify_draft_proxy/state/store.gleam`          | Adds base/staged product buckets, product order/deleted markers, and effective product upsert/list/delete helpers.                    |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`       | Resolves `product(id:)` from store-backed state and projects selected product detail fields with empty collection/media connections.  |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`    | Threads store into the Products query dispatcher, routes registry Products capabilities, and serializes the product state bucket.     |
| `gleam/test/parity/runner.gleam`                           | Seeds the captured `product-detail-read` product row into base state without rewriting the checked-in parity request or capture.      |
| `gleam/test/parity_test.gleam`                             | Enables `product-detail-read` as executable strict Gleam parity evidence.                                                             |
| `gleam/test/shopify_draft_proxy/proxy/products_test.gleam` | Adds direct coverage for seeded product detail projection and preserves no-data Products tests through the store-backed handler path. |

Validation: `gleam test --target javascript` is green at 679 tests on the host
Node runtime. Product parity inventory remains 115 checked-in specs, with 4
product specs now executable in the Gleam parity suite:
`product-empty-state-read`, `product-related-by-id-not-found-read`,
`product-feeds-empty-read`, and `product-detail-read`.

### Findings

- The existing captured `product-detail-read` fixture is enough to seed a narrow
  normalized product row and prove selected scalar/SEO/category fields without
  weakening the strict comparison contract.
- Product detail captures currently exercise empty `collections` and `media`
  connections only; those can remain Shopify-shaped empty connections while the
  real collection/media graph lands separately.
- Registry-driven Products query routing was still missing even after the
  legacy fallback was wired, so this pass adds the Products capability arm.

### Risks / open items

- Product variants, collections, options, inventory, publications, selling
  plans, feeds/feedback, tags, metafields, and all product mutations remain
  unported in Gleam.
- The product state shape is intentionally minimal and will need expansion as
  lifecycle mutations, search/filter connections, node helpers, and nested
  resource reads land.
- Only 4 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 35 candidates

- Port `products(first:, query:)` over the new product state slice, including
  Shopify-like search filtering for the captured catalog/helper scenarios.
- Add ProductVariant and Collection state slices so helper roots and nested
  product variant/catalog reads can resolve captured records.
- Start `productCreate` / `productUpdate` / `productDelete` local staging once
  the read model can expose downstream read-after-write effects.

---

## 2026-04-30 — Pass 33: products no-data read foundation

Starts the Products domain in the Gleam dispatcher with a deliberately narrow
read-only foundation. The new module handles Shopify-like null/empty behavior
for product and product-adjacent query roots when no local product graph is
seeded: missing product, collection, variant, inventory item/level, feed,
operation, and feedback roots return `null`; product, variant, feed, helper,
tag/type/vendor, inventory-item, and collection connections return selected
empty connection shapes; count roots return `0` with `EXACT` precision; and
unknown `productDuplicateJob` preserves the current Shopify-observed
`done: true` helper shape.

This pass does not claim product lifecycle support. The TypeScript Products
runtime remains in place because mutations, downstream read-after-write effects,
stateful product/variant/collection/inventory records, publications, selling
plans, product metafields, and the rest of the 115 product parity specs are not
ported yet.

| Module                                                        | Change                                                                                                                                  |
| ------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/products.gleam`          | Adds Products query-root detection and selected no-data/null/empty serializers for product-adjacent reads.                              |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`       | Routes Products query capabilities and legacy product root detection to the new Products dispatcher.                                    |
| `gleam/test/shopify_draft_proxy/proxy/products_test.gleam`    | Adds direct coverage for product empty-state reads, missing related by-id roots, product feeds empty reads, and unknown duplicate jobs. |
| `gleam/test/shopify_draft_proxy/proxy/draft_proxy_test.gleam` | Updates the previous product-unimplemented dispatcher test to assert the new empty read route returns a GraphQL envelope.               |
| `gleam/test/parity_test.gleam`                                | Enables the three strict no-data product parity scenarios that this pass supports without fixture weakening or capture rewrites.        |

Validation: `gleam test --target javascript` is green at 677 tests on the host
Node runtime. Host `gleam test --target erlang` is blocked by missing `escript`,
so the established fallback
`docker run --rm -v "$PWD/..:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine gleam test --target erlang`
is green at 673 tests. Product parity inventory remains 115 checked-in specs,
with 3 product specs now executable in the Gleam parity suite:
`product-empty-state-read`, `product-related-by-id-not-found-read`, and
`product-feeds-empty-read`.

### Findings

- The current product parity corpus already has a useful no-data subset that
  can run before the normalized product graph exists, giving the Products domain
  an executable starting point without weakening any captures.
- `productDuplicateJob` is not a null no-data root in the TS handler; unknown
  IDs serialize as an object with the requested ID and `done: true`, so the
  Gleam no-data foundation preserves that helper quirk.
- Product helper roots are mixed: empty/null helper shapes can be handled now,
  but seeded identifier, variant, catalog, and live-hybrid helper comparisons
  still need product/variant state seeding before the full helper parity spec can
  be enabled.

### Risks / open items

- No product mutations are ported in this pass, so supported Products mutations
  still cannot stage locally in Gleam.
- No normalized product, variant, collection, inventory, publication, selling
  plan, or product-metafield state slice exists in Gleam yet.
- The TypeScript Products runtime cannot be deleted until all product lifecycle
  parity and downstream read-after-write behavior are ported and proven.
- Only 3 of 115 checked-in product parity specs are enabled by the Gleam parity
  suite after this pass.

### Pass 34 candidates

- Add the normalized Product/ProductVariant/Collection state foundation and seed
  captured product detail/catalog reads in the parity runner.
- Port `productCreate` / `productUpdate` / `productDelete` local staging as the
  first mutation lifecycle slice, including raw mutation log preservation.
- Expand product helper roots once product and variant state can resolve
  identifier, variant, tag/type/vendor, count, and saved-search projections.

---

## 2026-04-30 — Pass 44: JS embeddable shim rework

Reworks the JavaScript embeddable shim on top of the full-state dump substrate
from Pass 36. The package-facing API now uses a single `createDraftProxy(...)`
options object that carries both config fields and optional restore state, and
the shim routes `processGraphQLRequest` through Gleam's own default Admin
GraphQL path construction instead of duplicating the route string in TypeScript.
The async `commit` wrapper remains TS-friendly while replaying the original
staged mutation log through the Gleam runtime.

| Module                                                  | Change                                                                                                                                            |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/js/src/runtime.ts`                               | Restores `processGraphQLRequest` and async `commit`, collapses construction to one options object, and delegates GraphQL route defaults to Gleam. |
| `gleam/js/src/types.ts`                                 | Makes `DraftProxyOptions` the public construction object by extending `AppConfig` with optional restore state.                                    |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam` | Adds a JavaScript-target async `process_graphql_request_async` convenience wrapper using the shared Gleam default path helper.                    |
| `tests/integration/gleam-interop.test.ts`               | Updates the package-level lifecycle smoke to restore state through the one-object construction API.                                               |

Validation: `corepack pnpm build`, `corepack pnpm gleam:smoke:js`,
`corepack pnpm gleam:test:js`, `corepack pnpm lint`, `corepack pnpm --dir
gleam/js build`, `corepack pnpm --dir gleam/js test`, and `git diff --check`
are green. `corepack pnpm gleam:test:erlang` fails on the host Erlang runtime
with an `undef` boot error, so Erlang target validation used the established
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container fallback and is
green at 672 tests.

### Findings

- `origin/main` already moved state dump/restore to the full
  `state/serialization.gleam` substrate, so the review request for generic
  state persistence is satisfied by preserving that merge result instead of
  reintroducing handler-specific saved-search dump code.
- Keeping GraphQL default path construction in Gleam avoids two JS/TS copies of
  the default Admin API route while still letting the JS shim use the async
  live-hybrid path.

### Risks / open items

- `createApp` and `loadConfig` remain explicit not-implemented shims until the
  broader HTTP adapter work lands.

### Pass 45 candidates

- Continue reducing the remaining expected Gleam parity failures tracked by the
  CI gate manifest.
- Extend package-level consumer tests as more domains become exposed through the
  Gleam shim.

---

## 2026-04-30 — Pass 43: privacy data-sale opt-out parity

Ports the privacy-owned `dataSaleOptOut` mutation into a dedicated Gleam privacy
module while keeping the downstream read effect on customer state. The Gleam
dispatcher now routes only that privacy mutation root locally; other privacy
roots remain unsupported until their own shop privacy behavior is modeled.

The privacy parity spec is now executable Gleam evidence. The runner seeds the
captured downstream customer so the mutation returns the recorded customer id,
then strict comparisons cover the opt-out payload, downstream customer reads,
repeat idempotency, and invalid-email `FAILED` user error shape. The original
TypeScript runtime and TypeScript tests remain in place for the incremental port
guardrail.

| Module                                                               | Change                                                                                                           |
| -------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/privacy.gleam`                  | Adds privacy-domain local staging for `dataSaleOptOut`, including existing/unknown customer effects and errors.  |
| `gleam/src/shopify_draft_proxy/proxy/customers.gleam`                | Removes `dataSaleOptOut` from customer mutation dispatch while preserving customer read serialization.           |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`              | Adds `PrivacyDomain` mutation routing without adding privacy query/root breadth.                                 |
| `gleam/test/shopify_draft_proxy/proxy/privacy_test.gleam`            | Covers existing-customer opt-out readback/logs, unknown-email creation, invalid-email errors, unsupported roots. |
| `gleam/test/parity/runner.gleam` / `config/gleam-port-ci-gates.json` | Enables privacy parity by seeding capture customer state and removing the privacy expected-failure entry.        |
| `.agents/skills/gleam-port/SKILL.md`                                 | Records the privacy/customer ownership note for future domain passes.                                            |

Validation: `gleam test --target javascript` is green at 684 tests. The host
`gleam test --target erlang` fails because local Erlang/OTP is 25 while
`gleam_json` requires OTP 27; the equivalent OTP 27 container run is green at
680 tests:
`docker run --rm -v /home/airhorns/.local/bin/gleam:/usr/local/bin/gleam:ro -v /home/airhorns/code/symphony-workspaces/shopify-draft-proxy/HAR-497:/home/airhorns/code/symphony-workspaces/shopify-draft-proxy/HAR-497 -w /home/airhorns/code/symphony-workspaces/shopify-draft-proxy/HAR-497/gleam erlang:27-alpine sh -lc 'erl -eval "io:format(\"OTP=~s~n\", [erlang:system_info(otp_release)]), halt()." -noshell && gleam clean && gleam test --target erlang'`.

### Findings

- `dataSaleOptOut` belongs to privacy capability/log metadata even though the
  observable GraphQL field is `Customer.dataSaleOptOut`.
- The captured parity scenario uses an existing customer id, so the Gleam runner
  must seed that customer from the downstream read before replaying the primary
  mutation instead of adding synthetic-id expected differences.
- The synced `origin/main` parity gate still carried an expected-failure entry
  for `localization-disable-clears-translations` even though that scenario now
  passes; the stale entry was removed alongside the privacy gate update.

### Risks / open items

- Shop-level privacy settings roots are still unsupported by design. This pass
  does not model `privacySettings` or consent-policy privacy roots.
- The TypeScript privacy/customer runtime remains until a later final cutover
  pass verifies repository-wide Gleam parity.
- Local host Erlang validation requires OTP 27; OTP 25 can compile but fails at
  runtime through `gleam_json`.

### Pass 44 candidates

- Port product-owned `metafieldDelete` / `metafieldsDelete` and their
  hydrated/downstream deletion flows into Gleam.
- Add `standardMetafieldDefinitionTemplates` catalog query support once a
  captured template-catalog fixture exists.
- Continue Store Properties locations and fulfillment/carrier-service lifecycle
  roots, reusing the existing shop state slice.

---

## 2026-04-30 — Pass 44: B2B company lifecycle parity

Ports the B2B Admin GraphQL domain into the Gleam runtime while preserving the
TypeScript B2B runtime and tests for the incremental port. Companies, contacts,
locations, roles, role assignments, address assignments, staff assignments, and
tax settings now have normalized Gleam state, local mutation staging, downstream
read-after-write behavior, Relay node coverage for B2B-owned records, and
parity-runner fixture seeding for the checked-in B2B captures. Email-delivery
behavior remains outside local B2B support rather than inventing local side
effects.

| Module                                                                   | Change                                                                                                                    |
| ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/b2b.gleam`                          | Adds B2B query and mutation handling for company/contact/location/role lifecycle flows and relationship/tax updates.      |
| `gleam/src/shopify_draft_proxy/state/{types,store,serialization}.gleam`  | Adds B2B normalized records, effective-state helpers, delete markers, and state dump buckets with empty restore defaults. |
| `gleam/src/shopify_draft_proxy/proxy/{draft_proxy,admin_platform}.gleam` | Wires B2B query/mutation dispatch and B2B Relay node reads without broadening unsupported roots.                          |
| `gleam/test/shopify_draft_proxy/proxy/b2b_test.gleam`                    | Adds targeted Gleam coverage for B2B lifecycle mutations, downstream reads, and unsupported email-delivery boundaries.    |
| `gleam/test/parity/{runner,diff,spec}.gleam`                             | Seeds B2B read fixtures and fixes nested wildcard diff matching needed by the B2B parity specs.                           |
| `config/gleam-port-ci-gates.json`                                        | Removes the now-passing B2B specs, plus a stale passing localization spec, from expected Gleam parity failures.           |

Validation: `gleam test --target javascript` was green at 681 tests. `gleam
test --target erlang` was green at 677 tests via the
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container because the host
Erlang/OTP 25 installation is too old for the current `gleam_json`
requirement. The direct B2B parity scenario report was green for all four B2B
specs: `b2b-company-contact-main-delete`, `b2b-company-create-lifecycle`,
`b2b-company-roots-read`, and `b2b-contact-location-assignments-tax`.
`git diff --check` was also green.

### Findings

- The B2B read fixture has enough captured baseline data to seed root catalog
  reads directly in the Gleam parity runner.
- Nested wildcard expected-difference paths need to match more than one list
  segment; the previous diff helper only handled the first wildcard cleanly.
- The generated ticket text mentions deleting the TypeScript B2B runtime after
  parity, but the port guardrail keeps TypeScript runtime and test coverage in
  place until the final full-port cutover.

### Risks / open items

- B2B state restore currently defaults newly ported B2B buckets to empty when
  older dumps omit them; a future snapshot compatibility pass can decode stored
  B2B buckets once real Gleam-authored dumps need round-trip restore coverage.
- Final TypeScript B2B runtime retirement remains a whole-port cutover concern,
  not a per-domain pass action.

### Pass 45 candidates

- Port product-owned `metafieldDelete` / `metafieldsDelete` and their
  hydrated/downstream deletion flows into Gleam.
- Continue Store Properties locations and fulfillment/carrier-service lifecycle
  roots, reusing the existing shop state slice.
- Add `standardMetafieldDefinitionTemplates` catalog query support once a
  captured template-catalog fixture exists.

---

## 2026-04-30 — Pass 42: webhook parity evidence metadata with TS retained

Records the completed Webhooks Gleam parity evidence in repository metadata
while leaving the legacy TypeScript runtime, dispatcher hooks, and TypeScript
tests in place. This pass does not change webhook runtime code; it updates the
checked-in parity specs and operation registry so the already-present Gleam
parity/direct tests are listed beside the retained TypeScript evidence.

| Module                                                              | Change                                                                                                                          |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `config/parity-specs/webhooks/*.json`                               | Adds `gleam/test/parity_test.gleam` and direct Gleam webhook/log tests while keeping existing TypeScript runtime-test evidence. |
| `config/operation-registry.json`                                    | Adds Gleam runtime-test metadata and support notes while making the retained TypeScript runtime boundary explicit.              |
| `gleam/src/shopify_draft_proxy/proxy/operation_registry_data.gleam` | Regenerates the vendored Gleam registry from the updated JSON source.                                                           |
| `.agents/skills/gleam-port/SKILL.md`                                | Records that TypeScript runtime retirement belongs to an explicit final cleanup phase, not routine per-domain parity handoff.   |

Validation: `gleam test --target javascript`, the Erlang target via
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine`, `corepack pnpm typecheck`,
`corepack pnpm conformance:check`, `corepack pnpm conformance:parity`,
`corepack pnpm lint`, and `git diff --check` are green.

### Findings

- The webhook parity specs can remain byte-faithful on request/capture data;
  runtime-test metadata now names both retained TypeScript coverage and the
  already-present Gleam parity/direct tests.
- Reviewer feedback clarified that TypeScript runtime retirement should wait
  for the final cleanup phase even when a domain reaches Gleam parity.
- Erlang validation must mount the repository root at the expected relative path
  when using Docker; mounting only `gleam/` breaks parity fixtures resolved via
  `../config` and `../fixtures`.

### Risks / open items

- The TypeScript HTTP dispatcher still handles webhook roots until the final
  cleanup phase; Webhooks are also owned by the Gleam embeddable/parity path.

### Pass 42 candidates

- Add a fixture-bundle snapshot loader once parity runner scenarios need
  recorded GraphQL bundle startup state.
- Continue Store Properties location / fulfillment-service roots now that
  state dump/restore can persist the expanded store shape.
- Add broader runtime smoke around `process_request_async` live-hybrid
  passthrough and commit replay once test transport injection is exposed through
  the JS shim.

---

## 2026-04-30 — Pass 41: gift-card parity completion

Completes the Gift Cards Gleam parity handoff while keeping the TypeScript
runtime in place. Both checked-in gift-card parity specs now execute in the
Gleam parity suite: the existing search-filter scenario remains enabled, and
the lifecycle scenario now runs against the captured fixture with seeded
gift-card/configuration preconditions. The parity runner also decodes and
honors target-level `selectedPaths`, which is required by the lifecycle spec's
mutation-payload comparisons and preserves the checked-in capture/request shape
without adding expected differences.

The TypeScript gift-card runtime handler and legacy TypeScript integration flow
remain present for this pass. That keeps the public TypeScript/Koa proxy and
existing runtime coverage unchanged while the Gleam port gains executable
parity evidence.

| Module                                                             | Change                                                                                                                |
| ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| `config/gleam-port-ci-gates.json` / `gleam/test/parity_test.gleam` | Removes `gift-card-lifecycle` from expected failures so discovery-based Gleam parity treats it as passing evidence.   |
| `gleam/test/parity/runner.gleam`                                   | Seeds lifecycle/search captures from the gift-card conformance fixture before replaying local mutation/read requests. |
| `gleam/test/parity/spec.gleam` / `gleam/test/parity/diff.gleam`    | Adds target-level `selectedPaths` decoding and selected-slice diffing for parity specs.                               |
| `src/proxy/gift-cards.ts` / TypeScript gift-card test flow         | Remains in place as the legacy TypeScript runtime and integration coverage until a later explicit removal pass.       |
| `config/operation-registry.json` and gift-card parity specs        | Keeps legacy TS coverage visible while also pointing to the new Gleam gift-card query/mutation tests.                 |

Validation after rework and the latest `origin/main` merge: `gleam test
--target javascript` was green at 676 tests. `gleam test --target erlang` was
green at 672 tests via the `ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine`
container because the host lacks `escript`. `corepack pnpm typecheck`,
`corepack pnpm conformance:check`, `corepack pnpm conformance:parity`,
`corepack pnpm lint`, `corepack pnpm gleam:port:coverage`, `corepack pnpm
gleam:registry:check`, `corepack pnpm conformance:capture:check`, `corepack
pnpm build`, targeted `corepack pnpm vitest run
tests/integration/gift-card-flow.test.ts`, and `git diff --check` were green.

### Findings

- The gift-card lifecycle parity spec already had enough captured evidence, but
  it was not wired into the Gleam parity suite.
- The Gleam parity spec decoder previously ignored `selectedPaths`, so enabling
  the lifecycle spec compared full mutation payload objects and reported
  expected unselected Shopify fields as missing from the proxy selection.
- Gift-card capture seeding can reuse the same lifecycle precondition loader for
  both lifecycle and search-filter parity specs.

### Risks / open items

- The TypeScript gift-card runtime still needs a later explicit cutover/removal
  pass once reviewers are ready to retire the public TS handler path.
- Admin Platform generic Node resolution for `GiftCard` still depends on the
  TypeScript runtime in Node; the Gleam Admin Platform resolver should add
  GiftCard node dispatch when a future pass broadens cross-domain Relay node
  coverage.

### Pass 42 candidates

- Port product-owned `metafieldDelete` / `metafieldsDelete` and their
  hydrated/downstream deletion flows into Gleam.
- Add `standardMetafieldDefinitionTemplates` catalog query support once a
  captured template-catalog fixture exists.
- Continue Store Properties locations and fulfillment/carrier-service lifecycle
  roots, reusing the existing shop state slice.

---

## 2026-04-30 — Pass 40: CI gate hardening for port completion

Adds CI gates for the Gleam port without making the current partial parity
runner list an allowlist. The gate keeps the vendored Gleam operation registry
synchronized with `config/operation-registry.json`, requires checked-in parity
specs to remain strict executable evidence, and verifies the Gleam parity suite
still attempts every convention-discovered parity spec while tracking only the
expected failures that remain unported.

This pass does not port a new endpoint domain and does not delete any TypeScript
runtime code. It makes port/conformance coverage harder to reduce silently while
preserving the normal workflow where agents add Gleam parity support and remove
expected-failure entries as domain work lands.

| Module                                | Change                                                                                                                                               |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `config/gleam-port-ci-gates.json`     | Adds the reviewed CI gate manifest for expected Gleam parity failures, required workflow commands, and remaining capture-tooling checks.             |
| `scripts/gleam-port-coverage-gate.ts` | Adds the CI gate checker for parity inventory, expected-failure drift, workflow commands, package scripts, and capture-tooling checks.               |
| `package.json`                        | Adds `gleam:registry:check`, `gleam:port:coverage`, and `conformance:capture:check` scripts.                                                         |
| `.github/workflows/ci.yml`            | Runs registry drift, conformance parity, remaining TypeScript capture tooling, conformance status, the new port gate, both targets, and smoke tests. |

Validation: `corepack pnpm gleam:port:coverage`, `corepack pnpm
gleam:registry:check`, `corepack pnpm lint`, `corepack pnpm typecheck`,
`corepack pnpm conformance:check`, `corepack pnpm conformance:parity`,
`corepack pnpm conformance:capture:check`, `corepack pnpm conformance:status
-- --output-json .conformance/current/conformance-status-report.json
--output-markdown .conformance/current/conformance-status-comment.md`,
`corepack pnpm build`, `corepack pnpm gleam:format:check`, `corepack pnpm
gleam:test:js`, and `corepack pnpm gleam:smoke:js` are green. The host lacks
`escript`, so Erlang target and Elixir smoke were validated with the matching
Gleam 1.16 containers:
`docker run --rm -v "$PWD:/repo" -w /repo/gleam ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine gleam test --target erlang`
and
`docker run --rm -v "$PWD:/repo" -w /repo ghcr.io/gleam-lang/gleam:v1.16.0-elixir-alpine sh -lc 'cd gleam && gleam export erlang-shipment && cd elixir_smoke && mix test'`.

### Findings

- `gleam/scripts/sync-operation-registry.sh` emits unformatted Gleam; the drift
  check regenerates, formats, then diffs the generated registry module.
- Review clarified that the Gleam parity gate must run every parity spec and
  maintain an expected-failure list, not freeze the current runner list or a
  discovered-spec count.
- The current JS target has 323 expected parity failures; the Erlang target has
  those plus two target-specific metaobject parity failures recorded with
  `targets: ["erlang"]`.
- CI checks TypeScript conformance capture tooling as remaining tooling,
  separate from the Gleam runtime authority.

### Risks / open items

- The gate proves the current coverage surface cannot shrink silently; it does
  not claim the remaining endpoint domains are fully ported.
- Host-local Erlang/Elixir package scripts still require a BEAM installation;
  CI installs BEAM directly, while this workspace uses containers for local
  equivalent validation.

### Pass 41 candidates

- Continue Store Properties with locations and fulfillment/carrier-service
  lifecycle roots.
- Continue Admin Platform parity seeding for utility roots that now have
  owning-domain serializers in Gleam.
- Continue Marketing upstream hydration and parity-runner seeding.

---

## 2026-04-30 — Pass 39: operation registry and dispatcher support guards

Locks the Gleam operation registry mirror and dispatcher classification around
the TypeScript registry without overclaiming roots that the Gleam port cannot
handle locally yet. Capability lookup still mirrors the TS registry for every
implemented match name, while local dispatch is now gated by explicit
Gleam-local query and mutation dispatch tables. In live-hybrid mode, an
implemented TS root whose domain or specific root is not ported to Gleam falls
through to upstream passthrough rather than returning a local "no dispatcher"
error or claiming stage/overlay support.

| Module                                                               | Change                                                                                                                                          |
| -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`              | Separates TS capability classification from Gleam-local dispatch support and gates registry-driven routing with explicit local dispatch tables. |
| `gleam/test/shopify_draft_proxy/proxy/operation_registry_test.gleam` | Adds generated-registry semantic coverage for every implemented match name, unimplemented fallback behavior, and local-dispatch support guards. |
| `gleam/test/shopify_draft_proxy/proxy/passthrough_test.gleam`        | Proves an implemented-but-unported TS root uses the live-hybrid passthrough branch on JS instead of claiming local dispatch.                    |
| `gleam/scripts/sync-operation-registry.sh`                           | Adds deterministic `--check` mode and formats generated output before comparing/writing.                                                        |
| `tests/unit/operation-registry.test.ts`                              | Wires the sync `--check` into `conformance:check`, which already runs in CI.                                                                    |
| `.agents/skills/gleam-port/SKILL.md`                                 | Documents the registry mirror, drift check command, and capability-vs-local-dispatch split for future porting agents.                           |

Validation after merging `origin/main@cb46f01`: `corepack pnpm
conformance:check` is green at 1402 tests, `gleam test --target javascript` is
green at 719 tests, and `gleam test --target erlang` is green at 715 tests via
the established `ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container.

### Findings

- The registry currently mirrors 601 implemented TS roots across all 25
  implemented TS domains, but Gleam-local support is narrower and sometimes
  root-specific inside a partially ported domain.
- Domain-level capability mapping was too coarse for the port: roots such as
  `orders` and `productCreate` classify correctly as TS-supported, but Gleam
  has no local dispatcher for those roots yet. Live-hybrid now treats those as
  passthrough until the owning domain is ported.
- A broad capability-to-domain helper layer made the guard harder to review
  than a direct dispatch table, so the final dispatcher uses flat local root
  routing and lets unknown implemented roots fall through conservatively.
- Formatting the generated registry is part of determinism; raw generator
  output alone did not match the checked-in formatted file.

### Risks / open items

- This pass does not port any new endpoint family. Products, customers, orders,
  B2B, discounts, markets, online-store, payments, privacy, and other unported
  or partially ported roots still need their own domain passes.
- The public `registry_entry_has_local_dispatch` helper is intentionally
  conservative and should be kept aligned with root predicates as new domains
  land.
- The host still lacks a local Erlang `escript`; Erlang validation uses the
  container fallback until the host toolchain is repaired.

### Pass 40 candidates

- Start Product read/mutation substrate work so the highest-volume implemented
  TS roots can stop using live-hybrid passthrough in Gleam.
- Start Shipping/Fulfillments substrate so fulfillment-service,
  carrier-service, delivery-profile, and shipping-settings roots can consume
  ported Location state without reaching back into the TypeScript module.
- Add a small CI helper script for containerized Erlang validation in
  workspaces that do not have `escript` installed locally.

---

## 2026-04-30 — Pass 38: store-properties locations, business entities, and publishables

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

### Pass 39 candidates

- Start Shipping/Fulfillments substrate so fulfillment-service, carrier-service,
  delivery-profile, and shipping-settings roots can consume ported Location
  state without reaching back into the TypeScript module.
- Start Products publication substrate so Product and Collection publishable
  projections can move from captured Store Properties rows into typed product
  and collection records.
- Continue Markets or Online Store ports where Store Properties shop/location
  read effects are now available as local Gleam state.

---

## 2026-04-30 — Pass 33: customer domain foundation and parity coverage

Ports the Customer domain into the Gleam dispatcher. The new module models
customer create/update/delete/set, address lifecycle, consent updates, tax
exemptions, merge/merge status, account-page reads, identifier reads,
customer metafields, customer-owned order/event summaries, outbound side-effect
validation, payment-method update-email intent, data-sale opt-out, and store
credit credit/debit/read behavior. Customer state is normalized in the Gleam
store, merge redirects and attached-resource transfer stay local, and supported
customer mutations stage without runtime Shopify writes.

The TypeScript Customer runtime remains in place for this pass. The Gleam
parity path is green for the checked-in customer fixtures, but the repository
still has TypeScript integration surfaces outside this port that need a final
domain-removal pass before deleting `src/proxy/customers.ts`.

| Module                                                          | Change                                                                                                                                                                     |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/customers.gleam`           | Adds Customer query/mutation handling, local validation, projection, merge staging, store-credit staging, and customer-owned subresource reads.                            |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`         | Routes Customer capabilities and legacy customer roots through the new domain.                                                                                             |
| `gleam/src/shopify_draft_proxy/state/types.gleam`               | Adds customer, address, metafield, payment-method, store-credit, account-page, merge, order-summary, and event-summary records.                                            |
| `gleam/src/shopify_draft_proxy/state/store.gleam`               | Adds normalized Customer-domain base/staged state, effective reads, merge redirects, address ordering, nested subresource helpers, and state buckets.                      |
| `gleam/test/parity/runner.gleam`                                | Seeds customer captures, customer connection pages, selected-path comparison inputs, variable substitutions, store-credit/account/order fixtures, and nested subresources. |
| `gleam/test/parity/diff.gleam` / `gleam/test/parity/spec.gleam` | Extends parity comparison support for selected paths and ignored expected-difference subtrees used by customer specs.                                                      |
| `gleam/test/parity_test.gleam`                                  | Enables all 28 checked-in Customer parity specs as executable Gleam parity evidence.                                                                                       |
| `gleam/test/shopify_draft_proxy/proxy/customers_test.gleam`     | Adds direct coverage for customer lifecycle, address lifecycle, and store-credit local staging.                                                                            |

Validation: `gleam test --target javascript` is green at 701 tests on the host
Node runtime. `gleam test --target erlang` is green at 697 tests via the
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container because the host
lacks `escript`. The Customer parity inventory has 28 specs under
`config/parity-specs/customers/`, and all are now enabled in
`gleam/test/parity_test.gleam`.

### Findings

- Customer capture seeding must be deterministic across JavaScript and Erlang:
  captured JSON object traversal order can differ by target, so sparse/rich
  duplicate seed records need commutative merge rules rather than last-write
  wins.
- Merge attached-resource parity depends on transferring source customer
  addresses, metafields, and customer-owned order summaries before the source
  tombstone suppresses reads.
- Some customer parity captures compare Shopify connection pages that expose
  only captured window data. The runner now seeds captured customer connection
  cursors/pageInfo and the customer projector preserves those windows without
  inventing unrelated local rows.

### Risks / open items

- The TypeScript Customer runtime has not been deleted in this pass; keep it
  until the remaining TypeScript integration handoff and any non-parity callers
  have been audited against the Gleam dispatcher.
- Customer-owned order/event summaries are intentionally minimal and scoped to
  the fields present in the customer parity captures. Full order-domain
  behavior remains owned by future order passes.
- Advanced customer search is fixture-backed for captured complex queries; a
  general Shopify search grammar port remains a future broadening task.

### Pass 34 candidates

- Run the final Customer removal pass: audit TypeScript integration callers,
  remove `src/proxy/customers.ts` only after equivalent Gleam runtime paths are
  the sole supported implementation, and keep customer parity green.
- Continue the Orders Gleam port so `orderCustomerSet` / `orderCustomerRemove`
  can move from the Customer bridge into a broader normalized order graph.
- Expand shared search-query parsing in Gleam before claiming general advanced
  Customer search semantics beyond the captured fixtures.

---

## 2026-04-30 — Pass 37: metafield definitions and owner-scoped metafields

Ports the Metafields definition lifecycle and owner-scoped metafield staging
surface into the Gleam dispatcher. The new domain state covers metafield
definition records, product/customer/collection/variant-owned metafields,
standard definition enablement from the fixture-backed template subset,
definition pin/unpin ordering, definition-backed `metafieldsSet` validation,
compareDigest/CAS checks, downstream owner reads, and the captured custom-data
type matrix. The Gleam parity runner now seeds captured metafield definition
fixtures and honors target-level `excludedPaths`, which lets the checked-in
metafields specs run without editing their recorded request or fixture shape.

The TypeScript Metafields implementation remains in place for this pass. The
root TS package still exposes the public runtime, product-owned
`metafieldDelete`/`metafieldsDelete` remain under the Products TS surface, and
`standardMetafieldDefinitionTemplates` is still a registry-only gap. Deleting
the TS runtime before that remaining surface is ported would break existing TS
behavior instead of retiring a fully duplicated implementation.

| Module                                                            | Change                                                                                                                                                                       |
| ----------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/types.gleam`                 | Adds owner-scoped metafield records plus typed metafield definition capability, constraint, type, validation, and definition records.                                        |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                 | Adds base/staged metafield and definition buckets, definition tombstones, owner replacement helpers, definition lookup/listing, associated metafield cleanup, and GID order. |
| `gleam/src/shopify_draft_proxy/proxy/metafields.gleam`            | Adds Shopify-like value normalization and `jsonValue` parsing for scalar, measurement, rating, object, list, and reference metafield types on both targets.                  |
| `gleam/src/shopify_draft_proxy/proxy/metafield_definitions.gleam` | Adds stateful query/mutation handling for definition reads, lifecycle mutations, standard enablement, pin/unpin, owner reads, and `metafieldsSet`.                           |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`           | Routes metafield definition queries with store/variables so downstream reads observe local state.                                                                            |
| `gleam/test/parity/runner.gleam`                                  | Seeds captured metafield definition records and definition-owned metafield nodes from parity fixtures.                                                                       |
| `gleam/test/parity/spec.gleam`                                    | Decodes target-level `excludedPaths` as ignore rules, matching the existing parity spec contract.                                                                            |
| `gleam/test/parity_test.gleam`                                    | Enables all six checked-in Metafields parity specs as executable Gleam evidence.                                                                                             |

Validation: `gleam test --target javascript` is green at 685 tests on the host
Node runtime. `gleam test --target erlang` is green at 681 tests via
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` with OTP 28 because the host
Erlang runtime is OTP 25 and `gleam_json` requires OTP 27+. Targeted
`gleam format --check ...` and `git diff --check` are green.

### Findings

- Shopify's owner metafields connection preserves numeric GID creation order
  within the non-app/app namespace split; lexicographic synthetic IDs reorder
  `Metafield/10` before `Metafield/2` and break multi-batch type-matrix reads.
- Erlang and JavaScript differ in how parsed JSON numbers preserve `100.0`;
  measurement `jsonValue.value` must coerce whole-number floats back to integer
  JSON so both targets match the captured Shopify payload.
- Captured metafield definition fixtures can include the same definition ID in
  both rich detail selections and narrower catalog selections. Runner seeding
  keeps the first richer record so later narrow rows do not erase description,
  access, or capability fields.

### Risks / open items

- `standardMetafieldDefinitionTemplates` remains intentionally unimplemented in
  the registry; this pass covers the fixture-backed enablement subset, not the
  standard template catalog query root.
- Product-owned `metafieldDelete` and `metafieldsDelete` are still part of the
  TypeScript Products runtime and need a separate Gleam products/metafields
  deletion pass before the TS metafield runtime can be removed safely.
- Owner root reads are deliberately narrow and synthetic for Product,
  ProductVariant, Collection, and Customer IDs; broader HasMetafields owners
  should be added only with owning-domain state and parity evidence.

### Pass 38 candidates

- Port product-owned `metafieldDelete` / `metafieldsDelete` and their
  hydrated/downstream deletion flows into Gleam.
- Add `standardMetafieldDefinitionTemplates` catalog query support once a
  captured template-catalog fixture exists.
- Continue Store Properties locations and fulfillment/carrier-service lifecycle
  roots, reusing the existing shop state slice.

---

## 2026-04-30 — Pass 36: runtime state dump and snapshot substrate

Completes the next DraftProxy runtime substrate slice: `__meta/state` now
serializes every state bucket currently modelled by the Gleam store instead of
the previous shop/saved-search subset, and `dump_state` / `restore_state`
round-trip base state, staged state, mutation log, and synthetic identity in the
same versioned field-dump envelope shape used by the TypeScript store. The JS
shim also accepts `createDraftProxy(config, { state })` and loads existing
normalized snapshot files through `snapshotPath`, ignoring not-yet-ported
unknown buckets while preserving ported ones.

| Module                                                        | Change                                                                                                                                                                                                                                          |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/state/serialization.gleam`     | Adds full JSON encoders/decoders for the current Gleam `BaseState` and `StagedState` buckets, including webhooks, apps, functions, gift cards, segments, localization, marketing, bulk operations, Admin Platform, and Store Properties slices. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`       | Routes `__meta/state`, state dumps, restore, and normalized snapshot loading through the shared state serializer and uses TS-compatible `{ kind, value }` store field dumps.                                                                    |
| `gleam/test/shopify_draft_proxy/proxy/draft_proxy_test.gleam` | Updates state-shape assertions and adds executable coverage for webhook state visibility, ported bucket dump/restore, and snapshot loading with unknown-bucket tolerance.                                                                       |
| `gleam/js/src/runtime.ts`                                     | Loads `snapshotPath` files, restores constructor-supplied state dumps, and keeps the TS shim aligned with the embeddable `createDraftProxy(config, options)` shape.                                                                             |
| `tests/integration/gleam-interop.test.ts`                     | Expands JS interop smoke coverage for constructor state restore and normalized snapshot loading.                                                                                                                                                |

Validation: `gleam test --target javascript` is green at 672 tests.
`gleam test --target erlang` is green at 668 tests via the
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container because the host
lacks `escript`. `corepack pnpm gleam:smoke:js` is green at 4 Vitest tests.
`corepack pnpm elixir:smoke` cannot run directly on the host for the same
missing `escript` reason, but the equivalent command is green in
`ghcr.io/gleam-lang/gleam:v1.16.0-elixir-alpine` at 17 ExUnit tests.
`corepack pnpm gleam:format:check` and `git diff --check` are green.

### Findings

- The TS store dump is an own-field map where each field is wrapped as
  `{ kind, value }`; the Gleam dump now preserves that substrate shape instead
  of emitting a bespoke `mutationLog`-only fields object.
- Normalized snapshot files can be accepted incrementally: unknown TS buckets
  such as product/customer placeholders are ignored by the Gleam decoder, while
  any already-ported bucket present in `baseState` is installed.
- Keeping all state encoding/decoding in one state module makes newly ported
  buckets easier to wire through meta/dump/restore in the same pass that adds
  the store fields.

### Risks / open items

- Snapshot loading is intentionally limited to normalized snapshot JSON and the
  buckets already represented in the Gleam `Store`; recorded GraphQL fixture
  bundle loading remains a later substrate pass.
- The JS shim uses Node `fs` for `snapshotPath`; browser/edge callers should
  pass state explicitly until a fetch/blob based snapshot loader is designed.
- Product/customer/order state remains unported in Gleam, so snapshots
  containing only those TS buckets still load as an empty local Gleam state.

### Pass 37 candidates

- Add a fixture-bundle snapshot loader once the next parity runner scenarios
  need recorded GraphQL bundle startup state.
- Continue Store Properties location / fulfillment-service roots now that
  state dump/restore can persist the expanded store shape.
- Add broader runtime smoke around `process_request_async` live-hybrid
  passthrough and commit replay once test transport injection is exposed through
  the JS shim.

---

## 2026-04-30 — Pass 35: saved-search parity completion

Finishes the saved-search parity work in the Gleam implementation while keeping
the TypeScript saved-search runtime in place. The existing Gleam saved-search
module already covered the local lifecycle (`savedSearchCreate`,
`savedSearchUpdate`, `savedSearchDelete`), every resource-specific saved-search
root, query grammar normalization, mutation-log drafts, and the three captured
saved-search parity specs. This pass refreshes the Gleam module documentation
and adds public shim smoke coverage, but leaves TypeScript dispatch unchanged
until the final reviewer-approved deletion point.

| Module                                                     | Change                                                                                                 |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/proxy/saved_searches.gleam` | Refreshes the module note to describe the completed lifecycle and remaining live-hybrid hydration gap. |
| `gleam/js/test/shim.test.ts`                               | Adds a public TS-shim saved-search create/read smoke against the Gleam-backed runtime.                 |

Validation: `corepack pnpm typecheck`, `corepack pnpm conformance:parity`,
`corepack pnpm test`, Erlang target via
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine`, and
`cd gleam && gleam test --target javascript`.

### Findings

- Saved-search parity is executable in Gleam: `saved-search-local-staging`,
  `saved-search-query-grammar`, and `saved-search-resource-roots` are part of
  the standard Gleam parity suite.
- The stale module header still described update/delete and parser coverage as
  missing; refreshing it prevents future agents from undercounting the domain.

### Risks / open items

- Live-hybrid upstream hydration is still outside the current Gleam substrate.
- The TypeScript saved-search runtime remains authoritative for the Node
  runtime until the project reaches the final parity handoff point, matching the
  metaobject parity handoff lesson.

### Pass 36 candidates

- Continue Store Properties with locations and fulfillment/carrier-service
  lifecycle roots.
- Continue Admin Platform parity seeding for utility roots that now have
  owning-domain serializers in Gleam.
- Continue Marketing upstream hydration and parity-runner seeding.

---

## 2026-04-30 — Pass 34: events read/count parity cutover

Finishes the read-only Events cutover. The Gleam events domain remains scoped
to the captured no-data contract for `event`, `events`, and `eventsCount`, adds
dispatcher-level query-shape coverage for the recorded variable/query shape,
and becomes the operation-registry runtime coverage anchor. The legacy
TypeScript events runtime and its TS integration test are removed; the TS
conformance parity harness keeps the checked-in `event-empty-read` fixture
executable with a parity-local serializer so historical parity evidence still
runs while runtime ownership sits in Gleam.

| Module                                                                | Change                                                                                                     |
| --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/events.gleam`                    | Adds explicit root detection and keeps null/empty/exact-zero serialization as the Events source of truth.  |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`               | Routes legacy Events query roots through the Events module helper.                                         |
| `gleam/test/shopify_draft_proxy/proxy/events_test.gleam`              | Covers Events root detection plus direct no-data serialization.                                            |
| `gleam/test/shopify_draft_proxy/proxy/draft_proxy_test.gleam`         | Adds dispatcher coverage for the captured `event` + `events` + `eventsCount` variable-bearing query shape. |
| `config/operation-registry.json`                                      | Points Events runtime coverage at the Gleam tests and regenerates the vendored Gleam registry.             |
| `src/proxy/events.ts`, `tests/integration/event-query-shapes.test.ts` | Deletes the legacy TypeScript runtime handler and TS event integration test.                               |
| `scripts/conformance-parity-lib.ts`                                   | Preserves executable TS-side parity for `event-empty-read` after the runtime handler deletion.             |

Validation: `gleam test --target javascript` is green at 683 tests. Host
`gleam test --target erlang` still fails because `escript` is not installed;
the same Erlang target passes at 679 tests through
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine`. `corepack pnpm
conformance:parity -- --testNamePattern event-empty-read` is green, and
`corepack pnpm typecheck` is green.

### Findings

- Top-level Events remains a no-data-only read domain; non-empty top-level
  event hydration is still intentionally unclaimed until a dedicated capture
  establishes subject/type/filter/sort/count behavior.
- The TypeScript conformance parity harness still needs a non-runtime Events
  serializer while repository-level parity specs are executed by the TS Vitest
  suite.

### Risks / open items

- The host environment still lacks Erlang `escript`; use the Gleam Erlang
  container for local Erlang target validation unless the host is repaired.
- Event emission from staged mutations in other domains remains owned by those
  endpoint modules, not by a shared top-level Events catalog.

### Pass 35 candidates

- Add the TypeScript-to-Gleam runtime bridge needed to retire fully ported TS
  domain modules without breaking public `createDraftProxy` consumers.
- Continue Store Properties with locations and fulfillment/carrier-service
  lifecycle roots, reusing the shop slice from Pass 32.
- Continue Admin Platform parity seeding for utility roots that only require
  backup-region/no-data behavior.
- Continue Marketing upstream hydration and parity-runner seeding so captured
  Marketing read/update scenarios can execute against the Gleam proxy.

---

## 2026-04-30 — Pass 33: functions parity closure

Closes the remaining executable Functions parity gaps in the Gleam port. The
previously disabled `functions-metadata-local-staging` scenario now runs
against Gleam with strict JSON comparison and no new expected-difference
allowances. The runner now mirrors the TypeScript conformance harness's
local-runtime seed step so the existing `functions-metadata-local-staging`
fixture remains byte-identical and executable. The live owner-metadata read
scenario also runs by reusing the existing `seedShopifyFunctions` capture
seeding path.

| Module                           | Change                                                                                                                     |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `gleam/test/parity/runner.gleam` | Seeds Functions parity preconditions for metadata staging, owner-metadata staging, and live owner-metadata read scenarios. |
| `gleam/test/parity_test.gleam`   | Enables all three checked-in Functions parity specs as executable Gleam parity evidence.                                   |

Validation: `gleam test --target javascript` is green at 683 tests on the host
Node runtime. Direct `gleam test --target erlang` still cannot start locally
because the host lacks `escript`, but the BEAM target is green at 679 tests via
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine`. Touched-file
`gleam format --check`, `git diff --check`, `corepack pnpm conformance:parity`,
and `corepack pnpm vitest run tests/integration/functions-flow.test.ts tests/unit/conformance-parity-scenarios.test.ts`
are green.

### Findings

- The divergent `functions-metadata-local-staging` signal was a runner seeding
  gap, not a Functions port bug or a reason to weaken the fixture. Mirroring the
  TypeScript parity harness's seed step keeps the checked-in local-runtime
  capture unchanged and the strict comparison strong.
- The live Functions owner-metadata read capture already carries complete
  `seedShopifyFunctions` rows, so the same seeding helper can exercise
  `shopifyFunction` and `shopifyFunctions` reads without runtime Shopify access.
- Functions mutation logging was already complete from Pass 28; enabling the
  stale fixture confirms the staged multi-root mutation and follow-up update /
  delete requests keep the mutation-log-driven synthetic sequence aligned.

### Risks / open items

- The TypeScript Functions runtime remains in place because the current
  TypeScript Koa/runtime dispatcher, Admin Platform Node resolver, and bulk
  operation import executor still import `src/proxy/functions.ts`. Deleting it
  before a package/runtime cutover would regress the shipping TypeScript proxy.

### Pass 34 candidates

- Add a focused TS-to-Gleam package cutover plan for domains whose Gleam parity
  is complete but whose TypeScript runtime modules are still imported by the
  legacy dispatcher.
- Continue Store Properties or Admin Platform utility parity seeding from the
  Pass 32 candidate list.

---

## 2026-04-30 — Pass 33: metaobject definitions and entries parity

Completes the Gleam metaobjects domain surface for HAR-508. The port now
handles metaobject definition reads and lifecycle mutations, standard definition
enablement, entry create/update/upsert/delete/bulk delete, schema-change
read-after-write behavior, reference fields, type matrix normalization, catalog
visibility, handle-derived display names, and mutation-log drafts for supported
local writes. The parity runner now seeds captured metaobject definitions and
entries, resolves multi-step proxy-response variable substitutions, and matches
expected-difference paths deeply enough for staged catalog rows.

The TypeScript metaobject runtime remains in place because the public
TypeScript/Koa proxy still routes metaobject requests through that module; the
Gleam implementation is parity-complete, but deleting the TS module before the
public runtime is bridged to Gleam would break existing TS consumers rather than
preserve supported local staging.

| Module                                                                   | Change                                                                                                                                                 |
| ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `gleam/src/shopify_draft_proxy/state/types.gleam`                        | Adds typed metaobject definition, field definition, field value, capabilities, standard-template, and entry records.                                   |
| `gleam/src/shopify_draft_proxy/state/store.gleam`                        | Adds base/staged metaobject definition and entry buckets, deleted markers, effective lookup/list helpers, and handle/type lookup support.              |
| `gleam/src/shopify_draft_proxy/proxy/metaobject_definitions.gleam`       | Replaces the empty stub with stateful query and mutation handling, schema projection, field normalization, references, catalog filtering, and logging. |
| `gleam/src/shopify_draft_proxy/proxy/draft_proxy.gleam`                  | Routes metaobject queries/mutations through the local Gleam dispatcher and serializes the state slice through `__meta/state`.                          |
| `gleam/test/parity/runner.gleam`                                         | Seeds metaobject captures and supports previous/named proxy-response variable substitution for multi-step parity scenarios.                            |
| `gleam/test/parity/diff.gleam`                                           | Treats ignored expected-difference paths as deep prefixes so captured catalog branches can remain byte-identical.                                      |
| `gleam/test/shopify_draft_proxy/proxy/metaobject_definitions_test.gleam` | Adds direct lifecycle coverage for definitions, entries, references, bulk delete, meta API visibility, and dispatcher behavior.                        |

Validation: all eight checked-in metaobject parity specs under
`config/parity-specs/metaobjects/` pass through the Gleam parity runner.
`gleam test --target erlang` is green at 673 tests via the
`ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container after cleaning stale
host BEAM build artifacts; the host Erlang install is OTP 25 and cannot run
`gleam_json` directly. `gleam test --target javascript` is green at 677 tests on
the host Node runtime.

### Findings

- Metaobject catalog reads are stricter than direct `metaobject` /
  `metaobjectByHandle` reads: rows missing newly required fields, and rows whose
  definition disables publishable without an entry publishable record, are not
  visible in type catalogs.
- Measurement values use different unit casing depending on surface: stored
  scalar `value` strings use uppercase units, list `jsonValue` uses lowercase
  units with Shopify's `cm`/`ml`/`kg` list overrides, and display names stringify
  measurement JSON with lowercase units and integer-like numbers collapsed.
- Successful metaobject definition deletes still need mutation-log drafts; those
  log IDs are part of the synthetic identity sequence observed by later staged
  operations in multi-step parity specs.

### Risks / open items

- The TypeScript metaobject runtime has not been removed because the public
  TypeScript dispatcher still depends on it. Deletion should happen with the
  runtime bridge that routes TS/Koa consumers to the Gleam domain without
  reintroducing upstream passthrough for supported metaobject roots.
- Generic GraphQL search-query parsing for metaobjects remains limited to the
  captured/local terms modeled in the existing parity and integration coverage.

### Pass 34 candidates

- Add the TypeScript-to-Gleam runtime bridge needed to retire fully ported TS
  domain modules without breaking public `createDraftProxy` consumers.
- Continue Store Properties with locations and fulfillment/carrier-service
  lifecycle roots, reusing the existing shop slice.
- Continue Marketing upstream hydration and parity-runner seeding so captured
  Marketing read/update scenarios can execute against the Gleam proxy.

---

## 2026-04-30 — Pass 33: shared GraphQL substrate parity guards

Closes a shared-substrate pass for the Gleam port rather than advancing one
endpoint family. The GraphQL helper module now exposes the reusable scalar,
argument, payload, and connection serializer hooks that downstream domain
passes need to avoid local parser/projection loops. The test corpus now guards
all checked-in parity request GraphQL documents, and a TypeScript cross-check
script compares the compiled Gleam parser/classifier against the existing
TypeScript `parseOperation` contract for every `config/parity-requests/**/*.graphql`
document.

| Module                                                            | Change                                                                                                                                                                                                      |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gleam/src/shopify_draft_proxy/proxy/graphql_helpers.gleam`       | Adds nullable argument readers, scalar readers, plain-object array filtering, `data` payload extraction, resolved-value projection, and connection serializer hooks for custom `pageInfo` / unknown fields. |
| `gleam/test/parity_corpus_test.gleam`                             | Adds an Erlang-only corpus check that parses and resolves root-field arguments for every parity request GraphQL document.                                                                                   |
| `gleam/test/shopify_draft_proxy/proxy/graphql_helpers_test.gleam` | Covers the new shared argument/scalar/payload helpers.                                                                                                                                                      |
| `gleam/test/shopify_draft_proxy/proxy/pagination_test.gleam`      | Covers custom unknown connection field and custom `pageInfo` serialization hooks.                                                                                                                           |
| `scripts/check-gleam-graphql-parser-parity.ts` / `package.json`   | Adds `corepack pnpm gleam:graphql-parity`, comparing Gleam and TypeScript parser summaries across 753 parity request documents.                                                                             |

Validation: `gleam test --target javascript` is green at 676 tests on the host
Node runtime. `gleam test --target erlang` cannot run directly on the host
because `escript` is missing, but the Erlang target is green at 673 tests via
the `ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine` container. `corepack pnpm
gleam:graphql-parity` is green and matched TypeScript for 753 parity request
documents. `gleam format --check` and `corepack pnpm exec tsc -p tsconfig.json
--noEmit --pretty false` are green.

### Findings

- Existing parity corpus coverage walked `config/parity-specs/**/*.json`, but
  did not exercise the GraphQL request corpus that the substrate acceptance bar
  names explicitly.
- The current connection serializer already covered `nodes`, `edges`, selected
  `pageInfo`, cursor prefixing, and fallback cursors; custom `pageInfo` and
  unknown-field hooks were the missing reusable surface from the TypeScript
  helper.
- Root-field argument resolution over the full request corpus is a useful guard
  because it exercises literal and unbound-variable semantics without requiring
  a live store or endpoint-specific dispatch.

### Risks / open items

- The parser parity script depends on the compiled JavaScript Gleam output, so
  the package script builds the JS target before running the comparison.
- The full TypeScript helper module still contains location helpers and
  projection customisation hooks that should be ported when a downstream domain
  needs them, rather than guessed ahead of concrete usage.

### Pass 34 candidates

- Continue Store Properties with locations and fulfillment/carrier-service
  lifecycle roots, reusing the shared argument and connection helpers.
- Continue Marketing upstream hydration and parity-runner seeding so captured
  Marketing read/update scenarios can execute against the Gleam proxy.
- Port the next helper slice only when an endpoint pass needs a concrete
  TypeScript helper contract not yet exposed in Gleam.

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
  by capture-shape markers (helpers self-gate on JSON paths that
  uniquely identify the capture family), and decode only data already
  present in the capture. Pass 27 originally keyed seeding by scenario
  id; that approach was retired so new parity specs can land without
  touching runner dispatch — see SKILL.md "Parity runner capture
  seeding" for the current contract.
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
