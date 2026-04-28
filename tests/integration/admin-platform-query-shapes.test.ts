import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { ProductRecord, ShopRecord } from '../../src/state/types.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const passthroughConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'passthrough',
};

function makeProduct(id: string, title: string): ProductRecord {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? null,
    title,
    handle: title.toLowerCase().replace(/\s+/gu, '-'),
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '2025-01-01T00:00:00.000Z',
    updatedAt: '2025-01-01T00:00:00.000Z',
    vendor: null,
    productType: null,
    tags: [],
    totalInventory: 0,
    tracksInventory: false,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: {
      title: null,
      description: null,
    },
    category: null,
  };
}

function makeShop(): ShopRecord {
  return {
    id: 'gid://shopify/Shop/400',
    name: 'Node Test Shop',
    myshopifyDomain: 'node-test-shop.myshopify.com',
    url: 'https://node-test-shop.myshopify.com',
    primaryDomain: {
      id: 'gid://shopify/Domain/400',
      host: 'node-test-shop.myshopify.com',
      url: 'https://node-test-shop.myshopify.com',
      sslEnabled: true,
    },
    contactEmail: 'owner@example.com',
    email: 'owner@example.com',
    currencyCode: 'USD',
    enabledPresentmentCurrencies: ['USD'],
    ianaTimezone: 'America/New_York',
    timezoneAbbreviation: 'EDT',
    timezoneOffset: '-0400',
    timezoneOffsetMinutes: -240,
    taxesIncluded: false,
    taxShipping: false,
    unitSystem: 'IMPERIAL_SYSTEM',
    weightUnit: 'POUNDS',
    shopAddress: {
      id: 'gid://shopify/ShopAddress/400',
      address1: '1 Main Street',
      address2: null,
      city: 'New York',
      company: null,
      coordinatesValidated: false,
      country: 'United States',
      countryCodeV2: 'US',
      formatted: ['1 Main Street', 'New York NY 10001', 'United States'],
      formattedArea: 'New York NY, United States',
      latitude: null,
      longitude: null,
      phone: null,
      province: 'New York',
      provinceCode: 'NY',
      zip: '10001',
    },
    plan: {
      partnerDevelopment: true,
      publicDisplayName: 'Development',
      shopifyPlus: false,
    },
    resourceLimits: {
      locationLimit: 1000,
      maxProductOptions: 3,
      maxProductVariants: 2048,
      redirectLimitReached: false,
    },
    features: {
      avalaraAvatax: false,
      branding: 'SHOPIFY',
      bundles: {
        eligibleForBundles: true,
        ineligibilityReason: null,
        sellsBundles: false,
      },
      captcha: true,
      cartTransform: {
        eligibleOperations: {
          expandOperation: true,
          mergeOperation: true,
          updateOperation: true,
        },
      },
      dynamicRemarketing: false,
      eligibleForSubscriptionMigration: false,
      eligibleForSubscriptions: false,
      giftCards: true,
      harmonizedSystemCode: true,
      legacySubscriptionGatewayEnabled: false,
      liveView: true,
      paypalExpressSubscriptionGatewayStatus: 'DISABLED',
      reports: true,
      sellsSubscriptions: false,
      showMetrics: true,
      storefront: true,
      unifiedMarkets: true,
    },
    paymentSettings: {
      supportedDigitalWallets: [],
    },
    shopPolicies: [],
  };
}

