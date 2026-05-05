# Discounts Endpoint Group

The discounts group has catalog-first local read support. Keep discount-specific capture, access-scope, and compatibility notes here instead of in `docs/architecture.md`.

## Current support and limitations

### Local read support and staged native lifecycle support

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

Staged app-managed discount lifecycle mutations:

- `discountCodeAppCreate`
- `discountCodeAppUpdate`
- `discountAutomaticAppCreate`
- `discountAutomaticAppUpdate`
- shared automatic lifecycle roots for staged app-managed automatic discounts:
  - `discountAutomaticActivate`
  - `discountAutomaticDeactivate`
  - `discountAutomaticDelete`

Staged discount bulk mutations:

- `discountCodeBulkActivate`
- `discountCodeBulkDeactivate`
- `discountCodeBulkDelete`
- `discountAutomaticBulkDelete`

### Unsupported roots still tracked by the registry

- `codeDiscountNodes`
- `automaticDiscounts`

### Behavior notes

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
- Product, variant, collection, customer, and segment links are represented as normalized IDs on discount item/context selections; serializers expose selected `id` fields and hydrate simple titles, customer display names, and segment names when the linked resource is already present in local state.
- App-managed discount objects (`DiscountCodeApp`, `DiscountAutomaticApp`) preserve `__typename`, title/status timestamps, usage counts, `combinesWith`, classes, code connections, and any captured app-managed metadata present in normalized state.
- `appDiscountType`, `discountId`, and `errorHistory` are read-only captured fixture fields. When `appDiscountType` is present, serializers preserve selected scalar/object fields such as `appKey`, `functionId`, `title`, and `description`; when a non-null Shopify app field has not been captured, the serializer returns `null` and emits `UNSUPPORTED_APP_DISCOUNT_FIELD` instead of inventing Function metadata.
- Discount query parsing uses the shared Shopify-style search parser.
- Local connection cursors use the proxy's synthetic `cursor:<gid>` form; parity specs document Shopify's opaque cursor values as non-contractual.
- `codeDiscountNodes` remains a known registry entry but is not promoted to locally implemented support until its node-specific shape has captured fixtures.
- Deprecated `automaticDiscounts` remains unsupported rather than mapped to `automaticDiscountNodes`; unknown/unsupported reads continue through the existing passthrough path outside snapshot-only parity execution.
- Native `DiscountCodeBasic` lifecycle support is implemented for create, update, activate, deactivate, and delete. These supported roots stage locally in memory, synthesize stable `DiscountCodeNode` / `DiscountRedeemCode` IDs and timestamps for the session, and are replayed through `__meta/commit` from the original raw mutation bodies in log order.
- App-managed discount create/update roots stage `DiscountCodeApp` and `DiscountAutomaticApp` records locally when the submitted `functionId` or `functionHandle` resolves to known `ShopifyFunction` metadata in local state. This Function metadata is treated as ownership evidence for Admin GraphQL staging only: the proxy records `appDiscountType` fields such as `appKey`, `functionId`, `title`, and `description`, but it does not execute external Shopify Function logic or claim checkout calculation fidelity. Missing Function metadata returns a local `DiscountUserError` instead of proxying the known app-discount root upstream.
- App-managed code discounts store code/redeem-code rows, `usageLimit`, `combinesWith`, subscription flags, metafields, timestamps, and app metadata; app-managed automatic discounts store automatic-node fields such as `recurringCycleLimit`, subscription flags, timestamps, and app metadata. Both appear through `discountNode`, `codeDiscountNode`, `codeDiscountNodeByCode`, `automaticDiscountNode`, `discountNodes`, and `discountNodesCount` immediately after staging.
- Staged app-managed discounts use the same Admin GraphQL activate/deactivate/delete lifecycle roots as native discounts. Activation first verifies that the discount's captured `appDiscountType.functionId` still resolves to local `ShopifyFunction` metadata; missing Function ownership returns `Discount could not be activated.` with code `INTERNAL_ERROR` and leaves local discount state unchanged.
- `discountCodeDeactivate` and `discountAutomaticDeactivate` expire local discounts by setting `endsAt` to the current synthetic timestamp and moving missing/future `startsAt` to that same timestamp. `discountCodeActivate` and `discountAutomaticActivate` are no-ops for already-active records; otherwise they clear stale elapsed `endsAt` values and move missing/future `startsAt` to the current synthetic timestamp. Unknown activate/deactivate IDs return an `INVALID` userError code.
- `discountRedeemCodeBulkAdd` and the introspected `discountCodeRedeemCodeBulkDelete` root stage locally for code-basic redeem-code changes. Bulk add accepts explicit code lists, appends stable local `DiscountRedeemCode` rows, returns a completed `DiscountRedeemCodeBulkCreation` shape for selected fields such as `id`, `done`, `codesCount`, `importedCount`, and `failedCount`, records durable `discountBulkOperations` state, and makes the new codes visible through detail `codes`, `codesCount`, `codeDiscountNodeByCode`, and catalog reads. Redeem-code bulk delete is intentionally limited to explicit redeem-code IDs on a known code discount; it removes those local code rows, returns a completed `Job` shape for selected fields such as `id`, `done`, and `query`, and refuses search/saved-search selectors locally to avoid broad redeem-code deletion without captured code matching semantics.
- `codeDiscountNodeByCode(code:)` matches known redeem codes case-insensitively, including staged redeem codes added by `discountRedeemCodeBulkAdd`. The live HAR-438 redeem-code bulk fixture verifies lowercase lookup against an uppercase Shopify code before and after local bulk changes.
- Code-basic creates and updates interpret percentage values and fixed-amount values, all-buyer context, all-items/product/collection item targeting, minimum subtotal/quantity requirements, `combinesWith`, starts/ends timestamps, and redeem code changes into normalized `DiscountRecord` state.
- Code-basic discount classes follow Shopify's merchandise-target inference: all-items `customerGets.items` emits `ORDER`, while product, variant, or collection entitlements emit `PRODUCT`. An explicit local `discountClass` input is preserved when present, and the staged payload exposes both plural `discountClasses` and singular `discountClass` for local projections.
- `discountCodeDeactivate` marks the staged code discount `EXPIRED`, `discountCodeActivate` marks it `ACTIVE`, and `discountCodeDelete` records the deleted ID so singular reads return `null` and catalog/count reads omit the discount. Shared activate/deactivate timestamp rewrites follow the behavior note above for all staged code discount types.
- Automatic basic amount-off lifecycle mutations are staged locally and never sent upstream at runtime once they pass captured validation guardrails. Creates and updates interpret percentage and fixed-amount `customerGets`, all-buyer context, all/products/collections item selections, `combinesWith`, minimum subtotal/quantity requirements, `startsAt`, and `endsAt` into normalized `DiscountAutomaticBasic` records.
- Automatic basic status is derived from staged timestamps at write time: future `startsAt` records are `SCHEDULED`, elapsed `endsAt` records are `EXPIRED`, and otherwise-visible records are `ACTIVE`. `discountAutomaticActivate` moves scheduled records to the current staged timestamp and clears elapsed `endsAt`; `discountAutomaticDeactivate` sets `endsAt` to the current staged timestamp and returns `EXPIRED`; `discountAutomaticDelete` removes the record from effective reads while preserving the staged deletion marker in meta state.
- BXGY code and automatic create/update mutations are staged locally and never sent upstream at runtime once they pass captured validation guardrails. Creates and updates interpret `customerBuys.value.quantity`, `customerGets.value.discountOnQuantity`, product/variant/collection item selections, all-buyer context, `combinesWith`, `startsAt`, `endsAt`, redeem codes for code discounts, and per-order limits into normalized `DiscountCodeBxgy` / `DiscountAutomaticBxgy` records.
- Captured 2026-04 BXGY validation rejects `customerGets.value.percentage` and `customerGets.value.discountAmount` for both code and automatic BXGY with `INVALID`, because only `discountOnQuantity` is accepted for BXGY entitlements. Code BXGY also returns Shopify's captured `customerGets.value.discountOnQuantity.quantity` blank error when the submitted value branch omits `discountOnQuantity`. Both code and automatic BXGY reject `customerGets.appliesOnSubscription` and `customerGets.appliesOnOneTimePurchase`; automatic variants use the message `This field is not supported by automatic bxgy discounts.`
- BXGY product, variant, and collection links are stored as the same normalized ID arrays used by other native discount item selections. BXGY defaults to the `PRODUCT` discount class. Downstream `discountNode`, `codeDiscountNode`, `codeDiscountNodeByCode`, `automaticDiscountNode`, `discountNodes`, `automaticDiscountNodes`, and `discountNodesCount` reads expose staged BXGY records through selected fields and hydrate simple linked-resource titles when those resources exist in local product/collection state.
- BXGY support is intentionally limited to Admin GraphQL staging and read-after-write visibility. It does not calculate checkout prices, enforce cart eligibility, or claim storefront discount application semantics.
- Free-shipping code and automatic lifecycle mutations stage native `DiscountCodeFreeShipping` and `DiscountAutomaticFreeShipping` records locally without upstream writes. The free-shipping model is separate from amount-off `customerGets`: it stores shipping `destinationSelection`, `maximumShippingPrice`, minimum subtotal/quantity requirements, `combinesWith`, one-time/subscription applicability, `recurringCycleLimit`, code-only `appliesOncePerCustomer` / `usageLimit`, and status/timestamp fields.
- Free-shipping records always use the `SHIPPING` discount class and appear through aggregate `discountNodes` / `discountNodesCount`, method-specific `codeDiscountNode`, `codeDiscountNodeByCode`, `automaticDiscountNode`, and `automaticDiscountNodes` reads. Checkout/order shipping-rate application is intentionally out of scope; this slice only models Admin GraphQL read-after-write visibility.
- Captured free-shipping validation currently covers invalid `combinesWith`, blank code-discount title, and minimum subtotal+quantity conflicts. Destination and money validation beyond those captured branches should not be invented; add a focused live conformance capture before promoting additional guardrails.
- Broad discount bulk roots (`discountCodeBulkActivate`, `discountCodeBulkDeactivate`, `discountCodeBulkDelete`, and `discountAutomaticBulkDelete`) stage locally for safe `ids`, non-blank `search`, and known `savedSearchId` selectors. They return completed `Job` payloads, write durable `discountBulkOperations` records with selector and matched discount IDs, and immediately update downstream discount reads: activate/deactivate changes code-discount status, code bulk delete hides matched code discounts, and automatic bulk delete hides matched automatic discounts. Blank or missing selectors and mutually exclusive selector combinations remain refused locally with `DiscountUserError` payloads to preserve the destructive-selector safety guardrail.
- Full checkout discount application and external Shopify Function execution remain outside local support. The app-discount support slice is Admin GraphQL staging/read-after-write fidelity only.
- Captured validation branches split into top-level GraphQL errors and mutation-scoped `DiscountUserError` payloads:
  - missing `$input` for `discountCodeBasicCreate` returns top-level `INVALID_VARIABLE`
  - inline `basicCodeDiscount: null` returns top-level `argumentLiteralsIncompatible`
  - duplicate codes, invalid date ranges, invalid product/variant references, unsupported collection+product entitlement combinations, unknown update IDs, invalid BXGY/free-shipping inputs, and mutually exclusive bulk selectors return `userErrors` on the mutation payload
