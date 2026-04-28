import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import type { AbandonedCheckoutRecord, AbandonmentRecord } from '../../src/state/types.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const liveHybridConfig: AppConfig = {
  ...snapshotConfig,
  readMode: 'live-hybrid',
};

function makeAbandonedCheckout(overrides: Partial<AbandonedCheckoutRecord> = {}): AbandonedCheckoutRecord {
  return {
    id: 'gid://shopify/AbandonedCheckout/1001',
    cursor: 'opaque-abandoned-checkout-cursor',
    data: {
      __typename: 'AbandonedCheckout',
      id: 'gid://shopify/AbandonedCheckout/1001',
      name: '#AC1001',
      abandonedCheckoutUrl: 'https://example.myshopify.com/checkouts/recover/1001',
      completedAt: null,
      createdAt: '2026-04-20T10:00:00Z',
      updatedAt: '2026-04-20T10:30:00Z',
      totalPriceSet: {
        shopMoney: {
          amount: '42.5',
          currencyCode: 'CAD',
        },
      },
      lineItems: [
        {
          __typename: 'AbandonedCheckoutLineItem',
          id: 'gid://shopify/AbandonedCheckoutLineItem/1',
          title: 'Recovery candle',
          quantity: 2,
        },
      ],
    },
    ...overrides,
  };
}

function makeRecoveredAbandonedCheckout(): AbandonedCheckoutRecord {
  return makeAbandonedCheckout({
    id: 'gid://shopify/AbandonedCheckout/1002',
    cursor: 'opaque-abandoned-checkout-cursor-2',
    data: {
      __typename: 'AbandonedCheckout',
      id: 'gid://shopify/AbandonedCheckout/1002',
      name: '#AC1002',
      abandonedCheckoutUrl: 'https://example.myshopify.com/checkouts/recover/1002',
      completedAt: '2026-04-21T12:00:00Z',
      createdAt: '2026-04-21T10:00:00Z',
      updatedAt: '2026-04-21T12:00:00Z',
      email: 'recovered@example.com',
      customer: {
        id: 'gid://shopify/Customer/1002',
        email: 'recovered@example.com',
      },
      totalPriceSet: {
        shopMoney: {
          amount: '99.95',
          currencyCode: 'CAD',
        },
      },
      lineItems: [
        {
          __typename: 'AbandonedCheckoutLineItem',
          id: 'gid://shopify/AbandonedCheckoutLineItem/2',
          title: 'Recovered lantern',
          quantity: 1,
        },
      ],
    },
  });
}

function makeAbandonment(overrides: Partial<AbandonmentRecord> = {}): AbandonmentRecord {
  return {
    id: 'gid://shopify/Abandonment/2001',
    abandonedCheckoutId: 'gid://shopify/AbandonedCheckout/1001',
    data: {
      __typename: 'Abandonment',
      id: 'gid://shopify/Abandonment/2001',
      abandonmentType: 'CHECKOUT',
      mostRecentStep: 'CHECKOUT',
      createdAt: '2026-04-20T10:35:00Z',
      emailState: 'NOT_SENT',
      emailSentAt: null,
      customerHasNoDraftOrderSinceAbandonment: true,
      customerHasNoOrderSinceAbandonment: true,
      inventoryAvailable: true,
      isFromCustomStorefront: false,
      isFromOnlineStore: true,
      isFromShopApp: false,
      isFromShopPay: false,
      isMostSignificantAbandonment: true,
      abandonedCheckoutPayload: {
        id: 'gid://shopify/AbandonedCheckout/1001',
      },
      productsAddedToCart: [
        {
          __typename: 'CustomerVisitProductInfo',
          id: 'gid://shopify/CustomerVisitProductInfo/1',
          title: 'Recovery candle',
        },
      ],
      productsViewed: [],
    },
    deliveryActivities: {},
    ...overrides,
  };
}

function makeRecoveredAbandonment(): AbandonmentRecord {
  return makeAbandonment({
    id: 'gid://shopify/Abandonment/2002',
    abandonedCheckoutId: 'gid://shopify/AbandonedCheckout/1002',
    data: {
      __typename: 'Abandonment',
      id: 'gid://shopify/Abandonment/2002',
      abandonmentType: 'CHECKOUT',
      mostRecentStep: 'CHECKOUT',
      createdAt: '2026-04-21T10:35:00Z',
      emailState: 'SENT',
      emailSentAt: '2026-04-21T11:00:00Z',
      abandonedCheckoutPayload: {
        id: 'gid://shopify/AbandonedCheckout/1002',
      },
      productsAddedToCart: [
        {
          __typename: 'CustomerVisitProductInfo',
          id: 'gid://shopify/CustomerVisitProductInfo/2',
          title: 'Recovered lantern',
        },
      ],
      productsViewed: [],
    },
    deliveryActivities: {},
  });
}

