# Discounts Endpoint Group

The discounts group has catalog-first local read support. Keep discount-specific capture, access-scope, and compatibility notes here instead of in `docs/architecture.md`.

## Local read support and staged code-basic lifecycle support

Overlay reads:

- `discountNodes`
- `discountNodesCount`
- `discountNode`
- `codeDiscountNode`
- `codeDiscountNodeByCode`
- `automaticDiscountNode`
- `automaticDiscountNodes`

Implemented native automatic-basic lifecycle staging:

- `discountAutomaticBasicCreate`
- `discountAutomaticBasicUpdate`
- `discountAutomaticActivate`
- `discountAutomaticDeactivate`
- `discountAutomaticDelete`

Staged native code-basic lifecycle mutations:

- `discountCodeBasicCreate`
- `discountCodeBasicUpdate`
- `discountCodeActivate`
- `discountCodeDeactivate`
- `discountCodeDelete`

Captured mutation validation guardrails, not implemented lifecycle support:

- `discountCodeBxgyCreate`
- `discountAutomaticBxgyCreate`
- `discountCodeFreeShippingCreate`
- `discountAutomaticFreeShippingCreate`
- `discountCodeBulkDeactivate`
- `discountAutomaticBulkDelete`

## Unsupported roots still tracked by the registry

- `codeDiscountNodes`
- `automaticDiscounts`
- Shopify Functions app-discount write roots:
  - `discountCodeAppCreate`
  - `discountCodeAppUpdate`
  - `discountAutomaticAppCreate`
  - `discountAutomaticAppUpdate`

## Behavior notes

- `discountNodes` and `discountNodesCount` are served locally in snapshot mode from normalized `DiscountRecord` state.
- Singular detail roots are served locally in snapshot mode from the same normalized `DiscountRecord` state:
  - `discountNode(id:)` returns either code or automatic discount records by node ID.
  - `codeDiscountNode(id:)` returns locally known code discounts by ID.
  - `codeDiscountNodeByCode(code:)` indexes locally known redeem codes.
  - `automaticDiscountNode(id:)` returns locally known automatic discounts by ID.
- `automaticDiscountNodes` is served locally from the automatic slice of the normalized discount graph with the same pagination, cursor, and query-filter behavior as aggregate `discountNodes`.
- Supported local catalog behavior includes `first` / `last`, `before` / `after`, `reverse`, `sortKey`, count `limit`, and captured query filters such as `status`, `combines_with`, `discount_class`, `type` / `discount_type`, date fields, `times_used`, and `app_id`.
- Supported detail serialization covers common native code/automatic fields captured from Shopify 2026-04: title, status, summary, starts/ends timestamps, created/updated timestamps, usage counts, discount classes, `combinesWith`, redeem-code connections, all-buyer context, `customerGets`, minimum subtotal/quantity requirements, metafields, and events.
- Snapshot misses for singular roots return `null`. Known detail records with no metafields/events return Shopify-like empty connections and null singular metafields.
- Product, variant, collection, customer, and segment links are represented as normalized IDs on discount item/context selections; serializers expose selected `id` fields and hydrate simple titles/display names when the linked resource is already present in local state.
- App-managed discount objects (`DiscountCodeApp`, `DiscountAutomaticApp`) preserve `__typename`, title/status timestamps, usage counts, `combinesWith`, classes, code connections, and any captured app-managed metadata present in normalized state.
- `appDiscountType`, `discountId`, and `errorHistory` are read-only captured fixture fields. When `appDiscountType` is present, serializers preserve selected scalar/object fields such as `appKey`, `functionId`, `title`, and `description`; when a non-null Shopify app field has not been captured, the serializer returns `null` and emits `UNSUPPORTED_APP_DISCOUNT_FIELD` instead of inventing Function metadata.
- Discount query parsing uses the shared Shopify-style search parser.
- Local connection cursors use the proxy's synthetic `cursor:<gid>` form; parity specs document Shopify's opaque cursor values as non-contractual.
- `codeDiscountNodes` remains a known registry entry but is not promoted to locally implemented support until its node-specific shape has captured fixtures.
- Deprecated `automaticDiscounts` remains unsupported rather than mapped to `automaticDiscountNodes`; unknown/unsupported reads continue through the existing passthrough path outside snapshot-only parity execution.
- Native `DiscountCodeBasic` lifecycle support is implemented for create, update, activate, deactivate, and delete. These supported roots stage locally in memory, synthesize stable `DiscountCodeNode` / `DiscountRedeemCode` IDs and timestamps for the session, and are replayed through `__meta/commit` from the original raw mutation bodies in log order.
- Code-basic creates and updates interpret percentage values and fixed-amount values, all-buyer context, all-items/product/collection item targeting, minimum subtotal/quantity requirements, `combinesWith`, starts/ends timestamps, and redeem code changes into normalized `DiscountRecord` state.
- `discountCodeDeactivate` marks the staged code discount `EXPIRED`, `discountCodeActivate` marks it `ACTIVE`, and `discountCodeDelete` records the deleted ID so singular reads return `null` and catalog/count reads omit the discount.
- Automatic basic amount-off lifecycle mutations are staged locally and never sent upstream at runtime once they pass captured validation guardrails. Creates and updates interpret percentage and fixed-amount `customerGets`, all-buyer context, all/products/collections item selections, `combinesWith`, minimum subtotal/quantity requirements, `startsAt`, and `endsAt` into normalized `DiscountAutomaticBasic` records.
- Automatic basic status is derived from staged timestamps at write time: future `startsAt` records are `SCHEDULED`, elapsed `endsAt` records are `EXPIRED`, and otherwise-visible records are `ACTIVE`. `discountAutomaticActivate` moves scheduled records to the current staged timestamp and clears elapsed `endsAt`; `discountAutomaticDeactivate` sets `endsAt` to the current staged timestamp and returns `EXPIRED`; `discountAutomaticDelete` removes the record from effective reads while preserving the staged deletion marker in meta state.
- Full discount mutation lifecycle support is still not implemented for BXGY, free-shipping, bulk, redeem-code, or app-managed discount roots. The mutation roots listed as validation guardrails above remain unimplemented in the operation registry until the proxy can locally emulate their supported lifecycle behavior and downstream read-after-write effects. Captured invalid requests are still answered locally so tests can rely on Shopify-like GraphQL validation and `DiscountUserError` contracts without sending known-bad writes upstream.
- Captured validation branches split into top-level GraphQL errors and mutation-scoped `DiscountUserError` payloads:
  - missing `$input` for `discountCodeBasicCreate` returns top-level `INVALID_VARIABLE`
  - inline `basicCodeDiscount: null` returns top-level `argumentLiteralsIncompatible`
  - duplicate codes, invalid date ranges, invalid product/variant references, unsupported collection+product entitlement combinations, unknown update IDs, invalid BXGY/free-shipping inputs, and mutually exclusive bulk selectors return `userErrors` on the mutation payload
