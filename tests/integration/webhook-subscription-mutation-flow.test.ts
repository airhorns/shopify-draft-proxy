import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';
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
          uri
          format
          name
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
        name: 'shop_update_sync',
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
      uri: 'https://example.com/hermes-webhook-local',
      format: 'JSON',
      name: 'shop_update_sync',
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
          uri
          format
          name
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
        name: 'shop_update_sync_renamed',
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
      uri: 'https://example.com/hermes-webhook-local-updated',
      format: 'JSON',
      name: 'shop_update_sync_renamed',
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
            uri
            name
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
      uri: 'https://example.com/hermes-webhook-local-updated',
      name: 'shop_update_sync_renamed',
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

    const deleteMutation = `mutation DeleteWebhookSubscription($id: ID!) {
      webhookSubscriptionDelete(id: $id) {
        deletedWebhookSubscriptionId
        userErrors {
          field
          message
        }
      }
    }`;
    const deleteVariables = { id: created.id };
    const deleteResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: deleteMutation,
      variables: deleteVariables,
    });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.webhookSubscriptionDelete).toEqual({
      deletedWebhookSubscriptionId: created.id,
      userErrors: [],
    });

    const readDeletedResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query WebhookSubscriptionReadAfterDelete($id: ID!) {
          webhookSubscription(id: $id) {
            id
          }
          webhookSubscriptions(first: 10, sortKey: ID) {
            nodes {
              id
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

    expect(readDeletedResponse.status).toBe(200);
    expect(readDeletedResponse.body.data).toEqual({
      webhookSubscription: null,
      webhookSubscriptions: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
      webhookSubscriptionsCount: {
        count: 0,
        precision: 'EXACT',
      },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'webhookSubscriptionCreate',
      'webhookSubscriptionUpdate',
      'webhookSubscriptionDelete',
    ]);
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
    ]);
    expect(logResponse.body.entries[0].requestBody).toEqual({
      query: createMutation,
      variables: createVariables,
    });
    expect(logResponse.body.entries[0].stagedResourceIds).toEqual([created.id]);
    expect(logResponse.body.entries[1].requestBody.variables).toEqual(updateVariables);
    expect(logResponse.body.entries[1].stagedResourceIds).toEqual([created.id]);
    expect(logResponse.body.entries[2].requestBody).toEqual({
      query: deleteMutation,
      variables: deleteVariables,
    });
    expect(logResponse.body.entries[2].stagedResourceIds).toEqual([created.id]);

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.webhookSubscriptions[created.id]).toBeUndefined();
    expect(stateResponse.body.stagedState.deletedWebhookSubscriptionIds[created.id]).toBe(true);
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

    const deleteKnownResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteKnownWebhook($id: ID!) {
          webhookSubscriptionDelete(id: $id) {
            deletedWebhookSubscriptionId
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: knownWebhook.id,
        },
      });

    expect(deleteKnownResponse.status).toBe(200);
    expect(deleteKnownResponse.body.data.webhookSubscriptionDelete).toEqual({
      deletedWebhookSubscriptionId: knownWebhook.id,
      userErrors: [],
    });

    const readDeletedKnownResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query DeletedKnownWebhook($id: ID!) {
          webhookSubscription(id: $id) {
            id
          }
          webhookSubscriptions(first: 10, sortKey: ID) {
            nodes {
              id
            }
          }
          webhookSubscriptionsCount {
            count
            precision
          }
        }`,
        variables: {
          id: knownWebhook.id,
        },
      });

    expect(readDeletedKnownResponse.body.data).toEqual({
      webhookSubscription: null,
      webhookSubscriptions: {
        nodes: [],
      },
      webhookSubscriptionsCount: {
        count: 0,
        precision: 'EXACT',
      },
    });

    const alreadyDeletedResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteAlreadyDeletedWebhook($id: ID!) {
          webhookSubscriptionDelete(id: $id) {
            deletedWebhookSubscriptionId
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: knownWebhook.id,
        },
      });

    expect(alreadyDeletedResponse.status).toBe(200);
    expect(alreadyDeletedResponse.body.data.webhookSubscriptionDelete).toEqual({
      deletedWebhookSubscriptionId: null,
      userErrors: [{ field: ['id'], message: 'Webhook subscription does not exist' }],
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
          deleteUnknown: webhookSubscriptionDelete(id: $unknownId) {
            deletedWebhookSubscriptionId
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
      deleteUnknown: {
        deletedWebhookSubscriptionId: null,
        userErrors: [{ field: ['id'], message: 'Webhook subscription does not exist' }],
      },
      createMissingUri: {
        webhookSubscription: null,
        userErrors: [{ field: ['webhookSubscription', 'callbackUrl'], message: "Address can't be blank" }],
      },
    });

    const missingVariableResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation MissingDeleteWebhookVariable($id: ID!) {
          webhookSubscriptionDelete(id: $id) {
            deletedWebhookSubscriptionId
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {},
      });

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

    const nullIdResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation NullDeleteWebhook {
          webhookSubscriptionDelete(id: null) {
            deletedWebhookSubscriptionId
            userErrors {
              field
              message
            }
          }
        }`,
      });

    expect(nullIdResponse.body).toEqual({
      errors: [
        {
          message:
            "Argument 'id' on Field 'webhookSubscriptionDelete' has an invalid value (null). Expected type 'ID!'.",
          path: ['mutation', 'webhookSubscriptionDelete', 'id'],
          extensions: {
            code: 'argumentLiteralsIncompatible',
            typeName: 'Field',
            argumentName: 'id',
          },
        },
      ],
    });

    const missingArgumentResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation MissingDeleteWebhookArgument {
          webhookSubscriptionDelete {
            deletedWebhookSubscriptionId
            userErrors {
              field
              message
            }
          }
        }`,
      });

    expect(missingArgumentResponse.body).toEqual({
      errors: [
        {
          message: "Field 'webhookSubscriptionDelete' is missing required arguments: id",
          path: ['mutation', 'webhookSubscriptionDelete'],
          extensions: {
            code: 'missingRequiredArguments',
            className: 'Field',
            name: 'webhookSubscriptionDelete',
            arguments: 'id',
          },
        },
      ],
    });

    const missingCreateTopicResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation MissingCreateWebhookTopic {
          webhookSubscriptionCreate(webhookSubscription: { uri: "https://example.com/no-topic" }) {
            webhookSubscription {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
      });

    expect(missingCreateTopicResponse.body).toEqual({
      errors: [
        {
          message: "Field 'webhookSubscriptionCreate' is missing required arguments: topic",
          path: ['mutation', 'webhookSubscriptionCreate'],
          extensions: {
            code: 'missingRequiredArguments',
            className: 'Field',
            name: 'webhookSubscriptionCreate',
            arguments: 'topic',
          },
        },
      ],
    });

    const nullUpdateInputResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation NullUpdateWebhookInput($id: ID!) {
          webhookSubscriptionUpdate(id: $id, webhookSubscription: null) {
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
          id: 'gid://shopify/WebhookSubscription/999999999999',
        },
      });

    expect(nullUpdateInputResponse.body).toEqual({
      errors: [
        {
          message:
            "Argument 'webhookSubscription' on Field 'webhookSubscriptionUpdate' has an invalid value (null). Expected type 'WebhookSubscriptionInput!'.",
          path: ['mutation', 'webhookSubscriptionUpdate', 'webhookSubscription'],
          extensions: {
            code: 'argumentLiteralsIncompatible',
            typeName: 'Field',
            argumentName: 'webhookSubscription',
          },
        },
      ],
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries).toEqual([]);
    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.webhookSubscriptions).toEqual({});
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
