# Shipping and Fulfillments Endpoint Group

This endpoint group is a coverage map for Admin GraphQL shipping, fulfillment, fulfillment-order, carrier-service, fulfillment-service, and delivery-profile behavior. Keep implementation details for locally staged order fulfillment slices in `docs/endpoints/orders.md`; use this file for broader boundaries and gaps.

## Current support and limitations

### Implemented roots

Local staged mutations currently live under the orders group because they operate on order-scoped fulfillment records and downstream `order(id:)` reads:

- `fulfillmentCreate`
- `fulfillmentEventCreate`
- `fulfillmentTrackingInfoUpdate`
- `fulfillmentCancel`
- `fulfillmentOrderHold`
- `fulfillmentOrderReleaseHold`
- `fulfillmentOrderMove`
- `fulfillmentOrderReportProgress`
- `fulfillmentOrderOpen`
- `fulfillmentOrderCancel`
- `fulfillmentOrderSubmitFulfillmentRequest`
- `fulfillmentOrderAcceptFulfillmentRequest`
- `fulfillmentOrderRejectFulfillmentRequest`
- `fulfillmentOrderSubmitCancellationRequest`
- `fulfillmentOrderAcceptCancellationRequest`
- `fulfillmentOrderRejectCancellationRequest`

Those roots are implemented in `tests/integration/order-fulfillment-flow.test.ts` and covered by `config/parity-specs/shipping-fulfillments/fulfillment*.json`. HAR-122/HAR-187 provide the evidence-backed fulfillment lifecycle slices; HAR-233 adds request/cancellation lifecycle evidence backed by `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/fulfillment-order-request-lifecycle.json`.

Top-level fulfillment and fulfillment-order reads are implemented as snapshot/local reads over the existing order graph:

- `fulfillment`
- `fulfillmentOrder`
- `fulfillmentOrders`
- `assignedFulfillmentOrders`
- `manualHoldsFulfillmentOrders`

`fulfillment(id:)` and `fulfillmentOrder(id:)` resolve only records already present on local `Order.fulfillments` / `Order.fulfillmentOrders` data and return `null` for missing IDs. Fulfillment detail reads serialize captured shipment fields including `events`, `deliveredAt`, `estimatedDeliveryAt`, `inTransitAt`, `service`, `location`, `originAddress`, `trackingInfo(first:)`, and fulfillment line items from the same order-backed fulfillment record used by nested `Order.fulfillments`. They do not invent fulfillment records, fulfillment orders, holds, delivery methods, or lifecycle replacement records absent from the snapshot or staged order graph.

HAR-370 adds an order-backed reverse-logistics slice implemented by the orders dispatcher but documented here because the
roots live in the shipping/fulfillments API area:

- `reverseDelivery`
- `reverseFulfillmentOrder`
- `reverseDeliveryCreateWithShipping`
- `reverseDeliveryShippingUpdate`
- `reverseFulfillmentOrderDispose`

`reverseFulfillmentOrder(id:)` and `reverseDelivery(id:)` resolve only records staged from local returns and return `null`
for missing IDs. `returnCreate` and `returnApproveRequest` create local reverse fulfillment order work from returned line
quantities. `reverseDeliveryCreateWithShipping` stores reverse delivery line items plus local tracking/label metadata;
`reverseDeliveryShippingUpdate` updates that metadata. `reverseFulfillmentOrderDispose` records disposition type/location
metadata, reduces remaining local quantities, and closes the reverse fulfillment order when all line work is disposed.
These roots do not call carriers, create real labels, notify customers, move inventory, or mutate locations at runtime.
Executable parity lives in `config/parity-specs/orders/return-reverse-logistics-local-staging.json`; live 2026-04 introspection
evidence lives in `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/return-reverse-logistics-introspection.json`.

`fulfillmentEventCreate` stages local events against an existing order-backed fulfillment and makes them immediately visible in both top-level and nested fulfillment detail reads. Captured 2026-04 behavior showed an `IN_TRANSIT` event updating `Fulfillment.displayStatus`, `estimatedDeliveryAt`, and `inTransitAt`; local staging mirrors that captured shipment-milestone slice while preserving the original raw mutation for commit replay and without contacting Shopify at runtime.