- The 2026-04 validation capture includes a live `currentAppInstallation.accessScopes` probe showing the current grant has `read_discounts` and `write_discounts`. A no-discount-scope access-denied fixture is still not available; local discount handling must never convert any future `ACCESS_DENIED` capture into successful staging.
- Code-basic lifecycle tests cover create-read-update-read-activate/deactivate-delete flows, meta log/state inspection, and commit replay mapping from synthetic IDs to authoritative Shopify IDs for later staged mutations. The live 2026-04 capture fixture `discount-code-basic-lifecycle.json` anchors the create/update/deactivate/activate/delete payload and downstream read shapes. Do not treat the remaining validation guardrails as full support for BXGY, free-shipping, automatic, app-discount, or bulk job happy paths.
- Automatic-basic lifecycle tests cover create-read-update-activate/deactivate-delete flows and downstream reads. The live 2026-04 fixtures `discount-automatic-basic-lifecycle.json` and `discount-automatic-basic-nodes-read.json` anchor the automatic lifecycle payload and read shapes.
- Future non-code-basic and non-automatic-basic discount mutation lifecycle support can reuse the staged discount graph exposed by the store so locally staged discount mutations can appear in catalog/count reads without upstream writes. Do not treat the current validation guardrails as full support for BXGY, free-shipping, redeem-code, app-managed, or bulk jobs.
- App-discount create/update mutation roots are explicitly classified as registry-only, unimplemented local-staging gaps rather than supported passthrough. In normal runtime they still take the unsupported mutation escape hatch and would hit Shopify; the mutation log includes a `registeredOperation` record plus `unsupported-app-discount-function-mutation` safety metadata so operators can distinguish them from supported local staging.
- The current safety stance is unsupported passthrough with loud observability. Supporting app-discount writes later requires conformance-backed staging for the specific Function-backed shape, including captured `appDiscountType.functionId` / app identity metadata, and must not execute external Shopify Function logic during proxy runtime.
- `scripts/capture-discount-conformance.ts` probes the live conformance app Admin access scopes through `currentAppInstallation.accessScopes`.
- The capture script records `read_discounts` and `write_discounts` availability before attempting discount catalog captures.
- The capture script also creates temporary native `DiscountCodeBasic` and `DiscountAutomaticBasic` records, captures singular detail payloads, and deletes those temporary records immediately after capture.
- `scripts/capture-discount-code-basic-lifecycle-conformance.ts` records native code-basic create/update/deactivate/activate/delete lifecycle evidence against Admin GraphQL 2026-04 and deletes the temporary discount after capture.
- `scripts/capture-discount-validation-conformance.ts` creates a temporary native `DiscountCodeBasic` only to settle the duplicate-code branch, captures representative validation failures, and deletes the seed discount immediately.
- The current discount capture script does not create app-managed discounts. Only capture app-discount read fixtures from an already safe existing app discount or a disposable Function-backed setup with explicit cleanup; do not create app-discount fixtures by invoking unknown merchant Function logic on the shared store.
- Tokens must come through `scripts/shopify-conformance-auth.mts`; repo `.env` files must not contain Admin access tokens.
- Discount capture fails before discount reads or writes when either required discount scope is missing.
- Discount capture files use the `discount-*` conformance naming convention only after scope checks pass.

## Validation anchors

- Discount reads: `tests/integration/discount-query-shapes.test.ts`
- Discount code-basic lifecycle: `tests/integration/discount-code-basic-lifecycle-flow.test.ts`
- Automatic basic lifecycle staging: `tests/integration/discount-automatic-basic-flow.test.ts`
- Discount mutation validation: `tests/integration/discount-mutation-validation.test.ts`
- Conformance fixtures and requests: `config/parity-specs/discount*.json` and matching files under `config/parity-requests/`; singular detail fixtures are `discount-code-basic-detail-read.json` and `discount-automatic-basic-detail-read.json` under the 2026-04 conformance fixture directory.
- Registry/coverage tests: `tests/unit/operation-registry.test.ts`, `tests/unit/graphql-operation-coverage.test.ts`
- Capture helper tests: `tests/unit/discount-conformance-lib.test.ts`
