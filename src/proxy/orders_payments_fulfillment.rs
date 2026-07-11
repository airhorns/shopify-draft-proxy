use super::*;

mod draft_orders;
mod fulfillment_orders;
mod orders;
mod payments;

pub(in crate::proxy) use self::draft_orders::*;
pub(in crate::proxy) use self::fulfillment_orders::*;
pub(in crate::proxy) use self::orders::*;
pub(in crate::proxy) use self::payments::*;

struct OrdersLocalLogEntry<'a> {
    request: &'a Request,
    query: &'a str,
    variables: &'a BTreeMap<String, ResolvedValue>,
    root_field: &'a str,
    staged_resource_ids: Vec<String>,
    outcome: OrdersLocalLogOutcome<'a>,
}

const ORDER_LIFECYCLE_HYDRATE_QUERY: &str = "query OrderManagementDownstreamRead($id: ID!) {\n  order(id: $id) {\n    id\n    name\n    closed\n    closedAt\n    cancelledAt\n    cancelReason\n    displayFinancialStatus\n    paymentGatewayNames\n    totalOutstandingSet {\n      shopMoney {\n        amount\n        currencyCode\n      }\n    }\n    currentTotalPriceSet {\n      shopMoney {\n        amount\n        currencyCode\n      }\n    }\n    customer {\n      id\n      email\n      displayName\n    }\n    transactions {\n      kind\n      status\n      gateway\n      amountSet {\n        shopMoney {\n          amount\n          currencyCode\n        }\n      }\n    }\n  }\n}";
const ORDER_INVOICE_SEND_EMAIL_HYDRATE_QUERY: &str = "query OrderInvoiceSendEmailValidationRead($id: ID!) {\n    order(id: $id) {\n      \n  id\n  name\n  email\n  customer {\n    id\n    email\n    displayName\n  }\n\n    }\n  }";

// Canonical customer hydrate issued for order-customer mutations (orderCustomerSet).
// The selection mirrors the order.customer projection these mutations expose, so a
// live backend returns the same shape the proxy then stores and re-projects.
const ORDER_CUSTOMER_SUMMARY_HYDRATE_QUERY: &str =
    "query CustomerHydrate($id: ID!) { customer(id: $id) { id email displayName } }";

const FULFILLMENT_EVENT_CREATED_AT: &str = "2024-01-01T00:00:03.000Z";
const FULFILLMENT_EVENT_STATUS_VALUES: &[&str] = &[
    "LABEL_PURCHASED",
    "LABEL_PRINTED",
    "READY_FOR_PICKUP",
    "CONFIRMED",
    "IN_TRANSIT",
    "OUT_FOR_DELIVERY",
    "ATTEMPTED_DELIVERY",
    "DELAYED",
    "DELIVERED",
    "FAILURE",
    "CARRIER_PICKED_UP",
];

// Draft-order hydration forwarded on a cold miss for draftOrder reads and
// update/delete/duplicate/complete/invoice-send mutations operating on a draft
// not created locally this scenario, then observed into staged state instead of
// a precondition seed. Shares the `.graphql` file with the capture scripts (via
// include_str!) so the recorded cassette byte-matches the proxy's forward under
// the strict cassette matcher. The file preserves the original constant's bytes
// (leading newline + indentation) so previously recorded cassettes still match.
const DRAFT_ORDER_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/orders/draft-order-hydrate.graphql");
// Order hydration for `orderEditBegin` operating on an order that was not
// created locally in this scenario. Forwarded verbatim on a cold miss and
// observed into staged state so the edit session is built from real line items,
// currency, and editability flags instead of a precondition seed. Shares the
// `.graphql` file with the capture scripts (via include_str!) so the recorded
// cassette byte-matches the proxy's forward under the strict cassette matcher.
const ORDER_EDIT_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/orders/order-edit-hydrate.graphql");
// Order hydration for `returnCreate` / `returnRequest` operating on an order that
// was not created locally in this scenario. Forwarded verbatim on a cold miss and
// observed into staged state so the return engine validates requested lines
// against the order's real fulfillment line items and any outstanding returns,
// instead of a precondition seed. Shares the `.graphql` file with the capture
// scripts (via include_str!) so the recorded cassette byte-matches the proxy's
// forward under the strict cassette matcher.
const RETURN_ORDER_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/orders/return-order-hydrate.graphql");
const ORDER_HYDRATE_QUERY: &str = r#"
    query OrdersOrderHydrate($id: ID!) {
      order(id: $id) {
        id
        name
        email
        note
        tags
        customAttributes { key value }
        customer { id email displayName }
        billingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        shippingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        currencyCode
        presentmentCurrencyCode
        displayFinancialStatus
        displayFulfillmentStatus
        currentTotalPriceSet { shopMoney { amount currencyCode } }
        totalPriceSet { shopMoney { amount currencyCode } }
        totalTaxSet { shopMoney { amount currencyCode } }
        totalDiscountsSet { shopMoney { amount currencyCode } }
        discountCodes
        lineItems(first: 10) {
          nodes {
            id
            title
            name
            quantity
            currentQuantity
            sku
            variantTitle
            requiresShipping
            taxable
            customAttributes { key value }
            originalUnitPriceSet { shopMoney { amount currencyCode } }
            originalTotalSet { shopMoney { amount currencyCode } }
            variant { id title sku }
            taxLines { title rate priceSet { shopMoney { amount currencyCode } } }
          }
        }
      }
    }