- The 2026-04 validation capture includes a live `currentAppInstallation.accessScopes` probe showing the current grant has `read_discounts` and `write_discounts`. A no-discount-scope access-denied fixture is still not available; local discount handling must never convert any future `ACCESS_DENIED` capture into successful staging.
- Code-basic lifecycle tests cover create-read-update-read-activate/deactivate-delete flows, customer/segment buyer-context links, discount-class inference, meta log/state inspection, and commit replay mapping from synthetic IDs to authoritative Shopify IDs for later staged mutations. Redeem-code bulk tests cover add/delete read-after-write behavior, completed local job payloads, broad destructive local refusal, safe id-scoped bulk staging, unsupported passthrough observability, and commit replay order. The live 2026-04 capture fixture `discount-code-basic-lifecycle.json` now runs through executable `captured-vs-proxy-request` parity for create, successful update with Shopify's current `discountAmount` input field, deactivate, activate, delete, and downstream reads. The live 2026-04 fixtures `discount-buyer-context-lifecycle.json` and `discount-class-inference.json` add executable code-basic parity for customer buyer selection, segment buyer selection, all/product/collection discount class inference, downstream `discountNode` / `codeDiscountNodeByCode` reads, cleanup userErrors, and `discount_class:product` count behavior. These parity contracts compare deterministic selected fields while runtime tests continue to cover synthetic IDs/timestamps, generated summaries, singular local `discountClass`, and commit replay. Do not treat the remaining validation guardrails as full support for BXGY, app-discount, or broad bulk job happy paths.
- Automatic-basic lifecycle tests cover create-read-update-activate/deactivate-delete flows, customer/segment buyer-context links, and downstream reads. The live 2026-04 fixtures `discount-automatic-basic-lifecycle.json`, `discount-automatic-basic-nodes-read.json`, and `discount-buyer-context-lifecycle.json` anchor the automatic lifecycle payload, read shapes, customer/segment buyer context serialization, and downstream `automaticDiscountNode` context reads.
- BXGY lifecycle tests cover code and automatic create-read-update-activate/deactivate-delete flows, product/variant/collection reference serialization, and local invalid reference userErrors. Existing validation captures anchor blank-title/all-items guardrails, while `discount-bxgy-disallowed-value-shapes.json` anchors the value-branch and subscription-flag userErrors. Broader happy-path live BXGY fixtures should be captured before tightening Shopify-specific summaries or edge validation.
- Free-shipping lifecycle tests cover code and automatic create-read-update-read-activate/deactivate-delete flows, including destination selection, maximum shipping price, minimum subtotal, one-time/subscription flags, and mutation-log operation order. The live 2026-04 fixture `discount-free-shipping-lifecycle.json` now runs through executable `captured-vs-proxy-request` parity for code and automatic create, update, shared activate/deactivate/delete, and singular downstream reads. The parity contract intentionally avoids pre-existing catalog row counts from the capture store while preserving strict comparison for the staged lifecycle fields it selects.
- HAR-437 app/bulk local parity lives in `config/parity-specs/discounts/discount-app-bulk-local-runtime.json` with the local-runtime fixture `fixtures/conformance/local-runtime/2026-04/discounts/discount-app-bulk-local-runtime.json`. It compares app-discount create/update payloads, app-managed automatic deactivate/activate/delete payloads, completed bulk job payloads, downstream app discount reads, downstream count/status/absence reads, and Function metadata-only ownership evidence through the generic `conformance:parity` runner.
- HAR-438 redeem-code live parity lives in `config/parity-specs/discounts/discount-redeem-code-bulk.json` with the fixture `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-redeem-code-bulk.json`. It covers redeem-code bulk add/delete payloads, exact and lowercase `codeDiscountNodeByCode` lookup, code-count read-after-write behavior, and removed-code null lookup after id-scoped bulk delete.
- Live app-discount success capture is currently blocked by disposable app setup rather than local runtime behavior: the checked-in conformance app contains payment-customization and publication-target extensions, but no discount Function extension. Add and deploy a disposable discount Function extension before recording live `discountCodeAppCreate` / `discountAutomaticAppCreate` success fixtures.
- Future discount work should expand live app-discount fixtures once a discount Function exists, tighten Shopify-specific app-discount validation/userError codes, and capture redeem-code search/saved-search deletion semantics before widening redeem-code bulk delete beyond explicit IDs.
- `scripts/capture-discount-conformance.ts` probes the live conformance app Admin access scopes through `currentAppInstallation.accessScopes`.
- The capture script records `read_discounts` and `write_discounts` availability before attempting discount catalog captures.
- The capture script also creates temporary native `DiscountCodeBasic` and `DiscountAutomaticBasic` records, captures singular detail payloads, and deletes those temporary records immediately after capture.
- `scripts/capture-discount-code-basic-lifecycle-conformance.ts` records native code-basic create/update/deactivate/activate/delete lifecycle evidence against Admin GraphQL 2026-04 and deletes the temporary discount after capture.
- `scripts/capture-discount-buyer-context-conformance.ts` records native code-basic and automatic-basic customer-to-segment buyer context transitions against Admin GraphQL 2026-04, then deletes the temporary discounts, segment, and customer after capture.
- `scripts/capture-discount-bxgy-lifecycle-conformance.ts` records native code and automatic BXGY create/update/deactivate/activate/delete lifecycle evidence with temporary product, variant, and collection prerequisites against Admin GraphQL 2026-04, then deletes the temporary discounts and prerequisite resources after capture.
- `scripts/capture-discount-bxgy-disallowed-value-shapes-conformance.ts` creates two temporary products, captures rejected code and automatic BXGY `customerGets.value` / subscription-flag branches against Admin GraphQL 2026-04, and deletes the temporary products after capture.
- `scripts/capture-discount-free-shipping-lifecycle-conformance.ts` records native code and automatic free-shipping create/update/deactivate/activate/delete lifecycle evidence against Admin GraphQL 2026-04 and deletes the temporary discounts after capture.
- `scripts/capture-discount-validation-conformance.ts` creates a temporary native `DiscountCodeBasic` only to settle the duplicate-code branch, captures representative validation failures, and deletes the seed discount immediately.
- `scripts/capture-discount-redeem-code-bulk-conformance.ts` creates a temporary native `DiscountCodeBasic`, captures redeem-code bulk add/delete and case-insensitive lookup behavior, and deletes the temporary discount immediately after capture.
- The current discount capture script does not create app-managed discounts. Only capture app-discount read fixtures from an already safe existing app discount or a disposable Function-backed setup with explicit cleanup; do not create app-discount fixtures by invoking unknown merchant Function logic on the shared store.
- Tokens must come through `scripts/shopify-conformance-auth.mts`; repo `.env` files must not contain Admin access tokens.
- Discount capture fails before discount reads or writes when either required discount scope is missing.
- Discount capture files use the `discount-*` conformance naming convention only after scope checks pass.

## Historical and developer notes

### Validation anchors

- Conformance fixtures and requests: `config/parity-specs/discounts/discount*.json` and matching files under `config/parity-requests/discounts/`; singular detail fixtures are `discount-code-basic-detail-read.json` and `discount-automatic-basic-detail-read.json`, and lifecycle evidence includes `discount-bxgy-lifecycle.json` plus `discount-free-shipping-lifecycle.json`, under the 2026-04 conformance fixture domain directory.
- Capture helper tests: `tests/unit/discount-conformance-lib.test.ts`
