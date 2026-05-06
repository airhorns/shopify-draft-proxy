# Selling Plans

HAR-308 adds local support for the selling-plan group roots that product subscription flows need, and HAR-432
reviewed that support against the current Shopify Admin docs, public reference examples, existing recordings, and
local runtime behavior:

- `sellingPlanGroup(id:)`
- `sellingPlanGroups(...)`
- `sellingPlanGroupCreate(input:, resources:)`
- `sellingPlanGroupUpdate(id:, input:)`
- `sellingPlanGroupDelete(id:)`
- `sellingPlanGroupAddProducts(id:, productIds:)`
- `sellingPlanGroupRemoveProducts(id:, productIds:)`
- `sellingPlanGroupAddProductVariants(id:, productVariantIds:)`
- `sellingPlanGroupRemoveProductVariants(id:, productVariantIds:)`
- `productJoinSellingPlanGroups(id:, sellingPlanGroupIds:)`
- `productLeaveSellingPlanGroups(id:, sellingPlanGroupIds:)`
- `productVariantJoinSellingPlanGroups(id:, sellingPlanGroupIds:)`
- `productVariantLeaveSellingPlanGroups(id:, sellingPlanGroupIds:)`

Selling-plan group state is normalized in memory with group scalar fields, nested selling-plan payload data, product membership IDs, and product-variant membership IDs. Supported mutations stage locally and are retained in the mutation log with the original raw GraphQL request for commit replay; they do not write to Shopify at runtime.

The current Shopify Admin docs and public examples continue to present these roots as product/purchase-option-scoped
Admin operations: group-centric add/remove roots take a selling-plan-group ID plus product or product-variant IDs,
while product-centric join/leave roots take a product or variant ID plus selling-plan-group IDs. The local model mirrors
that bidirectional association surface with one normalized group record rather than response-only patching.

Generic Admin `node(id:)` / `nodes(ids:)` dispatch resolves nested `SellingPlan` IDs by scanning effective selling-plan groups and projecting the stored selling-plan payload through the requested selection set. Missing selling-plan IDs return `null`, and adjacent product subscription Node families such as quantity price breaks remain unsupported until their own local lifecycle/read models have executable Node evidence.

## Current support and limitations

### Runtime behavior

`sellingPlanGroupCreate` creates a synthetic `SellingPlanGroup` and synthetic nested `SellingPlan` IDs. `resources.productIds` and `resources.productVariantIds` seed the initial memberships. `sellingPlanGroupUpdate` updates group scalar fields, stages nested selling-plan create/update/delete inputs, and returns `deletedSellingPlanIds` for locally known removed plans. Delete marks the group absent from subsequent detail, catalog, product, and variant reads.

Create/update inputs reject Shopify-captured selling-plan guardrails before staging: group `options` lists are limited to three entries, group and nested plan `position` values must fit signed int32, nested plan `options` lists are limited to three entries, nested plan `pricingPolicies` lists are limited to two entries, and every `sellingPlansToUpdate` entry must include an `id`. Billing and delivery policy kinds must remain compatible; create requests with recurring billing plus fixed delivery return `BILLING_AND_DELIVERY_POLICY_TYPES_MUST_BE_THE_SAME`, while update requests that replace only the delivery policy kind on an existing recurring plan return Shopify's `ONLY_ONE_OF_FIXED_OR_RECURRING_DELIVERY` guardrail.

Product membership and product-variant membership are tracked independently, matching the captured Shopify behavior where creating a group with `resources.productIds` applies to the product but does not automatically make `appliesToProductVariant` or `ProductVariant.sellingPlanGroupsCount` true for the product's default variant. `ProductVariant.sellingPlanGroups` still includes product-level group memberships visible through the variant, and direct product-variant add/remove mutations update the variant count and `appliesToProductVariant` fields.

HAR-299 also supports Shopify's product-centric membership roots. `productJoinSellingPlanGroups` / `productLeaveSellingPlanGroups` mutate the selected groups' `productIds` membership lists and return the selected `Product` payload. `productVariantJoinSellingPlanGroups` / `productVariantLeaveSellingPlanGroups` mutate `productVariantIds` and return the selected `ProductVariant` payload. These roots share the same normalized membership model as the group-centric add/remove mutations, so downstream product, variant, and selling-plan group reads stay consistent without runtime Shopify writes.

Product-centric and variant-centric join/leave roots reject validation failures before staging any membership changes. Empty `sellingPlanGroupIds` returns a `BLANK` userError, duplicate IDs within one request return `DUPLICATE`, join requests that would leave a product or variant in more than 31 distinct selling-plan groups return `SELLING_PLAN_GROUPS_TOO_MANY`, and leave requests for a known group the product or variant is not directly a member of return `NOT_A_MEMBER`. Group-centric add roots share the same per-resource cap and reject already-attached product or variant memberships with `TAKEN`; unknown add resources follow the captured Shopify field paths (`["productIds"]` / `["productVariantIds"]`). Rejected requests are retained in the mutation log as failed entries with the original raw mutation for observability, but commit replay skips them.

Unknown group IDs for update/delete/add/remove return Shopify-like `GROUP_DOES_NOT_EXIST` userErrors with `field: ["id"]`; remove payloads return `removedProductIds: null` or `removedProductVariantIds: null` on that branch. HAR-432 adds explicit local runtime coverage that unknown product, unknown product variant, and unknown selling-plan group association attempts stay side-effect free, return local userErrors, and never runtime-proxy to Shopify.

Shopify's Admin docs describe selling-plan groups as app-scoped purchase options that can be associated directly with products or product variants. The local model keeps those association lists explicit instead of deriving variant membership from product membership, because the captured 2026-04 lifecycle showed those read paths diverging.

### Known gaps

The checked-in 2026-04 captures cover lifecycle, group-centric association updates, product-centric association
updates, variant-centric association updates, unknown-group validation branches, and downstream read-after-write
behavior for staged products and variants. Broader Shopify
validation semantics are not yet exhaustively modeled: invalid nested selling-plan policy combinations, app ownership
or permission failures outside the captured option/pricing-policy/position/policy-kind guardrails, and
`sellingPlanGroups(query:, sortKey:, reverse:)` filtering/sorting beyond the staged
catalog shape should be treated as unsupported fidelity gaps until live conformance evidence is added. Do not expand
the supported contract for those branches without a capture or a focused runtime test that models the downstream
behavior.

## Historical and developer notes

### Conformance

`fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/selling-plan-group-lifecycle.json` captures the live 2026-04 lifecycle against a disposable product and selling-plan group, including cleanup. The capture records create/update/delete payloads, product and variant membership add/remove payloads, unknown-id userErrors, and downstream read-after-write effects.

`fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/selling-plan-group-input-validation.json` captures live 2025-01 create/update input validation branches for group option/position limits, nested plan option/pricing-policy/position limits, required update IDs, and policy kind guardrails. The parity replay creates one local seed group through the primary request, then replays strict invalid create/update targets without pre-seeding state.

Validation entry points:

- `corepack pnpm conformance:capture -- --run selling-plan-groups`
- `corepack pnpm conformance:capture -- --run selling-plan-group-input-validation`
- `config/parity-specs/admin-platform/admin-platform-selling-plan-node-reads.json`
- `config/parity-specs/products/sellingPlanGroupCreate-input-validation.json`
- `config/parity-specs/products/productJoinLeaveSellingPlanGroups-validation.json`
- `config/parity-specs/products/selling-plan-product-variant-associations.json`
- `config/parity-specs/products/selling-plan-group-lifecycle.json`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
