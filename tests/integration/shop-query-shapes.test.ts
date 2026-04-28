import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
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
      inventoryTransfers: {},
      inventoryTransferOrder: [],
      locations: {},
      locationOrder: [],
      fulfillmentServices: {},
      fulfillmentServiceOrder: [],
      carrierServices: {},
      carrierServiceOrder: [],
      inventoryShipments: {},
      inventoryShipmentOrder: [],
      shippingPackages: {},
      shippingPackageOrder: [],
      giftCards: {},
      giftCardOrder: [],
      giftCardConfiguration: null,
      collections: {},
      publications: {},
      channels: {},
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
      bulkOperationResults: {},
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
      deletedInventoryTransferIds: {},
      deletedFileIds: {},
      deletedCollectionIds: {},
      deletedPublicationIds: {},
      deletedLocationIds: {},
      deletedFulfillmentServiceIds: {},
      deletedCarrierServiceIds: {},
      deletedInventoryShipmentIds: {},
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
  const snapshotPath = join(mkdtempSync(join(tmpdir(), 'shopify-draft-proxy-shop-')), 'snapshot.json');
  writeFileSync(snapshotPath, `${JSON.stringify(snapshot, null, 2)}\n`, 'utf8');
  return snapshotPath;
}

const shopBaselineQuery = `query StorePropertiesShopBaseline {
  shop {
    id
    name
    myshopifyDomain
    url
    primaryDomain {
      host
      url
      sslEnabled
    }
    contactEmail
    email
    currencyCode
    enabledPresentmentCurrencies
    ianaTimezone
    timezoneAbbreviation
    timezoneOffset
    timezoneOffsetMinutes
    taxesIncluded
    taxShipping
    unitSystem
    weightUnit
    shopAddress {
      address1
      address2
      city
      company
      coordinatesValidated
      country
      countryCodeV2
      formatted
      formattedArea
      latitude
      longitude
      phone
      province
      provinceCode
      zip
    }
    plan {
      partnerDevelopment
      publicDisplayName
      shopifyPlus
    }
    resourceLimits {
      locationLimit
      maxProductOptions
      maxProductVariants
      redirectLimitReached
    }
    features {
      avalaraAvatax
      branding
      bundles {
        eligibleForBundles
        ineligibilityReason
        sellsBundles
      }
      captcha
      cartTransform {
        eligibleOperations {
          expandOperation
          mergeOperation
          updateOperation
        }
      }
      dynamicRemarketing
      eligibleForSubscriptionMigration
      eligibleForSubscriptions
      giftCards
      harmonizedSystemCode
      legacySubscriptionGatewayEnabled
      liveView
      paypalExpressSubscriptionGatewayStatus
      reports
      sellsSubscriptions
      showMetrics
      storefront
      unifiedMarkets
    }
    paymentSettings {
      supportedDigitalWallets
    }
    shopPolicies {
      id
      title
      body
      type
      url
      createdAt
      updatedAt
    }
  }
}`;

describe('shop query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves the baseline shop selection from a normalized snapshot without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('shop should resolve locally in snapshot mode');
    });
    const app = createApp({
      ...snapshotConfig,
      snapshotPath: writeSnapshot(makeShop()),
    }).callback();

    const response = await request(app).post('/admin/api/2026-04/graphql.json').send({ query: shopBaselineQuery });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        shop: {
          id: 'gid://shopify/Shop/63755419881',
          name: 'very-big-test-store',
          myshopifyDomain: 'very-big-test-store.myshopify.com',
          url: 'https://very-big-test-store.myshopify.com',
          primaryDomain: {
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
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns null instead of inventing shop data when the snapshot has no shop slice', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('empty shop snapshot should resolve locally in snapshot mode');
    });
    const app = createApp({
      ...snapshotConfig,
      snapshotPath: writeSnapshot(null),
    }).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({ query: 'query EmptyShop { shop { id name } }' });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({ data: { shop: null } });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('uses staged shop state as the live-hybrid overlay before calling upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('staged shop state should resolve locally in live-hybrid mode');
    });
    store.upsertBaseShop(makeShop());
    store.stageShop(
      makeShop({
        name: 'staged-store-name',
        shopPolicies: [
          {
            id: 'gid://shopify/ShopPolicy/1',
            title: 'Refund policy',
            body: '<p>Refunds are staged locally.</p>',
            type: 'REFUND_POLICY',
            url: 'https://very-big-test-store.myshopify.com/policies/refund-policy',
            createdAt: '2026-04-01T00:00:00Z',
            updatedAt: '2026-04-02T00:00:00Z',
          },
        ],
      }),
    );

    const app = createApp(liveHybridConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query LiveHybridShopOverlay {
          shop {
            name
            shopPolicies {
              id
              title
              body
              type
              url
              createdAt
              updatedAt
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        shop: {
          name: 'staged-store-name',
          shopPolicies: [
            {
              id: 'gid://shopify/ShopPolicy/1',
              title: 'Refund policy',
              body: '<p>Refunds are staged locally.</p>',
              type: 'REFUND_POLICY',
              url: 'https://very-big-test-store.myshopify.com/policies/refund-policy',
              createdAt: '2026-04-01T00:00:00Z',
              updatedAt: '2026-04-02T00:00:00Z',
            },
          ],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('exposes the shop slice through meta state without credential fields', async () => {
    const app = createApp({
      ...snapshotConfig,
      snapshotPath: writeSnapshot(makeShop()),
    }).callback();

    const response = await request(app).get('/__meta/state');

    expect(response.status).toBe(200);
    expect(response.body.baseState.shop).toMatchObject({
      id: 'gid://shopify/Shop/63755419881',
      name: 'very-big-test-store',
      shopPolicies: [],
    });
    expect(JSON.stringify(response.body)).not.toContain('accessToken');
    expect(JSON.stringify(response.body)).not.toContain('SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN');
  });
});
