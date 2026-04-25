import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import type { OrderRecord } from '../../src/state/types.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const liveHybridConfig: AppConfig = {
  ...snapshotConfig,
  readMode: 'live-hybrid',
};

function makeOrder(id: string, name: string, createdAt: string, overrides: Partial<OrderRecord> = {}): OrderRecord {
  return {
    id,
    name,
    createdAt,
    updatedAt: createdAt,
    displayFinancialStatus: 'PENDING',
    displayFulfillmentStatus: 'UNFULFILLED',
    note: null,
    tags: [],
    customAttributes: [],
    billingAddress: null,
    shippingAddress: null,
    subtotalPriceSet: {
      shopMoney: {
        amount: '10.0',
        currencyCode: 'CAD',
      },
    },
    currentTotalPriceSet: {
      shopMoney: {
        amount: '10.0',
        currencyCode: 'CAD',
      },
    },
    totalPriceSet: {
      shopMoney: {
        amount: '10.0',
        currencyCode: 'CAD',
      },
    },
    totalRefundedSet: {
      shopMoney: {
        amount: '0.0',
        currencyCode: 'CAD',
      },
    },
    customer: null,
    shippingLines: [],
    lineItems: [],
    transactions: [],
    refunds: [],
    returns: [],
    ...overrides,
  };
}

