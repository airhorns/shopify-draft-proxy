# Shipping and Fulfillments Endpoint Group

This endpoint group is a coverage map for Admin GraphQL shipping, fulfillment, fulfillment-order, carrier-service, fulfillment-service, and delivery-profile behavior. Keep implementation details for locally staged order fulfillment slices in `docs/endpoints/orders.md`; use this file for broader boundaries and gaps.

## Implemented roots

Local staged mutations currently live under the orders group because they operate on order-scoped fulfillment records and downstream `order(id:)` reads:

- `fulfillmentCreate`
- `fulfillmentEventCreate`
- `fulfillmentTrackingInfoUpdate`
- `fulfillmentCancel`

Those roots are implemented in `tests/integration/order-fulfillment-flow.test.ts` and covered by `config/parity-specs/fulfillment*.json`. HAR-122/HAR-187 provide the evidence-backed fulfillment lifecycle slices; this document does not duplicate those request/fixture details.

Top-level fulfillment and fulfillment-order reads are implemented as snapshot/local reads over the existing order graph:

- `fulfillment`
- `fulfillmentOrder`
- `fulfillmentOrders`
- `assignedFulfillmentOrders`
- `manualHoldsFulfillmentOrders`

`fulfillment(id:)` and `fulfillmentOrder(id:)` resolve only records already present on local `Order.fulfillments` / `Order.fulfillmentOrders` data and return `null` for missing IDs. Fulfillment detail reads serialize captured shipment fields including `events`, `deliveredAt`, `estimatedDeliveryAt`, `inTransitAt`, `service`, `location`, `originAddress`, `trackingInfo(first:)`, and fulfillment line items from the same order-backed fulfillment record used by nested `Order.fulfillments`. They do not invent fulfillment records, fulfillment orders, holds, delivery methods, or lifecycle replacement records absent from the snapshot or staged order graph.

`fulfillmentEventCreate` stages local events against an existing order-backed fulfillment and makes them immediately visible in both top-level and nested fulfillment detail reads. Captured 2026-04 behavior showed an `IN_TRANSIT` event updating `Fulfillment.displayStatus`, `estimatedDeliveryAt`, and `inTransitAt`; local staging mirrors that captured shipment-milestone slice while preserving the original raw mutation for commit replay and without contacting Shopify at runtime.

`fulfillmentOrders` lists local order-graph fulfillment orders, excludes `CLOSED` records unless `includeClosed: true` is selected, and supports the captured local subset of ID/status sorting, `reverse`, cursor pagination, and `query` terms for `id`, `status`, and `request_status`. `manualHoldsFulfillmentOrders` currently returns the captured no-hold empty connection because the local model does not store fulfillment holds yet. `assignedFulfillmentOrders` currently returns an empty local connection; the HAR-232 live fixture records that the active conformance credential receives `["The api_client is not associated with any fulfillment service."]` for that root, so broader assignment-status and fulfillment-service scope behavior remains an explicit access-scoped gap rather than guessed behavior.

Captured HAR-232 evidence lives at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/fulfillment-top-level-reads.json` with parity coverage in `config/parity-specs/fulfillment-top-level-reads.json`. HAR-235 detail/event evidence lives at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/fulfillment-detail-events-lifecycle.json` with parity coverage in `config/parity-specs/fulfillment-detail-events-lifecycle.json`. Runtime coverage is in `tests/integration/order-query-shapes.test.ts` and `tests/integration/order-fulfillment-flow.test.ts`.

Fulfillment service reads and lifecycle writes are implemented as a shipping/fulfillments slice because they create and mutate service-managed locations:

- `fulfillmentService`
- `fulfillmentServiceCreate`
- `fulfillmentServiceUpdate`
- `fulfillmentServiceDelete`

The current 2026-04 schema exposes detail lookup through top-level `fulfillmentService(id:)`; the list/catalog surface is `shop.fulfillmentServices`, not a separate top-level list root. Local staging stores fulfillment services in normalized state, creates an associated `Location` for new third-party services, keeps `Location.fulfillmentService` linked to the service record, and makes downstream `fulfillmentService(id:)`, `shop.fulfillmentServices`, `location(id:)`, and meta state/log reads observe the staged graph.

