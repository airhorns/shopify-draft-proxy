import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';

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