`FulfillmentOrder.deliveryMethod` is an optional local fixture field. When a normalized fulfillment-order record carries delivery-method data, the serializer returns the stored `DeliveryMethod` scalar fields selected by the query; when the record lacks delivery-method data, the field returns `null`. The proxy still does not generate delivery methods from order shipping lines, delivery profiles, or fulfillment-order lifecycle mutations without a captured scenario.

`fulfillmentOrders` lists local order-graph fulfillment orders, excludes `CLOSED` records unless `includeClosed: true` is selected, and supports the captured local subset of ID/status sorting, `reverse`, cursor pagination, and `query` terms for `id`, `status`, and `request_status`. `manualHoldsFulfillmentOrders` returns held local fulfillment orders after staged `fulfillmentOrderHold` calls and otherwise returns the captured no-hold empty connection. `assignedFulfillmentOrders` exposes local order-backed records for staged request/cancellation workflows so tests can observe request-status transitions without an upstream fulfillment-service callback. The HAR-232 live fixture records that the active conformance credential receives `["The api_client is not associated with any fulfillment service."]` for live `assignedFulfillmentOrders`, so broader assignment-status and fulfillment-service scope behavior remains an explicit access-scoped gap rather than guessed behavior.

HAR-234/HAR-367 add fulfillment-order lifecycle staging backed by `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/fulfillment-order-lifecycle.json`. Local support covers merchant-managed fulfillment orders already present on the local order graph:

- `fulfillmentOrderHold` records app-created hold metadata, moves the selected work to `ON_HOLD`, exposes it through `fulfillmentHolds` and `manualHoldsFulfillmentOrders`, and creates an `OPEN` remaining fulfillment order for partial holds.
- `fulfillmentOrderReleaseHold` clears local holds, restores `OPEN` status/actions for the held fulfillment order, re-expands the released line items to include the split remainder, and marks the partial-hold remainder order `CLOSED` with zero remaining quantity.
- `fulfillmentOrderMove` stages full or partial line-item moves by assigning selected work to a replacement fulfillment order at the requested location and leaving remaining quantities on the original order.
- `fulfillmentOrderReportProgress` changes local status to `IN_PROGRESS`; `fulfillmentOrderOpen` changes it back to `OPEN`.
- `fulfillmentOrderCancel` closes the original fulfillment order, clears its line items, and creates an `OPEN` replacement fulfillment order carrying the remaining work.
- `fulfillmentOrderSplit` reduces the original fulfillment-order line-item quantities and creates a synthetic `remainingFulfillmentOrder` for the split-off quantities, including captured `MERGE` supported-action visibility.
- `fulfillmentOrdersSetFulfillmentDeadline` writes the selected deadline to local `fulfillBy` fields and returns `success: true`; downstream `Order.fulfillmentOrders` reads expose the staged deadline.
- `fulfillmentOrderMerge` aggregates split fulfillment-order line quantities back onto the first selected fulfillment order, marks merged sibling fulfillment orders `CLOSED` with zero quantities, and preserves any staged fulfillment deadline.

HAR-234/HAR-367 captured but does not mark full support for `fulfillmentOrderReschedule`, `fulfillmentOrderClose`, `fulfillmentOrdersReroute`, or `fulfillmentOrderLineItemsPreparedForPickup`. The current disposable merchant-managed setup returns `Fulfillment order must be scheduled.` for reschedule, `The fulfillment order's assigned fulfillment service must be of api type` for close, a Shopify internal error for the attempted included-location reroute success branch, and `FULFILLMENT_ORDER_INVALID` for prepared-for-pickup against non-pickup fulfillment orders. The proxy mirrors the captured guardrails locally where modeled, but these roots remain registry-unimplemented until scheduled/API-service/pickup/reroute success setup and downstream read behavior are captured.

`fulfillmentOrderSubmitFulfillmentRequest` records a `FULFILLMENT_REQUEST` merchant request with message and `notify_customer` request options, transitions the submitted fulfillment order to `requestStatus: SUBMITTED`, and mirrors Shopify's partial-request split by shrinking the submitted line-item quantities and creating an unsubmitted replacement fulfillment order for remaining quantities. `fulfillmentOrderAcceptFulfillmentRequest` moves a submitted request to `status: IN_PROGRESS` / `requestStatus: ACCEPTED`. `fulfillmentOrderRejectFulfillmentRequest` moves it to `requestStatus: REJECTED` while preserving requested quantities.

