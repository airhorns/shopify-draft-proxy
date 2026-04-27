import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'passthrough',
};

const emptySnapshot = {
  shop: null,
  products: {},
  productVariants: {},
  productOptions: {},
  productOperations: {},
  locations: {},
  locationOrder: [],
  collections: {},
  publications: {},
  customers: {},
  customerAddresses: {},
  customerPaymentMethods: {},
  customerSegmentMembersQueries: {},
  segments: {},
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
  discounts: {},
  discountBulkOperations: {},
  paymentCustomizations: {},
  paymentCustomizationOrder: [],
  deletedPaymentCustomizationIds: {},
  customerMetafields: {},
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
  marketLocalizations: {},
  marketOrder: [],
  webPresences: {},
  webPresenceOrder: [],
  availableLocales: [],
  shopLocales: {},
  translations: {},
  catalogs: {},
  catalogOrder: [],
  priceLists: {},
  priceListOrder: [],
  deletedPriceListIds: {},
  deliveryProfiles: {},
  deliveryProfileOrder: [],
  productCollections: {},
  productMedia: {},
  files: {},
  fulfillmentServices: {},
  fulfillmentServiceOrder: [],
  carrierServices: {},
  carrierServiceOrder: [],
  productMetafields: {},
  metafieldDefinitions: {},
  metaobjectDefinitions: {},
  metaobjects: {},
  deletedProductIds: {},
  deletedFileIds: {},
  deletedFulfillmentServiceIds: {},
  deletedCarrierServiceIds: {},
  deletedLocationIds: {},
  deletedCollectionIds: {},
  deletedCustomerIds: {},
  deletedCustomerAddressIds: {},
  deletedCustomerPaymentMethodIds: {},
  deletedSegmentIds: {},
  deletedWebhookSubscriptionIds: {},
  deletedOnlineStoreArticleIds: {},
  deletedOnlineStoreBlogIds: {},
  deletedOnlineStorePageIds: {},
  deletedOnlineStoreCommentIds: {},
  deletedDiscountIds: {},
  deletedMarketIds: {},
  deletedCatalogIds: {},
  deletedWebPresenceIds: {},
  deletedShopLocales: {},
  deletedTranslations: {},
  deletedDeliveryProfileIds: {},
  deletedMetafieldDefinitionIds: {},
  deletedMetaobjectDefinitionIds: {},
  deletedMetaobjectIds: {},
  mergedCustomerIds: {},
  customerMergeRequests: {},
  orderMandatePayments: {},
  orders: {},
  draftOrders: {},
  calculatedOrders: {},
  abandonedCheckouts: {},
  abandonedCheckoutOrder: [],
  abandonments: {},
  abandonmentOrder: [],
};

