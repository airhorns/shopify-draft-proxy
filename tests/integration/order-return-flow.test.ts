import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import type { OrderRecord } from '../../src/state/types.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeFulfilledOrder(): OrderRecord {
  return {
    id: 'gid://shopify/Order/return-flow',
    name: '#RETURN-FLOW',
    createdAt: '2026-04-25T00:00:00.000Z',
    updatedAt: '2026-04-25T00:00:00.000Z',
    displayFinancialStatus: 'PAID',
    displayFulfillmentStatus: 'FULFILLED',
    note: null,
    tags: [],
    customAttributes: [],
    billingAddress: null,
    shippingAddress: null,
    subtotalPriceSet: { shopMoney: { amount: '20.0', currencyCode: 'CAD' } },
    currentTotalPriceSet: { shopMoney: { amount: '20.0', currencyCode: 'CAD' } },
    totalPriceSet: { shopMoney: { amount: '20.0', currencyCode: 'CAD' } },
    totalRefundedSet: { shopMoney: { amount: '0.0', currencyCode: 'CAD' } },
    customer: null,
    shippingLines: [],
    lineItems: [
      {
        id: 'gid://shopify/LineItem/return-flow',
        title: 'Return flow item',
        quantity: 2,
        sku: null,
        variantTitle: null,
        originalUnitPriceSet: null,
      },
    ],
    fulfillments: [
      {
        id: 'gid://shopify/Fulfillment/return-flow',
        status: 'SUCCESS',
        displayStatus: 'FULFILLED',
        createdAt: '2026-04-25T01:00:00.000Z',
        updatedAt: '2026-04-25T01:00:00.000Z',
        deliveredAt: null,
        estimatedDeliveryAt: null,
        inTransitAt: null,
        trackingInfo: [],
        events: [],
        fulfillmentLineItems: [
          {
            id: 'gid://shopify/FulfillmentLineItem/return-flow',
            lineItemId: 'gid://shopify/LineItem/return-flow',
            title: 'Return flow item',
            quantity: 2,
          },
        ],
        service: null,
        location: null,
        originAddress: null,
      },
    ],
    fulfillmentOrders: [],
    transactions: [],
    refunds: [],
    returns: [],
  };
}

describe('order return flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages return creation and lifecycle changes with downstream order and return reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('supported return mutations must not hit upstream in snapshot mode');
    });
    store.upsertBaseOrders([makeFulfilledOrder()]);
    const app = createApp(snapshotConfig).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReturnCreate($returnInput: ReturnInput!) {
          returnCreate(returnInput: $returnInput) {
            return {
              id
              name
              status
              totalQuantity
              order { id }
              returnLineItems(first: 5) {
                nodes {
                  id
                  quantity
                  processedQuantity
                  returnReason
                  returnReasonNote
                  fulfillmentLineItem { id lineItem { id title } }
                }
              }
              reverseFulfillmentOrders(first: 5) { nodes { id } }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          returnInput: {
            orderId: 'gid://shopify/Order/return-flow',
            returnLineItems: [
              {
                fulfillmentLineItemId: 'gid://shopify/FulfillmentLineItem/return-flow',
                quantity: 1,
                returnReason: 'UNWANTED',
                returnReasonNote: 'Changed mind',
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    const returnId = createResponse.body.data.returnCreate.return.id;
    expect(createResponse.body.data.returnCreate).toMatchObject({
      return: {
        name: '#RETURN-FLOW-R1',
        status: 'OPEN',
        totalQuantity: 1,
        order: { id: 'gid://shopify/Order/return-flow' },
        returnLineItems: {
          nodes: [
            {
              quantity: 1,
              processedQuantity: 0,
              returnReason: 'UNWANTED',
              returnReasonNote: 'Changed mind',
              fulfillmentLineItem: {
                id: 'gid://shopify/FulfillmentLineItem/return-flow',
                lineItem: {
                  id: 'gid://shopify/LineItem/return-flow',
                  title: 'Return flow item',
                },
              },
            },
          ],
        },
        reverseFulfillmentOrders: { nodes: [] },
      },
      userErrors: [],
    });

    const closeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReturnClose($id: ID!) {
          returnClose(id: $id) { return { id status closedAt } userErrors { field message } }
        }`,
        variables: { id: returnId },
      });
    expect(closeResponse.body.data.returnClose.return).toMatchObject({ id: returnId, status: 'CLOSED' });
    expect(closeResponse.body.data.returnClose.return.closedAt).toEqual(expect.any(String));

    const reopenResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReturnReopen($id: ID!) {
          returnReopen(id: $id) { return { id status closedAt } userErrors { field message } }
        }`,
        variables: { id: returnId },
      });
    expect(reopenResponse.body.data.returnReopen.return).toEqual({ id: returnId, status: 'OPEN', closedAt: null });

    const cancelResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReturnCancel($id: ID!) {
          returnCancel(id: $id) { return { id status } userErrors { field message } }
        }`,
        variables: { id: returnId },
      });
    expect(cancelResponse.body.data.returnCancel.return).toEqual({ id: returnId, status: 'CANCELED' });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReturnRead($id: ID!, $orderId: ID!) {
          return(id: $id) { id status totalQuantity order { id } }
          order(id: $orderId) { id returns(first: 5) { nodes { id status totalQuantity } } }
        }`,
        variables: { id: returnId, orderId: 'gid://shopify/Order/return-flow' },
      });

    expect(readResponse.body).toEqual({
      data: {
        return: {
          id: returnId,
          status: 'CANCELED',
          totalQuantity: 1,
          order: { id: 'gid://shopify/Order/return-flow' },
        },
        order: {
          id: 'gid://shopify/Order/return-flow',
          returns: {
            nodes: [{ id: returnId, status: 'CANCELED', totalQuantity: 1 }],
          },
        },
      },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries).toHaveLength(4);
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'ReturnCreate',
      'ReturnClose',
      'ReturnReopen',
      'ReturnCancel',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages return requests and returns local validation errors without upstream writes', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('return validation must not hit upstream in snapshot mode');
    });
    store.upsertBaseOrders([makeFulfilledOrder()]);
    const app = createApp(snapshotConfig).callback();

    const requestResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReturnRequest($input: ReturnRequestInput!) {
          returnRequest(input: $input) { return { id status } userErrors { field message } }
        }`,
        variables: {
          input: {
            orderId: 'gid://shopify/Order/return-flow',
            returnLineItems: [
              {
                fulfillmentLineItemId: 'gid://shopify/FulfillmentLineItem/return-flow',
                quantity: 1,
                returnReason: 'OTHER',
              },
            ],
          },
        },
      });

    expect(requestResponse.body.data.returnRequest).toMatchObject({
      return: { status: 'REQUESTED' },
      userErrors: [],
    });

    const validationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReturnCreate($returnInput: ReturnInput!) {
          returnCreate(returnInput: $returnInput) { return { id } userErrors { field message } }
        }`,
        variables: {
          returnInput: {
            orderId: 'gid://shopify/Order/return-flow',
            returnLineItems: [
              {
                fulfillmentLineItemId: 'gid://shopify/FulfillmentLineItem/missing',
                quantity: 1,
                returnReason: 'OTHER',
              },
            ],
          },
        },
      });

    expect(validationResponse.body).toEqual({
      data: {
        returnCreate: {
          return: null,
          userErrors: [
            {
              field: ['returnLineItems', '0', 'fulfillmentLineItemId'],
              message: 'Fulfillment line item does not exist.',
            },
          ],
        },
      },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'ReturnRequest',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
