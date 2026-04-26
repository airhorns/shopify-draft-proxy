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

describe('order fulfillment flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
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

  it('stages fulfillment creation, events, tracking history, cancellation, and meta visibility without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fulfillment detail/event lifecycle should not hit upstream in snapshot mode');
    });
    const order: OrderRecord = {
      id: 'gid://shopify/Order/fulfillment-events',
      name: '#FULFILL-EVENTS',
      createdAt: '2026-04-24T00:00:00.000Z',
      updatedAt: '2026-04-24T00:00:00.000Z',
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
          id: 'gid://shopify/LineItem/fulfillment-events',
          title: 'Fulfillment event item',
          quantity: 1,
          sku: null,
          variantTitle: null,
          originalUnitPriceSet: null,
        },
      ],
      fulfillments: [],
      fulfillmentOrders: [
        {
          id: 'gid://shopify/FulfillmentOrder/fulfillment-events',
          status: 'OPEN',
          requestStatus: 'UNSUBMITTED',
          assignedLocation: { name: 'Shop location' },
          lineItems: [
            {
              id: 'gid://shopify/FulfillmentOrderLineItem/fulfillment-events',
              lineItemId: 'gid://shopify/LineItem/fulfillment-events',
              title: 'Fulfillment event item',
              totalQuantity: 1,
              remainingQuantity: 1,
            },
          ],
        },
      ],
      transactions: [],
      refunds: [],
      returns: [],
    };
    store.upsertBaseOrders([order]);

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation FulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
          fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
            fulfillment {
              id
              status
              displayStatus
              trackingInfo(first: 1) { number url company }
              events(first: 5) {
                nodes { id status }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
              fulfillmentLineItems(first: 5) { nodes { id quantity lineItem { id title } } }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          fulfillment: {
            notifyCustomer: false,
            trackingInfo: {
              number: 'HAR235-CREATE',
              url: 'https://example.com/track/HAR235-CREATE',
              company: 'Hermes',
            },
            lineItemsByFulfillmentOrder: [{ fulfillmentOrderId: 'gid://shopify/FulfillmentOrder/fulfillment-events' }],
          },
          message: 'HAR-235 local fulfillment create',
        },
      });

    expect(createResponse.status).toBe(200);
    const fulfillmentId = createResponse.body.data.fulfillmentCreate.fulfillment.id;
    expect(createResponse.body.data.fulfillmentCreate).toMatchObject({
      fulfillment: {
        status: 'SUCCESS',
        displayStatus: 'FULFILLED',
        trackingInfo: [{ number: 'HAR235-CREATE', url: 'https://example.com/track/HAR235-CREATE', company: 'Hermes' }],
        events: {
          nodes: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
      },
      userErrors: [],
    });

    const eventResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation FulfillmentEventCreate($fulfillmentEvent: FulfillmentEventInput!) {
          fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
            fulfillmentEvent {
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
            userErrors { field message }
          }
        }`,
        variables: {
          fulfillmentEvent: {
            fulfillmentId,
            status: 'IN_TRANSIT',
            message: 'HAR-235 package scanned in transit',
            happenedAt: '2026-04-25T22:25:00Z',
            estimatedDeliveryAt: '2026-04-27T18:00:00Z',
            city: 'Toronto',
            province: 'Ontario',
            country: 'Canada',
            zip: 'M5H 2M9',
            address1: '123 Queen St W',
            latitude: 43.6532,
            longitude: -79.3832,
          },
        },
      });

    expect(eventResponse.status).toBe(200);
    const event = eventResponse.body.data.fulfillmentEventCreate.fulfillmentEvent;
    expect(eventResponse.body.data.fulfillmentEventCreate.userErrors).toEqual([]);
    expect(event).toMatchObject({
      status: 'IN_TRANSIT',
      message: 'HAR-235 package scanned in transit',
      happenedAt: '2026-04-25T22:25:00Z',
      estimatedDeliveryAt: '2026-04-27T18:00:00Z',
      city: 'Toronto',
      province: 'Ontario',
      country: 'Canada',
      zip: 'M5H 2M9',
      address1: '123 Queen St W',
      latitude: 43.6532,
      longitude: -79.3832,
    });

    const detailResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query FulfillmentDetail($orderId: ID!, $fulfillmentId: ID!) {
          fulfillment(id: $fulfillmentId) {
            id
            status
            displayStatus
            deliveredAt
            estimatedDeliveryAt
            inTransitAt
            trackingInfo(first: 1) { number url company }
            events(first: 5) {
              nodes { id status message happenedAt createdAt estimatedDeliveryAt city province country zip address1 latitude longitude }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          order(id: $orderId) {
            id
            displayFulfillmentStatus
            fulfillments(first: 5) {
              id
              status
              displayStatus
              deliveredAt
              estimatedDeliveryAt
              inTransitAt
              trackingInfo(first: 1) { number url company }
              events(first: 5) {
                nodes { id status message happenedAt createdAt estimatedDeliveryAt city province country zip address1 latitude longitude }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
          }
        }`,
        variables: { orderId: order.id, fulfillmentId },
      });

    expect(detailResponse.status).toBe(200);
    expect(detailResponse.body.data.fulfillment).toEqual(detailResponse.body.data.order.fulfillments[0]);
    expect(detailResponse.body.data.fulfillment).toMatchObject({
      id: fulfillmentId,
      status: 'SUCCESS',
      displayStatus: 'IN_TRANSIT',
      deliveredAt: null,
      estimatedDeliveryAt: '2026-04-27T18:00:00Z',
      inTransitAt: '2026-04-25T22:25:00Z',
      trackingInfo: [{ number: 'HAR235-CREATE', url: 'https://example.com/track/HAR235-CREATE', company: 'Hermes' }],
      events: {
        nodes: [event],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: `cursor:${event.id}`,
          endCursor: `cursor:${event.id}`,
        },
      },
    });

    const trackingResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation FulfillmentTrackingInfoUpdate($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean) {
          fulfillmentTrackingInfoUpdate(fulfillmentId: $fulfillmentId, trackingInfoInput: $trackingInfoInput, notifyCustomer: $notifyCustomer) {
            fulfillment {
              id
              displayStatus
              trackingInfo(first: 1) { number url company }
              events(first: 5) { nodes { id status message happenedAt } }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          fulfillmentId,
          notifyCustomer: false,
          trackingInfoInput: {
            number: 'HAR235-UPDATED',
            url: 'https://example.com/track/HAR235-UPDATED',
            company: 'Hermes Updated',
          },
        },
      });

    expect(trackingResponse.status).toBe(200);
    expect(trackingResponse.body.data.fulfillmentTrackingInfoUpdate).toMatchObject({
      fulfillment: {
        id: fulfillmentId,
        displayStatus: 'IN_TRANSIT',
        trackingInfo: [
          { number: 'HAR235-UPDATED', url: 'https://example.com/track/HAR235-UPDATED', company: 'Hermes Updated' },
        ],
        events: {
          nodes: [{ id: event.id, status: 'IN_TRANSIT', message: 'HAR-235 package scanned in transit' }],
        },
      },
      userErrors: [],
    });

    const cancelResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation FulfillmentCancel($id: ID!) {
          fulfillmentCancel(id: $id) {
            fulfillment {
              id
              status
              displayStatus
              trackingInfo(first: 1) { number url company }
              events(first: 5) { nodes { id status message happenedAt } }
            }
            userErrors { field message }
          }
        }`,
        variables: { id: fulfillmentId },
      });

    expect(cancelResponse.status).toBe(200);
    expect(cancelResponse.body.data.fulfillmentCancel).toMatchObject({
      fulfillment: {
        id: fulfillmentId,
        status: 'CANCELLED',
        displayStatus: 'CANCELED',
        trackingInfo: [
          { number: 'HAR235-UPDATED', url: 'https://example.com/track/HAR235-UPDATED', company: 'Hermes Updated' },
        ],
        events: {
          nodes: [{ id: event.id, status: 'IN_TRANSIT', message: 'HAR-235 package scanned in transit' }],
        },
      },
      userErrors: [],
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(
      logResponse.body.entries.map(
        (entry: { interpreted: { primaryRootField: string } }) => entry.interpreted.primaryRootField,
      ),
    ).toEqual(['fulfillmentCreate', 'fulfillmentEventCreate', 'fulfillmentTrackingInfoUpdate', 'fulfillmentCancel']);
    const stateResponse = await request(app).get('/__meta/state');
    const stagedFulfillment = stateResponse.body.stagedState.orders[order.id].fulfillments.find(
      (candidate: { id: string }) => candidate.id === fulfillmentId,
    );
    expect(stagedFulfillment.events).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: event.id,
          status: 'IN_TRANSIT',
          message: 'HAR-235 package scanned in transit',
        }),
      ]),
    );
    expect(stagedFulfillment.trackingInfo).toEqual([
      { number: 'HAR235-UPDATED', url: 'https://example.com/track/HAR235-UPDATED', company: 'Hermes Updated' },
    ]);
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
