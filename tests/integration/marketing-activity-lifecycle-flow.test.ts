import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const activitySelection = `
  id
  title
  createdAt
  updatedAt
  status
  statusLabel
  tactic
  marketingChannelType
  sourceAndMedium
  isExternal
  inMainWorkflowVersion
  urlParameterValue
  utmParameters {
    campaign
    source
    medium
  }
  marketingEvent {
    id
    type
    remoteId
    startedAt
    endedAt
    manageUrl
    previewUrl
    utmCampaign
    utmMedium
    utmSource
    description
    marketingChannelType
    sourceAndMedium
  }
`;

describe('marketing activity external lifecycle flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages create, update, upsert, delete, and delete-all with downstream marketing reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('marketing mutations stay local'));
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CreateMarketing($input: MarketingActivityCreateExternalInput!) {
          marketingActivityCreateExternal(input: $input) {
            marketingActivity {
              ${activitySelection}
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            title: 'HAR-213 Spring Campaign',
            remoteId: 'har-213-create',
            status: 'ACTIVE',
            remoteUrl: 'https://example.com/har-213-create',
            tactic: 'NEWSLETTER',
            marketingChannelType: 'EMAIL',
            utm: {
              campaign: 'har-213-create',
              source: 'newsletter',
              medium: 'email',
            },
          },
        },
      });

    expect(createResponse.status).toBe(200);
    const createdActivity = createResponse.body.data.marketingActivityCreateExternal.marketingActivity;
    expect(createdActivity).toMatchObject({
      id: 'gid://shopify/MarketingActivity/1',
      title: 'HAR-213 Spring Campaign',
      createdAt: '2024-01-01T00:00:00.000Z',
      updatedAt: '2024-01-01T00:00:00.000Z',
      status: 'ACTIVE',
      statusLabel: 'Sending',
      tactic: 'NEWSLETTER',
      marketingChannelType: 'EMAIL',
      sourceAndMedium: 'Email newsletter',
      isExternal: true,
      inMainWorkflowVersion: false,
      utmParameters: {
        campaign: 'har-213-create',
        source: 'newsletter',
        medium: 'email',
      },
      marketingEvent: {
        id: 'gid://shopify/MarketingEvent/2',
        remoteId: 'har-213-create',
        manageUrl: 'https://example.com/har-213-create',
        description: 'HAR-213 Spring Campaign',
      },
    });
    expect(createResponse.body.data.marketingActivityCreateExternal.userErrors).toEqual([]);

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UpdateMarketing($remoteId: String!, $input: MarketingActivityUpdateExternalInput!) {
          marketingActivityUpdateExternal(remoteId: $remoteId, input: $input) {
            marketingActivity {
              ${activitySelection}
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          remoteId: 'har-213-create',
          input: {
            title: 'HAR-213 Spring Campaign Paused',
            status: 'PAUSED',
            remoteUrl: 'https://example.com/har-213-create-paused',
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.marketingActivityUpdateExternal).toMatchObject({
      marketingActivity: {
        id: createdActivity.id,
        title: 'HAR-213 Spring Campaign Paused',
        updatedAt: '2024-01-01T00:00:02.000Z',
        status: 'PAUSED',
        statusLabel: 'Paused',
        marketingEvent: {
          id: createdActivity.marketingEvent.id,
          manageUrl: 'https://example.com/har-213-create-paused',
          description: 'HAR-213 Spring Campaign Paused',
        },
      },
      userErrors: [],
    });

    const upsertCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UpsertMarketing($input: MarketingActivityUpsertExternalInput!) {
          marketingActivityUpsertExternal(input: $input) {
            marketingActivity {
              ${activitySelection}
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            title: 'HAR-213 Upsert Campaign',
            remoteId: 'har-213-upsert',
            status: 'ACTIVE',
            remoteUrl: 'https://example.com/har-213-upsert',
            tactic: 'NEWSLETTER',
            marketingChannelType: 'EMAIL',
            utm: {
              campaign: 'har-213-upsert',
              source: 'newsletter',
              medium: 'email',
            },
          },
        },
      });

    expect(upsertCreateResponse.status).toBe(200);
    const upsertActivity = upsertCreateResponse.body.data.marketingActivityUpsertExternal.marketingActivity;
    expect(upsertActivity).toMatchObject({
      id: 'gid://shopify/MarketingActivity/5',
      status: 'ACTIVE',
      marketingEvent: {
        id: 'gid://shopify/MarketingEvent/6',
        remoteId: 'har-213-upsert',
      },
    });

    const upsertUpdateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UpsertMarketing($input: MarketingActivityUpsertExternalInput!) {
          marketingActivityUpsertExternal(input: $input) {
            marketingActivity {
              ${activitySelection}
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            title: 'HAR-213 Upsert Campaign Sent',
            remoteId: 'har-213-upsert',
            status: 'INACTIVE',
            remoteUrl: 'https://example.com/har-213-upsert-sent',
            tactic: 'NEWSLETTER',
            marketingChannelType: 'EMAIL',
            utm: {
              campaign: 'har-213-upsert',
              source: 'newsletter',
              medium: 'email',
            },
          },
        },
      });

    expect(upsertUpdateResponse.status).toBe(200);
    expect(upsertUpdateResponse.body.data.marketingActivityUpsertExternal).toMatchObject({
      marketingActivity: {
        id: upsertActivity.id,
        title: 'HAR-213 Upsert Campaign Sent',
        status: 'INACTIVE',
        statusLabel: 'Sent',
        marketingEvent: {
          id: upsertActivity.marketingEvent.id,
          endedAt: '2024-01-01T00:00:06.000Z',
          manageUrl: 'https://example.com/har-213-upsert-sent',
        },
      },
      userErrors: [],
    });

    const readBeforeDelete = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query ReadMarketing($activityId: ID!, $eventId: ID!, $remoteIds: [String!]) {
          marketingActivity(id: $activityId) {
            id
            title
            status
            marketingEvent { id remoteId manageUrl }
          }
          marketingActivities(first: 10, remoteIds: $remoteIds, sortKey: ID) {
            nodes { id title status marketingEvent { remoteId } }
          }
          marketingEvent(id: $eventId) {
            id
            remoteId
            description
            manageUrl
          }
        }`,
        variables: {
          activityId: createdActivity.id,
          eventId: createdActivity.marketingEvent.id,
          remoteIds: ['har-213-create', 'har-213-upsert'],
        },
      });

    expect(readBeforeDelete.status).toBe(200);
    expect(readBeforeDelete.body.data.marketingActivity).toMatchObject({
      id: createdActivity.id,
      title: 'HAR-213 Spring Campaign Paused',
      status: 'PAUSED',
      marketingEvent: {
        remoteId: 'har-213-create',
        manageUrl: 'https://example.com/har-213-create-paused',
      },
    });
    expect(readBeforeDelete.body.data.marketingActivities.nodes).toEqual([
      {
        id: createdActivity.id,
        title: 'HAR-213 Spring Campaign Paused',
        status: 'PAUSED',
        marketingEvent: { remoteId: 'har-213-create' },
      },
      {
        id: upsertActivity.id,
        title: 'HAR-213 Upsert Campaign Sent',
        status: 'INACTIVE',
        marketingEvent: { remoteId: 'har-213-upsert' },
      },
    ]);
    expect(readBeforeDelete.body.data.marketingEvent).toEqual({
      id: createdActivity.marketingEvent.id,
      remoteId: 'har-213-create',
      description: 'HAR-213 Spring Campaign Paused',
      manageUrl: 'https://example.com/har-213-create-paused',
    });

    const stateBeforeDelete = await request(app).get('/__meta/state');
    expect(stateBeforeDelete.body.stagedState.marketingActivities[createdActivity.id].data.title).toBe(
      'HAR-213 Spring Campaign Paused',
    );
    expect(stateBeforeDelete.body.stagedState.marketingActivities[upsertActivity.id].data.status).toBe('INACTIVE');
    expect(stateBeforeDelete.body.stagedState.marketingEvents[upsertActivity.marketingEvent.id].data.description).toBe(
      'HAR-213 Upsert Campaign Sent',
    );

    const deleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DeleteMarketing($remoteId: String) {
          marketingActivityDeleteExternal(remoteId: $remoteId) {
            deletedMarketingActivityId
            userErrors { field message code }
          }
        }`,
        variables: {
          remoteId: 'har-213-create',
        },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.marketingActivityDeleteExternal).toEqual({
      deletedMarketingActivityId: createdActivity.id,
      userErrors: [],
    });

    const readAfterDelete = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query ReadAfterDelete($activityId: ID!, $remoteIds: [String!]) {
          marketingActivity(id: $activityId) { id }
          marketingActivities(first: 10, remoteIds: $remoteIds, sortKey: ID) {
            nodes { id title }
          }
        }`,
        variables: {
          activityId: createdActivity.id,
          remoteIds: ['har-213-create', 'har-213-upsert'],
        },
      });

    expect(readAfterDelete.body.data.marketingActivity).toBeNull();
    expect(readAfterDelete.body.data.marketingActivities.nodes).toEqual([
      {
        id: upsertActivity.id,
        title: 'HAR-213 Upsert Campaign Sent',
      },
    ]);

    const deleteAllResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DeleteAllMarketing {
          marketingActivitiesDeleteAllExternal {
            job { id done }
            userErrors { field message code }
          }
        }`,
      });

    expect(deleteAllResponse.status).toBe(200);
    expect(deleteAllResponse.body.data.marketingActivitiesDeleteAllExternal).toEqual({
      job: {
        id: 'gid://shopify/Job/10',
        done: false,
      },
      userErrors: [],
    });

    const readAfterDeleteAll = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query ReadAfterDeleteAll($remoteIds: [String!]) {
          marketingActivities(first: 10, remoteIds: $remoteIds) {
            nodes { id }
          }
        }`,
        variables: {
          remoteIds: ['har-213-create', 'har-213-upsert'],
        },
      });

    expect(readAfterDeleteAll.body.data.marketingActivities.nodes).toEqual([]);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'marketingActivityCreateExternal',
      'marketingActivityUpdateExternal',
      'marketingActivityUpsertExternal',
      'marketingActivityUpsertExternal',
      'marketingActivityDeleteExternal',
      'marketingActivitiesDeleteAllExternal',
    ]);
    expect(logResponse.body.entries.every((entry: { status: string }) => entry.status === 'staged')).toBe(true);
    expect(logResponse.body.entries[0].requestBody.variables.input.remoteId).toBe('har-213-create');

    const stateAfterDeleteAll = await request(app).get('/__meta/state');
    expect(stateAfterDeleteAll.body.stagedState.deletedMarketingActivityIds).toMatchObject({
      [createdActivity.id]: true,
      [upsertActivity.id]: true,
    });
    expect(stateAfterDeleteAll.body.stagedState.deletedMarketingEventIds).toMatchObject({
      [createdActivity.marketingEvent.id]: true,
      [upsertActivity.marketingEvent.id]: true,
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns captured userErrors for invalid external lifecycle branches without staging records', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('marketing validation stays local'));
    const app = createApp(config).callback();

    const invalidCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InvalidCreate($input: MarketingActivityCreateExternalInput!) {
          marketingActivityCreateExternal(input: $input) {
            marketingActivity { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            title: 'HAR-213 Invalid Campaign',
            remoteId: 'har-213-invalid',
            status: 'ACTIVE',
            remoteUrl: 'https://example.com/har-213-invalid',
            tactic: 'NEWSLETTER',
            marketingChannelType: 'EMAIL',
          },
        },
      });

    expect(invalidCreateResponse.status).toBe(200);
    expect(invalidCreateResponse.body.data.marketingActivityCreateExternal).toEqual({
      marketingActivity: null,
      userErrors: [
        {
          field: ['input'],
          message: 'Non-hierarchical marketing activities must have UTM parameters or a URL parameter value.',
          code: 'NON_HIERARCHIAL_REQUIRES_UTM_URL_PARAMETER',
        },
      ],
    });

    const missingDeleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation MissingDelete($remoteId: String) {
          marketingActivityDeleteExternal(remoteId: $remoteId) {
            deletedMarketingActivityId
            userErrors { field message code }
          }
        }`,
        variables: {
          remoteId: 'missing-har-213',
        },
      });

    expect(missingDeleteResponse.body.data.marketingActivityDeleteExternal).toEqual({
      deletedMarketingActivityId: null,
      userErrors: [
        {
          field: null,
          message: 'Marketing activity does not exist.',
          code: 'MARKETING_ACTIVITY_DOES_NOT_EXIST',
        },
      ],
    });

    const logResponse = await request(app).get('/__meta/log');
    const stateResponse = await request(app).get('/__meta/state');
    expect(logResponse.body.entries).toEqual([]);
    expect(stateResponse.body.stagedState.marketingActivities).toEqual({});
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
