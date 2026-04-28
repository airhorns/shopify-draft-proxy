import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'passthrough',
};

describe('proxy capability classification', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('logs supported product mutations as staged-local intent instead of generic passthrough', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            productCreate: {
              product: { id: 'gid://shopify/Product/999' },
              userErrors: [],
            },
          },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      ),
    );

    const app = createApp(config);
    const query =
      'mutation ProductCreate { productCreate(product: { title: "Hat" }) { product { id } userErrors { field message } } }';

    const response = await request(app.callback())
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query,
      });

    expect(response.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();

    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'ProductCreate',
      requestBody: { query },
      status: 'staged',
      notes: 'Staged locally in the in-memory product draft store.',
    });
  });

  it('logs supported discount code-basic happy paths as staged-local intent', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('supported discount code-basic create should not hit upstream fetch');
    });

    const app = createApp(config);

    const query = `#graphql
      mutation CreateDiscount {
        discountCodeBasicCreate(
          basicCodeDiscount: {
            title: "Test"
            code: "TEST"
            startsAt: "2026-04-24T00:00:00Z"
            customerGets: { value: { percentage: 0.1 }, items: { all: true } }
          }
        ) {
          codeDiscountNode {
            id
          }
          userErrors {
            field
            message
          }
        }
      }
    `;

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({ query });

    expect(response.status).toBe(200);
    expect(response.body.data.discountCodeBasicCreate.userErrors).toEqual([]);
    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'discountCodeBasicCreate',
      status: 'staged',
      interpreted: {
        operationType: 'mutation',
        operationName: 'CreateDiscount',
        rootFields: ['discountCodeBasicCreate'],
        primaryRootField: 'discountCodeBasicCreate',
        capability: {
          operationName: 'discountCodeBasicCreate',
          domain: 'discounts',
          execution: 'stage-locally',
        },
      },
      notes: 'Staged locally in the in-memory discount draft store.',
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('logs app-managed discount mutations with Function evidence as staged-local intent', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('supported app-managed discount create should not hit upstream fetch');
    });
    store.upsertStagedShopifyFunction({
      id: '859fcac2-cf96-44db-8146-977445fa90c8',
      title: 'Function discount',
      handle: 'function-discount',
      apiType: 'DISCOUNT',
      description: 'Local Function evidence',
      appKey: 'local-app-key',
    });

    const app = createApp(config);

    const query = `#graphql
      mutation CreateAppDiscount {
        discountCodeAppCreate(
          codeAppDiscount: {
            title: "Function discount"
            code: "FUNCTION"
            startsAt: "2026-04-24T00:00:00Z"
            functionId: "859fcac2-cf96-44db-8146-977445fa90c8"
          }
        ) {
          codeAppDiscount {
            discountId
            title
            status
            appDiscountType { appKey functionId title description }
          }
          userErrors {
            field
            message
          }
        }
      }
    `;

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({ query });

    expect(response.status).toBe(200);
    expect(response.body.data.discountCodeAppCreate).toEqual({
      codeAppDiscount: {
        discountId: 'gid://shopify/DiscountCodeNode/1?shopify-draft-proxy=synthetic',
        title: 'Function discount',
        status: 'SCHEDULED',
        appDiscountType: {
          appKey: 'local-app-key',
          functionId: '859fcac2-cf96-44db-8146-977445fa90c8',
          title: 'Function discount',
          description: 'Local Function evidence',
        },
      },
      userErrors: [],
    });
    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'discountCodeAppCreate',
      status: 'staged',
      interpreted: {
        operationType: 'mutation',
        operationName: 'CreateAppDiscount',
        rootFields: ['discountCodeAppCreate'],
        primaryRootField: 'discountCodeAppCreate',
        capability: {
          domain: 'discounts',
          execution: 'stage-locally',
        },
      },
      notes: 'Staged locally in the in-memory discount draft store.',
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('marks app billing and access mutations as unsafe unsupported passthrough in logs', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            appSubscriptionCancel: {
              appSubscription: null,
              userErrors: [
                {
                  field: ['id'],
                  message: 'Subscription not found',
                },
              ],
            },
          },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      ),
    );

    const app = createApp(config);
    const query = `#graphql
      mutation CancelAppSubscription {
        appSubscriptionCancel(id: "gid://shopify/AppSubscription/0") {
          appSubscription {
            id
          }
          userErrors {
            field
            message
          }
        }
      }
    `;

    const response = await request(app.callback())
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({ query });

    expect(response.status).toBe(200);
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'CancelAppSubscription',
      status: 'proxied',
      interpreted: {
        operationType: 'mutation',
        operationName: 'CancelAppSubscription',
        rootFields: ['appSubscriptionCancel'],
        primaryRootField: 'appSubscriptionCancel',
        capability: {
          operationName: 'CancelAppSubscription',
          domain: 'unknown',
          execution: 'passthrough',
        },
        registeredOperation: {
          name: 'appSubscriptionCancel',
          domain: 'apps',
          execution: 'stage-locally',
          implemented: false,
        },
        safety: {
          classification: 'unsupported-app-billing-access-mutation',
          wouldProxyToShopify: true,
        },
      },
      notes:
        'Unsupported app billing/access mutation would be proxied to Shopify. These roots can alter merchant billing, installation state, app scopes, or delegated tokens and require conformance-backed local staging plus raw commit replay before support.',
    });
    expect(store.getLog()[0]?.interpreted.safety?.reason).toContain('merchant billing');
  });

  it('marks delivery customization, promise, and settings mutations as unsafe unsupported passthrough in logs', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: {
            deliveryCustomizationCreate: {
              deliveryCustomization: null,
              userErrors: [],
            },
          },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      );
    });

    const app = createApp(config);
    const probes = [
      {
        root: 'deliveryCustomizationCreate',
        classification: 'unsupported-delivery-customization-function-mutation',
        query: `#graphql
          mutation CreateDeliveryCustomization {
            deliveryCustomizationCreate(
              deliveryCustomization: { functionId: "00000000-0000-4000-8000-000000000000", title: "Test" }
            ) {
              deliveryCustomization {
                id
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
      },
      {
        root: 'deliveryPromiseProviderUpsert',
        classification: 'unsupported-delivery-promise-mutation',
        query: `#graphql
          mutation UpsertDeliveryPromiseProvider {
            deliveryPromiseProviderUpsert(locationId: "gid://shopify/Location/1", active: true) {
              deliveryPromiseProvider {
                id
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
      },
      {
        root: 'deliverySettingUpdate',
        classification: 'unsupported-delivery-settings-mutation',
        query: `#graphql
          mutation UpdateDeliverySetting {
            deliverySettingUpdate(setting: { legacyModeProfiles: false }) {
              setting {
                legacyModeProfiles
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
      },
    ];

    for (const probe of probes) {
      const response = await request(app.callback())
        .post('/admin/api/2025-01/graphql.json')
        .set('x-shopify-access-token', 'shpat_test')
        .send({ query: probe.query });

      expect(response.status).toBe(200);
    }

    expect(fetchSpy).toHaveBeenCalledTimes(probes.length);
    expect(store.getLog()).toHaveLength(probes.length);
    expect(store.getLog().map((entry) => entry.status)).toEqual(probes.map(() => 'proxied'));
    expect(store.getLog().map((entry) => entry.interpreted.registeredOperation?.name)).toEqual(
      probes.map((probe) => probe.root),
    );
    expect(store.getLog().map((entry) => entry.interpreted.safety?.classification)).toEqual(
      probes.map((probe) => probe.classification),
    );
    expect(store.getLog().map((entry) => entry.notes)).toEqual([
      'Unsupported delivery customization mutation would be proxied to Shopify. Delivery customizations are Shopify Function-backed and require conformance-backed local staging for function ownership, activation, metafields, and downstream reads before support.',
      'Unsupported delivery promise mutation would be proxied to Shopify. Delivery promise provider and participant roots require delivery-promise scope evidence, owner/location eligibility, and local read-after-write modeling before support.',
      'Unsupported delivery settings mutation would be proxied to Shopify. Delivery setting changes alter shop delivery configuration and require conformance-backed transition and downstream read modeling before support.',
    ]);
  });

  it('logs product merchandising mutation roots as registered unsupported gaps', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(JSON.stringify({ data: {} }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });
    const app = createApp(config).callback();

    const probes = [
      {
        root: 'productFeedCreate',
        query: `#graphql
          mutation ProductFeedCreate {
            productFeedCreate(input: { country: US, language: EN }) {
              productFeed {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
      },
      {
        root: 'productFeedDelete',
        query: `#graphql
          mutation ProductFeedDelete {
            productFeedDelete(id: "gid://shopify/ProductFeed/999999999") {
              deletedId
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
      },
      {
        root: 'productFullSync',
        query: `#graphql
          mutation ProductFullSync {
            productFullSync(id: "gid://shopify/ProductFeed/999999999") {
              id
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
      },
      {
        root: 'productBundleCreate',
        query: `#graphql
          mutation ProductBundleCreate {
            productBundleCreate(input: { title: "Bundle", components: [] }) {
              productBundleOperation {
                id
                status
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
      },
      {
        root: 'productBundleUpdate',
        query: `#graphql
          mutation ProductBundleUpdate {
            productBundleUpdate(input: { productId: "gid://shopify/Product/999999999", components: [] }) {
              productBundleOperation {
                id
                status
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
      },
      {
        root: 'combinedListingUpdate',
        query: `#graphql
          mutation CombinedListingUpdate {
            combinedListingUpdate(parentProductId: "gid://shopify/Product/999999999") {
              product {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
      },
    ];

    for (const probe of probes) {
      const response = await request(app)
        .post('/admin/api/2025-01/graphql.json')
        .set('x-shopify-access-token', 'shpat_test')
        .send({ query: probe.query });

      expect(response.status).toBe(200);
    }

    expect(fetchSpy).toHaveBeenCalledTimes(probes.length);
    expect(store.getLog()).toHaveLength(probes.length);
    expect(store.getLog().map((entry) => entry.status)).toEqual(probes.map(() => 'proxied'));
    expect(store.getLog().map((entry) => entry.interpreted.registeredOperation?.name)).toEqual(
      probes.map((probe) => probe.root),
    );
    expect(store.getLog().map((entry) => entry.interpreted.registeredOperation?.implemented)).toEqual(
      probes.map(() => false),
    );
  });

  it('logs dataSaleOptOut as staged local customer privacy intent', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('supported dataSaleOptOut should not proxy upstream');
    });

    const app = createApp(config);

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `#graphql
          mutation DataSaleOptOut($email: String!) {
            dataSaleOptOut(email: $email) {
              customerId
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { email: 'privacy@example.com' },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.dataSaleOptOut.userErrors).toEqual([]);
    expect(response.body.data.dataSaleOptOut.customerId).toMatch(/^gid:\/\/shopify\/Customer\//);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'DataSaleOptOut',
      status: 'staged',
      interpreted: {
        operationType: 'mutation',
        operationName: 'DataSaleOptOut',
        rootFields: ['dataSaleOptOut'],
        primaryRootField: 'dataSaleOptOut',
        capability: {
          operationName: 'DataSaleOptOut',
          domain: 'privacy',
          execution: 'stage-locally',
        },
      },
      notes: 'Staged locally in the in-memory customer privacy draft store.',
    });
  });

  it('forwards inbound headers and wraps the user agent for upstream passthrough requests', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { shop: { name: 'Example Shop' } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(config);

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .set('authorization', 'Bearer incoming_authorization')
      .set('user-agent', 'example-client/2.3')
      .set('x-shopify-access-token', 'shpat_forwarded')
      .set('x-request-id', 'request-123')
      .send({
        query: 'query ShopName { shop { name } }',
        variables: {},
      });

    expect(response.status).toBe(200);
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(fetchSpy.mock.calls[0]?.[1]).toMatchObject({
      method: 'POST',
      headers: {
        authorization: 'Bearer incoming_authorization',
        'content-type': 'application/json',
        'user-agent': 'shopify-draft-proxy (wrapping example-client/2.3)',
        'x-request-id': 'request-123',
        'x-shopify-access-token': 'shpat_forwarded',
      },
    });
  });

  it('logs generic publishable mutations as local Store properties staging', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('generic publishable product support should not proxy upstream');
    });

    const app = createApp(config);

    const createResponse = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query:
          'mutation { productCreate(product: { title: "Generic publishable hat", status: ACTIVE }) { product { id } userErrors { field message } } }',
      });

    const productId = createResponse.body.data.productCreate.product.id as string;

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `#graphql
          mutation PublishGeneric {
            publishablePublish(
              id: "${productId}"
              input: [{ publicationId: "gid://shopify/Publication/1" }]
            ) {
              publishable {
                ... on Product {
                  id
                  publishedOnCurrentPublication
                }
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
      });

    expect(response.status).toBe(200);
    expect(response.body.data.publishablePublish).toEqual({
      publishable: {
        id: productId,
        publishedOnCurrentPublication: true,
      },
      userErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog()).toHaveLength(2);
    expect(store.getLog()[1]).toMatchObject({
      operationName: 'publishablePublish',
      status: 'staged',
      interpreted: {
        operationType: 'mutation',
        operationName: 'PublishGeneric',
        rootFields: ['publishablePublish'],
        primaryRootField: 'publishablePublish',
        capability: {
          operationName: 'publishablePublish',
          domain: 'store-properties',
          execution: 'stage-locally',
        },
      },
      notes: 'Staged locally in the in-memory product draft store.',
    });
  });
});
