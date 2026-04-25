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

function makeOrder(id: string, overrides: Partial<OrderRecord> = {}): OrderRecord {
  return {
    id,
    name: '#FULFILLMENT-ORDER-LIFECYCLE',
    createdAt: '2026-04-25T00:00:00.000Z',
    updatedAt: '2026-04-25T00:00:00.000Z',
    displayFinancialStatus: 'PAID',
    displayFulfillmentStatus: 'UNFULFILLED',
    note: null,
    tags: [],
    customAttributes: [],
    billingAddress: null,
    shippingAddress: null,
    subtotalPriceSet: { shopMoney: { amount: '10.0', currencyCode: 'CAD' } },
    currentTotalPriceSet: { shopMoney: { amount: '10.0', currencyCode: 'CAD' } },
    totalPriceSet: { shopMoney: { amount: '10.0', currencyCode: 'CAD' } },
    totalRefundedSet: { shopMoney: { amount: '0.0', currencyCode: 'CAD' } },
    customer: null,
    shippingLines: [],
    lineItems: [
      {
        id: 'gid://shopify/LineItem/fulfillment-order-lifecycle',
        title: 'Fulfillment order lifecycle item',
        quantity: 2,
        sku: null,
        variantTitle: null,
        originalUnitPriceSet: null,
      },
    ],
    fulfillments: [],
    fulfillmentOrders: [],
    transactions: [],
    refunds: [],
    returns: [],
    ...overrides,
  };
}

