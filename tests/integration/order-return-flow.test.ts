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
              reverseFulfillmentOrders(first: 5) { nodes { id status lineItems(first: 5) { nodes { id totalQuantity remainingQuantity } } } }
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
        reverseFulfillmentOrders: {
          nodes: [
            {
              id: expect.any(String),
              status: 'OPEN',
              lineItems: { nodes: [{ id: expect.any(String), totalQuantity: 1, remainingQuantity: 1 }] },
            },
          ],
        },
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

  it('approves, processes, and stages reverse logistics for requested returns', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('return approval and reverse logistics must not hit upstream in snapshot mode');
    });
    store.upsertBaseOrders([makeFulfilledOrder()]);
    const app = createApp(snapshotConfig).callback();

    const requestResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReturnRequest($input: ReturnRequestInput!) {
          returnRequest(input: $input) {
            return {
              id
              status
              returnLineItems(first: 5) { nodes { id quantity processedQuantity unprocessedQuantity } }
              reverseFulfillmentOrders(first: 5) { nodes { id } }
            }
            userErrors { field message }
          }
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

    const returnId = requestResponse.body.data.returnRequest.return.id;
    const returnLineItemId = requestResponse.body.data.returnRequest.return.returnLineItems.nodes[0].id;
    expect(requestResponse.body.data.returnRequest.return).toMatchObject({
      status: 'REQUESTED',
      reverseFulfillmentOrders: { nodes: [] },
    });

    const approveResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReturnApproveRequest($input: ReturnApproveRequestInput!) {
          returnApproveRequest(input: $input) {
            return {
              id
              status
              order { id }
              reverseFulfillmentOrders(first: 5) {
                nodes {
                  id
                  status
                  lineItems(first: 5) {
                    nodes { id totalQuantity remainingQuantity returnLineItem { id } }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }`,
        variables: { input: { id: returnId } },
      });

    const reverseFulfillmentOrder =
      approveResponse.body.data.returnApproveRequest.return.reverseFulfillmentOrders.nodes[0];
    const reverseFulfillmentOrderLineItem = reverseFulfillmentOrder.lineItems.nodes[0];
    expect(approveResponse.body.data.returnApproveRequest).toMatchObject({
      return: {
        id: returnId,
        status: 'OPEN',
        order: { id: 'gid://shopify/Order/return-flow' },
        reverseFulfillmentOrders: {
          nodes: [
            {
              id: expect.any(String),
              status: 'OPEN',
              lineItems: {
                nodes: [
                  {
                    id: expect.any(String),
                    totalQuantity: 1,
                    remainingQuantity: 1,
                    returnLineItem: { id: returnLineItemId },
                  },
                ],
              },
            },
          ],
        },
      },
      userErrors: [],
    });

    const deliveryCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReverseDeliveryCreate(
          $reverseFulfillmentOrderId: ID!
          $reverseDeliveryLineItems: [ReverseDeliveryLineItemInput!]!
          $trackingInput: ReverseDeliveryTrackingInput
          $labelInput: ReverseDeliveryLabelInput
        ) {
          reverseDeliveryCreateWithShipping(
            reverseFulfillmentOrderId: $reverseFulfillmentOrderId
            reverseDeliveryLineItems: $reverseDeliveryLineItems
            trackingInput: $trackingInput
            labelInput: $labelInput
            notifyCustomer: true
          ) {
            reverseDelivery {
              id
              reverseFulfillmentOrder { id }
              reverseDeliveryLineItems(first: 5) {
                nodes {
                  id
                  quantity
                  reverseFulfillmentOrderLineItem { id remainingQuantity }
                }
              }
              deliverable {
                __typename
                ... on ReverseDeliveryShippingDeliverable {
                  tracking { number url company }
                  label { publicFileUrl }
                }
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          reverseFulfillmentOrderId: reverseFulfillmentOrder.id,
          reverseDeliveryLineItems: [
            { reverseFulfillmentOrderLineItemId: reverseFulfillmentOrderLineItem.id, quantity: 1 },
          ],
          trackingInput: { number: 'TRACK-1', url: 'https://tracking.example/1', company: 'Example Carrier' },
          labelInput: { publicFileUrl: 'https://labels.example/return.pdf' },
        },
      });

    const reverseDelivery = deliveryCreateResponse.body.data.reverseDeliveryCreateWithShipping.reverseDelivery;
    expect(deliveryCreateResponse.body.data.reverseDeliveryCreateWithShipping).toMatchObject({
      reverseDelivery: {
        id: expect.any(String),
        reverseFulfillmentOrder: { id: reverseFulfillmentOrder.id },
        reverseDeliveryLineItems: {
          nodes: [
            {
              quantity: 1,
              reverseFulfillmentOrderLineItem: {
                id: reverseFulfillmentOrderLineItem.id,
                remainingQuantity: 1,
              },
            },
          ],
        },
        deliverable: {
          __typename: 'ReverseDeliveryShippingDeliverable',
          tracking: { number: 'TRACK-1', url: 'https://tracking.example/1', company: 'Example Carrier' },
          label: { publicFileUrl: 'https://labels.example/return.pdf' },
        },
      },
      userErrors: [],
    });

    const updateDeliveryResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReverseDeliveryUpdate($reverseDeliveryId: ID!, $trackingInput: ReverseDeliveryTrackingInput) {
          reverseDeliveryShippingUpdate(reverseDeliveryId: $reverseDeliveryId, trackingInput: $trackingInput) {
            reverseDelivery {
              id
              deliverable {
                __typename
                ... on ReverseDeliveryShippingDeliverable { tracking { number url company } }
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          reverseDeliveryId: reverseDelivery.id,
          trackingInput: { number: 'TRACK-2', url: 'https://tracking.example/2', company: 'Updated Carrier' },
        },
      });
    expect(updateDeliveryResponse.body.data.reverseDeliveryShippingUpdate).toMatchObject({
      reverseDelivery: {
        id: reverseDelivery.id,
        deliverable: {
          __typename: 'ReverseDeliveryShippingDeliverable',
          tracking: { number: 'TRACK-2', url: 'https://tracking.example/2', company: 'Updated Carrier' },
        },
      },
      userErrors: [],
    });

    const processResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReturnProcess($input: ReturnProcessInput!) {
          returnProcess(input: $input) {
            return {
              id
              status
              closedAt
              returnLineItems(first: 5) { nodes { id quantity processedQuantity unprocessedQuantity } }
              reverseFulfillmentOrders(first: 5) {
                nodes { id status lineItems(first: 5) { nodes { id remainingQuantity } } }
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            returnId,
            returnLineItems: [{ id: returnLineItemId, quantity: 1 }],
            notifyCustomer: true,
          },
        },
      });
    expect(processResponse.body.data.returnProcess.return).toMatchObject({
      id: returnId,
      status: 'CLOSED',
      closedAt: expect.any(String),
      returnLineItems: { nodes: [{ id: returnLineItemId, quantity: 1, processedQuantity: 1, unprocessedQuantity: 0 }] },
      reverseFulfillmentOrders: {
        nodes: [
          {
            id: reverseFulfillmentOrder.id,
            status: 'OPEN',
            lineItems: { nodes: [{ id: reverseFulfillmentOrderLineItem.id, remainingQuantity: 0 }] },
          },
        ],
      },
    });

    const disposeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Dispose($dispositionInputs: [ReverseFulfillmentOrderDisposeInput!]!) {
          reverseFulfillmentOrderDispose(dispositionInputs: $dispositionInputs) {
            reverseFulfillmentOrderLineItems { id remainingQuantity dispositionType }
            userErrors { field message }
          }
        }`,
        variables: {
          dispositionInputs: [
            {
              reverseFulfillmentOrderLineItemId: reverseFulfillmentOrderLineItem.id,
              quantity: 1,
              dispositionType: 'RESTOCKED',
              locationId: 'gid://shopify/Location/return-flow',
            },
          ],
        },
      });
    expect(disposeResponse.body.data.reverseFulfillmentOrderDispose).toEqual({
      reverseFulfillmentOrderLineItems: [
        { id: reverseFulfillmentOrderLineItem.id, remainingQuantity: 0, dispositionType: 'RESTOCKED' },
      ],
      userErrors: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReverseRead($returnId: ID!, $orderId: ID!, $reverseDeliveryId: ID!, $reverseFulfillmentOrderId: ID!) {
          return(id: $returnId) { id status totalQuantity }
          order(id: $orderId) { id returns(first: 5) { nodes { id status totalQuantity } } }
          reverseDelivery(id: $reverseDeliveryId) {
            id
            deliverable {
              __typename
              ... on ReverseDeliveryShippingDeliverable { tracking { number } }
            }
          }
          reverseFulfillmentOrder(id: $reverseFulfillmentOrderId) {
            id
            status
            lineItems(first: 5) { nodes { id remainingQuantity dispositionType } }
            reverseDeliveries(first: 5) { nodes { id } }
          }
        }`,
        variables: {
          returnId,
          orderId: 'gid://shopify/Order/return-flow',
          reverseDeliveryId: reverseDelivery.id,
          reverseFulfillmentOrderId: reverseFulfillmentOrder.id,
        },
      });

    expect(readResponse.body.data).toMatchObject({
      return: { id: returnId, status: 'CLOSED', totalQuantity: 1 },
      order: { id: 'gid://shopify/Order/return-flow', returns: { nodes: [{ id: returnId, status: 'CLOSED' }] } },
      reverseDelivery: {
        id: reverseDelivery.id,
        deliverable: { __typename: 'ReverseDeliveryShippingDeliverable', tracking: { number: 'TRACK-2' } },
      },
      reverseFulfillmentOrder: {
        id: reverseFulfillmentOrder.id,
        status: 'CLOSED',
        lineItems: {
          nodes: [{ id: reverseFulfillmentOrderLineItem.id, remainingQuantity: 0, dispositionType: 'RESTOCKED' }],
        },
        reverseDeliveries: { nodes: [{ id: reverseDelivery.id }] },
      },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'ReturnRequest',
      'ReturnApproveRequest',
      'reverseDeliveryCreateWithShipping',
      'reverseDeliveryShippingUpdate',
      'ReturnProcess',
      'reverseFulfillmentOrderDispose',
    ]);
    const stateResponse = await request(app).get('/__meta/state');
    expect(JSON.stringify(stateResponse.body)).toContain(reverseDelivery.id);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('declines requested returns and validates invalid request lifecycle transitions locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('return decline validation must not hit upstream in snapshot mode');
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
    const returnId = requestResponse.body.data.returnRequest.return.id;

    const declineResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReturnDeclineRequest($input: ReturnDeclineRequestInput!) {
          returnDeclineRequest(input: $input) {
            return { id status decline { reason note } }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            id: returnId,
            declineReason: 'RETURN_PERIOD_ENDED',
            declineNote: 'Outside policy',
            notifyCustomer: true,
          },
        },
      });
    expect(declineResponse.body.data.returnDeclineRequest).toEqual({
      return: {
        id: returnId,
        status: 'DECLINED',
        decline: { reason: 'RETURN_PERIOD_ENDED', note: 'Outside policy' },
      },
      userErrors: [],
    });

    const invalidApproveResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReturnApproveRequest($input: ReturnApproveRequestInput!) {
          returnApproveRequest(input: $input) { return { id } userErrors { field message } }
        }`,
        variables: { input: { id: returnId } },
      });
    expect(invalidApproveResponse.body.data.returnApproveRequest).toEqual({
      return: null,
      userErrors: [
        {
          field: ['input', 'id'],
          message: 'Return is not approvable. Only returns with status REQUESTED can be approved.',
        },
      ],
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'ReturnRequest',
      'ReturnDeclineRequest',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('removes return line quantities and validates unsupported exchange removal locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('removeFromReturn must not hit upstream in snapshot mode');
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
              totalQuantity
              returnLineItems(first: 5) { nodes { id quantity } }
              reverseFulfillmentOrders(first: 5) { nodes { lineItems(first: 5) { nodes { id totalQuantity remainingQuantity } } } }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          returnInput: {
            orderId: 'gid://shopify/Order/return-flow',
            returnLineItems: [
              {
                fulfillmentLineItemId: 'gid://shopify/FulfillmentLineItem/return-flow',
                quantity: 2,
                returnReason: 'OTHER',
              },
            ],
          },
        },
      });
    const returnId = createResponse.body.data.returnCreate.return.id;
    const returnLineItemId = createResponse.body.data.returnCreate.return.returnLineItems.nodes[0].id;

    const removeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation RemoveFromReturn($returnId: ID!, $returnLineItems: [ReturnLineItemRemoveFromReturnInput!]) {
          removeFromReturn(returnId: $returnId, returnLineItems: $returnLineItems) {
            return {
              id
              totalQuantity
              returnLineItems(first: 5) { nodes { id quantity } }
              reverseFulfillmentOrders(first: 5) {
                nodes { lineItems(first: 5) { nodes { totalQuantity remainingQuantity } } }
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          returnId,
          returnLineItems: [{ returnLineItemId, quantity: 1 }],
        },
      });
    expect(removeResponse.body.data.removeFromReturn).toMatchObject({
      return: {
        id: returnId,
        totalQuantity: 1,
        returnLineItems: { nodes: [{ id: returnLineItemId, quantity: 1 }] },
        reverseFulfillmentOrders: {
          nodes: [{ lineItems: { nodes: [{ totalQuantity: 1, remainingQuantity: 1 }] } }],
        },
      },
      userErrors: [],
    });

    const exchangeRemovalResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation RemoveFromReturn($returnId: ID!, $exchangeLineItems: [ExchangeLineItemRemoveFromReturnInput!]) {
          removeFromReturn(returnId: $returnId, exchangeLineItems: $exchangeLineItems) {
            return { id }
            userErrors { field message }
          }
        }`,
        variables: {
          returnId,
          exchangeLineItems: [{ exchangeLineItemId: 'gid://shopify/ExchangeLineItem/1', quantity: 1 }],
        },
      });
    expect(exchangeRemovalResponse.body.data.removeFromReturn).toEqual({
      return: null,
      userErrors: [
        {
          field: ['exchangeLineItems'],
          message: 'Exchange line item removal is not supported by the local return model yet.',
        },
      ],
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'ReturnCreate',
      'RemoveFromReturn',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