Create/update support covers `name`, `callbackUrl`, `trackingSupport`, `inventoryManagement`, and `requiresShippingMethod`. Captured behavior showed create-time handle normalization from the service name and update-time handle stability when the name changes; the associated location name follows the updated service name. The local model accepts no callback URL or the captured app-safe `https://mock.shop/...` URL family and returns the captured `Callback url is not allowed` userError for other callback URLs.

Delete support covers unknown-id userErrors and inventory actions at the local state level. `DELETE` and `TRANSFER` remove the fulfillment-service location from local reads; `KEEP` converts the associated location to merchant-managed by clearing `fulfillmentService` and `isFulfillmentService`. Inventory movement itself remains local bookkeeping only until inventory-level transfer fixtures exist.

Callback, stock fetch, tracking fetch, and fulfillment-order notification endpoints are never invoked by local staging. The proxy records callback URL and capability flags only as Shopify-like service metadata.

Carrier service reads and lifecycle writes are implemented as a shipping/fulfillments slice because they affect checkout rate-provider configuration:

- `carrierService`
- `carrierServices`
- `carrierServiceCreate`
- `carrierServiceUpdate`
- `carrierServiceDelete`

Live Admin GraphQL 2026-04 schema introspection confirmed the top-level read roots, create/update roots, and `carrierServiceDelete`; `availableCarrierServices` also exists but remains registry-only until its location/availability shape is modeled. The local state stores carrier services as `DeliveryCarrierService` records with `name`, `formattedName`, `callbackUrl`, `active`, `supportsServiceDiscovery`, and internal created/updated timestamps for local sorting.

Snapshot reads return Shopify-like no-data structures: `carrierService(id:)` returns `null` for a missing service, and `carrierServices(...)` returns an empty connection with empty `nodes`/`edges`, false page booleans, and null cursors. Catalog support covers the captured slice for `query: "active:true|false"` and `query: "id:<numeric id or gid>"`, `sortKey: ID|CREATED_AT|UPDATED_AT`, `reverse`, and standard cursor pagination through the shared connection helpers.

Create/update support covers `input.name`, `input.callbackUrl`, `input.active`, and `input.supportsServiceDiscovery`. Captured behavior showed Shopify returning `formattedName` as `<name> (Rates provided by app)` for an app carrier service, update-time downstream visibility through both detail and catalog roots, blank-name create as `userErrors[{ field: null, message: "Shipping rate provider name can't be blank" }]`, unknown update as `field: null`, and unknown delete as `field: ["id"]` with `The carrier or app could not be found.`.

Delete support is enabled because the 2026-04 schema exposes `carrierServiceDelete(id:)` and the live lifecycle capture verified `deletedId` plus downstream detail/catalog absence after cleanup. Local delete only removes the staged/local record; it does not call Shopify or any external callback.

Carrier-service callback URLs and service-discovery flags are recorded only as Shopify-like metadata for read-after-write behavior. Local staging never invokes rate callbacks, service-discovery callbacks, or any checkout-rate side effects.

Delivery-profile reads are implemented as fixture-backed snapshot reads:

- `deliveryProfiles` returns a local connection from normalized `deliveryProfiles` snapshot state and supports `first`, `last`, `after`, `before`, `reverse`, and `merchantOwnedOnly` without contacting upstream Shopify.
- `deliveryProfile(id:)` returns the normalized profile detail when present and `null` for a missing id.
- Snapshot mode does not invent shipping profiles. With no normalized delivery-profile fixtures, `deliveryProfiles` returns an empty connection and `deliveryProfile(id:)` returns `null`.
- Nested profile detail serializes captured scalar counts, profile items, product/variant associations, profile location groups, locations, countries/provinces, zones, method definitions, rate providers, method conditions, selling-plan group connections, and unassigned locations when those fields exist in the normalized fixture.
- Product, variant, and location associations are stored as ids and projected from the existing product/location state. A delivery profile fixture should not duplicate full product, variant, or location blobs.
- Live read evidence for 2026-04 is checked in at `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/delivery-profiles-read.json`. The capture used `SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm conformance:capture-delivery-profiles`; no access-scope or manage-delivery-settings blocker was encountered for the current conformance credential.

