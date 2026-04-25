import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const emptyConnection = {
  nodes: [],
  edges: [],
  pageInfo: {
    hasNextPage: false,
    hasPreviousPage: false,
    startCursor: null,
    endCursor: null,
  },
};

function buildActivity(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    __typename: 'MarketingActivity',
    id: 'gid://shopify/MarketingActivity/1001',
    title: 'HAR-212 Spring Campaign',
    createdAt: '2026-04-24T10:00:00Z',
    updatedAt: '2026-04-24T10:30:00Z',
    status: 'ACTIVE',
    statusLabel: 'Active',
    tactic: 'NEWSLETTER',
    marketingChannelType: 'EMAIL',
    sourceAndMedium: 'Email newsletter',
    isExternal: true,
    inMainWorkflowVersion: true,
    app: {
      id: 'gid://shopify/App/1',
      name: 'Hermes Marketing',
    },
    marketingEvent: {
      __typename: 'MarketingEvent',
      id: 'gid://shopify/MarketingEvent/9001',
      type: 'NEWSLETTER',
      remoteId: 'har-212-event',
      startedAt: '2026-04-24T10:00:00Z',
      endedAt: null,
      scheduledToEndAt: null,
      manageUrl: null,
      previewUrl: null,
      utmCampaign: 'har-212',
      utmMedium: 'email',
      utmSource: 'newsletter',
      description: 'HAR-212 spring event',
      marketingChannelType: 'EMAIL',
      sourceAndMedium: 'Email newsletter',
    },
    ...overrides,
  };
}

function buildEvent(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    __typename: 'MarketingEvent',
    id: 'gid://shopify/MarketingEvent/9001',
    type: 'NEWSLETTER',
    remoteId: 'har-212-event',
    startedAt: '2026-04-24T10:00:00Z',
    endedAt: null,
    scheduledToEndAt: null,
    manageUrl: null,
    previewUrl: null,
    utmCampaign: 'har-212',
    utmMedium: 'email',
    utmSource: 'newsletter',
    description: 'HAR-212 spring event',
    marketingChannelType: 'EMAIL',
    sourceAndMedium: 'Email newsletter',
    ...overrides,
  };
}

