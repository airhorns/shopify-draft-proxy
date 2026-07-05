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
- `fulfillmentEventCreate`
- `fulfillmentServiceCreate`
- `fulfillmentServiceDelete`
- `fulfillmentServiceUpdate`
- `locationLocalPickupDisable`
- `locationLocalPickupEnable`
- `deliveryProfileCreate`
- `deliveryProfileRemove`
- `deliveryProfileUpdate`
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

The registry-only mutation roots are:

- `fulfillmentCreate`
- `fulfillmentCreateV2`
- `fulfillmentTrackingInfoUpdate`
- `fulfillmentCancel`
- `fulfillmentOrderCancel`
- `fulfillmentOrderClose`
- `fulfillmentOrderHold`
- `fulfillmentOrderLineItemsPreparedForPickup`
- `fulfillmentOrderMove`
- `fulfillmentOrderOpen`
- `fulfillmentOrderReleaseHold`
- `fulfillmentOrderReportProgress`
- `fulfillmentOrderReschedule`
- `fulfillmentOrdersReroute`
- `fulfillmentOrdersSetFulfillmentDeadline`
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

The Rust runtime has store-backed shipping and fulfillment slices for checked-in
parity requests and runtime tests. These slices stage or serialize local state
only for the request families recognized by the Rust dispatcher.

Fulfillment-service slices cover create, update, delete, downstream
`fulfillmentService(id:)`, associated `location(id:)`, after-delete absence,
name/handle validation, callback URL validation, duplicate/reserved handles,
removed public arguments, and delete inventory-action validation. Creation
creates a service-managed location in local state, update preserves service and
location identity, and delete applies the captured local location disposition
for the scenario. Successful service mutations keep original raw GraphQL input
for commit replay. For `requiresShippingMethod`, the local model follows the
captured public GraphQL default: an omitted argument stages `true` on both
create and update, while an explicit `false` value remains observable through
the mutation payload and downstream `fulfillmentService(id:)` reads.
Callback URL validation allows `mock.shop` hosts and the configured
`.myshopify.com` Admin origin host when one is present; cold/default proxy
configuration does not synthesize a `shopify-draft-proxy.local` allowed host.
The captured 2026-04 public schema does not expose `permitsSkuSharing`,
`inventorySyncEnabled`, or `fulfillmentOrdersOptIn` on
`fulfillmentServiceCreate`; those arguments return top-level
`argumentNotAccepted` GraphQL errors before resolver execution and do not stage
or log a service mutation.

Reverse-logistics shipping slices stage `reverseDeliveryCreateWithShipping`
from the return domain's reverse fulfillment order state. Explicit
`reverseDeliveryLineItems` inputs preserve one staged delivery line per input
with the requested quantity and line item; empty inputs expand to all reverse
fulfillment order lines at their total quantities. The recorded order/return
parity fixture uses one two-line return for empty expansion and a second
two-line return for explicit multi-line delivery creation.

Carrier-service slices cover create, update, delete, downstream
`carrierService(id:)`, `carrierServices(...)`, active filters, unknown-id
validation, blank create and update names, duplicate active app carriers,
callback URL validation, required-field GraphQL coercion for create-time
`active` and `supportsServiceDiscovery`, selected typed `userErrors.code`
branches, and after-delete absence. Rejected create validation and coercion
branches do not stage a carrier service or append a mutation-log entry.
Rejected blank-name updates do not stage a replacement name or append a
mutation-log entry; omitted update names preserve the existing local name while
applying other staged fields. The local model stores service name, formatted
name, callback URL, active flag, service-discovery flag, and stable synthetic
IDs for parity replay. `carrierServices(query:)` parses whitespace-separated
`field:value` tokens for the documented local fields `active` and `id`; multiple
tokens are combined with AND semantics. Unsupported filter fields or bare search
terms return an empty local connection rather than widening the result set.
`sortKey: ID`, `CREATED_AT`, and `UPDATED_AT` plus `reverse` are applied before
cursor windowing.

