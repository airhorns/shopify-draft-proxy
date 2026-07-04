---
title: 'Selling Plans'
description: 'Coverage notes and fidelity boundaries for Selling Plans.'
---

The selling-plans group tracks the Shopify Admin GraphQL selling-plan group
roots used by product subscription flows.

## Current support and limitations

### Implemented roots

Reads:

- `sellingPlanGroup(id:)`
- `sellingPlanGroups(...)`

Mutations:

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

### Local Behavior

The Rust runtime models staged selling-plan group lifecycle behavior for groups
created inside the current proxy session. Successful `sellingPlanGroupCreate`,
`sellingPlanGroupUpdate`, and `sellingPlanGroupDelete` calls stage local group
records and nested `SellingPlan` records without runtime Shopify writes, while
retaining the original raw mutations for commit replay.

`sellingPlanGroupCreate` persists nullable `input.appId` on the staged group,
and `sellingPlanGroupUpdate` changes or clears `appId` when the input includes
the field. Subsequent `sellingPlanGroup(id:)` reads return the staged `appId`
value from local state.

`sellingPlanGroupCreate` validates the captured model-backed create guardrails
after the shared input validator passes. Blank or absent group `name`, zero or
absent `sellingPlansToCreate`, more than 31 submitted plans, and per-plan
missing `billingPolicy` / `deliveryPolicy` return captured `userErrors`, return
`sellingPlanGroup: null`, and do not stage a group. `sellingPlanGroupUpdate`
does not apply the create-only lower-bound to an empty
`sellingPlansToCreate: []` list, but it rejects updates that would delete every
existing selling plan without creating a replacement. That update-only guard
returns `SELLING_PLAN_COUNT_LOWER_BOUND` at
`["input", "sellingPlansToDelete"]`, returns `sellingPlanGroup: null`, leaves
the group unchanged on subsequent reads, and records the raw mutation as a
failed local mutation for observability.

For nested selling-plan `pricingPolicies`, create and update validation rejects
lists that contain recurring pricing policies without a fixed pricing policy.
The local response matches Shopify's captured
`SELLING_PLAN_PRICING_POLICIES_MUST_CONTAIN_A_FIXED_PRICING_POLICY` userError
field/message/code shape for both `sellingPlansToCreate` and
`sellingPlansToUpdate`. Valid fixed+recurring policy lists stage locally, and
subsequent selling-plan reads return both `SellingPlanFixedPricingPolicy` and
`SellingPlanRecurringPricingPolicy` entries from staged state.

`SellingPlanGroup.summary` is computed from staged selling plans, not from the
group option labels. The local summary uses the selling-plan count,
singular/plural `frequency` wording, percentage min/max ranges across all
pricing policies, fixed-value min/max ranges using Shopify's whole-currency
summary display, and joins mixed percentage/fixed pieces with `·`.

Staged selling plans retain mixed fixed and recurring pricing policies in
`SellingPlan.pricingPolicies`. Fixed entries read back as
`SellingPlanFixedPricingPolicy`; recurring entries read back as
`SellingPlanRecurringPricingPolicy` with `afterCycle`, `createdAt`,
`adjustmentType`, and `adjustmentValue`. The 2026-04 lifecycle capture shows
Shopify rejects a pricing-policy list that contains only recurring policies and
no fixed policy; that validation branch is tracked separately from the mixed
policy read-back support described here.

Staged `sellingPlanGroupAddProducts`,
`sellingPlanGroupRemoveProducts`, `sellingPlanGroupAddProductVariants`,
`sellingPlanGroupRemoveProductVariants`, `productJoinSellingPlanGroups`,
`productLeaveSellingPlanGroups`, `productVariantJoinSellingPlanGroups`, and
`productVariantLeaveSellingPlanGroups` update membership edges for local
products, variants, and selling-plan groups. Downstream
`Product.sellingPlanGroups`, `Product.sellingPlanGroupsCount`,
`ProductVariant.sellingPlanGroups`, and `ProductVariant.sellingPlanGroupsCount`
read from the staged membership graph.

The top-level `sellingPlanGroups(...)` connection filters the staged group set
before applying sort, reverse order, and cursor windowing. Local query support
covers bare text plus `app_id`, `category`, `created_at`,
`delivery_frequency`, `id`, `name`, and `percentage_off`; an unrecognized keyed
filter returns no staged matches. Supported sort keys are `ID` by default,
`NAME`, `CREATED_AT`, and `UPDATED_AT`, with `UPDATED_AT` using the group's
effective stored timestamp. Captured 2026-04 behavior showed a delayed
description-only `sellingPlanGroupUpdate` did not move that group ahead of a
later-created group in `sortKey: UPDATED_AT, reverse: true` ordering, so local
staged group updates preserve the original effective timestamp for this sort.

Nested `Product.sellingPlanGroups(...)` and
`ProductVariant.sellingPlanGroups(...)` apply reverse order and cursor
windowing over the staged membership overlay, and the corresponding
`sellingPlanGroupsCount` fields return exact staged counts. Shopify Admin
GraphQL 2026-04 rejects `query` and `sortKey` arguments on those nested
connections, so the local overlay only models the schema-valid nested
connection arguments.

Snapshot reads over an empty local selling-plan store return Shopify-like no-data
shapes: `sellingPlanGroup(id:)` is `null` and `sellingPlanGroups(...)` is an
empty connection. In LiveHybrid, mutation roots that target live-store groups,
products, or variants not present in local state are forwarded upstream instead
of fabricating local not-found errors from empty state.

### Boundaries

Support is scoped to local staged groups and locally known product/variant
resources. The proxy does not hydrate arbitrary live-store selling-plan groups
into local state, does not claim full upstream parity for every
`SellingPlanGroupInput` field or selling-plan policy variant, and still relies
on existing product/variant state to expose downstream membership overlays.

Generic `node(id:)` / `nodes(ids:)` readback for selling-plan group and nested
selling-plan IDs is covered by the admin-platform endpoint group. Broader
Shopify selling-plan behavior outside the staged lifecycle and membership
surface remains unsupported until backed by runtime behavior and captured
parity evidence.