`fulfillmentOrderSubmitCancellationRequest` appends a `CANCELLATION_REQUEST` merchant request to an accepted fulfillment order while preserving Shopify's captured `requestStatus: ACCEPTED` immediately after submission. `fulfillmentOrderAcceptCancellationRequest` closes the fulfillment order with `requestStatus: CANCELLATION_ACCEPTED` and zeroes the modeled fulfillment-order line-item quantities. `fulfillmentOrderRejectCancellationRequest` keeps the order in progress with `requestStatus: CANCELLATION_REJECTED`. These supported roots append original raw mutations to the meta log and never call Shopify or fulfillment-service notification callbacks during normal runtime staging.

Captured HAR-232 evidence lives at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/fulfillment-top-level-reads.json` with parity coverage in `config/parity-specs/shipping-fulfillments/fulfillment-top-level-reads.json`. HAR-235 detail/event evidence lives at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/fulfillment-detail-events-lifecycle.json` with parity coverage in `config/parity-specs/shipping-fulfillments/fulfillment-detail-events-lifecycle.json`. HAR-233 request/cancellation evidence lives at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/fulfillment-order-request-lifecycle.json` with parity coverage in `config/parity-specs/shipping-fulfillments/fulfillment-order-request-lifecycle.json`. Runtime coverage is in `tests/integration/order-query-shapes.test.ts` and `tests/integration/order-fulfillment-flow.test.ts`.

Fulfillment service reads and lifecycle writes are implemented as a shipping/fulfillments slice because they create and mutate service-managed locations:

- `fulfillmentService`
- `fulfillmentServiceCreate`
- `fulfillmentServiceUpdate`
- `fulfillmentServiceDelete`

The current 2026-04 schema exposes detail lookup through top-level `fulfillmentService(id:)`; the list/catalog surface is `shop.fulfillmentServices`, not a separate top-level list root. Local staging stores fulfillment services in normalized state, creates an associated `Location` for new third-party services, keeps `Location.fulfillmentService` linked to the service record, and makes downstream `fulfillmentService(id:)`, `shop.fulfillmentServices`, `location(id:)`, and meta state/log reads observe the staged graph.

Create/update support covers `name`, `callbackUrl`, `trackingSupport`, `inventoryManagement`, and `requiresShippingMethod`. Captured behavior showed create-time handle normalization from the service name and update-time handle stability when the name changes; the associated location name follows the updated service name. The local model accepts no callback URL or the captured app-safe `https://mock.shop/...` URL family and returns the captured `Callback url is not allowed` userError for other callback URLs.

Delete support covers unknown-id userErrors and inventory actions at the local state level. `DELETE` and `TRANSFER` remove the fulfillment-service location from local reads; `KEEP` converts the associated location to merchant-managed by clearing `fulfillmentService` and `isFulfillmentService`. Inventory movement itself remains local bookkeeping only until inventory-level transfer fixtures exist.

Callback, stock fetch, tracking fetch, and fulfillment-order notification endpoints are never invoked by local staging. The proxy records callback URL and capability flags only as Shopify-like service metadata.

Executable parity evidence for the fulfillment-service lifecycle lives in `config/parity-specs/shipping-fulfillments/fulfillment-service-lifecycle.json`. The spec replays the captured create/update/delete lifecycle, downstream `fulfillmentService(id:)` and `location(id:)` reads, after-delete absence, and validation branches through local proxy requests. It compares the created service/location directly instead of the full captured `shop.fulfillmentServices` catalog because the disposable live store contained unrelated pre-existing services that are not required preconditions for isolated local staging.

Carrier service reads and lifecycle writes are implemented as a shipping/fulfillments slice because they affect checkout rate-provider configuration:

- `availableCarrierServices`
- `carrierService`
- `carrierServices`
- `carrierServiceCreate`
- `carrierServiceUpdate`
- `carrierServiceDelete`

Live Admin GraphQL 2026-04 schema introspection confirmed the top-level read roots, create/update roots, and `carrierServiceDelete`. HAR-320 safe-read evidence on 2025-01 captured `availableCarrierServices` returning `DeliveryCarrierServiceAndLocations` pairs for active Shopify Shipping carrier services and their available locations. The local state stores carrier services as `DeliveryCarrierService` records with `name`, `formattedName`, `callbackUrl`, `active`, `supportsServiceDiscovery`, and internal created/updated timestamps for local sorting.

