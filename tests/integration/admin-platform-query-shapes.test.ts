import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type {
  B2BCompanyRecord,
  BulkOperationRecord,
  CustomerPaymentMethodRecord,
  CustomerRecord,
  FileRecord,
  PaymentTermsTemplateRecord,
  ProductRecord,
  SavedSearchRecord,
  ShopRecord,
} from '../../src/state/types.js';

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

function makeCustomer(id: string, email: string): CustomerRecord {
  return {
    id,
    firstName: 'Relay',
    lastName: 'Customer',
    displayName: 'Relay Customer',
    email,
    legacyResourceId: id.split('/').at(-1) ?? null,
    locale: 'en',
    note: null,
    canDelete: true,
    verifiedEmail: true,
    taxExempt: false,
    state: 'DISABLED',
    tags: [],
    numberOfOrders: 0,
    amountSpent: { amount: '0.00', currencyCode: 'USD' },
    defaultEmailAddress: { emailAddress: email },
    defaultPhoneNumber: null,
    defaultAddress: null,
    createdAt: '2025-01-01T00:00:00.000Z',
    updatedAt: '2025-01-01T00:00:00.000Z',
  };
}

function makeCompany(id: string): B2BCompanyRecord {
  return {
    id,
    contactIds: [],
    locationIds: [],
    contactRoleIds: [],
    data: {
      id,
      name: 'Relay Company',
      note: 'B2B account',
      externalId: 'relay-company',
      createdAt: '2025-03-26T19:51:37Z',
      updatedAt: '2025-03-26T19:51:38Z',
      contactsCount: { count: 0 },
      locationsCount: { count: 0 },
    },
  };
}

function makeBulkOperation(id: string): BulkOperationRecord {
  return {
    id,
    status: 'COMPLETED',
    type: 'QUERY',
    errorCode: null,
    createdAt: '2026-04-27T00:00:00Z',
    completedAt: '2026-04-27T00:01:00Z',
    objectCount: '2',
    rootObjectCount: '1',
    fileSize: '200',
    url: 'https://example.com/bulk-result.jsonl',
    partialDataUrl: null,
    query: '{ products { edges { node { id title } } } }',
  };
}

function makeCustomerPaymentMethod(id: string, customerId: string): CustomerPaymentMethodRecord {
  return {
    id,
    customerId,
    instrument: null,
    revokedAt: null,
    revokedReason: null,
    subscriptionContracts: [],
  };
}

function makeSavedSearch(id: string): SavedSearchRecord {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? id,
    name: 'Relay saved products',
    query: 'tag:relay',
    resourceType: 'PRODUCT',
    searchTerms: 'tag:relay',
    filters: [{ key: 'tag', value: 'relay' }],
    cursor: null,
  };
}

function makePaymentTermsTemplate(id: string): PaymentTermsTemplateRecord {
  return {
    id,
    name: 'Relay Net 14',
    description: 'Within 14 days',
    dueInDays: 14,
    paymentTermsType: 'NET',
    translatedName: 'Relay Net 14',
  };
}

