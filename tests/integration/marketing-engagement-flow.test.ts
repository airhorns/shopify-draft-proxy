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

const engagementSelection = `
  occurredOn
  utcOffset
  isCumulative
  channelHandle
  impressionsCount
  viewsCount
  clicksCount
  uniqueClicksCount
  adSpend {
    amount
    currencyCode
  }
  sales {
    amount
    currencyCode
  }
  orders
  firstTimeCustomers
  returningCustomers
  marketingActivity {
    id
    adSpend {
      amount
      currencyCode
    }
  }
`;

describe('marketing engagement flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages activity-level engagement creates, duplicate replacement, downstream reads, and meta state locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('marketing engagement stays local'));
    const app = createApp(config).callback();

    const createActivity = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CreateMarketing($input: MarketingActivityCreateExternalInput!) {
          marketingActivityCreateExternal(input: $input) {
            marketingActivity { id adSpend { amount currencyCode } marketingEvent { remoteId } }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            title: 'HAR-214 Engagement Campaign',
            remoteId: 'har-214-engagement-campaign',
            status: 'ACTIVE',
            remoteUrl: 'https://example.com/har-214-engagement',
            tactic: 'NEWSLETTER',
            marketingChannelType: 'EMAIL',
            utm: {
              campaign: 'har-214-engagement-campaign',
              source: 'newsletter',
              medium: 'email',
            },
          },
        },
      });

    expect(createActivity.status).toBe(200);
    const activity = createActivity.body.data.marketingActivityCreateExternal.marketingActivity;
    expect(activity.adSpend).toBeNull();

    const createEngagement = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CreateEngagement($remoteId: String!, $engagement: MarketingEngagementInput!) {
          marketingEngagementCreate(remoteId: $remoteId, marketingEngagement: $engagement) {
            marketingEngagement {
              ${engagementSelection}
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          remoteId: 'har-214-engagement-campaign',
          engagement: {
            occurredOn: '2026-04-26',
            impressionsCount: 7,
            viewsCount: 5,
            clicksCount: 2,
            uniqueClicksCount: 1,
            adSpend: { amount: '3.21', currencyCode: 'USD' },
            sales: { amount: '12.34', currencyCode: 'USD' },
            orders: '1.5',
            firstTimeCustomers: '1.0',
            returningCustomers: '0.5',
            isCumulative: false,
            utcOffset: '+00:00',
          },
        },
      });

    expect(createEngagement.status).toBe(200);
    expect(createEngagement.body.data.marketingEngagementCreate).toMatchObject({
      marketingEngagement: {
        occurredOn: '2026-04-26',
        utcOffset: '+00:00',
        isCumulative: false,
        channelHandle: null,
        impressionsCount: 7,
        viewsCount: 5,
        clicksCount: 2,
        uniqueClicksCount: 1,
        adSpend: { amount: '3.21', currencyCode: 'USD' },
        sales: { amount: '12.34', currencyCode: 'USD' },
        orders: '1.5',
        firstTimeCustomers: '1.0',
        returningCustomers: '0.5',
        marketingActivity: {
          id: activity.id,
          adSpend: null,
        },
      },
      userErrors: [],
    });

    const duplicateEngagement = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DuplicateEngagement($activityId: ID!, $engagement: MarketingEngagementInput!) {
          marketingEngagementCreate(marketingActivityId: $activityId, marketingEngagement: $engagement) {
            marketingEngagement {
              occurredOn
              impressionsCount
              clicksCount
              adSpend { amount currencyCode }
              marketingActivity { id adSpend { amount currencyCode } }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          activityId: activity.id,
          engagement: {
            occurredOn: '2026-04-26',
            impressionsCount: 9,
            clicksCount: 4,
            adSpend: { amount: '4.56', currencyCode: 'USD' },
            isCumulative: false,
            utcOffset: '+00:00',
          },
        },
      });

    expect(duplicateEngagement.status).toBe(200);
    expect(duplicateEngagement.body.data.marketingEngagementCreate).toMatchObject({
      marketingEngagement: {
        occurredOn: '2026-04-26',
        impressionsCount: 9,
        clicksCount: 4,
        adSpend: { amount: '4.56', currencyCode: 'USD' },
        marketingActivity: {
          id: activity.id,
          adSpend: null,
        },
      },
      userErrors: [],
    });

    const downstreamRead = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query ReadActivity($id: ID!) {
          marketingActivity(id: $id) {
            id
            adSpend { amount currencyCode }
          }
        }`,
        variables: { id: activity.id },
      });

    expect(downstreamRead.status).toBe(200);
    expect(downstreamRead.body.data.marketingActivity).toEqual({
      id: activity.id,
      adSpend: null,
    });

    const state = await request(app).get('/__meta/state');
    expect(state.status).toBe(200);
    const engagementRecords = Object.values(state.body.stagedState.marketingEngagements) as Array<{
      id: string;
      marketingActivityId: string | null;
      remoteId: string | null;
      occurredOn: string;
      data: Record<string, unknown>;
    }>;
    expect(engagementRecords).toHaveLength(1);
    const engagementRecord = engagementRecords[0]!;
    expect(engagementRecord).toBeDefined();
    expect(engagementRecord).toMatchObject({
      marketingActivityId: activity.id,
      remoteId: 'har-214-engagement-campaign',
      occurredOn: '2026-04-26',
      data: {
        impressionsCount: 9,
        clicksCount: 4,
        adSpend: { amount: '4.56', currencyCode: 'USD' },
      },
    });

    const log = await request(app).get('/__meta/log');
    expect(log.status).toBe(200);
    const engagementLogEntries = log.body.entries.filter(
      (entry: { operationName: string | null }) => entry.operationName === 'marketingEngagementCreate',
    );
    expect(engagementLogEntries).toHaveLength(2);
    expect(engagementLogEntries[0]).toMatchObject({
      status: 'staged',
      path: '/admin/api/2025-01/graphql.json',
      stagedResourceIds: [engagementRecord.id],
    });
    expect(engagementLogEntries[0].requestBody.query).toContain('marketingEngagementCreate');
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('matches captured engagement validation and delete branches', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('marketing engagement stays local'));
    const app = createApp(config).callback();

    const missingIdentifier = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation MissingIdentifier($engagement: MarketingEngagementInput!) {
          marketingEngagementCreate(marketingEngagement: $engagement) {
            marketingEngagement { occurredOn impressionsCount }
            userErrors { field message code }
          }
        }`,
        variables: {
          engagement: {
            occurredOn: '2026-04-26',
            impressionsCount: 7,
            isCumulative: false,
            utcOffset: '+00:00',
          },
        },
      });

    expect(missingIdentifier.body.data.marketingEngagementCreate).toEqual({
      marketingEngagement: null,
      userErrors: [
        {
          field: null,
          message:
            'No identifier found. For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.',
          code: 'INVALID_MARKETING_ENGAGEMENT_ARGUMENT_MISSING',
        },
      ],
    });

    const invalidRemote = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InvalidRemote($remoteId: String!, $engagement: MarketingEngagementInput!) {
          marketingEngagementCreate(remoteId: $remoteId, marketingEngagement: $engagement) {
            marketingEngagement { occurredOn impressionsCount }
            userErrors { field message code }
          }
        }`,
        variables: {
          remoteId: 'missing-har-214',
          engagement: {
            occurredOn: '2026-04-26',
            impressionsCount: 7,
            isCumulative: false,
            utcOffset: '+00:00',
          },
        },
      });

    expect(invalidRemote.body.data.marketingEngagementCreate).toEqual({
      marketingEngagement: null,
      userErrors: [
        { field: null, message: 'Marketing activity does not exist.', code: 'MARKETING_ACTIVITY_DOES_NOT_EXIST' },
      ],
    });

    const invalidChannel = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InvalidChannel($channelHandle: String!, $engagement: MarketingEngagementInput!) {
          marketingEngagementCreate(channelHandle: $channelHandle, marketingEngagement: $engagement) {
            marketingEngagement { occurredOn impressionsCount channelHandle }
            userErrors { field message code }
          }
        }`,
        variables: {
          channelHandle: 'unknown-channel',
          engagement: {
            occurredOn: '2026-04-26',
            impressionsCount: 7,
            isCumulative: false,
            utcOffset: '+00:00',
          },
        },
      });

    expect(invalidChannel.body.data.marketingEngagementCreate).toEqual({
      marketingEngagement: null,
      userErrors: [
        {
          field: ['channelHandle'],
          message: 'The channel handle is not recognized. Please contact your partner manager for more information.',
          code: 'INVALID_CHANNEL_HANDLE',
        },
      ],
    });

    const missingDeleteSelector = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation MissingDeleteSelector {
          marketingEngagementsDelete {
            result
            userErrors { field message code }
          }
        }`,
      });

    expect(missingDeleteSelector.body.data.marketingEngagementsDelete).toEqual({
      result: null,
      userErrors: [
        {
          field: null,
          message:
            'Either the channel_handle or delete_engagements_for_all_channels must be provided when deleting a marketing engagement.',
          code: 'INVALID_DELETE_ENGAGEMENTS_ARGUMENTS',
        },
      ],
    });

    const deleteAll = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DeleteAll {
          marketingEngagementsDelete(deleteEngagementsForAllChannels: true) {
            result
            userErrors { field message code }
          }
        }`,
      });

    expect(deleteAll.body.data.marketingEngagementsDelete).toEqual({
      result: 'Engagement data marked for deletion for 0 channel(s)',
      userErrors: [],
    });

    const log = await request(app).get('/__meta/log');
    expect(log.body.entries.map((entry: { operationName: string | null }) => entry.operationName)).toEqual([
      'marketingEngagementsDelete',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