describe('marketing query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns Shopify-like empty marketing catalogs and null detail reads in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('marketing snapshot reads stay local'));
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query MarketingEmpty($activityId: ID!, $eventId: ID!, $activityQuery: String!, $eventQuery: String!) {
          marketingActivities(first: 2, query: $activityQuery, sortKey: TITLE, reverse: true) {
            nodes {
              id
              title
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
          marketingActivity(id: $activityId) {
            id
            title
          }
          marketingEvents(first: 2, query: $eventQuery, sortKey: ID) {
            nodes {
              id
              type
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
          marketingEvent(id: $eventId) {
            id
            type
          }
        }`,
        variables: {
          activityId: 'gid://shopify/MarketingActivity/999999999999',
          eventId: 'gid://shopify/MarketingEvent/999999999999',
          activityQuery: 'title:__har212_no_activity__',
          eventQuery: 'description:__har212_no_event__',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        marketingActivities: emptyConnection,
        marketingActivity: null,
        marketingEvents: emptyConnection,
        marketingEvent: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('serializes activity/event catalogs, details, filters, cursors, and pageInfo from local marketing state', async () => {
    store.upsertBaseMarketingActivities([
      {
        data: buildActivity({
          id: 'gid://shopify/MarketingActivity/1000',
          title: 'HAR-212 Older Campaign',
          createdAt: '2026-04-23T10:00:00Z',
        }),
        cursor: 'opaque-activity-1000',
      },
      {
        data: buildActivity(),
        cursor: 'opaque-activity-1001',
      },
    ]);
    store.upsertBaseMarketingEvents([
      {
        data: buildEvent(),
        cursor: 'opaque-event-9001',
      },
      {
        data: buildEvent({
          id: 'gid://shopify/MarketingEvent/9002',
          type: 'AD',
          startedAt: '2026-04-25T10:00:00Z',
          description: 'HAR-212 paid event',
        }),
        cursor: 'opaque-event-9002',
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('marketing snapshot reads stay local'));
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query MarketingCatalog($activityId: ID!, $eventId: ID!, $activityQuery: String!, $eventQuery: String!) {
          marketingActivities(first: 1, query: $activityQuery, sortKey: CREATED_AT, reverse: true) {
            nodes {
              id
              title
              createdAt
              status
              statusLabel
              tactic
              marketingChannelType
              sourceAndMedium
              app {
                id
                name
              }
              marketingEvent {
                id
                type
                remoteId
              }
            }
            edges {
              cursor
              node {
                id
                title
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          marketingActivity(id: $activityId) {
            id
            title
            isExternal
            inMainWorkflowVersion
          }
          marketingEvents(first: 1, query: $eventQuery, sortKey: STARTED_AT, reverse: true) {
            nodes {
              id
              type
              remoteId
              startedAt
              endedAt
              scheduledToEndAt
              manageUrl
              previewUrl
              utmCampaign
              utmMedium
              utmSource
              description
              marketingChannelType
              sourceAndMedium
            }
            edges {
              cursor
              node {
                id
                type
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          marketingEvent(id: $eventId) {
            id
            type
            description
          }
        }`,
        variables: {
          activityId: 'gid://shopify/MarketingActivity/1001',
          eventId: 'gid://shopify/MarketingEvent/9002',
          activityQuery: 'title:Campaign',
          eventQuery: 'type:AD',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.marketingActivities).toEqual({
      nodes: [
        {
          id: 'gid://shopify/MarketingActivity/1001',
          title: 'HAR-212 Spring Campaign',
          createdAt: '2026-04-24T10:00:00Z',
          status: 'ACTIVE',
          statusLabel: 'Active',
          tactic: 'NEWSLETTER',
          marketingChannelType: 'EMAIL',
          sourceAndMedium: 'Email newsletter',
          app: {
            id: 'gid://shopify/App/1',
            name: 'Hermes Marketing',
          },
          marketingEvent: {
            id: 'gid://shopify/MarketingEvent/9001',
            type: 'NEWSLETTER',
            remoteId: 'har-212-event',
          },
        },
      ],
      edges: [
        {
          cursor: 'opaque-activity-1001',
          node: {
            id: 'gid://shopify/MarketingActivity/1001',
            title: 'HAR-212 Spring Campaign',
          },
        },
      ],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: false,
        startCursor: 'opaque-activity-1001',
        endCursor: 'opaque-activity-1001',
      },
    });
    expect(response.body.data.marketingActivity).toEqual({
      id: 'gid://shopify/MarketingActivity/1001',
      title: 'HAR-212 Spring Campaign',
      isExternal: true,
      inMainWorkflowVersion: true,
    });
    expect(response.body.data.marketingEvents).toEqual({
      nodes: [
        {
          id: 'gid://shopify/MarketingEvent/9002',
          type: 'AD',
          remoteId: 'har-212-event',
          startedAt: '2026-04-25T10:00:00Z',
          endedAt: null,
          scheduledToEndAt: null,
          manageUrl: null,
          previewUrl: null,
          utmCampaign: 'har-212',
          utmMedium: 'email',
          utmSource: 'newsletter',
          description: 'HAR-212 paid event',
          marketingChannelType: 'EMAIL',
          sourceAndMedium: 'Email newsletter',
        },
      ],
      edges: [
        {
          cursor: 'opaque-event-9002',
          node: {
            id: 'gid://shopify/MarketingEvent/9002',
            type: 'AD',
          },
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'opaque-event-9002',
        endCursor: 'opaque-event-9002',
      },
    });
    expect(response.body.data.marketingEvent).toEqual({
      id: 'gid://shopify/MarketingEvent/9002',
      type: 'AD',
      description: 'HAR-212 paid event',
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
