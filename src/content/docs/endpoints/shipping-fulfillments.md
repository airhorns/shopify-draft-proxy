---
title: 'Shipping And Fulfillments Endpoint Group'
description: 'Coverage notes and fidelity boundaries for Shipping And Fulfillments Endpoint Group.'
---

This endpoint group covers Shopify Admin GraphQL shipping, delivery settings,
carrier service, fulfillment service, delivery profile, fulfillment order,
fulfillment, local pickup, shipping package, reverse logistics, and
order-editing shipping-line roots.

## Current support and limitations

### Implemented roots

The current Rust operation registry marks only bounded shipping/fulfillments
slices as implemented. Registry presence is a local-model commitment only; it
is not a claim that the whole shipping/fulfillments domain is supported for
arbitrary documents.

The implemented read roots are:

- `locationsAvailableForDeliveryProfilesConnection`

The implemented mutation roots are:

- `carrierServiceCreate`
- `carrierServiceDelete`
- `carrierServiceUpdate`
- `fulfillmentServiceCreate`
- `fulfillmentServiceDelete`
- `fulfillmentServiceUpdate`
- `locationLocalPickupDisable`
- `locationLocalPickupEnable`
- `shippingPackageDelete`
- `shippingPackageMakeDefault`
- `shippingPackageUpdate`

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
- `deliveryProfile`
- `deliveryProfiles`
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
- `fulfillmentConstraintRuleCreate`
- `fulfillmentConstraintRuleDelete`
- `fulfillmentConstraintRuleUpdate`
- `deliveryProfileCreate`
- `deliveryProfileRemove`
- `deliveryProfileUpdate`
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
empty/no-feature branch. Delivery profiles have fixture-backed read and bounded
custom-profile write slices for create/update/remove, validation, variant
dissociation, async removal payloads, and downstream null reads after removal.

Local pickup mutations stage settings on active local locations and retain the
original raw GraphQL request for commit replay. `locationLocalPickupEnable`
accepts captured standard pickup times, rejects non-standard values with
`CUSTOM_PICKUP_TIME_NOT_ALLOWED`, and rejects unknown or inactive locations with
`ACTIVE_LOCATION_NOT_FOUND`. `locationLocalPickupDisable` clears the staged
settings. Pickup changes are visible through `Location.localPickupSettingsV2`
and `locationsAvailableForDeliveryProfilesConnection` in snapshot mode and
after LiveHybrid reads hydrate the existing shipping locations.

Shipping package slices stage changes on known package records and retain the
original raw GraphQL request for commit replay. Shipping packages have no direct
Admin GraphQL package read root in the captured schema, so successful staging is
verified through local state/log behavior and targeted validation.

Reverse delivery roots are modeled through the orders and returns local graph:
`reverseDeliveryCreateWithShipping`, `reverseDeliveryShippingUpdate`, and
`reverseFulfillmentOrderDispose` stage reverse delivery, tracking/label, and
reverse fulfillment order disposition state locally, retain the original raw
mutation for commit replay, and expose caller-visible order and return effects
through `/endpoints/orders/` and `/endpoints/returns/`.
Order-edit shipping-line roots are also modeled through the orders local graph
when covered by their parity specs.

### Boundaries

- Implemented local slices should not be described as broad
  shipping/fulfillments root support beyond their covered request families.
- Most shipping/fulfillment roots remain `implemented: false` in the current
  operation registry. Reverse-logistics roots are implemented only for their
  covered local lifecycle and read-after-write slices.
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
