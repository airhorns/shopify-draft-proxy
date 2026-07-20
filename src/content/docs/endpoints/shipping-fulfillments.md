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

- `deliveryCustomization`
- `deliveryCustomizations`
- `deliveryPromiseParticipants`
- `deliveryPromiseProvider`
- `locationsAvailableForDeliveryProfilesConnection`

The implemented mutation roots are:

- `carrierServiceCreate`
- `carrierServiceDelete`
- `carrierServiceUpdate`
- `deliveryCustomizationActivation`
- `deliveryCustomizationCreate`
- `deliveryCustomizationDelete`
- `deliveryCustomizationUpdate`
- `deliveryPromiseParticipantsUpdate`
- `deliveryPromiseProviderUpsert`
- `fulfillmentEventCreate`
- `fulfillmentOrderLineItemsPreparedForPickup`
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
- `fulfillmentOrderMove`
- `fulfillmentOrderOpen`
- `fulfillmentOrderReleaseHold`
- `fulfillmentOrderReportProgress`
- `fulfillmentOrderReschedule`
- `fulfillmentOrdersReroute`
- `fulfillmentOrdersSetFulfillmentDeadline`
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
In LiveHybrid, `fulfillmentServiceUpdate` and `fulfillmentServiceDelete` can
hydrate a real app-owned fulfillment service by ID before applying local
lifecycle validation, and create/update uniqueness validation hydrates the
effective `shop.fulfillmentServices` catalog before checking service-name or
generated-handle conflicts. These hydration reads are query-only; supported
service mutations still stage locally and keep the original raw mutation for
commit replay.

Reverse-logistics shipping slices stage `reverseDeliveryCreateWithShipping`
from the return domain's reverse fulfillment order state. Explicit
`reverseDeliveryLineItems` inputs preserve one staged delivery line per input
with the requested quantity and line item; empty inputs expand to all reverse
fulfillment order lines at their total quantities. The recorded order/return
parity fixture uses one two-line return for empty expansion and a second
two-line return for explicit multi-line delivery creation. Live-hybrid create,
shipping-update, and disposal mutations query-hydrate cold authoritative
reverse-logistics resources and referenced locations before staging. Missing,
wrong-type, unrelated, duplicate, and over-quantity references fail atomically;
the handlers do not fabricate submitted relationships or append rejected
mutations to the ordered commit log. Valid hydrated mutations remain local-only
until explicit commit and are visible through reverse-logistics and generic
node reads.

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
In LiveHybrid, `carrierServiceUpdate` and `carrierServiceDelete` can hydrate a
real app-owned carrier service by ID before staging the lifecycle mutation, and
`carrierServiceCreate` hydrates the effective carrier-service catalog before
duplicate-name validation. The runtime does not send those supported mutations
upstream during staging; only the narrow hydrate queries are issued before local
validation and mutation-log recording.

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
`Order.fulfillmentOrders` reads. Supported actions for these order-backed
fulfillment orders are recomputed from current status and assignment: terminal
`CLOSED` / `CANCELLED` records return an empty action list, and merchant-managed
`OPEN` records do not advertise fulfillment-service-only `REPORT_PROGRESS`.
Split fulfillment orders preserve fulfillment-service actions observed on the
source order, while merge recomputes peer-sensitive actions so `MERGE` is absent
when no compatible open peer remains.
`fulfillmentOrderLineItemsPreparedForPickup` stages pickup preparation for
selected order-backed fulfillment orders that resolve from staged, observed, or
LiveHybrid-hydrated order state. The local branch validates every requested
fulfillment order before applying any state change: structurally invalid
non-fulfillment-order GIDs return Shopify's top-level `invalid id` /
`RESOURCE_NOT_FOUND` shape, unknown IDs return a payload `userErrors` entry on
the indexed `fulfillmentOrderId` field with `FULFILLMENT_ORDER_INVALID`, and
non-pickup, closed, canceled, or zero-remaining fulfillment orders return the
same payload user-error shape without staging or logging. Successful batches
move only the requested pickup fulfillment orders to `IN_PROGRESS`, recompute
their supported actions, mark their line items prepared for pickup, project
`fulfillableQuantity: 0` while preserving the stored remaining quantities, and
refresh the parent order's display fulfillment status. The mutation retains the
original raw request for commit replay, and the staged result is visible through
`fulfillmentOrder`, `fulfillmentOrders`, `assignedFulfillmentOrders`, and nested
`Order.fulfillmentOrders` reads. Existing public evidence covers the invalid-ID
and non-pickup validation branches; successful pickup preparation is covered by
focused Rust runtime tests until a disposable pickup-order capture is available.
Locally created order fulfillment orders derive their initial `assignedLocation`
from the first active observed/staged shop location that fulfills online orders;
the runtime does not fabricate
`gid://shopify/Location/1` when no such location is known. These slices operate
on local order-backed fulfillment records and are not a general
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
Top-level fulfillment-order catalogs compute an effective connection from the
read-only upstream catalog response plus local staged lifecycle records when
staged fulfillment-order state is present. Matching IDs are overlaid with the
staged record, unrelated upstream records remain visible, and staged-only
replacement or split records are appended before local filtering, sorting, and
cursor windowing. If upstream catalog hydration is unavailable, these roots
fall back to the staged local set. `fulfillmentOrders` excludes closed or
cancelled records unless `includeClosed: true` is present and models `id`,
`status`, `assigned_location_id`, `updated_at`, and free-text query terms.
`assignedFulfillmentOrders` excludes closed records and applies
`assignmentStatus` plus `locationIds`; `manualHoldsFulfillmentOrders` narrows to
held records and applies its modeled order-search terms. The catalog roots
share local `sortKey: ID` / `UPDATED_AT`, `reverse`, and cursor window
semantics over that effective set.
Generic `node(id:)` and `nodes(ids:)` now read order-backed fulfillment and
fulfillment-order resources from the same normalized order graph as
`fulfillment(id:)`, `fulfillmentOrder(id:)`, and nested `Order` selections:
`Fulfillment`, `FulfillmentEvent`, `FulfillmentLineItem`, `FulfillmentOrder`,
`FulfillmentHold`, and `FulfillmentOrderLineItem` resolve locally, reflect
staged lifecycle changes immediately, preserve duplicate/order semantics for
`nodes(ids:)`, and return `null` for missing or deleted IDs.

