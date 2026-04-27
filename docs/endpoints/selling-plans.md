# Selling Plans

HAR-308 adds local support for the selling-plan group roots that product subscription flows need:

- `sellingPlanGroup(id:)`
- `sellingPlanGroups(...)`
- `sellingPlanGroupCreate(input:, resources:)`
- `sellingPlanGroupUpdate(id:, input:)`
- `sellingPlanGroupDelete(id:)`
- `sellingPlanGroupAddProducts(id:, productIds:)`
- `sellingPlanGroupRemoveProducts(id:, productIds:)`
- `sellingPlanGroupAddProductVariants(id:, productVariantIds:)`
- `sellingPlanGroupRemoveProductVariants(id:, productVariantIds:)`

Selling-plan group state is normalized in memory with group scalar fields, nested selling-plan payload data, product membership IDs, and product-variant membership IDs. Supported mutations stage locally and are retained in the mutation log with the original raw GraphQL request for commit replay; they do not write to Shopify at runtime.

## Runtime behavior

`sellingPlanGroupCreate` creates a synthetic `SellingPlanGroup` and synthetic nested `SellingPlan` IDs. `resources.productIds` and `resources.productVariantIds` seed the initial memberships. `sellingPlanGroupUpdate` updates group scalar fields, stages nested selling-plan create/update/delete inputs, and returns `deletedSellingPlanIds` for locally known removed plans. Delete marks the group absent from subsequent detail, catalog, product, and variant reads.

Product membership and product-variant membership are tracked independently, matching the captured Shopify behavior where creating a group with `resources.productIds` applies to the product but does not automatically make `appliesToProductVariant` true for the product's default variant. `Product.sellingPlanGroups`, `Product.sellingPlanGroupsCount`, `ProductVariant.sellingPlanGroups`, and `ProductVariant.sellingPlanGroupsCount` read from the staged membership overlay.

Unknown group IDs for update/delete/add/remove return Shopify-like `GROUP_DOES_NOT_EXIST` userErrors with `field: ["id"]`; remove payloads return `removedProductIds: null` or `removedProductVariantIds: null` on that branch.

## Conformance

`fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/selling-plan-group-lifecycle.json` captures the live 2026-04 lifecycle against a disposable product and selling-plan group, including cleanup. The capture records create/update/delete payloads, product and variant membership add/remove payloads, unknown-id userErrors, and downstream read-after-write effects.

Validation entry points:

- `corepack pnpm conformance:capture-selling-plan-groups`
- `corepack pnpm vitest run tests/integration/selling-plan-group-flow.test.ts`
- `corepack pnpm conformance:check`
- `corepack pnpm conformance:parity`
