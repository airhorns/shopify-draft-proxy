import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

import request from 'supertest';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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

function webhookSubscription(
  id: string,
  overrides: Partial<WebhookSubscriptionRecord> = {},
): WebhookSubscriptionRecord {
  return {
    id,
    topic: 'SHOP_UPDATE',
    format: 'JSON',
    includeFields: ['id', 'name'],
    metafieldNamespaces: ['custom'],
    filter: '',
    createdAt: '2026-04-25T21:46:44Z',
    updatedAt: '2026-04-25T21:46:44Z',
    endpoint: {
      __typename: 'WebhookHttpEndpoint',
      callbackUrl: `https://example.com/webhooks/${id.split('/').at(-1)}`,
    },
    ...overrides,
  };
}

function minimalSnapshotBaseState(extra: Record<string, unknown>): Record<string, unknown> {
  return {
    products: {},
    productVariants: {},
    productOptions: {},
    collections: {},
    customers: {},
    productCollections: {},
    productMedia: {},
    productMetafields: {},
    deletedProductIds: {},
    deletedCollectionIds: {},
    deletedCustomerIds: {},
    ...extra,
  };
}

describe('webhook subscription query shapes', () => {
  let tempDir: string | null = null;

  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  afterEach(() => {
    if (tempDir) {
      rmSync(tempDir, { recursive: true, force: true });
      tempDir = null;
    }
  });

  it('returns Shopify-like empty webhook subscription shapes in snapshot mode', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('empty webhook snapshot reads must not fetch upstream'));
    const app = createApp(config);

    const response = await request(app.callback())
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query EmptyWebhookSubscriptions($id: ID!, $missingQuery: String!) {
          webhookSubscription(id: $id) {
            id
            topic
            format
          }
          webhookSubscriptions(first: 2, sortKey: ID) {
            nodes {
              id
            }
            edges {
              cursor
              node {
                id
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
          filteredCount: webhookSubscriptionsCount(query: $missingQuery, limit: 10) {
            count
            precision
          }
        }`,
        variables: {
          id: 'gid://shopify/WebhookSubscription/999999999999',
          missingQuery: 'id:999999999999',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        webhookSubscription: null,
        webhookSubscriptions: {
          nodes: [],
          edges: [],
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
        filteredCount: {
          count: 0,
          precision: 'EXACT',
        },
      },
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('loads webhook subscription records from normalized snapshot fixtures', async () => {
    tempDir = mkdtempSync(path.join(tmpdir(), 'shopify-draft-proxy-webhook-snapshot-'));
    const snapshotPath = path.join(tempDir, 'normalized-snapshot.json');
    const snapshotWebhook = webhookSubscription('gid://shopify/WebhookSubscription/1001');
    writeFileSync(
      snapshotPath,
      JSON.stringify(
        {
          kind: 'normalized-state-snapshot',
          baseState: minimalSnapshotBaseState({
            webhookSubscriptions: {
              [snapshotWebhook.id]: snapshotWebhook,
            },
            webhookSubscriptionOrder: [snapshotWebhook.id],
          }),
        },
        null,
        2,
      ),
    );
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(
      new Error('snapshot fixture webhook reads must not fetch upstream'),
    );
    const app = createApp({ ...config, snapshotPath });

    const response = await request(app.callback())
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query SnapshotWebhookSubscription($id: ID!) {
          webhookSubscription(id: $id) {
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
          webhookSubscriptions(first: 5, sortKey: ID) {
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
          id: snapshotWebhook.id,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.webhookSubscription).toEqual(snapshotWebhook);
    expect(response.body.data.webhookSubscriptions).toEqual({
      nodes: [{ id: snapshotWebhook.id }],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: `cursor:${snapshotWebhook.id}`,
        endCursor: `cursor:${snapshotWebhook.id}`,
      },
    });
    expect(response.body.data.webhookSubscriptionsCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('paginates, reverses, counts, and overlays staged webhook subscription records', async () => {
    const baseWebhook = webhookSubscription('gid://shopify/WebhookSubscription/1001', {
      topic: 'APP_UNINSTALLED',
      includeFields: [],
      metafieldNamespaces: [],
      endpoint: {
        __typename: 'WebhookEventBridgeEndpoint',
        arn: 'arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/example',
      },
    });
    const stagedWebhook = webhookSubscription('gid://shopify/WebhookSubscription/1002', {
      includeFields: ['id'],
      metafieldNamespaces: [],
      endpoint: {
        __typename: 'WebhookPubSubEndpoint',
        pubSubProject: 'hermes-project',
        pubSubTopic: 'shop-updates',
      },
    });
    store.upsertBaseWebhookSubscriptions([baseWebhook]);
    store.upsertStagedWebhookSubscription(stagedWebhook);
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('local webhook overlay reads must not fetch upstream'));
    const app = createApp(config);

    const firstPage = await request(app.callback())
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query WebhookSubscriptionPage($id: ID!) {
          webhookSubscription(id: $id) {
            id
            endpoint {
              __typename
              ... on WebhookPubSubEndpoint {
                pubSubProject
                pubSubTopic
              }
            }
          }
          webhookSubscriptions(first: 1, sortKey: ID) {
            nodes {
              id
              topic
            }
            edges {
              cursor
              node {
                id
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          limitedCount: webhookSubscriptionsCount(limit: 1) {
            count
            precision
          }
          missingCount: webhookSubscriptionsCount(query: "id:999999999999") {
            count
            precision
          }
        }`,
        variables: {
          id: stagedWebhook.id,
        },
      });

    expect(firstPage.status).toBe(200);
    expect(firstPage.body.data.webhookSubscription).toEqual({
      id: stagedWebhook.id,
      endpoint: {
        __typename: 'WebhookPubSubEndpoint',
        pubSubProject: 'hermes-project',
        pubSubTopic: 'shop-updates',
      },
    });
    expect(firstPage.body.data.webhookSubscriptions).toEqual({
      nodes: [{ id: baseWebhook.id, topic: 'APP_UNINSTALLED' }],
      edges: [{ cursor: `cursor:${baseWebhook.id}`, node: { id: baseWebhook.id } }],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: false,
        startCursor: `cursor:${baseWebhook.id}`,
        endCursor: `cursor:${baseWebhook.id}`,
      },
    });
    expect(firstPage.body.data.limitedCount).toEqual({
      count: 1,
      precision: 'AT_LEAST',
    });
    expect(firstPage.body.data.missingCount).toEqual({
      count: 0,
      precision: 'EXACT',
    });

    const secondPage = await request(app.callback())
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query WebhookSubscriptionNext($after: String!) {
          nextPage: webhookSubscriptions(first: 1, after: $after, sortKey: ID) {
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
          reversePage: webhookSubscriptions(first: 2, sortKey: ID, reverse: true) {
            nodes {
              id
            }
          }
        }`,
        variables: {
          after: firstPage.body.data.webhookSubscriptions.pageInfo.endCursor,
        },
      });

    expect(secondPage.status).toBe(200);
    expect(secondPage.body.data.nextPage).toEqual({
      nodes: [{ id: stagedWebhook.id }],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: true,
        startCursor: `cursor:${stagedWebhook.id}`,
        endCursor: `cursor:${stagedWebhook.id}`,
      },
    });
    expect(secondPage.body.data.reversePage).toEqual({
      nodes: [{ id: stagedWebhook.id }, { id: baseWebhook.id }],
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('projects unified uri values and filters catalog reads by uri, format, and topics', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('local webhook uri filters must not fetch upstream'));
    const httpWebhook = webhookSubscription('gid://shopify/WebhookSubscription/1001', {
      topic: 'SHOP_UPDATE',
      uri: 'https://example.com/webhooks/shop-update',
      name: 'shop_update_http',
      format: 'JSON',
      endpoint: {
        __typename: 'WebhookHttpEndpoint',
        callbackUrl: 'https://example.com/webhooks/shop-update',
      },
    });
    const pubSubWebhook = webhookSubscription('gid://shopify/WebhookSubscription/1002', {
      topic: 'APP_UNINSTALLED',
      uri: 'pubsub://hermes-project:app-uninstalled',
      name: 'app_uninstalled_pubsub',
      format: 'JSON',
      includeFields: [],
      metafieldNamespaces: [],
      endpoint: {
        __typename: 'WebhookPubSubEndpoint',
        pubSubProject: 'hermes-project',
        pubSubTopic: 'app-uninstalled',
      },
    });
    const eventBridgeWebhook = webhookSubscription('gid://shopify/WebhookSubscription/1003', {
      topic: 'ORDERS_CREATE',
      uri: 'arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/123456789/orders',
      name: 'orders_eventbridge',
      format: 'JSON',
      includeFields: ['id'],
      metafieldNamespaces: ['orders'],
      endpoint: {
        __typename: 'WebhookEventBridgeEndpoint',
        arn: 'arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/123456789/orders',
      },
    });
    store.upsertBaseWebhookSubscriptions([httpWebhook, pubSubWebhook, eventBridgeWebhook]);
    const app = createApp(config);

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query WebhookSubscriptionUriFilters($uri: String!, $topics: [WebhookSubscriptionTopic!]) {
          byUri: webhookSubscriptions(first: 5, uri: $uri, sortKey: ID) {
            nodes {
              id
              uri
              name
              endpoint {
                __typename
                ... on WebhookPubSubEndpoint {
                  pubSubProject
                  pubSubTopic
                }
              }
            }
          }
          byFormatAndTopic: webhookSubscriptions(first: 5, format: JSON, topics: $topics, sortKey: ID) {
            nodes {
              id
              topic
              uri
            }
          }
          byEndpointQuery: webhookSubscriptions(first: 5, query: "endpoint:aws.partner/shopify.com/123456789/orders", sortKey: ID) {
            nodes {
              id
              uri
            }
          }
          callbackUrlAlias: webhookSubscriptions(first: 5, callbackUrl: "https://example.com/webhooks/shop-update", sortKey: ID) {
            nodes {
              id
              uri
            }
          }
          endpointCount: webhookSubscriptionsCount(query: "endpoint:hermes-project") {
            count
            precision
          }
        }`,
        variables: {
          uri: pubSubWebhook.uri,
          topics: ['SHOP_UPDATE', 'ORDERS_CREATE'],
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.byUri.nodes).toEqual([
      {
        id: pubSubWebhook.id,
        uri: 'pubsub://hermes-project:app-uninstalled',
        name: 'app_uninstalled_pubsub',
        endpoint: {
          __typename: 'WebhookPubSubEndpoint',
          pubSubProject: 'hermes-project',
          pubSubTopic: 'app-uninstalled',
        },
      },
    ]);
    expect(response.body.data.byFormatAndTopic.nodes).toEqual([
      { id: httpWebhook.id, topic: 'SHOP_UPDATE', uri: 'https://example.com/webhooks/shop-update' },
      {
        id: eventBridgeWebhook.id,
        topic: 'ORDERS_CREATE',
        uri: 'arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/123456789/orders',
      },
    ]);
    expect(response.body.data.byEndpointQuery.nodes).toEqual([
      {
        id: eventBridgeWebhook.id,
        uri: 'arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/123456789/orders',
      },
    ]);
    expect(response.body.data.callbackUrlAlias.nodes).toEqual([
      { id: httpWebhook.id, uri: 'https://example.com/webhooks/shop-update' },
    ]);
    expect(response.body.data.endpointCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });
});