Delivery settings and delivery promise settings are read-only in snapshot mode
and return the captured empty/no-feature shape there. Live modes forward those
shop-wide settings reads upstream so the app sees the real merchant
configuration.

Delivery-promise provider and participant roots model the provider/participant
lifecycle locally. `deliveryPromiseProviderUpsert(locationId:)` stages one
normalized `DeliveryPromiseProvider` per app-owned fulfillment-service location,
updates the same provider ID on repeated upserts, preserves omitted configuration
fields from the existing provider, and returns typed provider user errors for
unknown locations, non-app-owned locations, invalid time zones, and overlong
time-zone input. `deliveryPromiseParticipantsUpdate(brandedPromiseHandle:)`
stages `DeliveryPromiseParticipant` membership for existing `ProductVariant`
owners, de-duplicates repeated add/remove IDs, treats removal of unknown members
as a no-op, tombstones removed participant IDs, and rejects invalid add owners
without staging. Any delivery-promise mutation batch with user errors is
atomic for the local model: no provider/participant state is staged and no
mutation-log entry is appended. Successful delivery-promise mutations retain the
original raw GraphQL request for commit replay and never write to Shopify during
normal proxy runtime.
`deliveryPromiseProvider(locationId:)`, `deliveryPromiseParticipants(...)`, and
generic `node(id:)` / `nodes(ids:)` read from the same effective delivery-promise
state, apply selected fields, aliases, inline fragments, `ownerIds`, `reverse`,
and cursor windows, preserve `nodes(ids:)` input order and duplicates, and return
`null` / empty connections for missing, removed, or unsupported IDs. In
LiveHybrid mode, provider reads hydrate independently per `locationId`, generic
Node reads hydrate independently per delivery-promise ID, and participant reads
hydrate independently per branded-promise handle and `ownerIds` filter. A
paginated participant baseline remains partial until a complete cursor traversal
reaches its terminal page, so one provider, participant page, or filter never suppresses an
unrelated upstream read. Complete baselines overlay staged upserts, membership
changes, and tombstones before applying local cursor windows. Dump/restore keeps
selector completeness and partial participant page progress without treating a
partially observed domain as a complete catalog. Live Shopify parity capture
for these roots currently requires delivery-promise app scopes, so the local
contract is covered by focused Rust runtime tests until those scopes are
available.

