---
title: 'Selling Plans'
description: 'Coverage notes and fidelity boundaries for Selling Plans.'
---

The selling-plans group tracks the Shopify Admin GraphQL selling-plan group
roots used by product subscription flows.

## Current Support And Limitations

### Tracked But Unimplemented Roots

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

The current Rust runtime does not include a store-backed selling-plan group
lifecycle model. These roots are present in registry metadata and captured
conformance evidence, but they are not marked as implemented local dispatch
roots and are not supported as local staging operations.

Without staged products or variants, downstream reads that select
`Product.sellingPlanGroups`, `Product.sellingPlanGroupsCount`,
`ProductVariant.sellingPlanGroups`, or `ProductVariant.sellingPlanGroupsCount`
return the same local no-data product/variant result as other absent product
reads instead of replaying captured fixture memberships.

### Boundaries

Selling-plan lifecycle mutations, product membership mutations, variant
membership mutations, selling-plan group catalog reads, and generic
`node(id:)` / `nodes(ids:)` resolution for nested `SellingPlan` IDs remain
unsupported until they are backed by a runtime store model and executable
read-after-write coverage.

### Evidence

- `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/selling-plan-group-lifecycle.json`
- `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/selling-plan-group-input-validation.json`
- `config/parity-specs/products/sellingPlanGroupCreate-input-validation.json`
- `config/parity-specs/products/productJoinLeaveSellingPlanGroups-validation.json`
- `config/parity-specs/products/selling-plan-product-variant-associations.json`
- `config/parity-specs/products/selling-plan-group-lifecycle.json`

These artifacts capture Shopify behavior for future local support work; they do
not make the current Rust runtime support claim.

### Validation

- `corepack pnpm conformance:check`
- `corepack pnpm rust:test`