function makeFile(id: string, contentType: FileRecord['contentType'], filename: string): FileRecord {
  return {
    id,
    alt: 'Relay file',
    contentType,
    createdAt: '2026-04-28T00:00:00.000Z',
    fileStatus: 'READY',
    filename,
    originalSource: `https://cdn.example.com/${filename}`,
    imageUrl: contentType === 'IMAGE' ? `https://cdn.example.com/${filename}` : null,
    imageWidth: contentType === 'IMAGE' ? 1200 : null,
    imageHeight: contentType === 'IMAGE' ? 800 : null,
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

  it('resolves supported resource GIDs through generic node roots', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('supported admin platform node reads should resolve locally in snapshot mode');
    });
    store.upsertBaseCustomers([makeCustomer('gid://shopify/Customer/8801', 'relay-customer@example.com')]);
    store.upsertBaseB2BCompanies([makeCompany('gid://shopify/Company/200')]);
    store.upsertBaseBulkOperations([makeBulkOperation('gid://shopify/BulkOperation/101')]);

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query SupportedNodeResolution($customerId: ID!, $ids: [ID!]!) {
          customerNode: node(id: $customerId) {
            __typename
            ... on Node {
              nodeId: id
            }
            ... on Customer {
              email
              displayName
            }
          }
          nodes(ids: $ids) {
            __typename
            ... on Node {
              nodeId: id
            }
            ... on Company {
              name
            }
            ... on BulkOperation {
              status
              type
            }
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/8801',
          ids: ['gid://shopify/Company/200', 'gid://shopify/BulkOperation/101', 'gid://shopify/InventoryItem/404'],
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        customerNode: {
          __typename: 'Customer',
          nodeId: 'gid://shopify/Customer/8801',
          email: 'relay-customer@example.com',
          displayName: 'Relay Customer',
        },
        nodes: [
          {
            __typename: 'Company',
            nodeId: 'gid://shopify/Company/200',
            name: 'Relay Company',
          },
          {
            __typename: 'BulkOperation',
            nodeId: 'gid://shopify/BulkOperation/101',
            status: 'COMPLETED',
            type: 'QUERY',
          },
          null,
        ],
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('resolves staged product option and option value IDs through generic node roots', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('product option node reads should resolve locally in snapshot mode');
    });
    const productId = 'gid://shopify/Product/42400';
    store.upsertBaseProducts([makeProduct(productId, 'Relay Option Product')]);

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation CreateProductOptions($productId: ID!, $options: [OptionCreateInput!]!) {
          productOptionsCreate(productId: $productId, options: $options) {
            product {
              id
              options {
                id
                name
                position
                values
                optionValues {
                  id
                  name
                  hasVariants
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
          productId,
          options: [
            {
              name: 'Color',
              values: [{ name: 'Red' }, { name: 'Blue' }],
            },
          ],
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.productOptionsCreate.userErrors).toEqual([]);
    const [option] = createResponse.body.data.productOptionsCreate.product.options;
    const [redValue, blueValue] = option.optionValues;

    const nodeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `query ProductOptionNodeResolution($optionId: ID!, $ids: [ID!]!) {
          optionNode: node(id: $optionId) {
            __typename
            ... on Node {
              nodeId: id
            }
            ... on ProductOption {
              name
              position
              values
              optionValues {
                __typename
                id
                name
                hasVariants
              }
            }
          }
          nodes(ids: $ids) {
            __typename
            ... on Node {
              nodeId: id
            }
            ... on ProductOption {
              name
              position
              values
            }
            ... on ProductOptionValue {
              name
              hasVariants
            }
          }
        }`,
        variables: {
          optionId: option.id,
          ids: [redValue.id, option.id, blueValue.id, 'gid://shopify/ProductOptionValue/404'],
        },
      });

    expect(nodeResponse.status).toBe(200);
    expect(nodeResponse.body).toEqual({
      data: {
        optionNode: {
          __typename: 'ProductOption',
          nodeId: option.id,
          name: 'Color',
          position: 1,
          values: [],
          optionValues: [
            {
              __typename: 'ProductOptionValue',
              id: redValue.id,
              name: 'Red',
              hasVariants: false,
            },
            {
              __typename: 'ProductOptionValue',
              id: blueValue.id,
              name: 'Blue',
              hasVariants: false,
            },
          ],
        },
        nodes: [
          {
            __typename: 'ProductOptionValue',
            nodeId: redValue.id,
            name: 'Red',
            hasVariants: false,
          },
          {
            __typename: 'ProductOption',
            nodeId: option.id,
            name: 'Color',
            position: 1,
            values: [],
          },
          {
            __typename: 'ProductOptionValue',
            nodeId: blueValue.id,
            name: 'Blue',
            hasVariants: false,
          },
          null,
        ],
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog()).toMatchObject([{ operationName: 'productOptionsCreate', status: 'staged' }]);
  });

  it('resolves supported Node IDs that do not have a one-to-one singular root', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('direct admin platform node serializers should resolve locally in snapshot mode');
    });
    store.upsertBaseShop(makeShop());
    store.upsertBaseCustomers([makeCustomer('gid://shopify/Customer/9100', 'relay-node-owner@example.com')]);
    store.upsertBaseCustomerPaymentMethods([
      makeCustomerPaymentMethod('gid://shopify/CustomerPaymentMethod/9100', 'gid://shopify/Customer/9100'),
    ]);
    store.upsertBaseSavedSearches([makeSavedSearch('gid://shopify/SavedSearch/9100')]);
    store.upsertBasePaymentTermsTemplates([makePaymentTermsTemplate('gid://shopify/PaymentTermsTemplate/14')]);
    store.stageCreateFiles([
      makeFile('gid://shopify/GenericFile/9100', 'FILE', 'relay.pdf'),
      makeFile('gid://shopify/MediaImage/9101', 'IMAGE', 'relay.jpg'),
    ]);

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query DirectNodeResolution($ids: [ID!]!) {
          nodes(ids: $ids) {
            __typename
            ... on Node {
              nodeId: id
            }
            ... on Shop {
              name
              myshopifyDomain
            }
            ... on CustomerPaymentMethod {
              revokedAt
            }
            ... on SavedSearch {
              name
              resourceType
              query
            }
            ... on PaymentTermsTemplate {
              name
              dueInDays
              paymentTermsType
            }
            ... on GenericFile {
              filename
              fileStatus
            }
            ... on MediaImage {
              alt
              fileStatus
              image {
                url
                width
                height
              }
            }
          }
        }`,
        variables: {
          ids: [
            'gid://shopify/Shop/400',
            'gid://shopify/CustomerPaymentMethod/9100',
            'gid://shopify/SavedSearch/9100',
            'gid://shopify/PaymentTermsTemplate/14',
            'gid://shopify/GenericFile/9100',
            'gid://shopify/MediaImage/9101',
          ],
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        nodes: [
          {
            __typename: 'Shop',
            nodeId: 'gid://shopify/Shop/400',
            name: 'Node Test Shop',
            myshopifyDomain: 'node-test-shop.myshopify.com',
          },
          {
            __typename: 'CustomerPaymentMethod',
            nodeId: 'gid://shopify/CustomerPaymentMethod/9100',
            revokedAt: null,
          },
          {
            __typename: 'SavedSearch',
            nodeId: 'gid://shopify/SavedSearch/9100',
            name: 'Relay saved products',
            resourceType: 'PRODUCT',
            query: 'tag:relay',
          },
          {
            __typename: 'PaymentTermsTemplate',
            nodeId: 'gid://shopify/PaymentTermsTemplate/14',
            name: 'Relay Net 14',
            dueInDays: 14,
            paymentTermsType: 'NET',
          },
          {
            __typename: 'GenericFile',
            nodeId: 'gid://shopify/GenericFile/9100',
            filename: 'relay.pdf',
            fileStatus: 'READY',
          },
          {
            __typename: 'MediaImage',
            nodeId: 'gid://shopify/MediaImage/9101',
            alt: 'Relay file',
            fileStatus: 'READY',
            image: {
              url: 'https://cdn.example.com/relay.jpg',
              width: 1200,
              height: 800,
            },
          },
        ],
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages Flow utility mutations locally without external trigger delivery or signature leakage', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('Flow utilities must not proxy'));

    const app = createApp(passthroughConfig).callback();
    const signatureResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation FlowGenerateSignature($payload: String!) {
          flowGenerateSignature(id: "gid://shopify/FlowTrigger/374", payload: $payload) {
            payload
            signature
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          payload: '{"customer_id":374}',
        },
      });
    const triggerResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation FlowTriggerReceive($payload: JSON) {
          flowTriggerReceive(handle: "har-374-local-trigger", payload: $payload) {
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          payload: { customer_id: 374, action: 'created' },
        },
      });

    expect(signatureResponse.status).toBe(200);
    expect(signatureResponse.body.data.flowGenerateSignature).toEqual({
      payload: '{"customer_id":374}',
      signature: expect.any(String),
      userErrors: [],
    });
    expect(signatureResponse.body.data.flowGenerateSignature.signature).toHaveLength(64);
    expect(triggerResponse.status).toBe(200);
    expect(triggerResponse.body.data.flowTriggerReceive.userErrors).toEqual([]);
    expect(fetchSpy).not.toHaveBeenCalled();

    const log = store.getLog();
    expect(log).toHaveLength(2);
    expect(log.map((entry) => ({ operationName: entry.operationName, status: entry.status }))).toEqual([
      { operationName: 'FlowGenerateSignature', status: 'staged' },
      { operationName: 'FlowTriggerReceive', status: 'staged' },
    ]);
    expect(log[0]?.variables).toEqual({ payload: '{"customer_id":374}' });
    expect(log[1]?.variables).toEqual({ payload: { customer_id: 374, action: 'created' } });
    expect(JSON.stringify(log)).not.toContain(signatureResponse.body.data.flowGenerateSignature.signature);

    const state = store.getState().stagedState;
    expect(Object.values(state.adminPlatformFlowSignatures)).toEqual([
      expect.objectContaining({
        flowTriggerId: 'gid://shopify/FlowTrigger/374',
        payloadSha256: expect.any(String),
        signatureSha256: expect.any(String),
      }),
    ]);
    expect(Object.values(state.adminPlatformFlowTriggers)).toEqual([
      expect.objectContaining({
        handle: 'har-374-local-trigger',
        payloadBytes: expect.any(Number),
        payloadSha256: expect.any(String),
      }),
    ]);
    expect(JSON.stringify(state)).not.toContain(signatureResponse.body.data.flowGenerateSignature.signature);
    expect(JSON.stringify(state)).not.toContain('customer_id');
    expect(JSON.stringify(state)).not.toContain('"action"');
  });

  it('mirrors captured Flow validation branches locally without staging', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('Flow validation must not proxy'));

    const app = createApp(passthroughConfig).callback();
    const invalidHandleResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation FlowTriggerReceive {
          flowTriggerReceive(handle: "har-374-missing", payload: { test: "value" }) {
            userErrors {
              field
              message
            }
          }
        }`,
      });
    const oversizeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation FlowTriggerReceive($payload: JSON) {
          flowTriggerReceive(handle: "har-374-missing", payload: $payload) {
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          payload: { value: 'x'.repeat(50_001) },
        },
      });
    const unknownSignatureResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation { flowGenerateSignature(id: "gid://shopify/FlowTrigger/0", payload: "{}") { signature userErrors { field message } } }`,
      });

    expect(invalidHandleResponse.status).toBe(200);
    expect(invalidHandleResponse.body.data.flowTriggerReceive.userErrors).toEqual([
      {
        field: ['body'],
        message: "Errors validating schema:\n  Invalid handle 'har-374-missing'.\n",
      },
    ]);
    expect(oversizeResponse.status).toBe(200);
    expect(oversizeResponse.body.data.flowTriggerReceive.userErrors).toEqual([
      {
        field: ['body'],
        message: 'Errors validating schema:\n  Properties size exceeds the limit of 50000 bytes.\n',
      },
    ]);
    expect(unknownSignatureResponse.status).toBe(200);
    expect(unknownSignatureResponse.body).toMatchObject({
      data: { flowGenerateSignature: null },
      errors: [
        {
          message: 'Invalid id: gid://shopify/FlowTrigger/0',
          extensions: { code: 'RESOURCE_NOT_FOUND' },
          path: ['flowGenerateSignature'],
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog()).toEqual([]);
    expect(store.getState().stagedState.adminPlatformFlowSignatureOrder).toEqual([]);
    expect(store.getState().stagedState.adminPlatformFlowTriggerOrder).toEqual([]);
  });

  it('stages backupRegionUpdate locally and preserves backupRegion read-after-write', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('backupRegionUpdate must not proxy'));

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation BackupRegionUpdate {
          backupRegionUpdate(region: { countryCode: CA }) {
            backupRegion {
              __typename
              id
              name
              ... on MarketRegionCountry {
                code
              }
            }
            userErrors {
              field
              message
              code
            }
          }
        }`,
      });
    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `query BackupRegionRead {
          backupRegion {
            __typename
            id
            name
            ... on MarketRegionCountry {
              code
            }
          }
        }`,
      });

    const expectedRegion = {
      __typename: 'MarketRegionCountry',
      id: 'gid://shopify/MarketRegionCountry/4062110417202',
      name: 'Canada',
      code: 'CA',
    };
    expect(response.status).toBe(200);
    expect(response.body.data.backupRegionUpdate).toEqual({
      backupRegion: expectedRegion,
      userErrors: [],
    });
    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.backupRegion).toEqual(expectedRegion);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'BackupRegionUpdate',
      status: 'staged',
      stagedResourceIds: ['gid://shopify/MarketRegionCountry/4062110417202'],
      interpreted: {
        capability: {
          operationName: 'BackupRegionUpdate',
          domain: 'admin-platform',
          execution: 'stage-locally',
        },
      },
    });
    expect(store.getState().stagedState.backupRegion).toEqual(expectedRegion);
  });

  it('mirrors captured backupRegionUpdate REGION_NOT_FOUND validation without staging', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('backupRegion validation must not proxy'));

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation BackupRegionUpdate {
          backupRegionUpdate(region: { countryCode: ZZ }) {
            backupRegion {
              __typename
              id
              name
            }
            userErrors {
              field
              message
              code
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body.data.backupRegionUpdate).toEqual({
      backupRegion: null,
      userErrors: [{ field: ['region'], message: 'Region not found.', code: 'REGION_NOT_FOUND' }],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog()).toEqual([]);
  });
});