describe('admin platform utility query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves safe utility read roots in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('admin platform utility reads should resolve locally in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query AdminPlatformUtilityReads($ids: [ID!]!, $domainId: ID!, $jobId: ID!) {
          publicApiVersions {
            __typename
            handle
            displayName
            supported
          }
          node(id: "gid://shopify/Product/0") {
            __typename
            id
          }
          nodes(ids: $ids) {
            __typename
            id
          }
          job(id: $jobId) {
            __typename
            id
            done
            query {
              __typename
            }
          }
          domain(id: $domainId) {
            id
            host
            url
            sslEnabled
          }
          backupRegion {
            __typename
            id
            name
            ... on MarketRegionCountry {
              code
            }
          }
          taxonomy {
            categories(first: 2, search: "zzzzzz-no-match-har-315") {
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
          }
        }`,
        variables: {
          ids: ['gid://shopify/Product/0', 'gid://shopify/Job/0', 'gid://shopify/Domain/0'],
          domainId: 'gid://shopify/Domain/0',
          jobId: 'gid://shopify/Job/0',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        publicApiVersions: [
          { __typename: 'ApiVersion', handle: '2025-07', displayName: '2025-07', supported: true },
          { __typename: 'ApiVersion', handle: '2025-10', displayName: '2025-10', supported: true },
          { __typename: 'ApiVersion', handle: '2026-01', displayName: '2026-01', supported: true },
          { __typename: 'ApiVersion', handle: '2026-04', displayName: '2026-04 (Latest)', supported: true },
          {
            __typename: 'ApiVersion',
            handle: '2026-07',
            displayName: '2026-07 (Release candidate)',
            supported: false,
          },
          { __typename: 'ApiVersion', handle: 'unstable', displayName: 'unstable', supported: false },
        ],
        node: null,
        nodes: [null, null, null],
        job: {
          __typename: 'Job',
          id: 'gid://shopify/Job/0',
          done: true,
          query: {
            __typename: 'QueryRoot',
          },
        },
        domain: null,
        backupRegion: {
          __typename: 'MarketRegionCountry',
          id: 'gid://shopify/MarketRegionCountry/4062110417202',
          name: 'Canada',
          code: 'CA',
        },
        taxonomy: {
          categories: {
            nodes: [],
            edges: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: null,
              endCursor: null,
            },
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors captured staff utility access blockers locally in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('staff utility blockers should resolve locally in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query StaffUtilityRead {
          staffMember {
            id
            exists
            active
          }
          staffMembers(first: 1) {
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
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      staffMember: null,
      staffMembers: null,
    });
    expect(response.body.errors).toEqual([
      expect.objectContaining({
        message: expect.stringContaining('Access denied for staffMember field.'),
        path: ['staffMember'],
        extensions: expect.objectContaining({ code: 'ACCESS_DENIED' }),
      }),
      expect.objectContaining({
        message: 'Access denied for staffMembers field.',
        path: ['staffMembers'],
        extensions: expect.objectContaining({ code: 'ACCESS_DENIED' }),
      }),
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('resolves locally modeled Node IDs while preserving missing and unsupported null entries', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('admin platform node reads should resolve locally in snapshot mode');
    });
    store.upsertBaseProducts([makeProduct('gid://shopify/Product/400', 'Node Product')]);
    store.upsertBaseShop(makeShop());

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query NodeResolution($ids: [ID!]!) {
          node(id: "gid://shopify/Product/400") {
            __typename
            ... on Node {
              nodeId: id
            }
            ... on Product {
              title
              handle
            }
          }
          nodes(ids: $ids) {
            __typename
            ... on Node {
              nodeId: id
            }
            ... on Product {
              title
            }
            ... on Domain {
              host
              url
              sslEnabled
            }
          }
        }`,
        variables: {
          ids: [
            'gid://shopify/Product/400',
            'gid://shopify/Domain/400',
            'gid://shopify/Product/404',
            'gid://shopify/Customer/400',
          ],
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        node: {
          __typename: 'Product',
          nodeId: 'gid://shopify/Product/400',
          title: 'Node Product',
          handle: 'node-product',
        },
        nodes: [
          {
            __typename: 'Product',
            nodeId: 'gid://shopify/Product/400',
            title: 'Node Product',
          },
          {
            __typename: 'Domain',
            nodeId: 'gid://shopify/Domain/400',
            host: 'node-test-shop.myshopify.com',
            url: 'https://node-test-shop.myshopify.com',
            sslEnabled: true,
          },
          null,
          null,
        ],
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('keeps Flow utility mutations as unsupported side-effect passthroughs', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { flowTriggerReceive: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(passthroughConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation FlowTriggerReceive {
          flowTriggerReceive(handle: "har-315", payload: "{}") {
            userErrors {
              message
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'FlowTriggerReceive',
      status: 'proxied',
      interpreted: {
        registeredOperation: {
          name: 'flowTriggerReceive',
          domain: 'admin-platform',
          execution: 'stage-locally',
          implemented: false,
        },
        safety: {
          classification: 'unsupported-flow-side-effect-mutation',
          wouldProxyToShopify: true,
        },
      },
      notes:
        'Unsupported Flow utility mutation would be proxied to Shopify. Flow signature generation and trigger delivery require local signing/trigger semantics plus raw commit replay before support.',
    });
  });
});