"#;
// These hydrate queries are forwarded verbatim to the backend; their exact text
// must match the recorded `OrdersDraftOrder*Hydrate` cassette calls (compact
// two-space layout, customer carries firstName/lastName) so the strict cassette
// matcher replays the recorded customer/variant responses instead of returning a
// mismatch.
const DRAFT_ORDER_CUSTOMER_HYDRATE_QUERY: &str =
    "query OrdersDraftOrderCustomerHydrate($id: ID!) {\n  customer(id: $id) { id email displayName firstName lastName }\n}\n";
const DRAFT_ORDER_VARIANT_HYDRATE_QUERY: &str =
    "query OrdersDraftOrderVariantHydrate($id: ID!) {\n  productVariant(id: $id) { id title sku taxable price inventoryItem { requiresShipping } product { title } }\n}\n";
const DRAFT_ORDER_VARIANTS_HYDRATE_QUERY: &str =
    "query OrdersDraftOrderVariantsHydrate($ids: [ID!]!) {\n  nodes(ids: $ids) { __typename ... on ProductVariant { id title sku taxable price inventoryItem { requiresShipping } product { title } } }\n}\n";
const ORDERS_FULFILLMENT_ORDER_HYDRATE_QUERY: &str = "query ShippingFulfillmentOrderHydrate($id: ID!) {\n    fulfillmentOrder(id: $id) {\n      id\n      status\n      requestStatus\n      fulfillAt\n      fulfillBy\n      updatedAt\n      supportedActions {\n        action\n      }\n      assignedLocation {\n        name\n        location {\n          id\n          name\n        }\n      }\n      fulfillmentHolds {\n        id\n        handle\n        reason\n        reasonNotes\n        displayReason\n        heldByApp {\n          id\n          title\n        }\n        heldByRequestingApp\n      }\n      merchantRequests(first: 10) {\n        nodes {\n          kind\n          message\n          requestOptions\n        }\n      }\n      lineItems(first: 20) {\n        nodes {\n          id\n          totalQuantity\n          remainingQuantity\n          lineItem {\n            id\n            title\n            quantity\n            fulfillableQuantity\n          }\n        }\n      }\n      order {\n        id\n        name\n        displayFulfillmentStatus\n      }\n    }\n  }";
// Order hydration for `orderMarkAsPaid` operating on an order that was not
// created locally in this scenario. The proxy forwards this exact query (it is
// byte-identical to the `OrdersOrderHydrate` recording so the strict cassette
// matcher accepts it) to fetch the order's money-bag/transaction state from the
// backend, observes it into staged state, then applies the mutation locally.
const ORDER_MARK_AS_PAID_HYDRATE_QUERY: &str =
    "#graphql\n  fragment OrderMarkAsPaidMoneyBagFields on Order {\n    id\n    name\n    createdAt\n    updatedAt\n    closed\n    closedAt\n    cancelledAt\n    cancelReason\n    presentmentCurrencyCode\n    displayFinancialStatus\n    displayFulfillmentStatus\n    paymentGatewayNames\n    totalOutstandingSet {\n      shopMoney { amount currencyCode }\n      presentmentMoney { amount currencyCode }\n    }\n    currentTotalPriceSet {\n      shopMoney { amount currencyCode }\n      presentmentMoney { amount currencyCode }\n    }\n    totalPriceSet {\n      shopMoney { amount currencyCode }\n      presentmentMoney { amount currencyCode }\n    }\n    transactions {\n      id\n      kind\n      status\n      gateway\n      amountSet {\n        shopMoney { amount currencyCode }\n        presentmentMoney { amount currencyCode }\n      }\n    }\n  }\n\n  query OrdersOrderHydrate($id: ID!) {\n    order(id: $id) {\n      ...OrderMarkAsPaidMoneyBagFields\n    }\n  }";