Snapshot reads return Shopify-like no-data structures: `carrierService(id:)` returns `null` for a missing service, and `carrierServices(...)` returns an empty connection with empty `nodes`/`edges`, false page booleans, and null cursors. Catalog support covers the captured slice for `query: "active:true|false"` and `query: "id:<numeric id or gid>"`, `sortKey: ID|CREATED_AT|UPDATED_AT`, `reverse`, and standard cursor pagination through the shared connection helpers.

`availableCarrierServices` serializes active local carrier services paired with active merchant-managed local locations. With no local carrier services it returns an empty list. Location selections use the same local `Location` serializer as `location(id:)`, so staged local-pickup settings are visible in the returned location list without calling carrier callbacks or Shopify upstream in snapshot mode.

Create/update support covers `input.name`, `input.callbackUrl`, `input.active`, and `input.supportsServiceDiscovery`. Captured behavior showed Shopify returning `formattedName` as `<name> (Rates provided by app)` for an app carrier service, update-time downstream visibility through both detail and catalog roots, blank-name create as `userErrors[{ field: null, message: "Shipping rate provider name can't be blank" }]`, unknown update as `field: null`, and unknown delete as `field: ["id"]` with `The carrier or app could not be found.`.

Delete support is enabled because the 2026-04 schema exposes `carrierServiceDelete(id:)` and the live lifecycle capture verified `deletedId` plus downstream detail/catalog absence after cleanup. Local delete only removes the staged/local record; it does not call Shopify or any external callback.

Carrier-service callback URLs and service-discovery flags are recorded only as Shopify-like metadata for read-after-write behavior. Local staging never invokes rate callbacks, service-discovery callbacks, or any checkout-rate side effects.

Executable parity evidence for the carrier-service lifecycle lives in `config/parity-specs/shipping-fulfillments/carrier-service-lifecycle.json`. The spec replays the captured create/update/delete lifecycle, downstream detail and active-filter catalog reads, after-delete absence, and validation branches through local proxy requests. It omits the captured opaque id-filter cursor branch from the replay comparison because the isolated proxy execution uses synthetic carrier-service IDs, while runtime coverage still exercises id-filter behavior directly.

Delivery-profile reads are implemented as fixture-backed snapshot reads:

- `locationsAvailableForDeliveryProfilesConnection` returns active local locations through the shared connection helpers and supports `first`, `last`, `after`, `before`, and `reverse`.
- `deliveryProfiles` returns a local connection from normalized `deliveryProfiles` snapshot state and supports `first`, `last`, `after`, `before`, `reverse`, and `merchantOwnedOnly` without contacting upstream Shopify.
- `deliveryProfile(id:)` returns the normalized profile detail when present and `null` for a missing id.
- Snapshot mode does not invent shipping profiles. With no normalized delivery-profile fixtures, `deliveryProfiles` returns an empty connection and `deliveryProfile(id:)` returns `null`.
- Nested profile detail serializes captured scalar counts, profile items, product/variant associations, profile location groups, locations, countries/provinces, zones, method definitions, rate providers, method conditions, selling-plan group connections, and unassigned locations when those fields exist in the normalized fixture.
- Generic `node(id:)` / `nodes(ids:)` dispatch resolves the modeled nested delivery-profile records already present in the normalized profile graph: `DeliveryLocationGroup`, `DeliveryZone`, `DeliveryCountry`, `DeliveryProvince`, `DeliveryMethodDefinition`, `DeliveryRateDefinition`, `DeliveryParticipant`, and `DeliveryCondition`. Missing IDs and delivery-profile-adjacent records outside that graph still return `null`.
- Product, variant, and location associations are stored as ids and projected from the existing product/location state. A delivery profile fixture should not duplicate full product, variant, or location blobs.
- Live read evidence for 2026-04 is checked in at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/delivery-profiles-read.json`. The capture used `SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm conformance:capture-delivery-profiles`; no access-scope or manage-delivery-settings blocker was encountered for the current conformance credential.
- HAR-462 Node parity for nested delivery-profile records is registered in `config/parity-specs/admin-platform/admin-platform-delivery-profile-node-reads.json` and compares generic `nodes(ids:)` projections against the captured 2026-04 `deliveryProfile(id:)` downstream payload. `DeliveryProfileItem` remains unsupported because the normalized profile item model is product/variant keyed and the current capture does not expose a stable profile-item Node id; order-scoped `DeliveryMethod` records remain under order/fulfillment modeling rather than delivery-profile dispatch.

Delivery-profile writes are implemented for a deliberately bounded, conformance-backed custom-profile subset:

- `deliveryProfileCreate`
- `deliveryProfileUpdate`
- `deliveryProfileRemove`

- `deliveryProfileCreate(profile:)` stages a merchant-owned, non-default delivery profile locally. Supported input fields are `name`, `locationGroupsToCreate` / `profileLocationGroups`, nested `locations`, `zonesToCreate`, `countries`, static `rateDefinition` method definitions, weight/price conditions, and `variantsToAssociate`.
- `deliveryProfileUpdate(id:, profile:)` stages profile renames, variant association/dissociation, location-group create/update/delete, location add/remove, zone create/update/delete, method-definition create/update/delete, condition update/delete, and selling-plan group id association bookkeeping.
- `deliveryProfileRemove(id:)` stages custom-profile removal locally and returns a Shopify-like asynchronous `Job` payload with `done: false`; downstream local reads treat the profile as removed immediately so tests can observe the staged graph without waiting for Shopify's background job.
- Successful create/update/remove mutations append staged mutation-log entries with the original GraphQL request body for commit replay. Validation branches with no state change return local `userErrors` and are not added to the commit log.
- Variant association moves the variant into the target local profile and removes it from other locally known delivery profiles so downstream `deliveryProfile` / `deliveryProfiles` reads stay single-owner for the modeled variant.
- Captured 2026-04 write evidence is checked in at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/delivery-profile-writes.json` and registered by `config/parity-specs/shipping-fulfillments/delivery-profile-lifecycle.json`. The capture covered blank-name validation, nested create, nested update, condition delete, variant dissociation, missing update/remove, default-profile removal denial, async removal job payload, and downstream null read after removal. No access-scope or manage-delivery-settings blocker was encountered for the current conformance credential.

