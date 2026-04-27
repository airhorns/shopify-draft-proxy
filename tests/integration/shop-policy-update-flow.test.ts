import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import {
  defaultPaymentTermsTemplateOrder,
  defaultPaymentTermsTemplateRecordMap,
  type NormalizedStateSnapshotFile,
  type ShopRecord,
} from '../../src/state/types.js';

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

function makeShop(overrides: Partial<ShopRecord> = {}): ShopRecord {
  return {
    id: 'gid://shopify/Shop/63755419881',
    name: 'very-big-test-store',
    myshopifyDomain: 'very-big-test-store.myshopify.com',
    url: 'https://very-big-test-store.myshopify.com',
    primaryDomain: {
      id: 'gid://shopify/Domain/93049946345',
      host: 'very-big-test-store.myshopify.com',
      url: 'https://very-big-test-store.myshopify.com',
      sslEnabled: true,
    },
    contactEmail: 'shopify@gadget.dev',
    email: 'shopify@gadget.dev',
    currencyCode: 'CAD',
    enabledPresentmentCurrencies: ['CAD'],
    ianaTimezone: 'America/Toronto',
    timezoneAbbreviation: 'EDT',
    timezoneOffset: '-0400',
    timezoneOffsetMinutes: -240,
    taxesIncluded: false,
    taxShipping: false,
    unitSystem: 'METRIC_SYSTEM',
    weightUnit: 'KILOGRAMS',
    shopAddress: {
      id: 'gid://shopify/ShopAddress/63755419881',
      address1: '103 ossington',
      address2: null,
      city: 'Ottawa',
      company: null,
      coordinatesValidated: false,
      country: 'Canada',
      countryCodeV2: 'CA',
      formatted: ['103 ossington', 'Ottawa ON k1s3b7', 'Canada'],
      formattedArea: 'Ottawa ON, Canada',
      latitude: 45.389817,
      longitude: -75.68692920000001,
      phone: '',
      province: 'Ontario',
      provinceCode: 'ON',
      zip: 'k1s3b7',
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
    ...overrides,
  };
}

function writeSnapshot(shop: ShopRecord | null): string {
  const snapshot: NormalizedStateSnapshotFile = {
    kind: 'normalized-state-snapshot',
    baseState: {
      shop,
      products: {},
      productVariants: {},
      productOptions: {},
      productOperations: {},
      collections: {},
      publications: {},
      channels: {},
      locations: {},
      locationOrder: [],
      fulfillmentServices: {},
      fulfillmentServiceOrder: [],
      carrierServices: {},
      carrierServiceOrder: [],
      shippingPackages: {},
      shippingPackageOrder: [],
      giftCards: {},
      giftCardOrder: [],
      giftCardConfiguration: null,
      customers: {},
      customerAddresses: {},
      customerPaymentMethods: {},
      customerAccountPages: {},
      customerAccountPageOrder: [],
      customerDataErasureRequests: {},
      storeCreditAccounts: {},
      storeCreditAccountTransactions: {},
      segments: {},
      customerSegmentMembersQueries: {},
      webhookSubscriptions: {},
      webhookSubscriptionOrder: [],
      marketingActivities: {},
      marketingActivityOrder: [],
      marketingEvents: {},
      marketingEventOrder: [],
      marketingEngagements: {},
      marketingEngagementOrder: [],
      deletedMarketingActivityIds: {},
      deletedMarketingEventIds: {},
      deletedMarketingEngagementIds: {},
      onlineStoreArticles: {},
      onlineStoreArticleOrder: [],
      onlineStoreBlogs: {},
      onlineStoreBlogOrder: [],
      onlineStorePages: {},
      onlineStorePageOrder: [],
      onlineStoreComments: {},
      onlineStoreCommentOrder: [],
      savedSearches: {},
      savedSearchOrder: [],
      bulkOperations: {},
      bulkOperationOrder: [],
      discounts: {},
      discountBulkOperations: {},
      paymentCustomizations: {},
      paymentCustomizationOrder: [],
      paymentTermsTemplates: defaultPaymentTermsTemplateRecordMap,
      paymentTermsTemplateOrder: defaultPaymentTermsTemplateOrder,
      shopifyFunctions: {},
      shopifyFunctionOrder: [],
      validations: {},
      validationOrder: [],
      cartTransforms: {},
      cartTransformOrder: [],
      taxAppConfiguration: null,
      deletedPaymentCustomizationIds: {},
      businessEntities: {},
      businessEntityOrder: [],
      b2bCompanies: {},
      b2bCompanyOrder: [],
      b2bCompanyContacts: {},
      b2bCompanyContactOrder: [],
      b2bCompanyContactRoles: {},
      b2bCompanyContactRoleOrder: [],
      b2bCompanyLocations: {},
      b2bCompanyLocationOrder: [],
      markets: {},
      marketOrder: [],
      webPresences: {},
      webPresenceOrder: [],
      marketLocalizations: {},
      availableLocales: [],
      shopLocales: {},
      translations: {},
      catalogs: {},
      catalogOrder: [],
      priceLists: {},
      priceListOrder: [],
      deliveryProfiles: {},
      deliveryProfileOrder: [],
      sellingPlanGroups: {},
      sellingPlanGroupOrder: [],
      abandonedCheckouts: {},
      abandonedCheckoutOrder: [],
      abandonments: {},
      abandonmentOrder: [],
      productCollections: {},
      productMedia: {},
      files: {},
      productMetafields: {},
      metafieldDefinitions: {},
      metaobjectDefinitions: {},
      metaobjects: {},
      customerMetafields: {},
      deletedProductIds: {},
      deletedFileIds: {},
      deletedCollectionIds: {},
      deletedPublicationIds: {},
      deletedLocationIds: {},
      deletedFulfillmentServiceIds: {},
      deletedCarrierServiceIds: {},
      deletedShippingPackageIds: {},
      deletedGiftCardIds: {},
      deletedCustomerIds: {},
      deletedCustomerAddressIds: {},
      deletedCustomerPaymentMethodIds: {},
      deletedSegmentIds: {},
      deletedWebhookSubscriptionIds: {},
      deletedOnlineStoreArticleIds: {},
      deletedOnlineStoreBlogIds: {},
      deletedOnlineStorePageIds: {},
      deletedOnlineStoreCommentIds: {},
      deletedSavedSearchIds: {},
      deletedDiscountIds: {},
      deletedValidationIds: {},
      deletedCartTransformIds: {},
      deletedMarketIds: {},
      deletedCatalogIds: {},
      deletedPriceListIds: {},
      deletedWebPresenceIds: {},
      deletedShopLocales: {},
      deletedTranslations: {},
      deletedDeliveryProfileIds: {},
      deletedSellingPlanGroupIds: {},
      deletedMetafieldDefinitionIds: {},
      deletedMetaobjectDefinitionIds: {},
      deletedMetaobjectIds: {},
      mergedCustomerIds: {},
      customerMergeRequests: {},
    },
  };
  const snapshotPath = join(mkdtempSync(join(tmpdir(), 'shopify-draft-proxy-shop-policy-')), 'snapshot.json');
  writeFileSync(snapshotPath, `${JSON.stringify(snapshot, null, 2)}\n`, 'utf8');
  return snapshotPath;
}

const shopPolicyUpdateMutation = `mutation ShopPolicyUpdate($shopPolicy: ShopPolicyInput!) {
  shopPolicyUpdate(shopPolicy: $shopPolicy) {
    shopPolicy {
      id
      title
      body
      type
      url
      createdAt
      updatedAt
      translations(locale: "fr") {
        key
        locale
        outdated
        updatedAt
        value
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}`;

const shopPoliciesReadQuery = `query ShopPoliciesRead {
  shop {
    shopPolicies {
      id
      title
      body
      type
      url
      createdAt
      updatedAt
      translations(locale: "fr") {
        key
        locale
        outdated
        updatedAt
        value
      }
    }
  }
}`;

describe('shopPolicyUpdate local staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages supported policy updates locally and exposes read-after-write shopPolicies in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('shopPolicyUpdate should not proxy upstream');
    });
    const app = createApp({
      ...snapshotConfig,
      snapshotPath: writeSnapshot(makeShop()),
    }).callback();
    const body = '<p>Local contact policy</p>';

    const mutationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: shopPolicyUpdateMutation,
        operationName: 'ShopPolicyUpdate',
        variables: {
          shopPolicy: {
            type: 'CONTACT_INFORMATION',
            body,
          },
        },
      });

    expect(mutationResponse.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(mutationResponse.body.data.shopPolicyUpdate.userErrors).toEqual([]);
    expect(mutationResponse.body.data.shopPolicyUpdate.shopPolicy).toMatchObject({
      id: 'gid://shopify/ShopPolicy/1',
      title: 'Contact',
      body,
      type: 'CONTACT_INFORMATION',
      url: 'https://checkout.shopify.com/63755419881/policies/1.html?locale=en',
      createdAt: '2024-01-01T00:00:00.000Z',
      updatedAt: '2024-01-01T00:00:00.000Z',
      translations: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({ query: shopPoliciesReadQuery });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.shop.shopPolicies).toEqual([mutationResponse.body.data.shopPolicyUpdate.shopPolicy]);

    const metaResponse = await request(app).get('/__meta/log');
    expect(metaResponse.status).toBe(200);
    expect(metaResponse.body.entries).toHaveLength(1);
    expect(metaResponse.body.entries[0]).toMatchObject({
      operationName: 'ShopPolicyUpdate',
      status: 'staged',
      requestBody: {
        operationName: 'ShopPolicyUpdate',
        variables: {
          shopPolicy: {
            type: 'CONTACT_INFORMATION',
            body,
          },
        },
      },
    });
    expect(metaResponse.body.entries[0].requestBody.query).toBe(shopPolicyUpdateMutation);
  });

  it('returns Shopify-like userErrors for oversized policy bodies without changing downstream policies', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('validation should resolve locally');
    });
    const app = createApp({
      ...snapshotConfig,
      snapshotPath: writeSnapshot(makeShop()),
    }).callback();

    const mutationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: shopPolicyUpdateMutation,
        variables: {
          shopPolicy: {
            type: 'CONTACT_INFORMATION',
            body: 'x'.repeat(512 * 1024 + 1),
          },
        },
      });

    expect(mutationResponse.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(mutationResponse.body.data.shopPolicyUpdate).toEqual({
      shopPolicy: null,
      userErrors: [
        {
          field: ['shopPolicy', 'body'],
          message: 'Body is too big (maximum is 512 KB)',
          code: 'TOO_BIG',
        },
      ],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({ query: shopPoliciesReadQuery });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.shop.shopPolicies).toEqual([]);
  });

  it('short-circuits supported policy mutations in live-hybrid mode when a shop baseline exists', async () => {
    const existingPolicy = {
      id: 'gid://shopify/ShopPolicy/42438689001',
      title: 'Contact',
      body: '<p>Before</p>',
      type: 'CONTACT_INFORMATION',
      url: 'https://checkout.shopify.com/63755419881/policies/42438689001.html?locale=en',
      createdAt: '2026-04-25T11:52:28Z',
      updatedAt: '2026-04-25T11:52:28Z',
    };
    store.upsertBaseShop(makeShop({ shopPolicies: [existingPolicy] }));
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('live-hybrid policy mutation/read should resolve locally once shop is seeded');
    });
    const app = createApp(liveHybridConfig).callback();

    const mutationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: shopPolicyUpdateMutation,
        variables: {
          shopPolicy: {
            type: 'CONTACT_INFORMATION',
            body: '<p>After</p>',
          },
        },
      });
    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({ query: shopPoliciesReadQuery });

    expect(mutationResponse.status).toBe(200);
    expect(readResponse.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(mutationResponse.body.data.shopPolicyUpdate.shopPolicy).toMatchObject({
      id: existingPolicy.id,
      title: existingPolicy.title,
      body: '<p>After</p>',
      type: existingPolicy.type,
      url: existingPolicy.url,
      createdAt: existingPolicy.createdAt,
      translations: [],
    });
    expect(mutationResponse.body.data.shopPolicyUpdate.shopPolicy.updatedAt).not.toBe(existingPolicy.updatedAt);
    expect(readResponse.body.data.shop.shopPolicies).toEqual([mutationResponse.body.data.shopPolicyUpdate.shopPolicy]);
  });
});