## Registry-only coverage map

These roots are known Admin GraphQL shipping/fulfillment surface area, but they are not locally implemented. They are registered with `implemented: false` as explicit future local-model commitments, not as supported passthrough behavior.

Fulfillment-order mutations:

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

Carrier services:

- `availableCarrierServices`

Delivery profiles:

- `locationsAvailableForDeliveryProfilesConnection`
- `deliveryProfileCreate`
- `deliveryProfileRemove`
- `deliveryProfileUpdate`

Shipping-line order-edit roots:

- `orderEditAddShippingLine`
- `orderEditRemoveShippingLine`
- `orderEditUpdateShippingLine`

## Behavior boundaries

- The proxy must not treat any registry-only root above as supported runtime behavior. Until a root has local state modeling and executable tests, unsupported mutations remain on the generic unsupported path and must stay visible in observability.
- Top-level `fulfillment(id:)` now has missing-id, tracking-info, line-item, detail, and event-history shape evidence for records already present on the local order graph. Broader access-scope behavior still needs separate captures before expansion.
- Fulfillment orders are created by Shopify after order routing, not by a direct create mutation. Local support needs to model fulfillment-order generation from order/draft-order creation, location assignment, line-item grouping, status/requestStatus, holds, merchant requests, and delivery methods.
- Fulfillment-order visibility is scope-sensitive. `assignedFulfillmentOrders`, `fulfillmentOrders`, and `Order.fulfillmentOrders` can return different subsets depending on assigned, merchant-managed, third-party, and marketplace fulfillment-order scopes.
- Fulfillment-order lifecycle mutations can create replacement orders, split or merge line items, change assigned locations, add/release holds, change deadlines, and update request status. Do not model one of these as a simple status patch without captured downstream reads.
- Fulfillment-service mutations couple service records to locations. Creation automatically creates a location, update does not replace `LocationEdit` for service-managed location details, and deletion has inventory/location disposition semantics. HAR-236 covers the first local service/location lifecycle slice; broader inventory transfer fidelity still needs dedicated inventory-level captures.
- Broader carrier-service support still depends on app ownership, `write_shipping` access, plan eligibility, available-service/location pairing, and service-discovery callback semantics outside the locally staged catalog/lifecycle slice.
- Delivery-profile write support still needs local modeling for nested profile/location-group/zone/rate validation, variant reassignment, selling-plan associations, default profile behavior, and asynchronous removal job semantics before any delivery-profile mutation can be marked supported.
- Shipping lines and delivery methods are nested under orders, draft orders, calculated orders, fulfillment orders, and delivery profiles. A root-level registry entry can only cover the mutation/query root; nested field fidelity still needs scenario-specific fixtures and downstream read assertions.

## Validation anchors

- Implemented order-scoped fulfillments: `tests/integration/order-fulfillment-flow.test.ts`
- Implemented top-level fulfillment reads: `tests/integration/order-query-shapes.test.ts`
- Implemented fulfillment services: `tests/integration/fulfillment-service-flow.test.ts`
- Implemented carrier services: `tests/integration/carrier-service-flow.test.ts`
- Implemented delivery-profile reads: `tests/integration/delivery-profile-query-shapes.test.ts`
- Existing fulfillment parity specs and requests: `config/parity-specs/fulfillment*.json` and matching files under `config/parity-requests/`
- Carrier-service capture/parity metadata: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/carrier-service-lifecycle.json` and `config/parity-specs/carrier-service-lifecycle.json`
- Delivery-profile read capture: `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/delivery-profiles-read.json`
- Existing order docs for fulfilled order read-after-write behavior: `docs/endpoints/orders.md`
- Registry/coverage tests: `tests/unit/operation-registry.test.ts`, `tests/integration/proxy-capability-classification.test.ts`
