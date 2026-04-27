# Discounts Endpoint Group

The discounts group has catalog-first local read support. Keep discount-specific capture, access-scope, and compatibility notes here instead of in `docs/architecture.md`.

## Local read support and staged native lifecycle support

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

Staged native BXGY lifecycle mutations:

- `discountCodeBxgyCreate`
- `discountCodeBxgyUpdate`
- `discountAutomaticBxgyCreate`
- `discountAutomaticBxgyUpdate`
- shared code lifecycle roots for staged BXGY code discounts:
  - `discountCodeActivate`
  - `discountCodeDeactivate`
  - `discountCodeDelete`
- shared automatic lifecycle roots for staged BXGY automatic discounts:
  - `discountAutomaticActivate`
  - `discountAutomaticDeactivate`
  - `discountAutomaticDelete`

Staged native free-shipping lifecycle mutations:

- `discountCodeFreeShippingCreate`
- `discountCodeFreeShippingUpdate`
- `discountAutomaticFreeShippingCreate`
- `discountAutomaticFreeShippingUpdate`
- shared `discountCodeActivate` / `discountCodeDeactivate` / `discountCodeDelete`
- shared `discountAutomaticActivate` / `discountAutomaticDeactivate` / `discountAutomaticDelete`

Captured mutation validation guardrails, not implemented lifecycle support:

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
- Supported detail serialization covers common native code/automatic fields captured from Shopify 2026-04: title, status, summary, starts/ends timestamps, created/updated timestamps, usage counts, discount classes, `combinesWith`, redeem-code connections, all-buyer context, `customerGets`, BXGY `customerBuys`, minimum subtotal/quantity requirements, metafields, and events.
- Snapshot misses for singular roots return `null`. Known detail records with no metafields/events return Shopify-like empty connections and null singular metafields.
- Product, variant, collection, customer, and segment links are represented as normalized IDs on discount item/context selections; serializers expose selected `id` fields and hydrate simple titles/display names when the linked resource is already present in local state.
- App-managed discount objects (`DiscountCodeApp`, `DiscountAutomaticApp`) preserve `__typename`, title/status timestamps, usage counts, `combinesWith`, classes, code connections, and any captured app-managed metadata present in normalized state.
- `appDiscountType`, `discountId`, and `errorHistory` are read-only captured fixture fields. When `appDiscountType` is present, serializers preserve selected scalar/object fields such as `appKey`, `functionId`, `title`, and `description`; when a non-null Shopify app field has not been captured, the serializer returns `null` and emits `UNSUPPORTED_APP_DISCOUNT_FIELD` instead of inventing Function metadata.
- Discount query parsing uses the shared Shopify-style search parser.
- Local connection cursors use the proxy's synthetic `cursor:<gid>` form; parity specs document Shopify's opaque cursor values as non-contractual.
- `codeDiscountNodes` remains a known registry entry but is not promoted to locally implemented support until its node-specific shape has captured fixtures.
- Deprecated `automaticDiscounts` remains unsupported rather than mapped to `automaticDiscountNodes`; unknown/unsupported reads continue through the existing passthrough path outside snapshot-only parity execution.
- Native `DiscountCodeBasic` lifecycle support is implemented for create, update, activate, deactivate, and delete. These supported roots stage locally in memory, synthesize stable `DiscountCodeNode` / `DiscountRedeemCode` IDs and timestamps for the session, and are replayed through `__meta/commit` from the original raw mutation bodies in log order.
- `discountRedeemCodeBulkAdd` and the introspected `discountCodeRedeemCodeBulkDelete` root have a locally handled safe subset for code-basic redeem-code staging, but remain unimplemented in the registry until full async lifecycle and failure semantics are conformance-backed. Bulk add accepts explicit code lists, appends stable local `DiscountRedeemCode` rows, returns a completed `DiscountRedeemCodeBulkCreation` shape for selected fields such as `id`, `done`, `codesCount`, `importedCount`, and `failedCount`, and makes the new codes visible through detail `codes`, `codesCount`, `codeDiscountNodeByCode`, and catalog reads. Bulk delete is intentionally limited to explicit redeem-code IDs on a known code discount; it removes those local code rows, returns a completed `Job` shape for selected fields such as `id`, `done`, and `query`, and refuses search/saved-search selectors locally to avoid broad destructive writes.
- Code-basic creates and updates interpret percentage values and fixed-amount values, all-buyer context, all-items/product/collection item targeting, minimum subtotal/quantity requirements, `combinesWith`, starts/ends timestamps, and redeem code changes into normalized `DiscountRecord` state.
- `discountCodeDeactivate` marks the staged code discount `EXPIRED`, `discountCodeActivate` marks it `ACTIVE`, and `discountCodeDelete` records the deleted ID so singular reads return `null` and catalog/count reads omit the discount.
- Automatic basic amount-off lifecycle mutations are staged locally and never sent upstream at runtime once they pass captured validation guardrails. Creates and updates interpret percentage and fixed-amount `customerGets`, all-buyer context, all/products/collections item selections, `combinesWith`, minimum subtotal/quantity requirements, `startsAt`, and `endsAt` into normalized `DiscountAutomaticBasic` records.
- Automatic basic status is derived from staged timestamps at write time: future `startsAt` records are `SCHEDULED`, elapsed `endsAt` records are `EXPIRED`, and otherwise-visible records are `ACTIVE`. `discountAutomaticActivate` moves scheduled records to the current staged timestamp and clears elapsed `endsAt`; `discountAutomaticDeactivate` sets `endsAt` to the current staged timestamp and returns `EXPIRED`; `discountAutomaticDelete` removes the record from effective reads while preserving the staged deletion marker in meta state.
- BXGY code and automatic create/update mutations are staged locally and never sent upstream at runtime once they pass captured validation guardrails. Creates and updates interpret `customerBuys.value.quantity`, `customerGets.value.discountOnQuantity`, product/variant/collection item selections, all-buyer context, `combinesWith`, `startsAt`, `endsAt`, redeem codes for code discounts, and per-order limits into normalized `DiscountCodeBxgy` / `DiscountAutomaticBxgy` records.
- BXGY product, variant, and collection links are stored as the same normalized ID arrays used by other native discount item selections. Downstream `discountNode`, `codeDiscountNode`, `codeDiscountNodeByCode`, `automaticDiscountNode`, `discountNodes`, `automaticDiscountNodes`, and `discountNodesCount` reads expose staged BXGY records through selected fields and hydrate simple linked-resource titles when those resources exist in local product/collection state.
- BXGY support is intentionally limited to Admin GraphQL staging and read-after-write visibility. It does not calculate checkout prices, enforce cart eligibility, or claim storefront discount application semantics.
- Free-shipping code and automatic lifecycle mutations stage native `DiscountCodeFreeShipping` and `DiscountAutomaticFreeShipping` records locally without upstream writes. The free-shipping model is separate from amount-off `customerGets`: it stores shipping `destinationSelection`, `maximumShippingPrice`, minimum subtotal/quantity requirements, `combinesWith`, one-time/subscription applicability, `recurringCycleLimit`, code-only `appliesOncePerCustomer` / `usageLimit`, and status/timestamp fields.
- Free-shipping records appear through aggregate `discountNodes` / `discountNodesCount`, method-specific `codeDiscountNode`, `codeDiscountNodeByCode`, `automaticDiscountNode`, and `automaticDiscountNodes` reads. Checkout/order shipping-rate application is intentionally out of scope; this slice only models Admin GraphQL read-after-write visibility.
- Captured free-shipping validation currently covers invalid `combinesWith`, blank code-discount title, and minimum subtotal+quantity conflicts. Destination and money validation beyond those captured branches should not be invented; add a focused live conformance capture before promoting additional guardrails.
- Broad destructive discount bulk roots (`discountCodeBulkActivate`, `discountCodeBulkDeactivate`, `discountCodeBulkDelete`, and `discountAutomaticBulkDelete`) remain unimplemented. Blank `search` selectors and missing selector cases are refused locally with `DiscountUserError` payloads instead of being silently proxied; non-blank unsupported selector shapes continue through the unsupported passthrough escape hatch with registry metadata in the mutation log.
- Full discount mutation lifecycle support is still not implemented for broad code/automatic bulk, redeem-code async lifecycle, or app-managed discount roots. The mutation roots listed as validation guardrails above remain unimplemented in the operation registry until the proxy can locally emulate their supported lifecycle behavior and downstream read-after-write effects. Captured invalid requests are still answered locally so tests can rely on Shopify-like GraphQL validation and `DiscountUserError` contracts without sending known-bad writes upstream.
- Captured validation branches split into top-level GraphQL errors and mutation-scoped `DiscountUserError` payloads:
  - missing `$input` for `discountCodeBasicCreate` returns top-level `INVALID_VARIABLE`
  - inline `basicCodeDiscount: null` returns top-level `argumentLiteralsIncompatible`
  - duplicate codes, invalid date ranges, invalid product/variant references, unsupported collection+product entitlement combinations, unknown update IDs, invalid BXGY/free-shipping inputs, and mutually exclusive bulk selectors return `userErrors` on the mutation payload