describe('order query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns the captured empty-state baseline for order roots in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { order: { id: 'gid://shopify/Order/1' } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query OrderEmptyState($missingOrderId: ID!, $first: Int!) {
          lookup: order(id: $missingOrderId) {
            id
            name
          }
          catalog: orders(first: $first, sortKey: CREATED_AT, reverse: true) {
            edges {
              cursor
              node {
                id
                name
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          totals: ordersCount {
            count
            precision
          }
        }`,
        variables: {
          missingOrderId: 'gid://shopify/Order/0',
          first: 1,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        lookup: null,
        catalog: {
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        totals: {
          count: 0,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('passes order roots through upstream in live-hybrid mode while order overlay storage is still unimplemented', async () => {
    const upstreamPayload = {
      data: {
        order: {
          id: 'gid://shopify/Order/101',
          name: '#101',
        },
        orders: {
          edges: [
            {
              cursor: 'opaque-cursor',
              node: {
                id: 'gid://shopify/Order/101',
                name: '#101',
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'opaque-cursor',
            endCursor: 'opaque-cursor',
          },
        },
        ordersCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
    };

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(
      async () =>
        new Response(JSON.stringify(upstreamPayload), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
    );

    const app = createApp(liveHybridConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `query OrderLiveHybrid($id: ID!, $first: Int!) {
          order(id: $id) { id name }
          orders(first: $first, sortKey: CREATED_AT, reverse: true) {
            edges { cursor node { id name } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          ordersCount { count precision }
        }`,
        variables: {
          id: 'gid://shopify/Order/101',
          first: 1,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual(upstreamPayload);
    expect(fetchSpy).toHaveBeenCalledOnce();
  });

  it('filters, sorts, paginates, and count-limits local order catalog reads in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('snapshot order catalog reads must not hit upstream');
    });

    store.stageCreateOrder(
      makeOrder('gid://shopify/Order/1001', '#1001', '2026-04-20T00:00:00.000Z', {
        displayFinancialStatus: 'PAID',
        tags: ['vip'],
        email: 'vip-one@example.com',
      }),
    );
    store.stageCreateOrder(
      makeOrder('gid://shopify/Order/1002', '#1002', '2026-04-22T00:00:00.000Z', {
        displayFinancialStatus: 'PENDING',
        displayFulfillmentStatus: 'FULFILLED',
        tags: ['wholesale'],
        email: 'wholesale@example.com',
      }),
    );
    store.stageCreateOrder(
      makeOrder('gid://shopify/Order/1003', '#1003', '2026-04-21T00:00:00.000Z', {
        displayFinancialStatus: 'PAID',
        tags: ['vip', 'priority'],
        email: 'vip-two@example.com',
      }),
    );

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query OrderCatalogSemantics($tagQuery: String!, $statusQuery: String!, $after: String, $limit: Int, $unlimited: Int) {
          newestVip: orders(first: 1, query: $tagQuery, sortKey: CREATED_AT, reverse: true) {
            nodes { id name tags email }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          nextVip: orders(first: 1, after: $after, query: $tagQuery, sortKey: CREATED_AT, reverse: true) {
            nodes { id name tags email }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          oldestVip: orders(first: 1, query: $tagQuery, sortKey: CREATED_AT, reverse: false) {
            nodes { id name }
          }
          byName: orders(first: 3, query: "name:1003", sortKey: CREATED_AT, reverse: true) {
            nodes { id name }
          }
          byStatus: orders(first: 3, query: $statusQuery, sortKey: CREATED_AT, reverse: true) {
            nodes { id name displayFinancialStatus displayFulfillmentStatus }
          }
          savedSearch: orders(first: 3, savedSearchId: "gid://shopify/SavedSearch/1") {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          limitedVipCount: ordersCount(query: $tagQuery, limit: $limit) { count precision }
          exactVipCount: ordersCount(query: $tagQuery, limit: $unlimited) { count precision }
          savedSearchCount: ordersCount(savedSearchId: "gid://shopify/SavedSearch/1") { count precision }
        }`,
        variables: {
          tagQuery: 'tag:vip',
          statusQuery: 'financial_status:paid fulfillment_status:unfulfilled',
          after: 'cursor:gid://shopify/Order/1003',
          limit: 1,
          unlimited: null,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        newestVip: {
          nodes: [
            {
              id: 'gid://shopify/Order/1003',
              name: '#1003',
              tags: ['vip', 'priority'],
              email: 'vip-two@example.com',
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: 'cursor:gid://shopify/Order/1003',
            endCursor: 'cursor:gid://shopify/Order/1003',
          },
        },
        nextVip: {
          nodes: [
            {
              id: 'gid://shopify/Order/1001',
              name: '#1001',
              tags: ['vip'],
              email: 'vip-one@example.com',
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: true,
            startCursor: 'cursor:gid://shopify/Order/1001',
            endCursor: 'cursor:gid://shopify/Order/1001',
          },
        },
        oldestVip: {
          nodes: [
            {
              id: 'gid://shopify/Order/1001',
              name: '#1001',
            },
          ],
        },
        byName: {
          nodes: [
            {
              id: 'gid://shopify/Order/1003',
              name: '#1003',
            },
          ],
        },
        byStatus: {
          nodes: [
            {
              id: 'gid://shopify/Order/1003',
              name: '#1003',
              displayFinancialStatus: 'PAID',
              displayFulfillmentStatus: 'UNFULFILLED',
            },
            {
              id: 'gid://shopify/Order/1001',
              name: '#1001',
              displayFinancialStatus: 'PAID',
              displayFulfillmentStatus: 'UNFULFILLED',
            },
          ],
        },
        savedSearch: {
          nodes: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        limitedVipCount: {
          count: 1,
          precision: 'AT_LEAST',
        },
        exactVipCount: {
          count: 2,
          precision: 'EXACT',
        },
        savedSearchCount: {
          count: 0,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns Shopify-like empty detail shapes for absent merchant order nested data', async () => {
    const order = makeOrder('gid://shopify/Order/112', '#112', '2026-04-24T00:00:00.000Z');
    store.upsertBaseOrders([order]);

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query OrderMerchantEmptyDetail($id: ID!) {
          order(id: $id) {
            id
            currentSubtotalPriceSet { shopMoney { amount currencyCode } }
            totalReceivedSet { shopMoney { amount currencyCode } }
            netPaymentSet { shopMoney { amount currencyCode } }
            totalRefundedShippingSet { shopMoney { amount currencyCode } }
            totalShippingPriceSet { shopMoney { amount currencyCode } }
            shippingLines(first: 2) {
              nodes { title }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            lineItems(first: 2) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            fulfillmentOrders(first: 2) {
              nodes { id status }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            fulfillments(first: 2) {
              id
              status
            }
            returns(first: 2) {
              nodes { id status }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }`,
        variables: {
          id: order.id,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.order).toEqual({
      id: order.id,
      currentSubtotalPriceSet: {
        shopMoney: {
          amount: '10.0',
          currencyCode: 'CAD',
        },
      },
      totalReceivedSet: {
        shopMoney: {
          amount: '0.0',
          currencyCode: 'CAD',
        },
      },
      netPaymentSet: {
        shopMoney: {
          amount: '0.0',
          currencyCode: 'CAD',
        },
      },
      totalRefundedShippingSet: {
        shopMoney: {
          amount: '0.0',
          currencyCode: 'CAD',
        },
      },
      totalShippingPriceSet: {
        shopMoney: {
          amount: '0.0',
          currencyCode: 'CAD',
        },
      },
      shippingLines: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
      lineItems: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
      fulfillmentOrders: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
      fulfillments: [],
      returns: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
    });
  });

  it('serves empty top-level fulfillment roots in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('top-level fulfillment empty reads must not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query EmptyTopLevelFulfillmentReads(
          $fulfillmentId: ID!
          $fulfillmentOrderId: ID!
          $first: Int!
        ) {
          missingFulfillment: fulfillment(id: $fulfillmentId) {
            id
            status
          }
          missingFulfillmentOrder: fulfillmentOrder(id: $fulfillmentOrderId) {
            id
            status
          }
          fulfillmentOrders(first: $first, includeClosed: true) {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          assignedFulfillmentOrders(first: $first, assignmentStatus: FULFILLMENT_REQUESTED) {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          manualHoldsFulfillmentOrders(first: $first) {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: {
          fulfillmentId: 'gid://shopify/Fulfillment/999999999999',
          fulfillmentOrderId: 'gid://shopify/FulfillmentOrder/999999999999',
          first: 2,
        },
      });

    const emptyConnection = {
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    };

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        missingFulfillment: null,
        missingFulfillmentOrder: null,
        fulfillmentOrders: emptyConnection,
        assignedFulfillmentOrders: emptyConnection,
        manualHoldsFulfillmentOrders: emptyConnection,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('keeps top-level fulfillment reads consistent with nested order fulfillment data', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('top-level fulfillment detail reads must not hit upstream in snapshot mode');
    });

    const openFulfillmentOrder = {
      id: 'gid://shopify/FulfillmentOrder/100',
      status: 'OPEN',
      requestStatus: 'UNSUBMITTED',
      assignedLocation: { name: 'Shop location' },
      deliveryMethod: {
        id: 'gid://shopify/DeliveryMethod/100',
        methodType: 'SHIPPING',
        presentedName: 'Standard',
        serviceCode: 'STANDARD',
        minDeliveryDateTime: null,
        maxDeliveryDateTime: null,
        sourceReference: null,
      },
      lineItems: [
        {
          id: 'gid://shopify/FulfillmentOrderLineItem/101',
          lineItemId: 'gid://shopify/LineItem/fulfillment-top-level',
          title: 'Fulfillment top-level item',
          totalQuantity: 1,
          remainingQuantity: 1,
        },
      ],
    };
    const closedFulfillmentOrder = {
      id: 'gid://shopify/FulfillmentOrder/200',
      status: 'CLOSED',
      requestStatus: 'UNSUBMITTED',
      assignedLocation: { name: 'Shop location' },
      lineItems: [
        {
          id: 'gid://shopify/FulfillmentOrderLineItem/201',
          lineItemId: 'gid://shopify/LineItem/fulfillment-top-level',
          title: 'Fulfillment top-level item',
          totalQuantity: 1,
          remainingQuantity: 0,
        },
      ],
    };
    const fulfillment = {
      id: 'gid://shopify/Fulfillment/300',
      status: 'SUCCESS',
      displayStatus: 'FULFILLED',
      createdAt: '2026-04-25T21:07:57Z',
      updatedAt: '2026-04-25T21:07:57Z',
      trackingInfo: [
        {
          number: 'HAR232-TRACK',
          url: 'https://example.com/track/HAR232-TRACK',
          company: 'Hermes',
        },
      ],
      fulfillmentLineItems: [
        {
          id: 'gid://shopify/FulfillmentLineItem/301',
          lineItemId: 'gid://shopify/LineItem/fulfillment-top-level',
          title: 'Fulfillment top-level item',
          quantity: 1,
        },
      ],
    };
    const order = makeOrder('gid://shopify/Order/fulfillment-top-level', '#FULFILL-TOP', '2026-04-25T00:00:00.000Z', {
      displayFinancialStatus: 'PAID',
      displayFulfillmentStatus: 'FULFILLED',
      lineItems: [
        {
          id: 'gid://shopify/LineItem/fulfillment-top-level',
          title: 'Fulfillment top-level item',
          quantity: 1,
          sku: null,
          variantTitle: null,
          originalUnitPriceSet: null,
        },
      ],
      fulfillments: [fulfillment],
      fulfillmentOrders: [openFulfillmentOrder, closedFulfillmentOrder],
    });
    store.upsertBaseOrders([order]);

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query TopLevelFulfillmentReads(
          $orderId: ID!
          $fulfillmentId: ID!
          $fulfillmentOrderId: ID!
          $afterOpen: String!
        ) {
          fulfillment(id: $fulfillmentId) {
            id
            status
            displayStatus
            createdAt
            updatedAt
            trackingInfo { number url company }
            fulfillmentLineItems(first: 5) {
              nodes { id quantity lineItem { id title } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          fulfillmentOrder(id: $fulfillmentOrderId) {
            id
            status
            requestStatus
            assignedLocation { name }
            deliveryMethod {
              id
              methodType
              presentedName
              serviceCode
              minDeliveryDateTime
              maxDeliveryDateTime
              sourceReference
            }
            lineItems(first: 5) {
              nodes { id totalQuantity remainingQuantity lineItem { id title } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          order(id: $orderId) {
            id
            fulfillments(first: 5) {
              id
              status
              displayStatus
              createdAt
              updatedAt
              trackingInfo { number url company }
              fulfillmentLineItems(first: 5) {
                nodes { id quantity lineItem { id title } }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            fulfillmentOrders(first: 5) {
              nodes {
                id
                status
                requestStatus
                assignedLocation { name }
                deliveryMethod {
                  id
                  methodType
                  presentedName
                  serviceCode
                  minDeliveryDateTime
                  maxDeliveryDateTime
                  sourceReference
                }
                lineItems(first: 5) {
                  nodes { id totalQuantity remainingQuantity lineItem { id title } }
                  pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                }
              }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          openOnly: fulfillmentOrders(first: 5, sortKey: ID) {
            nodes { id status deliveryMethod { methodType presentedName } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          allFirst: fulfillmentOrders(first: 1, includeClosed: true, sortKey: ID) {
            nodes { id status }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          allNext: fulfillmentOrders(first: 1, after: $afterOpen, includeClosed: true, sortKey: ID) {
            nodes { id status }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          closedByQuery: fulfillmentOrders(first: 5, includeClosed: true, sortKey: ID, query: "status:closed") {
            nodes { id status }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          reversed: fulfillmentOrders(first: 1, includeClosed: true, sortKey: ID, reverse: true) {
            nodes { id status }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          manualHoldsFulfillmentOrders(first: 5) {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: {
          orderId: order.id,
          fulfillmentId: fulfillment.id,
          fulfillmentOrderId: closedFulfillmentOrder.id,
          afterOpen: 'cursor:gid://shopify/FulfillmentOrder/100',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.fulfillment).toEqual(response.body.data.order.fulfillments[0]);
    expect(response.body.data.fulfillmentOrder).toEqual(response.body.data.order.fulfillmentOrders.nodes[1]);
    expect(response.body.data.order.fulfillmentOrders.nodes[0].deliveryMethod).toEqual({
      id: 'gid://shopify/DeliveryMethod/100',
      methodType: 'SHIPPING',
      presentedName: 'Standard',
      serviceCode: 'STANDARD',
      minDeliveryDateTime: null,
      maxDeliveryDateTime: null,
      sourceReference: null,
    });
    expect(response.body.data.fulfillmentOrder.deliveryMethod).toBeNull();
    expect(response.body.data.openOnly.nodes).toEqual([
      {
        id: openFulfillmentOrder.id,
        status: 'OPEN',
        deliveryMethod: {
          methodType: 'SHIPPING',
          presentedName: 'Standard',
        },
      },
    ]);
    expect(response.body.data.allFirst).toEqual({
      nodes: [{ id: openFulfillmentOrder.id, status: 'OPEN' }],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/FulfillmentOrder/100',
        endCursor: 'cursor:gid://shopify/FulfillmentOrder/100',
      },
    });
    expect(response.body.data.allNext).toEqual({
      nodes: [{ id: closedFulfillmentOrder.id, status: 'CLOSED' }],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: true,
        startCursor: 'cursor:gid://shopify/FulfillmentOrder/200',
        endCursor: 'cursor:gid://shopify/FulfillmentOrder/200',
      },
    });
    expect(response.body.data.closedByQuery.nodes).toEqual([{ id: closedFulfillmentOrder.id, status: 'CLOSED' }]);
    expect(response.body.data.reversed.nodes).toEqual([{ id: closedFulfillmentOrder.id, status: 'CLOSED' }]);
    expect(response.body.data.manualHoldsFulfillmentOrders).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