Delivery settings and promise settings have a narrow read-only snapshot slice:

- `deliverySettings`
- `deliveryPromiseSettings`

The HAR-324 live probe against `harry-test-heelo.myshopify.com` on Admin GraphQL 2025-01 captured the current empty/no-feature settings branch: `deliverySettings.legacyModeProfiles` is `false`, `legacyModeBlocked.blocked` is `false`, `legacyModeBlocked.reasons` is `null`, `deliveryPromiseSettings.deliveryDatesEnabled` is `false`, and `deliveryPromiseSettings.processingTime` is `null`. Snapshot mode returns that shape locally without hitting upstream Shopify.

`deliverySettingUpdate` remains unsupported. Changing delivery settings can alter shop delivery configuration and legacy-mode behavior, so it must not be marked supported until success, rollback/cleanup, validation errors, and downstream read-after-write effects are captured and modeled locally.

Delivery customization and delivery promise roots remain explicit HAR-324 declared gaps:

- `deliveryCustomization`
- `deliveryCustomizations`
- `deliveryCustomizationCreate`
- `deliveryCustomizationUpdate`
- `deliveryCustomizationDelete`
- `deliveryCustomizationActivation`
- `deliveryPromiseParticipants`
- `deliveryPromiseParticipantsUpdate`
- `deliveryPromiseProvider`
- `deliveryPromiseProviderUpsert`

Current live evidence is blocker-only for these families. The configured conformance credential has `read_shipping` / `write_shipping`, but it does not have `read_delivery_customizations`, `read_delivery_promises`, or the corresponding delivery-write scope families. Probing `deliveryCustomization` and `deliveryCustomizations` returned `ACCESS_DENIED` requiring `read_delivery_customizations`; probing `deliveryPromiseParticipants` and `deliveryPromiseProvider` returned `ACCESS_DENIED` requiring `read_delivery_promises`.

Delivery customization mutations are Shopify Function-backed and depend on external function IDs, activation eligibility, function ownership, and metafields. Delivery promise mutations depend on branded promise handles, participant owner eligibility, provider state by location, and delivery-promise access scopes. Until those branches are captured and locally modeled, they stay on the unsupported mutation passthrough path with registered-operation and safety metadata in the mutation log.

Shipping settings roots implemented by HAR-320:

- `locationLocalPickupEnable`
- `locationLocalPickupDisable`
- `shippingPackageUpdate`
- `shippingPackageMakeDefault`
- `shippingPackageDelete`