- The 2026-04 validation capture includes a live `currentAppInstallation.accessScopes` probe showing the current grant has `read_discounts` and `write_discounts`. A no-discount-scope access-denied fixture is still not available; local discount handling must never convert any future `ACCESS_DENIED` capture into successful staging.
- Code-basic lifecycle tests cover create-read-update-read-activate/deactivate-delete flows, meta log/state inspection, and commit replay mapping from synthetic IDs to authoritative Shopify IDs for later staged mutations. Redeem-code bulk tests cover add/delete read-after-write behavior, completed local job payloads, broad destructive local refusal, unsupported passthrough observability, and commit replay order. The live 2026-04 capture fixture `discount-code-basic-lifecycle.json` now runs through executable `captured-vs-proxy-request` parity for create, successful update with Shopify's current `discountAmount` input field, deactivate, activate, delete, and downstream reads. The parity contract compares deterministic selected fields while runtime tests continue to cover synthetic IDs/timestamps, generated summaries, and commit replay. Do not treat the remaining validation guardrails as full support for BXGY, app-discount, or broad bulk job happy paths.
- Automatic-basic lifecycle tests cover create-read-update-activate/deactivate-delete flows and downstream reads. The live 2026-04 fixtures `discount-automatic-basic-lifecycle.json` and `discount-automatic-basic-nodes-read.json` anchor the automatic lifecycle payload and read shapes.
- BXGY lifecycle tests cover code and automatic create-read-update-activate/deactivate-delete flows, product/variant/collection reference serialization, and local invalid reference userErrors. Existing validation captures anchor blank-title/all-items guardrails; broader happy-path live BXGY fixtures should be captured before tightening Shopify-specific summaries or edge validation.
- Free-shipping lifecycle tests cover code and automatic create-read-update-read-activate/deactivate-delete flows, including destination selection, maximum shipping price, minimum subtotal, one-time/subscription flags, and mutation-log operation order. The live 2026-04 fixture `discount-free-shipping-lifecycle.json` now runs through executable `captured-vs-proxy-request` parity for code and automatic create, update, shared activate/deactivate/delete, and singular downstream reads. The parity contract intentionally avoids pre-existing catalog row counts from the capture store while preserving strict comparison for the staged lifecycle fields it selects.
- Future discount mutation lifecycle support can reuse the staged discount graph exposed by the store so locally staged discount mutations can appear in catalog/count reads without upstream writes. Do not treat the current validation guardrails as full support for app-managed discounts, broad bulk jobs, or checkout price calculation.
- App-discount create/update mutation roots are explicitly classified as registry-only, unimplemented local-staging gaps rather than supported passthrough. In normal runtime they still take the unsupported mutation escape hatch and would hit Shopify; the mutation log includes a `registeredOperation` record plus `unsupported-app-discount-function-mutation` safety metadata so operators can distinguish them from supported local staging.
- The current safety stance is unsupported passthrough with loud observability. Supporting app-discount writes later requires conformance-backed staging for the specific Function-backed shape, including captured `appDiscountType.functionId` / app identity metadata, and must not execute external Shopify Function logic during proxy runtime.
- `scripts/capture-discount-conformance.ts` probes the live conformance app Admin access scopes through `currentAppInstallation.accessScopes`.
- The capture script records `read_discounts` and `write_discounts` availability before attempting discount catalog captures.
- The capture script also creates temporary native `DiscountCodeBasic` and `DiscountAutomaticBasic` records, captures singular detail payloads, and deletes those temporary records immediately after capture.
- `scripts/capture-discount-code-basic-lifecycle-conformance.ts` records native code-basic create/update/deactivate/activate/delete lifecycle evidence against Admin GraphQL 2026-04 and deletes the temporary discount after capture.
- `scripts/capture-discount-bxgy-lifecycle-conformance.ts` records native code and automatic BXGY create/update/deactivate/activate/delete lifecycle evidence with temporary product, variant, and collection prerequisites against Admin GraphQL 2026-04, then deletes the temporary discounts and prerequisite resources after capture.
- `scripts/capture-discount-free-shipping-lifecycle-conformance.ts` records native code and automatic free-shipping create/update/deactivate/activate/delete lifecycle evidence against Admin GraphQL 2026-04 and deletes the temporary discounts after capture.
- `scripts/capture-discount-validation-conformance.ts` creates a temporary native `DiscountCodeBasic` only to settle the duplicate-code branch, captures representative validation failures, and deletes the seed discount immediately.
- The current discount capture script does not create app-managed discounts. Only capture app-discount read fixtures from an already safe existing app discount or a disposable Function-backed setup with explicit cleanup; do not create app-discount fixtures by invoking unknown merchant Function logic on the shared store.
- Tokens must come through `scripts/shopify-conformance-auth.mts`; repo `.env` files must not contain Admin access tokens.
- Discount capture fails before discount reads or writes when either required discount scope is missing.
- Discount capture files use the `discount-*` conformance naming convention only after scope checks pass.

## Validation anchors

- Discount reads: `tests/integration/discount-query-shapes.test.ts`
- Discount code-basic lifecycle: `tests/integration/discount-code-basic-lifecycle-flow.test.ts`
- Automatic basic lifecycle staging: `tests/integration/discount-automatic-basic-flow.test.ts`
- BXGY lifecycle staging: `tests/integration/discount-bxgy-flow.test.ts`
- Free-shipping lifecycle staging: `tests/integration/discount-free-shipping-lifecycle-flow.test.ts`
- Discount mutation validation: `tests/integration/discount-mutation-validation.test.ts`
- Conformance fixtures and requests: `config/parity-specs/discount*.json` and matching files under `config/parity-requests/`; singular detail fixtures are `discount-code-basic-detail-read.json` and `discount-automatic-basic-detail-read.json`, and lifecycle evidence includes `discount-bxgy-lifecycle.json` plus `discount-free-shipping-lifecycle.json`, under the 2026-04 conformance fixture directory.
- Registry/coverage tests: `tests/unit/operation-registry.test.ts`, `tests/unit/graphql-operation-coverage.test.ts`
- Capture helper tests: `tests/unit/discount-conformance-lib.test.ts`
