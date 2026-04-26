import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { WebhookSubscriptionRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function baseWebhookSubscription(id: string): WebhookSubscriptionRecord {
  return {
    id,
    topic: 'APP_UNINSTALLED',
    format: 'JSON',
    includeFields: [],
    metafieldNamespaces: [],
    filter: '',
    createdAt: '2026-04-25T21:46:44Z',
    updatedAt: '2026-04-25T21:46:44Z',
    endpoint: {
      __typename: 'WebhookHttpEndpoint',
      callbackUrl: 'https://example.com/webhooks/base',
    },
  };
}

describe('webhook subscription mutation flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages webhookSubscriptionCreate and webhookSubscriptionUpdate locally with read-after-write and meta visibility', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('webhook subscription staging must not fetch upstream'));
    const app = createApp(config).callback();

    const createMutation = `mutation CreateWebhookSubscription(
      $topic: WebhookSubscriptionTopic!
      $webhookSubscription: WebhookSubscriptionInput!
    ) {
      webhookSubscriptionCreate(topic: $topic, webhookSubscription: $webhookSubscription) {
        webhookSubscription {
          id
          topic
          format
          includeFields
          metafieldNamespaces
          filter
          createdAt
          updatedAt
          endpoint {
            __typename
            ... on WebhookHttpEndpoint {
              callbackUrl
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }`;
    const createVariables = {
      topic: 'SHOP_UPDATE',
      webhookSubscription: {
        filter: '',
        format: 'JSON',
        includeFields: ['id', 'name'],
        metafieldNamespaces: ['custom'],
        uri: 'https://example.com/hermes-webhook-local',
      },
    };

    const createResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: createMutation,
      variables: createVariables,
    });

    expect(createResponse.status).toBe(200);
    const created = createResponse.body.data.webhookSubscriptionCreate.webhookSubscription;
    expect(created).toMatchObject({
      id: expect.stringMatching(/^gid:\/\/shopify\/WebhookSubscription\/[0-9]+\?shopify-draft-proxy=synthetic$/u),
      topic: 'SHOP_UPDATE',
      format: 'JSON',
      includeFields: ['id', 'name'],
      metafieldNamespaces: ['custom'],
      filter: '',
      endpoint: {
        __typename: 'WebhookHttpEndpoint',
        callbackUrl: 'https://example.com/hermes-webhook-local',
      },
    });
    expect(createResponse.body.data.webhookSubscriptionCreate.userErrors).toEqual([]);

    const updateMutation = `mutation UpdateWebhookSubscription($id: ID!, $webhookSubscription: WebhookSubscriptionInput!) {
      webhookSubscriptionUpdate(id: $id, webhookSubscription: $webhookSubscription) {
        webhookSubscription {
          id
          topic
          format
          includeFields
          metafieldNamespaces
          filter
          createdAt
          updatedAt
          endpoint {
            __typename
            ... on WebhookHttpEndpoint {
              callbackUrl
            }
          }
        }
        userErrors {
          field
          message
        }
      }
    }`;
    const updateVariables = {
      id: created.id,
      webhookSubscription: {
        filter: '',
        format: 'JSON',
        includeFields: ['id'],
        metafieldNamespaces: [],
        uri: 'https://example.com/hermes-webhook-local-updated',
      },
    };

    const updateResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: updateMutation,
      variables: updateVariables,
    });

    expect(updateResponse.status).toBe(200);
    const updated = updateResponse.body.data.webhookSubscriptionUpdate.webhookSubscription;
    expect(updated).toMatchObject({
      id: created.id,
      topic: 'SHOP_UPDATE',
      format: 'JSON',
      includeFields: ['id'],
      metafieldNamespaces: [],
      filter: '',
      createdAt: created.createdAt,
      endpoint: {
        __typename: 'WebhookHttpEndpoint',
        callbackUrl: 'https://example.com/hermes-webhook-local-updated',
      },
    });
    expect(updated.updatedAt).not.toBe(created.updatedAt);
    expect(updateResponse.body.data.webhookSubscriptionUpdate.userErrors).toEqual([]);

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query WebhookSubscriptionReadAfterWrite($id: ID!) {
          webhookSubscription(id: $id) {
            id
            topic
            format
            includeFields
            metafieldNamespaces
            endpoint {
              __typename
              ... on WebhookHttpEndpoint {
                callbackUrl
              }
            }
          }
          webhookSubscriptions(first: 10, sortKey: ID) {
            nodes {
              id
              endpoint {
                __typename
                ... on WebhookHttpEndpoint {
                  callbackUrl
                }
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          webhookSubscriptionsCount {
            count
            precision
          }
        }`,
        variables: {
          id: created.id,
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.webhookSubscription).toEqual({
      id: created.id,
      topic: 'SHOP_UPDATE',
      format: 'JSON',
      includeFields: ['id'],
      metafieldNamespaces: [],
      endpoint: {
        __typename: 'WebhookHttpEndpoint',
        callbackUrl: 'https://example.com/hermes-webhook-local-updated',
      },
    });
    expect(readResponse.body.data.webhookSubscriptions.nodes).toEqual([
      {
        id: created.id,
        endpoint: {
          __typename: 'WebhookHttpEndpoint',
          callbackUrl: 'https://example.com/hermes-webhook-local-updated',
        },
      },
    ]);
    expect(readResponse.body.data.webhookSubscriptions.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: `cursor:${created.id}`,
      endCursor: `cursor:${created.id}`,
    });
    expect(readResponse.body.data.webhookSubscriptionsCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'webhookSubscriptionCreate',
      'webhookSubscriptionUpdate',
    ]);
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual(['staged', 'staged']);
    expect(logResponse.body.entries[0].requestBody).toEqual({
      query: createMutation,
      variables: createVariables,
    });
    expect(logResponse.body.entries[0].stagedResourceIds).toEqual([created.id]);
    expect(logResponse.body.entries[1].requestBody.variables).toEqual(updateVariables);
    expect(logResponse.body.entries[1].stagedResourceIds).toEqual([created.id]);

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.webhookSubscriptions[created.id]).toMatchObject({
      id: created.id,
      includeFields: ['id'],
      metafieldNamespaces: [],
      endpoint: {
        __typename: 'WebhookHttpEndpoint',
        callbackUrl: 'https://example.com/hermes-webhook-local-updated',
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('updates known base webhook subscriptions and mirrors captured validation branches without staging', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('webhook subscription validation must not fetch upstream'));
    const knownWebhook = baseWebhookSubscription('gid://shopify/WebhookSubscription/1001');
    store.upsertBaseWebhookSubscriptions([knownWebhook]);
    const app = createApp(config).callback();

    const updateKnownResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateKnownWebhook($id: ID!, $webhookSubscription: WebhookSubscriptionInput!) {
          webhookSubscriptionUpdate(id: $id, webhookSubscription: $webhookSubscription) {
            webhookSubscription {
              id
              topic
              includeFields
              endpoint {
                __typename
                ... on WebhookHttpEndpoint {
                  callbackUrl
                }
              }
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: knownWebhook.id,
          webhookSubscription: {
            includeFields: ['id'],
            uri: 'https://example.com/webhooks/base-updated',
          },
        },
      });

    expect(updateKnownResponse.status).toBe(200);
    expect(updateKnownResponse.body.data.webhookSubscriptionUpdate).toEqual({
      webhookSubscription: {
        id: knownWebhook.id,
        topic: 'APP_UNINSTALLED',
        includeFields: ['id'],
        endpoint: {
          __typename: 'WebhookHttpEndpoint',
          callbackUrl: 'https://example.com/webhooks/base-updated',
        },
      },
      userErrors: [],
    });

    store.reset();
    resetSyntheticIdentity();
    const validationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation WebhookSubscriptionValidationBranches($unknownId: ID!, $unknownUpdateInput: WebhookSubscriptionInput!) {
          updateUnknown: webhookSubscriptionUpdate(id: $unknownId, webhookSubscription: $unknownUpdateInput) {
            webhookSubscription {
              id
            }
            userErrors {
              field
              message
            }
          }
          createMissingUri: webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: { format: JSON }) {
            webhookSubscription {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          unknownId: 'gid://shopify/WebhookSubscription/999999999999',
          unknownUpdateInput: {
            format: 'JSON',
            uri: 'https://example.com/hermes-webhook-conformance-unknown',
          },
        },
      });

    expect(validationResponse.status).toBe(200);
    expect(validationResponse.body.data).toEqual({
      updateUnknown: {
        webhookSubscription: null,
        userErrors: [{ field: ['id'], message: 'Webhook subscription does not exist' }],
      },
      createMissingUri: {
        webhookSubscription: null,
        userErrors: [{ field: ['webhookSubscription', 'callbackUrl'], message: "Address can't be blank" }],
      },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries).toEqual([]);
    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.webhookSubscriptions).toEqual({});
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