`locationLocalPickupEnable(localPickupSettings:)` locally stores `pickupTime` and `instructions` on the targeted active `Location` and returns the captured `DeliveryLocalPickupSettings` payload shape. `locationLocalPickupDisable(locationId:)` clears those local settings and returns the disabled `locationId`. Downstream `location(id:)`, `locationsAvailableForDeliveryProfilesConnection`, and `availableCarrierServices.locations` reads observe the staged `localPickupSettingsV2` value. Unknown or inactive locations return the captured `ACTIVE_LOCATION_NOT_FOUND` userError field paths: `["localPickupSettings"]` for enable and `["locationId"]` for disable.

Shipping package mutations stage against normalized local package records. `shippingPackageUpdate` persists name, type, default flag, weight, and dimensions for known package IDs; `shippingPackageMakeDefault` clears the previous local default and marks the selected package as default; `shippingPackageDelete` records local deletion and returns `deletedId`. Admin GraphQL 2025-01 exposes no package read root in the captured schema, so immediate visibility is through meta state/log inspection and downstream in-memory package bookkeeping. Unknown package IDs return Shopify's captured top-level `RESOURCE_NOT_FOUND` / `invalid id` GraphQL error instead of a payload `userErrors` array.

HAR-320 fulfillment-constraint evidence is blocker-only. The current conformance credential receives `ACCESS_DENIED` for `fulfillmentConstraintRules` without `read_fulfillment_constraint_rules` and for `fulfillmentConstraintRuleCreate`, `fulfillmentConstraintRuleUpdate`, and `fulfillmentConstraintRuleDelete` without `write_fulfillment_constraint_rules`. These roots are registry-only until a scoped test app can capture Shopify Function ownership, metafield behavior, success payloads, and downstream rule reads.

### Registry-only coverage map

These roots are known Admin GraphQL shipping/fulfillment surface area, but they are not locally implemented. They are registered with `implemented: false` as explicit future local-model commitments, not as supported passthrough behavior.

Fulfillment-order mutations:

- `fulfillmentOrderClose`
- `fulfillmentOrderLineItemsPreparedForPickup`
- `fulfillmentOrderReschedule`
- `fulfillmentOrdersReroute`

Fulfillment constraint rules:

- `fulfillmentConstraintRules`
- `fulfillmentConstraintRuleCreate`
- `fulfillmentConstraintRuleDelete`
- `fulfillmentConstraintRuleUpdate`

Delivery customizations and promises:

- `deliveryCustomization`
- `deliveryCustomizations`
- `deliveryCustomizationCreate`
- `deliveryCustomizationUpdate`
- `deliveryCustomizationDelete`
- `deliveryCustomizationActivation`
- `deliveryPromiseParticipants`
- `deliveryPromiseParticipantsUpdate`
- `deliveryPromiseProvider`
- `deliveryPromiseProviderUpsert`
- `deliverySettingUpdate`

Shipping-line order-edit roots:

- `orderEditAddShippingLine` is implemented through the orders calculated-edit model. It stages shipping lines on `CalculatedOrder.shippingLines`, recalculates totals, and materializes committed shipping lines on `orderEditCommit` without runtime Shopify writes.
- `orderEditRemoveShippingLine` is implemented through the orders calculated-edit model. It removes locally known calculated shipping lines, recalculates totals, and preserves userErrors for unknown shipping-line IDs.
- `orderEditUpdateShippingLine` is implemented through the orders calculated-edit model. It updates locally known calculated shipping line title/price, recalculates totals, and preserves userErrors for unknown shipping-line IDs.

### Behavior boundaries