describe('order fulfillment flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages fulfillment-order hold and release with downstream held-order reads and mutation log entries', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fulfillmentOrderHold and fulfillmentOrderReleaseHold must not hit upstream in snapshot mode');
    });
    const fulfillmentOrder = {
      id: 'gid://shopify/FulfillmentOrder/hold-release',
      status: 'OPEN',
      requestStatus: 'UNSUBMITTED',
      fulfillAt: '2026-04-25T22:00:00Z',
      fulfillBy: null,
      assignedLocation: {
        name: 'My Custom Location',
        locationId: 'gid://shopify/Location/source',
      },
      supportedActions: ['CREATE_FULFILLMENT', 'REPORT_PROGRESS', 'MOVE', 'HOLD', 'SPLIT'],
      lineItems: [
        {
          id: 'gid://shopify/FulfillmentOrderLineItem/hold-release',
          lineItemId: 'gid://shopify/LineItem/fulfillment-order-lifecycle',
          title: 'Fulfillment order lifecycle item',
          totalQuantity: 2,
          remainingQuantity: 2,
        },
      ],
    };
    const order = makeOrder('gid://shopify/Order/hold-release', { fulfillmentOrders: [fulfillmentOrder] });
    store.upsertBaseOrders([order]);

    const app = createApp(snapshotConfig).callback();
    const holdResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Hold($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
          fulfillmentOrderHold(id: $id, fulfillmentHold: $fulfillmentHold) {
            fulfillmentHold { id handle reason reasonNotes heldByRequestingApp }
            fulfillmentOrder {
              id
              status
              requestStatus
              supportedActions { action }
              fulfillmentHolds { id handle reason reasonNotes heldByRequestingApp }
              lineItems(first: 5) { nodes { id totalQuantity remainingQuantity lineItem { id title } } }
            }
            remainingFulfillmentOrder {
              id
              status
              lineItems(first: 5) { nodes { totalQuantity remainingQuantity lineItem { id } } }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          id: fulfillmentOrder.id,
          fulfillmentHold: {
            reason: 'OTHER',
            reasonNotes: 'Local lifecycle hold',
            handle: 'local-lifecycle-hold',
            notifyMerchant: false,
            fulfillmentOrderLineItems: [{ id: 'gid://shopify/FulfillmentOrderLineItem/hold-release', quantity: 1 }],
          },
        },
      });

    expect(holdResponse.status).toBe(200);
    expect(holdResponse.body.data.fulfillmentOrderHold.userErrors).toEqual([]);
    expect(holdResponse.body.data.fulfillmentOrderHold.fulfillmentOrder).toMatchObject({
      id: fulfillmentOrder.id,
      status: 'ON_HOLD',
      requestStatus: 'UNSUBMITTED',
      supportedActions: [{ action: 'RELEASE_HOLD' }, { action: 'HOLD' }, { action: 'MOVE' }],
      fulfillmentHolds: [
        {
          handle: 'local-lifecycle-hold',
          reason: 'OTHER',
          reasonNotes: 'Local lifecycle hold',
          heldByRequestingApp: true,
        },
      ],
      lineItems: {
        nodes: [
          {
            id: 'gid://shopify/FulfillmentOrderLineItem/hold-release',
            totalQuantity: 1,
            remainingQuantity: 1,
            lineItem: {
              id: 'gid://shopify/LineItem/fulfillment-order-lifecycle',
              title: 'Fulfillment order lifecycle item',
            },
          },
        ],
      },
    });
    expect(holdResponse.body.data.fulfillmentOrderHold.remainingFulfillmentOrder).toMatchObject({
      status: 'OPEN',
      lineItems: {
        nodes: [
          {
            totalQuantity: 1,
            remainingQuantity: 1,
            lineItem: { id: 'gid://shopify/LineItem/fulfillment-order-lifecycle' },
          },
        ],
      },
    });

    const heldRead = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query HeldReads($id: ID!, $first: Int!) {
          order(id: $id) {
            fulfillmentOrders(first: $first) {
              nodes { id status fulfillmentHolds { id handle } }
            }
          }
          manualHoldsFulfillmentOrders(first: $first) {
            nodes { id status fulfillmentHolds { id handle } }
          }
        }`,
        variables: { id: order.id, first: 5 },
      });

    expect(heldRead.body.data.manualHoldsFulfillmentOrders.nodes).toEqual([
      {
        id: fulfillmentOrder.id,
        status: 'ON_HOLD',
        fulfillmentHolds: [
          {
            id: expect.any(String),
            handle: 'local-lifecycle-hold',
          },
        ],
      },
    ]);
    expect(heldRead.body.data.order.fulfillmentOrders.nodes).toHaveLength(2);

    const releaseResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ReleaseHold($id: ID!, $holdIds: [ID!], $externalId: String) {
          fulfillmentOrderReleaseHold(id: $id, holdIds: $holdIds, externalId: $externalId) {
            fulfillmentOrder { id status fulfillmentHolds { id } supportedActions { action } }
            userErrors { field message }
          }
        }`,
        variables: {
          id: fulfillmentOrder.id,
          holdIds: [holdResponse.body.data.fulfillmentOrderHold.fulfillmentHold.id],
          externalId: 'local-lifecycle-hold',
        },
      });

    expect(releaseResponse.status).toBe(200);
    expect(releaseResponse.body.data.fulfillmentOrderReleaseHold).toEqual({
      fulfillmentOrder: {
        id: fulfillmentOrder.id,
        status: 'OPEN',
        fulfillmentHolds: [],
        supportedActions: [
          { action: 'CREATE_FULFILLMENT' },
          { action: 'REPORT_PROGRESS' },
          { action: 'MOVE' },
          { action: 'HOLD' },
          { action: 'SPLIT' },
        ],
      },
      userErrors: [],
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries).toHaveLength(2);
    expect(
      logResponse.body.entries.map((entry: { operationName: string; status: string }) => [
        entry.operationName,
        entry.status,
      ]),
    ).toEqual([
      ['fulfillmentOrderHold', 'staged'],
      ['fulfillmentOrderReleaseHold', 'staged'],
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages fulfillment-order move, progress/open, cancel, and captured guardrails without upstream calls', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fulfillment-order lifecycle mutations must not hit upstream in snapshot mode');
    });
    const fulfillmentOrder = {
      id: 'gid://shopify/FulfillmentOrder/lifecycle',
      status: 'OPEN',
      requestStatus: 'UNSUBMITTED',
      fulfillAt: '2026-04-25T22:00:00Z',
      fulfillBy: null,
      assignedLocation: {
        name: 'My Custom Location',
        locationId: 'gid://shopify/Location/source',
      },
      supportedActions: ['CREATE_FULFILLMENT', 'REPORT_PROGRESS', 'MOVE', 'HOLD', 'SPLIT'],
      lineItems: [
        {
          id: 'gid://shopify/FulfillmentOrderLineItem/lifecycle',
          lineItemId: 'gid://shopify/LineItem/fulfillment-order-lifecycle',
          title: 'Fulfillment order lifecycle item',
          totalQuantity: 2,
          remainingQuantity: 2,
        },
      ],
    };
    const order = makeOrder('gid://shopify/Order/lifecycle', { fulfillmentOrders: [fulfillmentOrder] });
    store.upsertBaseOrders([order]);

    const app = createApp(snapshotConfig).callback();
    const moveResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Move($id: ID!, $newLocationId: ID!, $fulfillmentOrderLineItems: [FulfillmentOrderLineItemInput!]) {
          fulfillmentOrderMove(id: $id, newLocationId: $newLocationId, fulfillmentOrderLineItems: $fulfillmentOrderLineItems) {
            movedFulfillmentOrder { id status assignedLocation { name location { id name } } lineItems(first: 5) { nodes { totalQuantity remainingQuantity } } }
            originalFulfillmentOrder { id status lineItems(first: 5) { nodes { totalQuantity remainingQuantity } } }
            remainingFulfillmentOrder { id status lineItems(first: 5) { nodes { totalQuantity remainingQuantity } } }
            userErrors { field message }
          }
        }`,
        variables: {
          id: fulfillmentOrder.id,
          newLocationId: 'gid://shopify/Location/destination',
          fulfillmentOrderLineItems: [{ id: 'gid://shopify/FulfillmentOrderLineItem/lifecycle', quantity: 1 }],
        },
      });

    expect(moveResponse.status).toBe(200);
    expect(moveResponse.body.data.fulfillmentOrderMove.userErrors).toEqual([]);
    const movedFulfillmentOrderId = moveResponse.body.data.fulfillmentOrderMove.movedFulfillmentOrder.id;
    expect(moveResponse.body.data.fulfillmentOrderMove).toMatchObject({
      movedFulfillmentOrder: {
        status: 'OPEN',
        assignedLocation: {
          name: 'Shop location',
          location: {
            id: 'gid://shopify/Location/destination',
            name: 'Shop location',
          },
        },
        lineItems: { nodes: [{ totalQuantity: 1, remainingQuantity: 1 }] },
      },
      originalFulfillmentOrder: {
        id: fulfillmentOrder.id,
        status: 'OPEN',
        lineItems: { nodes: [{ totalQuantity: 1, remainingQuantity: 1 }] },
      },
      remainingFulfillmentOrder: {
        id: fulfillmentOrder.id,
        status: 'OPEN',
        lineItems: { nodes: [{ totalQuantity: 1, remainingQuantity: 1 }] },
      },
    });

    const progressResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Progress($id: ID!, $progressReport: FulfillmentOrderReportProgressInput) {
          fulfillmentOrderReportProgress(id: $id, progressReport: $progressReport) {
            fulfillmentOrder { id status supportedActions { action } }
            userErrors { field message }
          }
        }`,
        variables: {
          id: movedFulfillmentOrderId,
          progressReport: { reasonNotes: 'Local progress' },
        },
      });
    expect(progressResponse.body.data.fulfillmentOrderReportProgress).toEqual({
      fulfillmentOrder: {
        id: movedFulfillmentOrderId,
        status: 'IN_PROGRESS',
        supportedActions: [
          { action: 'CREATE_FULFILLMENT' },
          { action: 'REPORT_PROGRESS' },
          { action: 'HOLD' },
          { action: 'MARK_AS_OPEN' },
        ],
      },
      userErrors: [],
    });

    const openResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Open($id: ID!) {
          fulfillmentOrderOpen(id: $id) {
            fulfillmentOrder { id status supportedActions { action } }
            userErrors { field message }
          }
        }`,
        variables: { id: movedFulfillmentOrderId },
      });
    expect(openResponse.body.data.fulfillmentOrderOpen.fulfillmentOrder.status).toBe('OPEN');

    const cancelResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Cancel($id: ID!) {
          fulfillmentOrderCancel(id: $id) {
            fulfillmentOrder { id status lineItems(first: 5) { nodes { id } } }
            replacementFulfillmentOrder { id status lineItems(first: 5) { nodes { totalQuantity remainingQuantity } } }
            userErrors { field message }
          }
        }`,
        variables: { id: movedFulfillmentOrderId },
      });
    expect(cancelResponse.body.data.fulfillmentOrderCancel).toMatchObject({
      fulfillmentOrder: {
        id: movedFulfillmentOrderId,
        status: 'CLOSED',
        lineItems: { nodes: [] },
      },
      replacementFulfillmentOrder: {
        status: 'OPEN',
        lineItems: { nodes: [{ totalQuantity: 1, remainingQuantity: 1 }] },
      },
      userErrors: [],
    });

    const guardrailResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Guardrails($id: ID!, $fulfillAt: DateTime!, $message: String) {
          fulfillmentOrderReschedule(id: $id, fulfillAt: $fulfillAt) {
            fulfillmentOrder { id }
            userErrors { field message }
          }
          fulfillmentOrderClose(id: $id, message: $message) {
            fulfillmentOrder { id }
            userErrors { field message }
          }
        }`,
        variables: {
          id: fulfillmentOrder.id,
          fulfillAt: '2026-04-28T00:00:00Z',
          message: 'close guardrail',
        },
      });
    expect(guardrailResponse.body.data).toEqual({
      fulfillmentOrderReschedule: {
        fulfillmentOrder: null,
        userErrors: [{ field: null, message: 'Fulfillment order must be scheduled.' }],
      },
      fulfillmentOrderClose: {
        fulfillmentOrder: null,
        userErrors: [
          {
            field: null,
            message: "The fulfillment order's assigned fulfillment service must be of api type",
          },
        ],
      },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(
      logResponse.body.entries.map((entry: { operationName: string; status: string }) => [
        entry.operationName,
        entry.status,
      ]),
    ).toEqual([
      ['fulfillmentOrderMove', 'staged'],
      ['fulfillmentOrderReportProgress', 'staged'],
      ['fulfillmentOrderOpen', 'staged'],
      ['fulfillmentOrderCancel', 'staged'],
      ['fulfillmentOrderReschedule', 'staged'],
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured fulfillmentTrackingInfoUpdate missing-fulfillmentId variable error in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error(
        'fulfillmentTrackingInfoUpdate should not hit upstream in snapshot mode for the captured missing-id branch',
      );
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation FulfillmentTrackingInfoUpdateMissingId($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean) {
          fulfillmentTrackingInfoUpdate(fulfillmentId: $fulfillmentId, trackingInfoInput: $trackingInfoInput, notifyCustomer: $notifyCustomer) {
            fulfillment {
              id
              status
              trackingInfo {
                number
                url
                company
              }
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          trackingInfoInput: {
            number: 'HERMES-TRACK-UPDATE',
            url: 'https://example.com/track/HERMES-TRACK-UPDATE',
            company: 'Hermes',
          },
          notifyCustomer: false,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $fulfillmentId of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured fulfillmentTrackingInfoUpdate inline argument-validation errors in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error(
        'fulfillmentTrackingInfoUpdate should not hit upstream in snapshot mode for the captured inline validation branches',
      );
    });

    const app = createApp(snapshotConfig).callback();
    const missingArgumentResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation FulfillmentTrackingInfoUpdateInlineMissingId($trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean) {
          fulfillmentTrackingInfoUpdate(trackingInfoInput: $trackingInfoInput, notifyCustomer: $notifyCustomer) {
            fulfillment {
              id
              status
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          trackingInfoInput: {
            number: 'HERMES-TRACK-UPDATE',
            url: 'https://example.com/track/HERMES-TRACK-UPDATE',
            company: 'Hermes',
          },
          notifyCustomer: false,
        },
      });

    expect(missingArgumentResponse.status).toBe(200);
    expect(missingArgumentResponse.body).toEqual({
      errors: [
        {
          message: "Field 'fulfillmentTrackingInfoUpdate' is missing required arguments: fulfillmentId",
          path: ['mutation', 'fulfillmentTrackingInfoUpdate'],
          extensions: {
            code: 'missingRequiredArguments',
            className: 'Field',
            name: 'fulfillmentTrackingInfoUpdate',
            arguments: 'fulfillmentId',
          },
        },
      ],
    });

    const nullArgumentResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation FulfillmentTrackingInfoUpdateInlineNullId($trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean) {
          fulfillmentTrackingInfoUpdate(fulfillmentId: null, trackingInfoInput: $trackingInfoInput, notifyCustomer: $notifyCustomer) {
            fulfillment {
              id
              status
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          trackingInfoInput: {
            number: 'HERMES-TRACK-UPDATE',
            url: 'https://example.com/track/HERMES-TRACK-UPDATE',
            company: 'Hermes',
          },
          notifyCustomer: false,
        },
      });

    expect(nullArgumentResponse.status).toBe(200);
    expect(nullArgumentResponse.body).toEqual({
      errors: [
        {
          message:
            "Argument 'fulfillmentId' on Field 'fulfillmentTrackingInfoUpdate' has an invalid value (null). Expected type 'ID!'.",
          path: ['mutation', 'fulfillmentTrackingInfoUpdate', 'fulfillmentId'],
          extensions: {
            code: 'argumentLiteralsIncompatible',
            typeName: 'Field',
            argumentName: 'fulfillmentId',
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured fulfillmentCancel validation errors in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error(
        'fulfillmentCancel should not hit upstream in snapshot mode for the captured validation branches',
      );
    });

    const app = createApp(snapshotConfig).callback();
    const missingVariableResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation FulfillmentCancelMissingId($id: ID!) {
          fulfillmentCancel(id: $id) {
            fulfillment {
              id
              status
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {},
      });

    expect(missingVariableResponse.status).toBe(200);
    expect(missingVariableResponse.body).toEqual({
      errors: [
        {
          message: 'Variable $id of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });

    const missingArgumentResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation FulfillmentCancelInlineMissingId {
          fulfillmentCancel {
            fulfillment {
              id
              status
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {},
      });

    expect(missingArgumentResponse.status).toBe(200);
    expect(missingArgumentResponse.body).toEqual({
      errors: [
        {
          message: "Field 'fulfillmentCancel' is missing required arguments: id",
          path: ['mutation', 'fulfillmentCancel'],
          extensions: {
            code: 'missingRequiredArguments',
            className: 'Field',
            name: 'fulfillmentCancel',
            arguments: 'id',
          },
        },
      ],
    });

    const nullArgumentResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation FulfillmentCancelInlineNullId {
          fulfillmentCancel(id: null) {
            fulfillment {
              id
              status
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {},
      });

    expect(nullArgumentResponse.status).toBe(200);
    expect(nullArgumentResponse.body).toEqual({
      errors: [
        {
          message: "Argument 'id' on Field 'fulfillmentCancel' has an invalid value (null). Expected type 'ID!'.",
          path: ['mutation', 'fulfillmentCancel', 'id'],
          extensions: {
            code: 'argumentLiteralsIncompatible',
            typeName: 'Field',
            argumentName: 'id',
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured fulfillment lifecycle validation branches in live-hybrid mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('captured fulfillment lifecycle validation branches should not hit upstream in live-hybrid mode');
    });

    const app = createApp(liveHybridConfig).callback();

    const trackingResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation FulfillmentTrackingInfoUpdateMissingId($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean) {
          fulfillmentTrackingInfoUpdate(fulfillmentId: $fulfillmentId, trackingInfoInput: $trackingInfoInput, notifyCustomer: $notifyCustomer) {
            fulfillment { id status }
            userErrors { field message }
          }
        }`,
        variables: {
          trackingInfoInput: {
            number: 'HERMES-TRACK-UPDATE',
            url: 'https://example.com/track/HERMES-TRACK-UPDATE',
            company: 'Hermes',
          },
          notifyCustomer: false,
        },
      });

    expect(trackingResponse.status).toBe(200);
    expect(trackingResponse.body).toEqual({
      errors: [
        {
          message: 'Variable $fulfillmentId of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });

    const cancelResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation FulfillmentCancelMissingId($id: ID!) {
          fulfillmentCancel(id: $id) {
            fulfillment { id status }
            userErrors { field message }
          }
        }`,
        variables: {},
      });

    expect(cancelResponse.status).toBe(200);
    expect(cancelResponse.body).toEqual({
      errors: [
        {
          message: 'Variable $id of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages fulfillment tracking updates and cancellation in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fulfillment lifecycle happy path should not hit upstream in snapshot mode');
    });
    const order: OrderRecord = {
      id: 'gid://shopify/Order/fulfillment-lifecycle',
      name: '#FULFILL',
      createdAt: '2026-04-24T00:00:00.000Z',
      updatedAt: '2026-04-24T00:00:00.000Z',
      displayFinancialStatus: 'PAID',
      displayFulfillmentStatus: 'FULFILLED',
      note: null,
      tags: [],
      customAttributes: [],
      billingAddress: null,
      shippingAddress: null,
      subtotalPriceSet: { shopMoney: { amount: '10.0', currencyCode: 'CAD' } },
      currentTotalPriceSet: { shopMoney: { amount: '10.0', currencyCode: 'CAD' } },
      totalPriceSet: { shopMoney: { amount: '10.0', currencyCode: 'CAD' } },
      totalRefundedSet: { shopMoney: { amount: '0.0', currencyCode: 'CAD' } },
      customer: null,
      shippingLines: [],
      lineItems: [
        {
          id: 'gid://shopify/LineItem/fulfillment-lifecycle',
          title: 'Fulfillment lifecycle item',
          quantity: 1,
          sku: null,
          variantTitle: null,
          originalUnitPriceSet: null,
        },
      ],
      fulfillments: [
        {
          id: 'gid://shopify/Fulfillment/fulfillment-lifecycle',
          status: 'SUCCESS',
          displayStatus: 'FULFILLED',
          createdAt: '2026-04-24T00:00:00.000Z',
          updatedAt: '2026-04-24T00:00:00.000Z',
          trackingInfo: [
            {
              number: 'HERMES-CREATE',
              url: 'https://example.com/track/HERMES-CREATE',
              company: 'Hermes',
            },
          ],
          fulfillmentLineItems: [
            {
              id: 'gid://shopify/FulfillmentLineItem/fulfillment-lifecycle',
              lineItemId: 'gid://shopify/LineItem/fulfillment-lifecycle',
              title: 'Fulfillment lifecycle item',
              quantity: 1,
            },
          ],
        },
      ],
      fulfillmentOrders: [],
      transactions: [],
      refunds: [],
      returns: [],
    };
    store.upsertBaseOrders([order]);

    const app = createApp(snapshotConfig).callback();
    const trackingResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation FulfillmentTrackingInfoUpdateParityPlan($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean) {
          fulfillmentTrackingInfoUpdate(fulfillmentId: $fulfillmentId, trackingInfoInput: $trackingInfoInput, notifyCustomer: $notifyCustomer) {
            fulfillment {
              id
              status
              trackingInfo {
                number
                url
                company
              }
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          fulfillmentId: 'gid://shopify/Fulfillment/fulfillment-lifecycle',
          notifyCustomer: false,
          trackingInfoInput: {
            number: 'HERMES-UPDATE',
            url: 'https://example.com/track/HERMES-UPDATE',
            company: 'Hermes',
          },
        },
      });

    expect(trackingResponse.status).toBe(200);
    expect(trackingResponse.body).toEqual({
      data: {
        fulfillmentTrackingInfoUpdate: {
          fulfillment: {
            id: 'gid://shopify/Fulfillment/fulfillment-lifecycle',
            status: 'SUCCESS',
            trackingInfo: [
              {
                number: 'HERMES-UPDATE',
                url: 'https://example.com/track/HERMES-UPDATE',
                company: 'Hermes',
              },
            ],
          },
          userErrors: [],
        },
      },
    });

    const cancelResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation FulfillmentCancelParityPlan($id: ID!) {
          fulfillmentCancel(id: $id) {
            fulfillment {
              id
              status
              displayStatus
              trackingInfo {
                number
                url
                company
              }
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: 'gid://shopify/Fulfillment/fulfillment-lifecycle',
        },
      });

    expect(cancelResponse.status).toBe(200);
    expect(cancelResponse.body).toEqual({
      data: {
        fulfillmentCancel: {
          fulfillment: {
            id: 'gid://shopify/Fulfillment/fulfillment-lifecycle',
            status: 'CANCELLED',
            displayStatus: 'CANCELED',
            trackingInfo: [
              {
                number: 'HERMES-UPDATE',
                url: 'https://example.com/track/HERMES-UPDATE',
                company: 'Hermes',
              },
            ],
          },
          userErrors: [],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured fulfillmentCreate invalid-fulfillment-order error in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fulfillmentCreate should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation FulfillmentCreateUnknownFulfillmentOrder($fulfillment: FulfillmentInput!, $message: String) {
          fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
            fulfillment {
              id
              status
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          fulfillment: {
            notifyCustomer: false,
            trackingInfo: {
              number: 'HERMES-PROBE',
              url: 'https://example.com/track/HERMES-PROBE',
              company: 'Hermes',
            },
            lineItemsByFulfillmentOrder: [{ fulfillmentOrderId: 'gid://shopify/FulfillmentOrder/0' }],
          },
          message: 'hermes fulfillment probe',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'invalid id',
          extensions: {
            code: 'RESOURCE_NOT_FOUND',
          },
          path: ['fulfillmentCreate'],
        },
      ],
      data: {
        fulfillmentCreate: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured fulfillmentCreate invalid-fulfillment-order error in live-hybrid mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error(
        'fulfillmentCreate should not hit upstream in live-hybrid mode for the captured invalid-id branch',
      );
    });

    const app = createApp(liveHybridConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation FulfillmentCreateUnknownFulfillmentOrder($fulfillment: FulfillmentInput!, $message: String) {
          fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
            fulfillment {
              id
              status
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          fulfillment: {
            notifyCustomer: false,
            trackingInfo: {
              number: 'HERMES-PROBE-LIVE',
              url: 'https://example.com/track/HERMES-PROBE-LIVE',
              company: 'Hermes',
            },
            lineItemsByFulfillmentOrder: [{ fulfillmentOrderId: 'gid://shopify/FulfillmentOrder/0' }],
          },
          message: 'hermes fulfillment probe live-hybrid',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'invalid id',
          extensions: {
            code: 'RESOURCE_NOT_FOUND',
          },
          path: ['fulfillmentCreate'],
        },
      ],
      data: {
        fulfillmentCreate: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
