import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import type { ShopRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
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

const fulfillmentServiceSelection = `#graphql
    id
    handle
    serviceName
    callbackUrl
    trackingSupport
    inventoryManagement
    requiresShippingMethod
    type
    location {
      id
      name
      isFulfillmentService
      fulfillsOnlineOrders
      shipsInventory
    }
`;

describe('fulfillment service local staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves empty and missing fulfillment-service reads locally in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fulfillment service reads should resolve locally in snapshot mode');
    });
    store.upsertBaseShop(makeShop());
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          query MissingFulfillmentService($id: ID!) {
            fulfillmentService(id: $id) {
              ${fulfillmentServiceSelection}
            }
            shop {
              fulfillmentServices {
                ${fulfillmentServiceSelection}
              }
            }
          }
        `,
        variables: { id: 'gid://shopify/FulfillmentService/999999999999' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        fulfillmentService: null,
        shop: {
          fulfillmentServices: [],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages create, update, and delete lifecycle locally with associated location visibility and meta state', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fulfillment service lifecycle should not hit upstream in snapshot mode');
    });
    store.upsertBaseShop(makeShop());
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          mutation CreateFulfillmentService(
            $name: String!
            $callbackUrl: URL
            $trackingSupport: Boolean
            $inventoryManagement: Boolean
            $requiresShippingMethod: Boolean
          ) {
            fulfillmentServiceCreate(
              name: $name
              callbackUrl: $callbackUrl
              trackingSupport: $trackingSupport
              inventoryManagement: $inventoryManagement
              requiresShippingMethod: $requiresShippingMethod
            ) {
              fulfillmentService {
                ${fulfillmentServiceSelection}
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          name: 'Hermes Local FS',
          callbackUrl: 'https://mock.shop/fulfillment-service',
          trackingSupport: true,
          inventoryManagement: true,
          requiresShippingMethod: true,
        },
      });

    expect(createResponse.status).toBe(200);
    const createdService = createResponse.body.data.fulfillmentServiceCreate.fulfillmentService;
    expect(createResponse.body.data.fulfillmentServiceCreate.userErrors).toEqual([]);
    expect(createdService).toMatchObject({
      handle: 'hermes-local-fs',
      serviceName: 'Hermes Local FS',
      callbackUrl: 'https://mock.shop/fulfillment-service',
      trackingSupport: true,
      inventoryManagement: true,
      requiresShippingMethod: true,
      type: 'THIRD_PARTY',
      location: {
        name: 'Hermes Local FS',
        isFulfillmentService: true,
        fulfillsOnlineOrders: true,
        shipsInventory: false,
      },
    });

    const detailResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          query FulfillmentServiceRead($id: ID!, $locationId: ID!) {
            fulfillmentService(id: $id) {
              ${fulfillmentServiceSelection}
            }
            location(id: $locationId) {
              id
              name
              isFulfillmentService
              fulfillmentService {
                id
                handle
                serviceName
                callbackUrl
              }
            }
            shop {
              fulfillmentServices {
                id
                serviceName
              }
            }
          }
        `,
        variables: { id: createdService.id, locationId: createdService.location.id },
      });

    expect(detailResponse.status).toBe(200);
    expect(detailResponse.body.data.fulfillmentService).toEqual(createdService);
    expect(detailResponse.body.data.location).toEqual({
      id: createdService.location.id,
      name: 'Hermes Local FS',
      isFulfillmentService: true,
      fulfillmentService: {
        id: createdService.id,
        handle: 'hermes-local-fs',
        serviceName: 'Hermes Local FS',
        callbackUrl: 'https://mock.shop/fulfillment-service',
      },
    });
    expect(detailResponse.body.data.shop.fulfillmentServices).toEqual([
      {
        id: createdService.id,
        serviceName: 'Hermes Local FS',
      },
    ]);

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `
          mutation UpdateFulfillmentService($id: ID!, $name: String!) {
            fulfillmentServiceUpdate(
              id: $id
              name: $name
              trackingSupport: false
              inventoryManagement: false
              requiresShippingMethod: false
            ) {
              fulfillmentService {
                ${fulfillmentServiceSelection}
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { id: createdService.id, name: 'Hermes Local FS Updated' },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.fulfillmentServiceUpdate.userErrors).toEqual([]);
    expect(updateResponse.body.data.fulfillmentServiceUpdate.fulfillmentService).toMatchObject({
      id: createdService.id,
      handle: 'hermes-local-fs',
      serviceName: 'Hermes Local FS Updated',
      trackingSupport: false,
      inventoryManagement: false,
      requiresShippingMethod: false,
      location: {
        id: createdService.location.id,
        name: 'Hermes Local FS Updated',
      },
    });

    const logAfterUpdate = await request(app).get('/__meta/log');
    expect(logAfterUpdate.body.entries.map((entry: { status: string }) => entry.status)).toEqual(['staged', 'staged']);
    expect(
      logAfterUpdate.body.entries.map(
        (entry: { interpreted: { primaryRootField: string } }) => entry.interpreted.primaryRootField,
      ),
    ).toEqual(['fulfillmentServiceCreate', 'fulfillmentServiceUpdate']);

    const stateAfterUpdate = await request(app).get('/__meta/state');
    expect(Object.keys(stateAfterUpdate.body.stagedState.fulfillmentServices)).toEqual([createdService.id]);
    expect(Object.keys(stateAfterUpdate.body.stagedState.locations)).toEqual([createdService.location.id]);

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteFulfillmentService($id: ID!) {
          fulfillmentServiceDelete(id: $id, inventoryAction: DELETE) {
            deletedId
            userErrors {
              field
              message
            }
          }
        }`,
        variables: { id: createdService.id },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.fulfillmentServiceDelete).toEqual({
      deletedId: createdService.id.split('?')[0],
      userErrors: [],
    });

    const afterDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query AfterDelete($serviceId: ID!, $locationId: ID!) {
          fulfillmentService(id: $serviceId) {
            id
          }
          location(id: $locationId) {
            id
          }
          shop {
            fulfillmentServices {
              id
            }
          }
        }`,
        variables: { serviceId: createdService.id, locationId: createdService.location.id },
      });

    expect(afterDeleteResponse.body).toEqual({
      data: {
        fulfillmentService: null,
        location: null,
        shop: {
          fulfillmentServices: [],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns captured validation-style userErrors without staging invalid lifecycle requests', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid fulfillment service lifecycle should resolve locally in snapshot mode');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation InvalidCreate($name: String!, $callbackUrl: URL) {
          fulfillmentServiceCreate(name: $name, callbackUrl: $callbackUrl) {
            fulfillmentService {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: { name: '', callbackUrl: 'https://example.com/fulfillment-service' },
      });

    expect(createResponse.body).toEqual({
      data: {
        fulfillmentServiceCreate: {
          fulfillmentService: null,
          userErrors: [
            {
              field: ['name'],
              message: "Name can't be blank",
            },
            {
              field: ['callbackUrl'],
              message: 'Callback url is not allowed',
            },
          ],
        },
      },
    });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UnknownUpdate($id: ID!) {
          fulfillmentServiceUpdate(id: $id, name: "Nope") {
            fulfillmentService {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: { id: 'gid://shopify/FulfillmentService/999999999999' },
      });

    expect(updateResponse.body.data.fulfillmentServiceUpdate).toEqual({
      fulfillmentService: null,
      userErrors: [
        {
          field: ['id'],
          message: 'Fulfillment service could not be found.',
        },
      ],
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UnknownDelete($id: ID!) {
          fulfillmentServiceDelete(id: $id, inventoryAction: DELETE) {
            deletedId
            userErrors {
              field
              message
            }
          }
        }`,
        variables: { id: 'gid://shopify/FulfillmentService/999999999999' },
      });

    expect(deleteResponse.body.data.fulfillmentServiceDelete).toEqual({
      deletedId: null,
      userErrors: [
        {
          field: ['id'],
          message: 'Fulfillment service could not be found.',
        },
      ],
    });
  });
});