Fulfillment and fulfillment-order slices cover fixture-backed top-level reads,
detail/event reads, hold/release, move, open/report-progress, close,
reschedule guardrails, deadline setting, assigned-order filtering, and selected
validation branches. Captured public 2026-04 behavior allows
`fulfillmentOrderOpen` to mark an `IN_PROGRESS` fulfillment order `OPEN`, but a
second open attempt on an already-`OPEN` fulfillment order returns a base
`userErrors` entry (`field: null`) and leaves the local fulfillment-order
status, supported actions, and timestamp unchanged; this public `UserError`
shape exposes `field` / `message` only. Fulfillment holds expose Shopify-like
localized `displayReason` strings for the public hold reason set, including
`AWAITING_RETURN_ITEMS` as `Exchange items awaiting return delivery`, and
unknown or non-visible reasons fall back to `Other`. Store-backed local staging
now covers `fulfillmentOrderMove`, `fulfillmentOrderOpen`,
`fulfillmentOrderReportProgress`, `fulfillmentOrdersSetFulfillmentDeadline`,
and `fulfillmentCreate` plus deprecated `fulfillmentCreateV2` payload
`Fulfillment.name` reference numbers as `<orderName>-F<n>` for order-backed
fulfillment sequences, plus
`fulfillmentOrderSubmitFulfillmentRequest`,
`fulfillmentOrderAcceptFulfillmentRequest`,
`fulfillmentOrderRejectFulfillmentRequest`,
`fulfillmentOrderSubmitCancellationRequest`,
`fulfillmentOrderAcceptCancellationRequest`,
`fulfillmentOrderRejectCancellationRequest`, `fulfillmentOrderSplit`, and
`fulfillmentOrderMerge` against fulfillment orders present on staged or
hydrated local orders. Request-status transitions, merchant request records,
split-off remaining fulfillment orders, and merged line-item quantities are
written into the local order graph and are visible through `fulfillmentOrder`,
`fulfillmentOrders`, `assignedFulfillmentOrders`, and nested
`Order.fulfillmentOrders` reads. Top-level `fulfillmentOrders(...)` follows the
captured connection arguments for staged local records: `includeClosed` defaults
to `false`, `sortKey: ID` and timestamp-like sort keys are applied before
`reverse`, and cursor windows are cut from the filtered/sorted list. Nested
`Order.fulfillmentOrders(...)` keeps the public Order-field argument boundary
captured from Shopify: local projection applies `displayable`, `first`/`last`,
cursors, and `reverse` there instead of inventing top-level `includeClosed` /
`sortKey` behavior for the nested field. Locally created order fulfillment
orders derive their initial `assignedLocation` from the first active
observed/staged shop location that fulfills online orders; the runtime does not
fabricate `gid://shopify/Location/1` when no such location is known. These
slices operate on local order-backed fulfillment records and are not a general
fulfillment-service execution engine. `fulfillmentOrdersSetFulfillmentDeadline`
stages `fulfillBy` for every requested fulfillment order that exists in local or
hydrated order state, including `CLOSED` and `CANCELLED` fulfillment orders.
When none of the requested IDs resolve, it returns `success: false` with a single
user error: `field: null`, message
`Fulfillment orders could not be found.`, and `code: null`.
`fulfillmentOrderMove` resolves the destination from staged or hydrated
location records; missing or inactive destinations return the local
`Location not found.` user error, and successful move payloads serialize the
assigned-location id/name from that stored location rather than from fixture
constants.

Delivery settings and delivery promise settings are read-only in the captured
empty/no-feature branch. Delivery profiles have fixture-backed read and bounded
write slices for create/update/remove, validation, variant dissociation, async
removal payloads, and downstream null reads after removal. Custom profiles are
fully staged from create/update inputs covered by the delivery-profile parity
requests. In LiveHybrid mode, `deliveryProfileUpdate` can hydrate an existing
default profile and stage proxy-modelable updates without writing to Shopify at
runtime. Captured Admin GraphQL 2026-04 behavior accepts a default-profile name
input with empty `userErrors` while preserving the public default display name
and incrementing `version`; unsupported side effects such as rate recalculation
remain outside this slice. Delivery profile name validation accepts exactly 128
characters and rejects 129-character names on both create and update with a
public `UserError` payload containing `field` and `message`; `code` is not
selectable on the captured Admin GraphQL 2026-04 `UserError` type. Location
IDs supplied in delivery-profile location groups must resolve from staged,
observed, or LiveHybrid-hydrated location state; unknown IDs return the public
`The Location could not be found for this shop.` userError instead of creating a
synthetic location. `deliveryProfiles(first/last/after/before/reverse:)` uses
the staged effective profile order to compute page windows and `pageInfo`
boundary cursors instead of returning a canned connection envelope.

Local pickup mutations stage settings on active local locations and retain the
original raw GraphQL request for commit replay. `locationLocalPickupEnable`
accepts captured standard pickup times, rejects non-standard values with
`CUSTOM_PICKUP_TIME_NOT_ALLOWED`, and rejects unknown or inactive locations with
`ACTIVE_LOCATION_NOT_FOUND`. `locationLocalPickupDisable` clears the staged
settings on active locations and rejects unknown or inactive locations with
`ACTIVE_LOCATION_NOT_FOUND` on `locationId`; failed disable payloads return
`locationId: null`. Pickup changes are visible through
`Location.localPickupSettingsV2` and
`locationsAvailableForDeliveryProfilesConnection` in snapshot mode and after
LiveHybrid reads hydrate the existing shipping locations.

Shipping package slices stage changes on package records already present in the
local staged/observed store or hydrated from Shopify in LiveHybrid mode, and
they retain the original raw GraphQL request for commit replay. The runtime does
not seed canned package dimensions, weights, or names: absent or locally deleted
package IDs return Shopify's top-level `RESOURCE_NOT_FOUND` envelope, while
observed or hydrated package records preserve their real fields across partial
updates. Making a package default clears the default flag across every known
package record instead of relying on a fixed ID list. Shipping packages have no
direct Admin GraphQL package read root in the captured schema, so successful
staging is verified through local state/log behavior and targeted validation.

Reverse delivery, reverse fulfillment disposal, and order-edit shipping-line
roots are modeled through the orders and returns local graph when covered by
their parity specs. Their caller-visible order and return effects should be read
with `/endpoints/orders/` and `/endpoints/returns/`.

### Boundaries

- Implemented local slices should not be described as broad
  shipping/fulfillments root support beyond their covered request families.
- Delivery customization and delivery promise mutations are Shopify
  Function-backed or provider-backed and remain unsupported until function
  ownership, activation eligibility, metafields, provider state, validation,
  cleanup, and downstream reads are modeled locally.
- Fulfillment constraint rule metadata roots are covered by the Functions
  endpoint group, not by the shipping/fulfillments local slices.
- Validation-only shipping and fulfillment specs prove guardrail payloads and
  no-stage behavior for those inputs only. They do not make the corresponding
  mutation roots generally supported.
- Fulfillment-service inventory transfer semantics, checkout pickup/rate
  calculation, carrier callback execution, carrier service-discovery side
  effects, and full shipping-package discovery/validation remain outside the
  supported local slices.
- Unsupported mutation documents outside the modeled local slices follow the
  configured unsupported path and must remain visible in logs/observability.
