# Shipping and Fulfillments Endpoint Group

This endpoint group is a coverage map for Admin GraphQL shipping, fulfillment, fulfillment-order, carrier-service, fulfillment-service, and delivery-profile behavior. Keep implementation details for locally staged order fulfillment slices in `docs/endpoints/orders.md`; use this file for broader boundaries and gaps.

## Implemented roots

Local staged mutations currently live under the orders group because they operate on order-scoped fulfillment records and downstream `order(id:)` reads:

- `fulfillmentCreate`
- `fulfillmentTrackingInfoUpdate`
- `fulfillmentCancel`

Those roots are implemented in `tests/integration/order-fulfillment-flow.test.ts` and covered by `config/parity-specs/fulfillment*.json`. HAR-122/HAR-187 provide the evidence-backed fulfillment lifecycle slices; this document does not duplicate those request/fixture details.

## Registry-only coverage map

These roots are known Admin GraphQL shipping/fulfillment surface area, but they are not locally implemented. They are registered with `implemented: false` as explicit future local-model commitments, not as supported passthrough behavior.

Top-level fulfillment:

- `fulfillment`
- `fulfillmentEventCreate`

Fulfillment-order reads:

- `assignedFulfillmentOrders`
- `fulfillmentOrder`
- `fulfillmentOrders`
- `manualHoldsFulfillmentOrders`

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

Fulfillment services:

- `fulfillmentService`
- `fulfillmentServiceCreate`
- `fulfillmentServiceDelete`
- `fulfillmentServiceUpdate`

Carrier services:

- `availableCarrierServices`
- `carrierService`
- `carrierServices`
- `carrierServiceCreate`
- `carrierServiceDelete`
- `carrierServiceUpdate`

Delivery profiles:

- `deliveryProfile`
- `deliveryProfiles`
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
- Top-level `fulfillment(id:)` is not equivalent to order-scoped `order.fulfillments`; first-class fulfillment lookup needs its own missing-id, access-scope, tracking-info, event, and line-item shape evidence.
- Fulfillment orders are created by Shopify after order routing, not by a direct create mutation. Local support needs to model fulfillment-order generation from order/draft-order creation, location assignment, line-item grouping, status/requestStatus, holds, merchant requests, and delivery methods.
- Fulfillment-order visibility is scope-sensitive. `assignedFulfillmentOrders`, `fulfillmentOrders`, and `Order.fulfillmentOrders` can return different subsets depending on assigned, merchant-managed, third-party, and marketplace fulfillment-order scopes.
- Fulfillment-order lifecycle mutations can create replacement orders, split or merge line items, change assigned locations, add/release holds, change deadlines, and update request status. Do not model one of these as a simple status patch without captured downstream reads.
- Fulfillment-service mutations couple service records to locations. Creation automatically creates a location, update does not replace `LocationEdit` for service-managed location details, and deletion has inventory/location disposition semantics.
- Carrier-service support depends on app ownership, `write_shipping` access, plan eligibility, callback URL behavior, active/service-discovery flags, and active-only catalog behavior.
- Delivery profiles are nested shipping-rate configuration, not just scalar profile records. Local support needs location groups, zones, method definitions, conditions, variant assignments, selling-plan associations, default profile behavior, and asynchronous removal job semantics.
- Shipping lines and delivery methods are nested under orders, draft orders, calculated orders, fulfillment orders, and delivery profiles. A root-level registry entry can only cover the mutation/query root; nested field fidelity still needs scenario-specific fixtures and downstream read assertions.

## Validation anchors

- Implemented order-scoped fulfillments: `tests/integration/order-fulfillment-flow.test.ts`
- Existing fulfillment parity specs and requests: `config/parity-specs/fulfillment*.json` and matching files under `config/parity-requests/`
- Existing order docs for fulfilled order read-after-write behavior: `docs/endpoints/orders.md`
- Registry/coverage tests: `tests/unit/operation-registry.test.ts`, `tests/integration/proxy-capability-classification.test.ts`