- The proxy must not treat any registry-only root above as supported runtime behavior. Until a root has local state modeling and executable tests, unsupported mutations remain on the generic unsupported path and must stay visible in observability.
- Top-level `fulfillment(id:)` now has missing-id, tracking-info, line-item, detail, and event-history shape evidence for records already present on the local order graph. Broader access-scope behavior still needs separate captures before expansion.
- Fulfillment orders are created by Shopify after order routing, not by a direct create mutation. Current local support can split existing order-backed fulfillment orders during fulfillment-request submission, but broader generation from order/draft-order creation, location assignment, line-item grouping, holds, and delivery methods still needs separate coverage.
- Fulfillment-order visibility is scope-sensitive. `assignedFulfillmentOrders`, `fulfillmentOrders`, and `Order.fulfillmentOrders` can return different subsets depending on assigned, merchant-managed, third-party, and marketplace fulfillment-order scopes.
- Fulfillment-order lifecycle mutations can create replacement orders, split or merge line items, change assigned locations, add/release holds, change deadlines, and update request status. Do not model one of these as a simple status patch without captured downstream reads.
- Fulfillment-service mutations couple service records to locations. Creation automatically creates a location, update does not replace `LocationEdit` for service-managed location details, and deletion has inventory/location disposition semantics. HAR-236 covers the first local service/location lifecycle slice; broader inventory transfer fidelity still needs dedicated inventory-level captures.
- Broader carrier-service support still depends on app ownership, `write_shipping` access, plan eligibility, available-service/location pairing, and service-discovery callback semantics outside the locally staged catalog/lifecycle slice.
- Delivery-profile write support is intentionally limited to custom merchant-owned profiles with static rate definitions. Carrier/service participants, callback-backed rates, full selling-plan routing semantics, legacy-mode transitions, default-profile mutation behavior beyond captured remove denial, and Shopify's full delivery-setting eligibility/access matrix remain excluded until separately captured and modeled.
- Delivery settings read support is read-only and reflects the captured no-legacy-mode/no-promise-settings branch. Do not infer that `deliverySettingUpdate` is safe to stage merely because the read shape is local.
- Delivery customization roots are Shopify Function-backed. Do not convert validation-only or access-denied evidence into local create/update/delete/activation support without modeling function ownership, function interface eligibility, metafields, activation limits, and downstream reads.
- Delivery promise provider/participant roots are access-scope and eligibility sensitive. The current credential blocker is evidence for declared-gap status, not evidence for empty successful reads.
- Local-pickup support is limited to `Location.localPickupSettingsV2` read-after-write behavior. Checkout pickup option ranking, pickup inventory eligibility, notification behavior, and local-delivery coupling require separate captures.
- Shipping package support is a local staging slice for known package records. Package discovery, carrier package compatibility, checkout rate calculation, and full package validation remain future work because the captured schema has no direct package read root.
- Shipping lines and delivery methods are nested under orders, draft orders, calculated orders, fulfillment orders, and delivery profiles. A root-level registry entry can only cover the mutation/query root; nested field fidelity still needs scenario-specific fixtures and downstream read assertions.

## Historical and developer notes

### Validation anchors

- Implemented order-scoped fulfillments: `tests/integration/order-fulfillment-flow.test.ts`
- Implemented top-level fulfillment reads: `tests/integration/order-query-shapes.test.ts`
- Implemented fulfillment services: `tests/integration/fulfillment-service-flow.test.ts`
- Implemented carrier services: `tests/integration/carrier-service-flow.test.ts`
- Implemented delivery settings reads: `tests/integration/delivery-settings-query-shapes.test.ts`
- Implemented shipping settings/package/pickup slice: `tests/integration/shipping-settings-flow.test.ts`
- Implemented delivery-profile reads: `tests/integration/delivery-profile-query-shapes.test.ts`
- Implemented delivery-profile writes: `tests/integration/delivery-profile-lifecycle-flow.test.ts`
- Existing fulfillment parity specs and requests: `config/parity-specs/shipping-fulfillments/fulfillment*.json` and matching files under `config/parity-requests/shipping-fulfillments/`
- Fulfillment-order request/cancellation fixture: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/fulfillment-order-request-lifecycle.json`
- Carrier-service capture/parity metadata: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/carrier-service-lifecycle.json` and `config/parity-specs/shipping-fulfillments/carrier-service-lifecycle.json`
- Delivery-profile read capture: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/delivery-profiles-read.json`
- Delivery-profile write capture: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/delivery-profile-writes.json`
- Delivery settings/customization/promise probe evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/shipping-fulfillments/delivery-customization-promise-settings-blockers.json`
- Shipping settings/package/pickup/constraint evidence: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/shipping-fulfillments/shipping-settings-package-pickup-constraints.json` and `config/parity-specs/shipping-fulfillments/shipping-settings-package-pickup-constraints.json`
- Existing order docs for fulfilled order read-after-write behavior: `docs/endpoints/orders.md`
- Registry/coverage tests: `tests/unit/operation-registry.test.ts`, `tests/integration/proxy-capability-classification.test.ts`