Delivery profiles have fixture-backed read and bounded write slices for
create/update/remove, validation, variant dissociation, async removal payloads,
and downstream null reads after removal. Custom profiles are fully staged from
create/update inputs covered by the delivery-profile parity requests. In
LiveHybrid mode, `deliveryProfileUpdate` can hydrate an existing default profile
and stage proxy-modelable updates without writing to Shopify at runtime. Cold
updates hydrate the authoritative profile-item, variant, location-group,
location, zone, method-definition, selling-plan-group, and unassigned-location
relationships, following each selected connection until its final page. A
scalar-only update preserves that baseline and its Count precision metadata;
explicit relationship additions and removals are applied over the hydrated
profile without clearing unrelated rows. Missing relationship selections remain
unknown during normalization, while relationships that Shopify selected as
empty normalize to authoritative empty lists.
Captured Admin GraphQL 2026-04 behavior accepts a default-profile name input
with empty `userErrors` while preserving the public default display name and
incrementing `version`. The `delivery-profile-default-update` parity scenario
strictly compares the whole selected mutation profile and immediate downstream
readback against a real default profile whose items require multiple hydrate
pages. Unsupported side effects such as rate recalculation remain outside this
slice. Delivery profile name validation accepts exactly 128
characters and rejects 129-character names on both create and update with a
public `UserError` payload containing `field` and `message`; `code` is not
selectable on the captured Admin GraphQL 2026-04 `UserError` type. Location IDs
supplied in delivery-profile location groups must resolve from staged, observed,
or LiveHybrid-hydrated location state; unknown IDs return the public
`The Location could not be found for this shop.` userError instead of creating a
synthetic location. Delivery-profile `variantsToAssociate` inputs add
associations only for `ProductVariant` IDs resolved from staged/base product
state or LiveHybrid `nodes(ids:)` hydration. Nonexistent, inaccessible, or
wrong-shop variant lookups that hydrate as missing nodes are left unassociated;
wrong GID types return a top-level `RESOURCE_NOT_FOUND` error and leave profile
state unchanged. Indeterminate hydration failures, including upstream transport
failures and malformed hydrate payloads, do not count as existence and do not
stage associations. Profile item product/variant IDs, titles, and relationships
derive from the resolved product/variant state instead of placeholder products.
Captured 2026-04 parity target `delivery-profile-variant-associations` covers a
valid staged variant, nonexistent association targets, wrong-GID-type failures,
and downstream reads after invalid update attempts.
`deliveryProfiles(first/last/after/before/reverse:)` merges
observed merchant baseline profiles, including the default profile, with staged
profile creates/updates/removals before computing page windows and `pageInfo`
boundary cursors instead of returning a canned connection envelope. Captured
2026-04 parity target `delivery-profile-post-create-catalog-keeps-default`
creates a disposable profile and then lists `deliveryProfiles`, asserting the
merchant default profile remains visible alongside the staged create.

Delivery customization slices stage create, update, activation, and delete
mutations locally without writing to Shopify during normal proxy runtime.
Successful mutations retain the original raw GraphQL request for commit replay;
validation failures return `userErrors` and do not stage records or append
mutation-log entries. The local record model stores the customization id, title,
enabled state, owning Shopify Function identity, selected Function metadata,
metafields, and timestamps. Create resolves a Function by handle from the
current app when needed, rejects missing or ambiguous Function identifiers,
enforces the active-customization limit, validates required title/enabled input
and metafield fields, and preserves `$app` metafield namespace behavior for the
requesting API client. Update preserves Function identity, supports title,
enabled state, and metafield replacement, and rejects unknown customization IDs
or attempts to move a customization to another Function. Activation updates
known IDs idempotently and reports unknown or over-limit inputs through
Shopify-shaped `userErrors`; delete tombstones known IDs so later detail and
generic Node reads return null. `deliveryCustomization(id:)` and
`deliveryCustomizations(first/last/after/before/query/sortKey/reverse:)` read
from the staged customization store, return Shopify-like null/empty shapes when
no local data exists, apply selected fields and connection windows, and reflect
read-after-write state immediately. Generic `node(id:)` and `nodes(ids:)` reads
resolve staged delivery customizations through the same normalized record,
preserve `nodes(ids:)` input order, and return null for missing or deleted IDs.

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
- Delivery promise mutations are provider-backed and remain unsupported until
  provider state, validation, cleanup, and downstream reads are modeled locally.
- Delivery customization runtime behavior is covered by local integration tests.
  Live Shopify parity capture for successful lifecycle writes also requires an
  installed delivery-customization Shopify Function in the conformance app; when
  that Function is unavailable, proxy-only runtime tests must not be treated as
  captured Shopify evidence.
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