const ORDERS_FULFILLMENT_HYDRATE_QUERY: &str = r#"#graphql
  query ShippingFulfillmentEventCreateFulfillmentHydrate($id: ID!) {
    fulfillment(id: $id) {
      id
      status
      displayStatus
      createdAt
      updatedAt
      deliveredAt
      estimatedDeliveryAt
      inTransitAt
      trackingInfo(first: 1) { number url company }
      events(first: 5) {
        nodes {
          id
          status
          message
          happenedAt
          createdAt
          estimatedDeliveryAt
          city
          province
          country
          zip
          address1
          latitude
          longitude
        }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
      service {
        id
        handle
        serviceName
        trackingSupport
        type
        location { id name }
      }
      location { id name }
      originAddress { address1 address2 city countryCode provinceCode zip }
      fulfillmentLineItems(first: 5) {
        nodes { id quantity lineItem { id title } }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
      order { id name displayFulfillmentStatus }
    }
  }
"#;
// Fulfillment-lifecycle hydration for `fulfillmentCancel` / `fulfillmentTrackingInfoUpdate`
// operating on a fulfillment that was not created locally in this scenario. Byte-identical
// to the recorded `OrdersFulfillmentHydrate` query so the strict cassette matcher accepts
// it; resolves the fulfillment's owning order plus the sibling fulfillment states (status /
// displayStatus / trackingInfo) the proxy needs to evaluate the state-machine preconditions
// (already-cancelled, already-delivered) locally.
const ORDERS_FULFILLMENT_LIFECYCLE_HYDRATE_QUERY: &str = "query OrdersFulfillmentHydrate($id: ID!) { fulfillment(id: $id) { id order { id name email phone createdAt updatedAt closed closedAt cancelledAt cancelReason displayFinancialStatus displayFulfillmentStatus note tags fulfillments { id status displayStatus createdAt updatedAt trackingInfo { number url company } } } } }";
// Best-effort second-stage enrichment for the lifecycle hydrate. Byte-identical to the
// recorded `OrderFulfillmentLifecycleRead` query so the strict cassette matcher accepts it;
// fetches the order's full fulfillment view *including* `fulfillmentLineItems` so a downstream
// order read observes line items the bare `OrdersFulfillmentHydrate` projection omits. When the
// backend has no such recording the cassette miss is non-fatal and the proxy falls back to the
// stage-one order.
const ORDER_FULFILLMENT_LIFECYCLE_READ_QUERY: &str = "query OrderFulfillmentLifecycleRead($id: ID!) {\n  order(id: $id) {\n    id\n    name\n    updatedAt\n    displayFulfillmentStatus\n    fulfillments(first: 5) {\n      id\n      status\n      displayStatus\n      createdAt\n      updatedAt\n      trackingInfo {\n        number\n        url\n        company\n      }\n      fulfillmentLineItems(first: 5) {\n        nodes {\n          id\n          quantity\n          lineItem {\n            id\n            title\n          }\n        }\n      }\n    }\n    fulfillmentOrders(first: 5) {\n      nodes {\n        id\n        status\n        requestStatus\n        lineItems(first: 5) {\n          nodes {\n            id\n            totalQuantity\n            remainingQuantity\n            lineItem {\n              id\n              title\n            }\n          }\n        }\n      }\n    }\n  }\n}";
