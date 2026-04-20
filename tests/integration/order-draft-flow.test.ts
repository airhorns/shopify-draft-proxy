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
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'live-hybrid',
};

describe('order draft flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('mirrors the captured orderUpdate inline missing-id GraphQL validation branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderUpdate should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InlineMissingOrderId {
          orderUpdate(input: { note: "inline missing id", tags: ["inline", "missing-id"] }) {
            order {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: "Argument 'id' on InputObject 'OrderInput' is required. Expected type ID!",
          path: ['mutation', 'orderUpdate', 'input', 'id'],
          extensions: {
            code: 'missingRequiredInputObjectAttribute',
            argumentName: 'id',
            argumentType: 'ID!',
            inputObjectType: 'OrderInput',
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderUpdate inline null-id GraphQL validation branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderUpdate should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InlineNullOrderId {
          orderUpdate(input: { id: null, note: "inline null id", tags: ["inline", "null-id"] }) {
            order {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: "Argument 'id' on InputObject 'OrderInput' has an invalid value (null). Expected type 'ID!'.",
          path: ['mutation', 'orderUpdate', 'input', 'id'],
          extensions: {
            code: 'argumentLiteralsIncompatible',
            typeName: 'InputObject',
            argumentName: 'id',
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderUpdate missing-id INVALID_VARIABLE branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderUpdate should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UpdateOrderWithoutId($input: OrderInput!) {
          orderUpdate(input: $input) {
            order {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            note: 'snapshot missing-order-id probe',
            tags: ['snapshot', 'missing-order-id'],
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $input of type OrderInput! was provided invalid value for id (Expected value to not be null)',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: {
              note: 'snapshot missing-order-id probe',
              tags: ['snapshot', 'missing-order-id'],
            },
            problems: [{ path: ['id'], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderUpdate unknown-id userError in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderUpdate should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UpdateMissingOrder($input: OrderInput!) {
          orderUpdate(input: $input) {
            order {
              id
              name
              updatedAt
              note
              tags
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            id: 'gid://shopify/Order/0',
            note: 'snapshot unknown-order probe',
            tags: ['snapshot', 'unknown-order'],
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        orderUpdate: {
          order: null,
          userErrors: [{ field: ['id'], message: 'Order does not exist' }],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderUpdate unknown-id userError in live-hybrid mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderUpdate should not hit upstream in live-hybrid mode when the target order id is unknown');
    });

    const app = createApp(liveHybridConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `mutation UpdateMissingOrder($input: OrderInput!) {
          orderUpdate(input: $input) {
            order {
              id
              name
              updatedAt
              note
              tags
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            id: 'gid://shopify/Order/0',
            note: 'live-hybrid unknown-order probe',
            tags: ['live-hybrid', 'unknown-order'],
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        orderUpdate: {
          order: null,
          userErrors: [{ field: ['id'], message: 'Order does not exist' }],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