describe('abandoned checkout and abandonment query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns captured Shopify-like empty/no-data roots in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('snapshot abandoned checkout reads must not hit upstream');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query AbandonedCheckoutEmpty($abandonmentId: ID!, $checkoutId: ID!, $first: Int!) {
          abandonedCheckouts(first: $first, sortKey: CREATED_AT, reverse: true) {
            nodes { id name }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          abandonedCheckoutsCount { count precision }
          abandonment(id: $abandonmentId) { id }
          abandonmentByAbandonedCheckoutId(abandonedCheckoutId: $checkoutId) { id }
        }`,
        variables: {
          abandonmentId: 'gid://shopify/Abandonment/0',
          checkoutId: 'gid://shopify/AbandonedCheckout/0',
          first: 2,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        abandonedCheckouts: {
          nodes: [],
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        abandonedCheckoutsCount: {
          count: 0,
          precision: 'EXACT',
        },
        abandonment: null,
        abandonmentByAbandonedCheckoutId: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('projects seeded non-empty abandoned checkout and abandonment records through requested selections', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('snapshot seeded abandoned checkout reads must not hit upstream');
    });
    store.upsertBaseAbandonedCheckouts([makeAbandonedCheckout()]);
    store.upsertBaseAbandonments([makeAbandonment()]);

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query AbandonedCheckoutSeeded($abandonmentId: ID!, $checkoutId: ID!) {
          abandonedCheckouts(first: 1) {
            edges {
              cursor
              node {
                __typename
                id
                name
                totalPriceSet { shopMoney { amount currencyCode } }
                lineItems(first: 1) {
                  nodes { id title quantity }
                  pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          abandonedCheckoutsCount(limit: 0) { count precision }
          abandonment(id: $abandonmentId) {
            id
            abandonmentType
            emailState
            abandonedCheckoutPayload { id name }
            productsAddedToCart(first: 1) { nodes { id title } }
          }
          abandonmentByAbandonedCheckoutId(abandonedCheckoutId: $checkoutId) { id emailState }
        }`,
        variables: {
          abandonmentId: 'gid://shopify/Abandonment/2001',
          checkoutId: 'gid://shopify/AbandonedCheckout/1001',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.abandonedCheckouts.edges).toEqual([
      {
        cursor: 'opaque-abandoned-checkout-cursor',
        node: {
          __typename: 'AbandonedCheckout',
          id: 'gid://shopify/AbandonedCheckout/1001',
          name: '#AC1001',
          totalPriceSet: {
            shopMoney: {
              amount: '42.5',
              currencyCode: 'CAD',
            },
          },
          lineItems: {
            nodes: [
              {
                id: 'gid://shopify/AbandonedCheckoutLineItem/1',
                title: 'Recovery candle',
                quantity: 2,
              },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: 'cursor:gid://shopify/AbandonedCheckoutLineItem/1',
              endCursor: 'cursor:gid://shopify/AbandonedCheckoutLineItem/1',
            },
          },
        },
      },
    ]);
    expect(response.body.data.abandonedCheckouts.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: 'opaque-abandoned-checkout-cursor',
      endCursor: 'opaque-abandoned-checkout-cursor',
    });
    expect(response.body.data.abandonedCheckoutsCount).toEqual({ count: 0, precision: 'AT_LEAST' });
    expect(response.body.data.abandonment).toMatchObject({
      id: 'gid://shopify/Abandonment/2001',
      abandonmentType: 'CHECKOUT',
      emailState: 'NOT_SENT',
      abandonedCheckoutPayload: {
        id: 'gid://shopify/AbandonedCheckout/1001',
        name: '#AC1001',
      },
      productsAddedToCart: {
        nodes: [{ id: 'gid://shopify/CustomerVisitProductInfo/1', title: 'Recovery candle' }],
      },
    });
    expect(response.body.data.abandonmentByAbandonedCheckoutId).toEqual({
      id: 'gid://shopify/Abandonment/2001',
      emailState: 'NOT_SENT',
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('filters seeded abandoned checkouts with documented search terms in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('snapshot abandoned checkout query filters must not hit upstream');
    });
    store.upsertBaseAbandonedCheckouts([makeAbandonedCheckout(), makeRecoveredAbandonedCheckout()]);
    store.upsertBaseAbandonments([makeAbandonment(), makeRecoveredAbandonment()]);

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query AbandonedCheckoutSearchFilters {
          open: abandonedCheckouts(first: 5, query: "status:open") {
            nodes { id name }
          }
          recovered: abandonedCheckouts(first: 5, query: "recovery_state:recovered email_state:sent") {
            nodes { id name }
          }
          defaultText: abandonedCheckouts(first: 5, query: "lantern") {
            nodes { id name }
          }
          idRange: abandonedCheckouts(first: 5, query: "id:>=1002") {
            nodes { id name }
          }
          recentCount: abandonedCheckoutsCount(query: "created_at:>=2026-04-21") {
            count
            precision
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        open: {
          nodes: [{ id: 'gid://shopify/AbandonedCheckout/1001', name: '#AC1001' }],
        },
        recovered: {
          nodes: [{ id: 'gid://shopify/AbandonedCheckout/1002', name: '#AC1002' }],
        },
        defaultText: {
          nodes: [{ id: 'gid://shopify/AbandonedCheckout/1002', name: '#AC1002' }],
        },
        idRange: {
          nodes: [{ id: 'gid://shopify/AbandonedCheckout/1002', name: '#AC1002' }],
        },
        recentCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages abandonment delivery status updates locally and preserves raw mutation order', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('abandonment delivery status updates must not hit upstream');
    });
    store.upsertBaseAbandonedCheckouts([makeAbandonedCheckout()]);
    store.upsertBaseAbandonments([makeAbandonment()]);

    const app = createApp(liveHybridConfig).callback();
    const unknownResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UnknownAbandonment($abandonmentId: ID!, $marketingActivityId: ID!) {
          abandonmentUpdateActivitiesDeliveryStatuses(
            abandonmentId: $abandonmentId
            marketingActivityId: $marketingActivityId
            deliveryStatus: SENT
          ) {
            abandonment { id }
            userErrors { field message }
          }
        }`,
        variables: {
          abandonmentId: 'gid://shopify/Abandonment/0',
          marketingActivityId: 'gid://shopify/MarketingActivity/0',
        },
      });

    const stagedResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        operationName: 'StageDeliveryStatus',
        query: `mutation StageDeliveryStatus($abandonmentId: ID!, $marketingActivityId: ID!, $deliveredAt: DateTime!) {
          abandonmentUpdateActivitiesDeliveryStatuses(
            abandonmentId: $abandonmentId
            marketingActivityId: $marketingActivityId
            deliveryStatus: SENT
            deliveredAt: $deliveredAt
            deliveryStatusChangeReason: "HAR-300 local proof"
          ) {
            abandonment { id emailState emailSentAt }
            userErrors { field message }
          }
        }`,
        variables: {
          abandonmentId: 'gid://shopify/Abandonment/2001',
          marketingActivityId: 'gid://shopify/MarketingActivity/3001',
          deliveredAt: '2026-04-27T00:00:00Z',
        },
      });

    expect(unknownResponse.status).toBe(200);
    expect(unknownResponse.body.data.abandonmentUpdateActivitiesDeliveryStatuses).toEqual({
      abandonment: null,
      userErrors: [{ field: ['abandonmentId'], message: 'abandonment_not_found' }],
    });
    expect(stagedResponse.status).toBe(200);
    expect(stagedResponse.body.data.abandonmentUpdateActivitiesDeliveryStatuses).toEqual({
      abandonment: {
        id: 'gid://shopify/Abandonment/2001',
        emailState: 'SENT',
        emailSentAt: '2026-04-27T00:00:00Z',
      },
      userErrors: [],
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(
      (logResponse.body.entries as Array<{ requestBody: { query: string }; operationName: string | null }>).map(
        (entry) => ({
          operationName: entry.operationName,
          query: entry.requestBody.query,
        }),
      ),
    ).toEqual([
      {
        operationName: 'abandonmentUpdateActivitiesDeliveryStatuses',
        query: expect.stringContaining('UnknownAbandonment'),
      },
      {
        operationName: 'abandonmentUpdateActivitiesDeliveryStatuses',
        query: expect.stringContaining('StageDeliveryStatus'),
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