describe('meta routes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('exposes a lightweight health endpoint', async () => {
    const app = createApp(config);
    const server = app.callback();

    const health = await request(server).get('/__meta/health');
    expect(health.status).toBe(200);
    expect(health.body).toEqual({
      ok: true,
      message: 'shopify-draft-proxy is running',
    });
  });

  it('serves an operator web UI from the meta surface', async () => {
    const app = createApp(config);
    const server = app.callback();

    const response = await request(server).get('/__meta');

    expect(response.status).toBe(200);
    expect(response.headers['content-type']).toContain('text/html');
    expect(response.text).toContain('<h1>Shopify Draft Proxy</h1>');
    expect(response.text).toContain('data-action-path="/__meta/commit"');
    expect(response.text).toContain('data-action-path="/__meta/reset"');
    expect(response.text).toContain('id="operation-log-json"');
    expect(response.text).toContain('No operations staged.');
  });

  it('renders the current operation log and staged state in the operator web UI', async () => {
    const app = createApp(config);
    const server = app.callback();

    const createResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateDraft($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Staged <Hat>',
            status: 'DRAFT',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.productCreate.userErrors).toEqual([]);

    const response = await request(server).get('/__meta');

    expect(response.status).toBe(200);
    expect(response.text).toContain('productCreate');
    expect(response.text).toContain('staged');
    expect(response.text).toContain('Staged &lt;Hat&gt;');
    expect(response.text).toContain('gid://shopify/MutationLogEntry/1');
  });

  it('exposes a reset endpoint', async () => {
    const app = createApp(config);
    const server = app.callback();

    const reset = await request(server).post('/__meta/reset');
    expect(reset.status).toBe(200);
    expect(reset.body).toEqual({
      ok: true,
      message: 'state reset',
    });
  });

  it('exposes runtime-only staged object graph state without mutating it', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const orderCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation MetaStateOrderCreate($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              name
              note
              tags
              lineItems(first: 5) {
                nodes {
                  id
                  title
                  quantity
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
          order: {
            email: 'meta-state-order@example.com',
            note: 'meta state order',
            tags: ['meta-state', 'order'],
            lineItems: [
              {
                title: 'Meta state order line',
                quantity: 1,
                priceSet: {
                  shopMoney: {
                    amount: '3.00',
                    currencyCode: 'CAD',
                  },
                },
              },
            ],
          },
        },
      });

    expect(orderCreateResponse.status).toBe(200);
    expect(orderCreateResponse.body.data.orderCreate.userErrors).toEqual([]);

    const draftOrderCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation MetaStateDraftOrderCreate($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              name
              email
              tags
              lineItems(first: 5) {
                nodes {
                  id
                  title
                  quantity
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
          input: {
            email: 'meta-state-draft-order@example.com',
            tags: ['meta-state', 'draft-order'],
            lineItems: [
              {
                title: 'Meta state draft-order line',
                quantity: 1,
                originalUnitPrice: '4.00',
                requiresShipping: false,
                taxable: false,
                sku: 'meta-state-draft-order',
              },
            ],
          },
        },
      });

    expect(draftOrderCreateResponse.status).toBe(200);
    expect(draftOrderCreateResponse.body.data.draftOrderCreate.userErrors).toEqual([]);

    const stateResponse = await request(app).get('/__meta/state');
    const repeatedStateResponse = await request(app).get('/__meta/state');
    const logResponse = await request(app).get('/__meta/log');
    const orderId = orderCreateResponse.body.data.orderCreate.order.id;
    const draftOrderId = draftOrderCreateResponse.body.data.draftOrderCreate.draftOrder.id;

    expect(stateResponse.status).toBe(200);
    expect(stateResponse.body.baseState.orders).toEqual({});
    expect(stateResponse.body.baseState.draftOrders).toEqual({});
    expect(stateResponse.body.baseState.calculatedOrders).toEqual({});
    expect(stateResponse.body.stagedState.orders[orderId]).toMatchObject({
      id: orderId,
      name: '#1',
      note: 'meta state order',
      tags: ['meta-state', 'order'],
    });
    expect(stateResponse.body.stagedState.draftOrders[draftOrderId]).toMatchObject({
      id: draftOrderId,
      name: '#D1',
      email: 'meta-state-draft-order@example.com',
      tags: ['draft-order', 'meta-state'],
    });
    expect(stateResponse.body.stagedState.calculatedOrders).toEqual({});
    expect(repeatedStateResponse.body).toEqual(stateResponse.body);
    expect(logResponse.body.entries).toHaveLength(2);
  });

  it('replays staged mutations in original order, stops on the first upstream failure, and persists commit statuses in the log', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementationOnce(async () => {
        return new Response(
          JSON.stringify({ data: { productCreate: { product: { id: 'gid://shopify/Product/1' }, userErrors: [] } } }),
          {
            status: 200,
            headers: { 'content-type': 'application/json' },
          },
        );
      })
      .mockImplementationOnce(async () => {
        return new Response(JSON.stringify({ errors: [{ message: 'write scope denied' }] }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      });

    const app = createApp(config);
    const server = app.callback();

    const mutationDocument =
      'mutation CreateDraft($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }';

    for (const title of ['First Commit Draft', 'Second Commit Draft', 'Third Commit Draft']) {
      const createResponse = await request(server)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query: mutationDocument,
          operationName: 'CreateDraft',
          variables: {
            product: {
              title,
              status: 'DRAFT',
            },
          },
        });

      expect(createResponse.status).toBe(200);
      expect(createResponse.body.data.productCreate.userErrors).toEqual([]);
    }

    const commitResponse = await request(server)
      .post('/__meta/commit')
      .set('x-shopify-access-token', 'shpat_commit_test')
      .set('authorization', 'Bearer commit_authorization')
      .set('user-agent', 'commit-client/1.0')
      .set('x-request-id', 'commit-request-123');

    expect(commitResponse.status).toBe(200);
    expect(commitResponse.body).toEqual({
      ok: false,
      stopIndex: 1,
      attempts: [
        expect.objectContaining({
          operationName: 'productCreate',
          path: '/admin/api/2025-01/graphql.json',
          success: true,
          status: 'committed',
          upstreamStatus: 200,
          upstreamBody: { data: { productCreate: { product: { id: 'gid://shopify/Product/1' }, userErrors: [] } } },
          upstreamError: null,
          responseBody: { data: { productCreate: { product: { id: 'gid://shopify/Product/1' }, userErrors: [] } } },
        }),
        expect.objectContaining({
          operationName: 'productCreate',
          path: '/admin/api/2025-01/graphql.json',
          success: false,
          status: 'failed',
          upstreamStatus: 200,
          upstreamBody: { errors: [{ message: 'write scope denied' }] },
          upstreamError: null,
          responseBody: { errors: [{ message: 'write scope denied' }] },
        }),
      ],
    });

    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(fetchSpy.mock.calls[0]?.[0].toString()).toBe('https://example.myshopify.com/admin/api/2025-01/graphql.json');
    expect(fetchSpy.mock.calls[0]?.[1]).toMatchObject({
      method: 'POST',
      headers: {
        authorization: 'Bearer commit_authorization',
        'content-type': 'application/json',
        'user-agent': 'shopify-draft-proxy (wrapping commit-client/1.0)',
        'x-request-id': 'commit-request-123',
        'x-shopify-access-token': 'shpat_commit_test',
      },
    });
    expect(fetchSpy.mock.calls[0]?.[1]?.body).toBe(
      JSON.stringify({
        query: mutationDocument,
        variables: {
          product: {
            title: 'First Commit Draft',
            status: 'DRAFT',
          },
        },
        operationName: 'CreateDraft',
      }),
    );

    const log = await request(server).get('/__meta/log');
    expect(log.status).toBe(200);
    expect(log.body.entries).toHaveLength(3);
    expect(log.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'committed',
      'failed',
      'staged',
    ]);
  });

  it('does not replay unsupported mutations that already proxied upstream', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ data: { unsupportedMutation: { ok: true } } }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({ data: { productCreate: { product: { id: 'gid://shopify/Product/1' }, userErrors: [] } } }),
          {
            status: 200,
            headers: { 'content-type': 'application/json' },
          },
        ),
      );

    const app = createApp(config);
    const server = app.callback();

    const proxiedResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_passthrough_test')
      .send({
        query: 'mutation Passthrough { unsupportedMutation { ok } }',
      });
    const stagedResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateDraft($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Only Staged Replays',
            status: 'DRAFT',
          },
        },
      });
    const commitResponse = await request(server)
      .post('/__meta/commit')
      .set('x-shopify-access-token', 'shpat_commit_test');

    expect(proxiedResponse.status).toBe(200);
    expect(stagedResponse.status).toBe(200);
    expect(commitResponse.status).toBe(200);
    expect(commitResponse.body).toEqual({
      ok: true,
      stopIndex: null,
      attempts: [
        expect.objectContaining({
          operationName: 'productCreate',
          path: '/admin/api/2025-01/graphql.json',
          success: true,
          status: 'committed',
          upstreamStatus: 200,
          upstreamError: null,
        }),
      ],
    });
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(fetchSpy.mock.calls[0]?.[1]?.body).toBe(
      JSON.stringify({
        query: 'mutation Passthrough { unsupportedMutation { ok } }',
        variables: {},
      }),
    );
    expect(fetchSpy.mock.calls[1]?.[1]?.body).toBe(
      JSON.stringify({
        query:
          'mutation CreateDraft($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Only Staged Replays',
            status: 'DRAFT',
          },
        },
      }),
    );

    const log = await request(server).get('/__meta/log');
    expect(log.body.entries.map((entry: { status: string }) => entry.status)).toEqual(['proxied', 'committed']);
  });

  it('maps proxy-created product ids to upstream ids across chained commit replay attempts', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            data: {
              productCreate: {
                product: { id: 'gid://shopify/Product/9001', title: 'Chained Commit Draft' },
                userErrors: [],
              },
            },
          }),
          {
            status: 200,
            headers: { 'content-type': 'application/json' },
          },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            data: {
              productPublish: {
                product: { id: 'gid://shopify/Product/9001' },
                userErrors: [],
              },
            },
          }),
          {
            status: 200,
            headers: { 'content-type': 'application/json' },
          },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            data: {
              productSet: {
                product: { id: 'gid://shopify/Product/9001', title: 'Chained Commit Final' },
                productSetOperation: null,
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
    const server = app.callback();

    const createResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateDraft($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Chained Commit Draft',
            status: 'DRAFT',
          },
        },
      });
    const proxyProductId = createResponse.body.data.productCreate.product.id as string;
    const publishResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation PublishInline { productPublish(input: { id: "${proxyProductId}", productPublications: [{ publicationId: "gid://shopify/Publication/1" }] }) { product { id } userErrors { field message } } }`,
      });
    const productSetResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation ProductSetUpdate($identifier: ProductSetIdentifiers, $input: ProductSetInput!, $synchronous: Boolean!) { productSet(identifier: $identifier, input: $input, synchronous: $synchronous) { product { id title } productSetOperation { id status } userErrors { field message } } }',
        variables: {
          identifier: {
            id: proxyProductId,
          },
          input: {
            title: 'Chained Commit Final',
          },
          synchronous: true,
        },
      });

    expect(createResponse.status).toBe(200);
    expect(proxyProductId).toMatch(/^gid:\/\/shopify\/Product\/[0-9]+\?shopify-draft-proxy=synthetic$/u);
    expect(publishResponse.status).toBe(200);
    expect(publishResponse.body.data.productPublish.userErrors).toEqual([]);
    expect(productSetResponse.status).toBe(200);
    expect(productSetResponse.body.data.productSet.userErrors).toEqual([]);

    const commitResponse = await request(server)
      .post('/__meta/commit')
      .set('x-shopify-access-token', 'shpat_commit_test');

    expect(commitResponse.status).toBe(200);
    expect(commitResponse.body).toEqual({
      ok: true,
      stopIndex: null,
      attempts: [
        expect.objectContaining({
          operationName: 'productCreate',
          success: true,
          status: 'committed',
          upstreamStatus: 200,
          upstreamError: null,
        }),
        expect.objectContaining({
          operationName: 'productPublish',
          success: true,
          status: 'committed',
          upstreamStatus: 200,
          upstreamError: null,
        }),
        expect.objectContaining({
          operationName: 'productSet',
          success: true,
          status: 'committed',
          upstreamStatus: 200,
          upstreamError: null,
        }),
      ],
    });
    expect(fetchSpy).toHaveBeenCalledTimes(3);
    expect(fetchSpy.mock.calls[0]?.[1]?.body).toBe(
      JSON.stringify({
        query:
          'mutation CreateDraft($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Chained Commit Draft',
            status: 'DRAFT',
          },
        },
      }),
    );
    expect(fetchSpy.mock.calls[1]?.[1]?.body).toBe(
      JSON.stringify({
        query:
          'mutation PublishInline { productPublish(input: { id: "gid://shopify/Product/9001", productPublications: [{ publicationId: "gid://shopify/Publication/1" }] }) { product { id } userErrors { field message } } }',
      }),
    );
    expect(fetchSpy.mock.calls[2]?.[1]?.body).toBe(
      JSON.stringify({
        query:
          'mutation ProductSetUpdate($identifier: ProductSetIdentifiers, $input: ProductSetInput!, $synchronous: Boolean!) { productSet(identifier: $identifier, input: $input, synchronous: $synchronous) { product { id title } productSetOperation { id status } userErrors { field message } } }',
        variables: {
          identifier: {
            id: 'gid://shopify/Product/9001',
          },
          input: {
            title: 'Chained Commit Final',
          },
          synchronous: true,
        },
      }),
    );

    const log = await request(server).get('/__meta/log');
    expect(log.body.entries).toEqual([
      expect.objectContaining({
        status: 'committed',
        stagedResourceIds: [proxyProductId],
      }),
      expect.objectContaining({
        status: 'committed',
        stagedResourceIds: [proxyProductId],
      }),
      expect.objectContaining({
        status: 'committed',
        stagedResourceIds: [proxyProductId],
      }),
    ]);
  });

  it('records transport failures and leaves later staged mutations pending', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValueOnce(new Error('network down'));

    const app = createApp(config);
    const server = app.callback();

    for (const title of ['First Transport Failure Draft', 'Second Transport Failure Draft']) {
      const createResponse = await request(server)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query:
            'mutation CreateDraft($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
          variables: {
            product: {
              title,
              status: 'DRAFT',
            },
          },
        });

      expect(createResponse.status).toBe(200);
    }

    const commitResponse = await request(server).post('/__meta/commit');

    expect(commitResponse.status).toBe(200);
    expect(commitResponse.body).toEqual({
      ok: false,
      stopIndex: 0,
      attempts: [
        expect.objectContaining({
          operationName: 'productCreate',
          path: '/admin/api/2025-01/graphql.json',
          success: false,
          status: 'failed',
          upstreamStatus: null,
          upstreamBody: null,
          upstreamError: { message: 'network down' },
          responseBody: { errors: [{ message: 'network down' }] },
        }),
      ],
    });

    const log = await request(server).get('/__meta/log');
    expect(log.body.entries.map((entry: { status: string }) => entry.status)).toEqual(['failed', 'staged']);
  });

  it('resets staged state, hydrated cache state, mutation logs, and synthetic IDs', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            products: {
              nodes: [
                {
                  id: 'gid://shopify/Product/9001',
                  title: 'Hydrated Base Hat',
                  handle: 'hydrated-base-hat',
                  status: 'ACTIVE',
                  createdAt: '2024-02-01T00:00:00.000Z',
                  updatedAt: '2024-02-02T00:00:00.000Z',
                },
              ],
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query: 'query { products(first: 10) { nodes { id title handle status createdAt updatedAt } } }',
    });

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Staged Reset Hat" }) { product { id title createdAt } userErrors { field message } } }',
    });

    const createdProduct = createResponse.body.data.productCreate.product;
    const stateBeforeReset = await request(app).get('/__meta/state');
    const logBeforeReset = await request(app).get('/__meta/log');

    expect(stateBeforeReset.body.baseState.products['gid://shopify/Product/9001']).toMatchObject({
      title: 'Hydrated Base Hat',
    });
    expect(stateBeforeReset.body.stagedState.products[createdProduct.id]).toMatchObject({
      title: 'Staged Reset Hat',
    });
    expect(logBeforeReset.body.entries).toHaveLength(1);
    expect(logBeforeReset.body.entries[0]).toMatchObject({
      operationName: 'productCreate',
      status: 'staged',
    });

    const resetResponse = await request(app).post('/__meta/reset');
    const stateAfterReset = await request(app).get('/__meta/state');
    const logAfterReset = await request(app).get('/__meta/log');

    expect(resetResponse.status).toBe(200);
    expect(resetResponse.body).toEqual({
      ok: true,
      message: 'state reset',
    });
    expect(stateAfterReset.body).toEqual({
      baseState: emptySnapshot,
      stagedState: emptySnapshot,
    });
    expect(logAfterReset.body).toEqual({ entries: [] });

    const createAfterReset = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Staged Reset Hat" }) { product { id title createdAt } userErrors { field message } } }',
    });

    expect(createAfterReset.body.data.productCreate.product).toEqual(createdProduct);
  });

  it('exposes safe effective proxy configuration and runtime mode', async () => {
    const app = createApp({
      ...config,
      port: 4123,
      readMode: 'snapshot',
      snapshotPath: 'fixtures/snapshots/dev-store.json',
    });

    const response = await request(app.callback()).get('/__meta/config');

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      runtime: {
        readMode: 'snapshot',
      },
      proxy: {
        port: 4123,
        shopifyAdminOrigin: 'https://example.myshopify.com',
      },
      snapshot: {
        enabled: true,
        path: 'fixtures/snapshots/dev-store.json',
      },
    });
  });

  it('reports disabled snapshot configuration without inventing a path', async () => {
    const app = createApp(config);

    const response = await request(app.callback()).get('/__meta/config');

    expect(response.status).toBe(200);
    expect(response.body.snapshot).toEqual({
      enabled: false,
      path: null,
    });
  });

  it('exposes an empty ordered mutation log before anything is staged', async () => {
    const app = createApp(config);

    const response = await request(app.callback()).get('/__meta/log');

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      entries: [],
    });
  });

  it('exposes raw staged mutation documents with interpreted metadata', async () => {
    const app = createApp(config);
    const server = app.callback();
    const query =
      'mutation StageHat($title: String!) { productCreate(product: { title: $title }) { product { id title } userErrors { field message } } }';
    const variables = { title: 'Hat' };
    const secondQuery =
      'mutation StageShirt { productCreate(product: { title: "Shirt" }) { product { id title } userErrors { field message } } }';

    const mutation = await request(server).post('/admin/api/2025-01/graphql.json').send({
      query,
      variables,
    });
    const secondMutation = await request(server).post('/admin/api/2025-01/graphql.json').send({
      query: secondQuery,
    });

    expect(mutation.status).toBe(200);
    expect(secondMutation.status).toBe(200);

    const response = await request(server).get('/__meta/log');

    expect(response.status).toBe(200);
    expect(response.body.entries).toHaveLength(2);
    expect(response.body.entries.map((entry: { query: string }) => entry.query)).toEqual([query, secondQuery]);
    expect(response.body.entries[0]).toMatchObject({
      id: 'gid://shopify/MutationLogEntry/1',
      operationName: 'productCreate',
      query,
      variables,
      requestBody: {
        query,
        variables,
      },
      stagedResourceIds: [expect.stringMatching(/^gid:\/\/shopify\/Product\/[0-9]+\?shopify-draft-proxy=synthetic$/u)],
      status: 'staged',
      interpreted: {
        operationType: 'mutation',
        operationName: 'StageHat',
        rootFields: ['productCreate'],
        primaryRootField: 'productCreate',
        capability: {
          operationName: 'productCreate',
          domain: 'products',
          execution: 'stage-locally',
        },
      },
    });
    expect(response.body.entries[1]).toMatchObject({
      operationName: 'productCreate',
      query: secondQuery,
      variables: {},
      requestBody: {
        query: secondQuery,
      },
      stagedResourceIds: [expect.stringMatching(/^gid:\/\/shopify\/Product\/[0-9]+\?shopify-draft-proxy=synthetic$/u)],
      status: 'staged',
      interpreted: {
        operationType: 'mutation',
        operationName: 'StageShirt',
        rootFields: ['productCreate'],
        primaryRootField: 'productCreate',
        capability: {
          operationName: 'productCreate',
          domain: 'products',
          execution: 'stage-locally',
        },
      },
    });
  });

  it('keeps unsupported mutation passthrough visible in the inspected log', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { unsupportedMutation: { ok: true } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(config);
    const server = app.callback();
    const query = 'mutation Passthrough { unsupportedMutation { ok } }';

    const mutation = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({ query });

    expect(mutation.status).toBe(200);

    const response = await request(server).get('/__meta/log');

    expect(response.status).toBe(200);
    expect(response.body.entries).toHaveLength(1);
    expect(response.body.entries[0]).toMatchObject({
      operationName: 'Passthrough',
      query,
      variables: {},
      status: 'proxied',
      interpreted: {
        operationType: 'mutation',
        operationName: 'Passthrough',
        rootFields: ['unsupportedMutation'],
        primaryRootField: 'unsupportedMutation',
        capability: {
          operationName: 'Passthrough',
          domain: 'unknown',
          execution: 'passthrough',
        },
      },
    });
  });
});
