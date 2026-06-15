---
title: 'Shipping And Fulfillments Endpoint Group'
description: 'Coverage notes and fidelity boundaries for Shipping And Fulfillments Endpoint Group.'
---

This endpoint group covers Shopify Admin GraphQL shipping, delivery settings,
carrier service, fulfillment service, delivery profile, fulfillment order,
fulfillment, local pickup, shipping package, reverse logistics, and
order-editing shipping-line roots.

## Current support and limitations

### Supported roots

The current Rust operation registry marks a bounded delivery-profile slice as
locally implemented. Registry presence remains a local-model commitment only; it
is not a claim that the whole shipping/fulfillments domain is supported for
arbitrary documents.

The registry-only read roots are:

- `reverseDelivery`
- `reverseFulfillmentOrder`
- `fulfillment`
- `assignedFulfillmentOrders`
- `fulfillmentOrder`
- `fulfillmentOrders`
- `manualHoldsFulfillmentOrders`
- `fulfillmentService`
- `availableCarrierServices`
- `carrierService`
- `carrierServices`
- `deliveryCustomization`
- `deliveryCustomizations`
- `deliveryPromiseParticipants`
- `deliveryPromiseProvider`
- `deliveryPromiseSettings`
- `deliverySettings`
- `locationsAvailableForDeliveryProfilesConnection`
- `fulfillmentConstraintRules`

The registry-only mutation roots are:

- `fulfillmentCreate`
- `fulfillmentEventCreate`
- `fulfillmentTrackingInfoUpdate`
- `fulfillmentCancel`
- `fulfillmentOrderAcceptCancellationRequest`
- `fulfillmentOrderAcceptFulfillmentRequest`
- `fulfillmentOrderCancel`
- `fulfillmentOrderClose`
- `fulfillmentOrderHold`
- `fulfillmentOrderLineItemsPreparedForPickup`
- `fulfillmentOrderMerge`
- `fulfillmentOrderMove`
- `fulfillmentOrderOpen`
- `fulfillmentOrderRejectCancellationRequest`
- `fulfillmentOrderRejectFulfillmentRequest`
- `fulfillmentOrderReleaseHold`
- `fulfillmentOrderReportProgress`
- `fulfillmentOrderReschedule`
- `fulfillmentOrdersReroute`
- `fulfillmentOrdersSetFulfillmentDeadline`
- `fulfillmentOrderSplit`
- `fulfillmentOrderSubmitCancellationRequest`
- `fulfillmentOrderSubmitFulfillmentRequest`
- `fulfillmentServiceCreate`
- `fulfillmentServiceDelete`
- `fulfillmentServiceUpdate`
- `carrierServiceCreate`
- `carrierServiceDelete`
- `carrierServiceUpdate`
- `locationLocalPickupDisable`
- `locationLocalPickupEnable`
- `shippingPackageDelete`
- `shippingPackageMakeDefault`
- `shippingPackageUpdate`
- `fulfillmentConstraintRuleCreate`
- `fulfillmentConstraintRuleDelete`
- `fulfillmentConstraintRuleUpdate`
- `deliveryCustomizationActivation`
- `deliveryCustomizationCreate`
- `deliveryCustomizationDelete`
- `deliveryCustomizationUpdate`
- `deliveryPromiseParticipantsUpdate`
- `deliveryPromiseProviderUpsert`
- `deliverySettingUpdate`
- `orderEditAddShippingLine`
- `orderEditRemoveShippingLine`
- `orderEditUpdateShippingLine`
- `reverseDeliveryCreateWithShipping`
- `reverseDeliveryShippingUpdate`
- `reverseFulfillmentOrderDispose`

### Local behavior

The Rust runtime has scenario-backed shipping and fulfillment slices for ported
parity requests and runtime tests. These slices stage or serialize local state
only for the request families recognized by the Rust dispatcher.

Delivery-profile support is store-backed for `deliveryProfileCreate`,
`deliveryProfileUpdate`, `deliveryProfileRemove`, downstream
`deliveryProfile(id:)`, and `deliveryProfiles(...)`. The modeled custom-profile
subset stages profile names, nested location groups, locations, zones, countries,
method definitions, rate definitions, method conditions, condition deletes,
variant association/dissociation, profile list/detail reads, and async removal
job payloads without sending the write upstream during runtime handling.
Removing a staged profile tombstones it locally so `deliveryProfile(id:)`
returns `null`; the original raw mutations remain in the mutation log for
`/__meta/commit` replay.

Fulfillment-service slices cover create, update, delete, downstream
`fulfillmentService(id:)`, associated `location(id:)`, after-delete absence,
name/handle validation, callback URL validation, duplicate/reserved handles,
removed public arguments, and delete inventory-action validation. Creation
creates a service-managed location in local state, update preserves service and
location identity, and delete applies the captured local location disposition
for the scenario. Successful service mutations keep original raw GraphQL input
for commit replay.
The captured 2026-04 public schema does not expose `permitsSkuSharing`,
`inventorySyncEnabled`, or `fulfillmentOrdersOptIn` on
`fulfillmentServiceCreate`; those arguments return top-level
`argumentNotAccepted` GraphQL errors before resolver execution and do not stage
or log a service mutation.

Carrier-service slices cover create, update, delete, downstream
`carrierService(id:)`, `carrierServices(...)`, active filters, unknown-id
validation, blank create and update names, duplicate active app carriers,
callback URL validation, selected typed `userErrors.code` branches, and
after-delete absence. Rejected blank-name updates do not stage a replacement
name or append a mutation-log entry; omitted update names preserve the existing
local name while applying other staged fields. The local model stores service
name, formatted name, callback URL, active flag, service-discovery flag, and
stable synthetic IDs for parity replay.

Fulfillment and fulfillment-order slices cover fixture-backed top-level reads,
detail/event reads, hold/release, move, open/report-progress, close,
reschedule guardrails, request/cancellation request transitions, split, merge,
deadline setting, assigned-order filtering, and selected validation branches.
These slices operate on local order-backed fulfillment records and are not a
general fulfillment-service execution engine.

Delivery settings and delivery promise settings are read-only in the captured
empty/no-feature branch. Delivery-profile validation covers the captured
blank/too-long name, unknown location, empty zone-country, create-time
update-only input, missing-profile update/remove, default-profile remove, and
successful nested create/update/remove branches used by the checked-in parity
specs.

Local pickup and shipping package slices stage settings on known local
locations or package records. Pickup changes are visible through the captured
`Location`, `locationsAvailableForDeliveryProfilesConnection`, and
`availableCarrierServices.locations` surfaces. Shipping packages have no direct
Admin GraphQL package read root in the captured schema, so successful staging is
verified through local state/log behavior and targeted validation.

Reverse delivery and order-edit shipping-line roots are modeled through the
orders and returns local graph when covered by their parity specs. Their
caller-visible order and return effects should be read with
`/endpoints/orders/` and `/endpoints/returns/`.

### Boundaries

- Most shipping/fulfillments roots remain `implemented: false` in the current
  operation registry. Scenario-backed Rust helpers should not be described as
  broad root support.
- Delivery-profile support is bounded to custom-profile local staging and the
  selected read-after-write effects above. It is not full Shopify delivery
  settings emulation, carrier callback execution, checkout rate calculation, or
  complete delivery-profile catalog behavior.
- Delivery customization and delivery promise mutations are Shopify
  Function-backed or provider-backed and remain unsupported until function
  ownership, activation eligibility, metafields, provider state, validation,
  cleanup, and downstream reads are modeled locally.
- Fulfillment constraint rules remain registry-only because current evidence is
  access-scope blocker evidence, not success/read-after-write behavior.
- Validation-only shipping and fulfillment specs prove guardrail payloads and
  no-stage behavior for those inputs only. They do not make the corresponding
  mutation roots generally supported.
- Fulfillment-service inventory transfer semantics, checkout pickup/rate
  calculation, carrier callback execution, carrier service-discovery side
  effects, and full shipping-package discovery/validation remain outside the
  supported local slices.
- Unsupported mutation documents outside the ported local slices follow the
  configured unsupported path and must remain visible in logs/observability.

### Evidence

- Registry status: `src/operation_registry.rs`
- Runtime coverage: `tests/graphql_routes.rs`
- Shipping/fulfillment parity specs: `config/parity-specs/shipping-fulfillments/*.json`
- Shipping/fulfillment fixtures: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/shipping-fulfillments/*.json` and `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/*.json`
- Related order/return shipping specs: `config/parity-specs/orders/return-reverse-logistics-local-staging.json`, `config/parity-specs/orders/return-reverse-logistics-recorded.json`, and the order-edit shipping-line specs under `config/parity-specs/orders/`

### Validation

- `corepack pnpm lint`
- `corepack pnpm rust:test`
