import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import type { OrderRecord, ProductRecord, ProductVariantRecord } from '../../src/state/types.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const liveHybridConfig: AppConfig = {
  ...snapshotConfig,
  readMode: 'live-hybrid',
};

function seedBaseVariantCatalog(): ProductVariantRecord {
  const product: ProductRecord = {
    id: 'gid://shopify/Product/9001',
    legacyResourceId: '9001',
    title: 'Hermes Winter Jacket',
    handle: 'hermes-winter-jacket',
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    vendor: 'Hermes',
    productType: 'Outerwear',
    tags: [],
    totalInventory: 10,
    tracksInventory: true,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: {
      title: null,
      description: null,
    },
    category: null,
  };

  const variant: ProductVariantRecord = {
    id: 'gid://shopify/ProductVariant/9002',
    productId: product.id,
    title: 'Blue / Large',
    sku: 'hermes-jacket-blue-large',
    barcode: null,
    price: '25.00',
    compareAtPrice: null,
    taxable: true,
    inventoryPolicy: 'DENY',
    inventoryQuantity: 10,
    selectedOptions: [
      { name: 'Color', value: 'Blue' },
      { name: 'Size', value: 'Large' },
    ],
    inventoryItem: {
      id: 'gid://shopify/InventoryItem/9003',
      tracked: true,
      requiresShipping: true,
      measurement: null,
      countryCodeOfOrigin: null,
      provinceCodeOfOrigin: null,
      harmonizedSystemCode: null,
    },
  };

  store.upsertBaseProducts([product]);
  store.replaceBaseVariantsForProduct(product.id, [variant]);
  return variant;
}

async function createLocalOrder(
  app: ReturnType<ReturnType<typeof createApp>['callback']>,
  accessToken?: string,
): Promise<string> {
  const requestBuilder = request(app)
    .post('/admin/api/2025-01/graphql.json')
    .send({
      query: `mutation CreateOrderForEdit($order: OrderCreateOrderInput!) {
        orderCreate(order: $order) {
          order {
            id
            name
          }
          userErrors {
            field
            message
          }
        }
      }`,
      variables: {
        order: {
          email: 'order-edit@example.com',
          note: 'pre-edit order',
          lineItems: [
            {
              title: 'Original staged line item',
              quantity: 1,
              originalUnitPriceSet: {
                shopMoney: {
                  amount: '10.00',
                  currencyCode: 'CAD',
                },
              },
              sku: 'original-line',
            },
          ],
        },
      },
    });

  if (accessToken) {
    requestBuilder.set('x-shopify-access-token', accessToken);
  }

  const response = await requestBuilder;
  expect(response.status).toBe(200);
  expect(response.body.data.orderCreate.userErrors).toEqual([]);
  return response.body.data.orderCreate.order.id as string;
}

describe('order edit flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('mirrors the captured orderEditBegin missing-id INVALID_VARIABLE branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderEditBegin missing-id parity should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation OrderEditBeginMissingId($id: ID!) {
          orderEditBegin(id: $id) {
            calculatedOrder {
              id
            }
            userErrors {
              field
              message
            }
            successMessages
          }
        }`,
        variables: {},
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $id of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderEditBegin missing-id INVALID_VARIABLE branch in live-hybrid mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderEditBegin missing-id parity should not hit upstream in live-hybrid mode');
    });

    const app = createApp(liveHybridConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'test-token')
      .send({
        query: `mutation OrderEditBeginMissingId($id: ID!) {
          orderEditBegin(id: $id) {
            calculatedOrder {
              id
            }
            userErrors {
              field
              message
            }
            successMessages
          }
        }`,
        variables: {},
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $id of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderEditAddVariant missing-id INVALID_VARIABLE branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderEditAddVariant missing-id parity should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation OrderEditAddVariantMissingId($id: ID!, $variantId: ID!, $quantity: Int!) {
          orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity) {
            calculatedOrder {
              id
            }
            calculatedLineItem {
              id
            }
            userErrors {
              field
              message
            }
            successMessages
          }
        }`,
        variables: {
          variantId: 'gid://shopify/ProductVariant/0',
          quantity: 1,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $id of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderEditAddVariant missing-id INVALID_VARIABLE branch in live-hybrid mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderEditAddVariant missing-id parity should not hit upstream in live-hybrid mode');
    });

    const app = createApp(liveHybridConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'test-token')
      .send({
        query: `mutation OrderEditAddVariantMissingId($id: ID!, $variantId: ID!, $quantity: Int!) {
          orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity) {
            calculatedOrder {
              id
            }
            calculatedLineItem {
              id
            }
            userErrors {
              field
              message
            }
            successMessages
          }
        }`,
        variables: {
          variantId: 'gid://shopify/ProductVariant/0',
          quantity: 1,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $id of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderEditSetQuantity missing-id INVALID_VARIABLE branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderEditSetQuantity missing-id parity should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation OrderEditSetQuantityMissingId($id: ID!, $lineItemId: ID!, $quantity: Int!) {
          orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: $quantity) {
            calculatedOrder {
              id
            }
            calculatedLineItem {
              id
            }
            userErrors {
              field
              message
            }
            successMessages
          }
        }`,
        variables: {
          lineItemId: 'gid://shopify/CalculatedLineItem/0',
          quantity: 1,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $id of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderEditSetQuantity missing-id INVALID_VARIABLE branch in live-hybrid mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderEditSetQuantity missing-id parity should not hit upstream in live-hybrid mode');
    });

    const app = createApp(liveHybridConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'test-token')
      .send({
        query: `mutation OrderEditSetQuantityMissingId($id: ID!, $lineItemId: ID!, $quantity: Int!) {
          orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: $quantity) {
            calculatedOrder {
              id
            }
            calculatedLineItem {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          lineItemId: 'gid://shopify/CalculatedLineItem/0',
          quantity: 1,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $id of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderEditCommit missing-id INVALID_VARIABLE branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderEditCommit missing-id parity should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation OrderEditCommitMissingId($id: ID!, $notifyCustomer: Boolean, $staffNote: String) {
          orderEditCommit(id: $id, notifyCustomer: $notifyCustomer, staffNote: $staffNote) {
            order {
              id
              name
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          notifyCustomer: false,
          staffNote: 'missing id probe',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $id of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderEditCommit missing-id INVALID_VARIABLE branch in live-hybrid mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderEditCommit missing-id parity should not hit upstream in live-hybrid mode');
    });

    const app = createApp(liveHybridConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'test-token')
      .send({
        query: `mutation OrderEditCommitMissingId($id: ID!, $notifyCustomer: Boolean, $staffNote: String) {
          orderEditCommit(id: $id, notifyCustomer: $notifyCustomer, staffNote: $staffNote) {
            order {
              id
              name
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          notifyCustomer: false,
          staffNote: 'missing id probe',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $id of type ID! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: null,
            problems: [{ path: [], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages a happy-path orderUpdate locally in snapshot mode for a base order and replays the edited note/tags through downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderUpdate base-order parity should not hit upstream in snapshot mode');
    });

    const baseOrder: OrderRecord = {
      id: 'gid://shopify/Order/7001',
      name: '#7001',
      createdAt: '2024-01-01T00:00:00.000Z',
      updatedAt: '2024-01-01T00:00:00.000Z',
      displayFinancialStatus: 'PAID',
      displayFulfillmentStatus: 'UNFULFILLED',
      note: 'base order note',
      tags: ['base', 'existing'],
      customAttributes: [{ key: 'source', value: 'live-order' }],
      billingAddress: null,
      shippingAddress: null,
      subtotalPriceSet: {
        shopMoney: {
          amount: '10.0',
          currencyCode: 'CAD',
        },
      },
      currentTotalPriceSet: {
        shopMoney: {
          amount: '10.0',
          currencyCode: 'CAD',
        },
      },
      totalPriceSet: {
        shopMoney: {
          amount: '10.0',
          currencyCode: 'CAD',
        },
      },
      totalRefundedSet: {
        shopMoney: {
          amount: '0.0',
          currencyCode: 'CAD',
        },
      },
      customer: null,
      shippingLines: [],
      lineItems: [
        {
          id: 'gid://shopify/LineItem/7002',
          title: 'Existing order line item',
          quantity: 1,
          sku: 'existing-order-line',
          variantTitle: null,
          originalUnitPriceSet: {
            shopMoney: {
              amount: '10.0',
              currencyCode: 'CAD',
            },
          },
        },
      ],
      transactions: [],
      refunds: [],
      returns: [],
    };

    (store as unknown as { upsertBaseOrders: (orders: OrderRecord[]) => void }).upsertBaseOrders([baseOrder]);

    const app = createApp(snapshotConfig).callback();

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UpdateBaseOrder($input: OrderInput!) {
          orderUpdate(input: $input) {
            order {
              id
              name
              updatedAt
              note
              tags
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            id: baseOrder.id,
            note: 'snapshot-updated-base-order-note',
            tags: ['vip', 'edited'],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body).toEqual({
      data: {
        orderUpdate: {
          order: {
            id: baseOrder.id,
            name: '#7001',
            updatedAt: '2024-01-01T00:00:01.000Z',
            note: 'snapshot-updated-base-order-note',
            tags: ['edited', 'vip'],
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query OrderAfterBaseUpdate($id: ID!) {
          order(id: $id) {
            id
            note
            tags
          }
          orders(first: 5) {
            nodes {
              id
              note
              tags
            }
          }
          ordersCount {
            count
            precision
          }
        }`,
        variables: { id: baseOrder.id },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        order: {
          id: baseOrder.id,
          note: 'snapshot-updated-base-order-note',
          tags: ['edited', 'vip'],
        },
        orders: {
          nodes: [
            {
              id: baseOrder.id,
              note: 'snapshot-updated-base-order-note',
              tags: ['edited', 'vip'],
            },
          ],
        },
        ordersCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages a happy-path orderUpdate locally in live-hybrid mode for a base order without proxying upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderUpdate base-order parity should not hit upstream in live-hybrid mode');
    });

    const baseOrder: OrderRecord = {
      id: 'gid://shopify/Order/7101',
      name: '#7101',
      createdAt: '2024-01-01T00:00:00.000Z',
      updatedAt: '2024-01-01T00:00:00.000Z',
      displayFinancialStatus: 'PAID',
      displayFulfillmentStatus: 'UNFULFILLED',
      note: 'live-hybrid base order note',
      tags: ['base', 'hybrid'],
      customAttributes: [{ key: 'source', value: 'live-order' }],
      billingAddress: null,
      shippingAddress: null,
      subtotalPriceSet: {
        shopMoney: {
          amount: '12.0',
          currencyCode: 'CAD',
        },
      },
      currentTotalPriceSet: {
        shopMoney: {
          amount: '12.0',
          currencyCode: 'CAD',
        },
      },
      totalPriceSet: {
        shopMoney: {
          amount: '12.0',
          currencyCode: 'CAD',
        },
      },
      totalRefundedSet: {
        shopMoney: {
          amount: '0.0',
          currencyCode: 'CAD',
        },
      },
      customer: null,
      shippingLines: [],
      lineItems: [
        {
          id: 'gid://shopify/LineItem/7102',
          title: 'Existing live-hybrid order line item',
          quantity: 1,
          sku: 'existing-live-order-line',
          variantTitle: null,
          originalUnitPriceSet: {
            shopMoney: {
              amount: '12.0',
              currencyCode: 'CAD',
            },
          },
        },
      ],
      transactions: [],
      refunds: [],
      returns: [],
    };

    store.upsertBaseOrders([baseOrder]);

    const app = createApp(liveHybridConfig).callback();

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'test-token')
      .send({
        query: `mutation UpdateBaseOrderLiveHybrid($input: OrderInput!) {
          orderUpdate(input: $input) {
            order {
              id
              updatedAt
              note
              tags
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            id: baseOrder.id,
            note: 'live-hybrid-updated-base-order-note',
            tags: ['edited', 'priority'],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body).toEqual({
      data: {
        orderUpdate: {
          order: {
            id: baseOrder.id,
            updatedAt: '2024-01-01T00:00:00.000Z',
            note: 'live-hybrid-updated-base-order-note',
            tags: ['edited', 'priority'],
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'test-token')
      .send({
        query: `query BaseOrderAfterLiveHybridUpdate($id: ID!) {
          order(id: $id) {
            id
            note
            tags
          }
          orders(first: 5) {
            nodes {
              id
              note
              tags
            }
          }
        }`,
        variables: { id: baseOrder.id },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        order: {
          id: baseOrder.id,
          note: 'live-hybrid-updated-base-order-note',
          tags: ['edited', 'priority'],
        },
        orders: {
          nodes: [
            {
              id: baseOrder.id,
              note: 'live-hybrid-updated-base-order-note',
              tags: ['edited', 'priority'],
            },
          ],
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages expanded orderUpdate fields locally and replays them through downstream order reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('expanded orderUpdate parity should not hit upstream in snapshot mode');
    });

    const baseOrder: OrderRecord = {
      id: 'gid://shopify/Order/7201',
      name: '#7201',
      createdAt: '2024-01-01T00:00:00.000Z',
      updatedAt: '2024-01-01T00:00:00.000Z',
      email: 'before@example.com',
      phone: '+16135550000',
      poNumber: null,
      displayFinancialStatus: 'PAID',
      displayFulfillmentStatus: 'UNFULFILLED',
      note: 'expanded base order note',
      tags: ['base'],
      customAttributes: [{ key: 'source', value: 'base-order' }],
      metafields: [
        {
          id: 'gid://shopify/Metafield/7202',
          orderId: 'gid://shopify/Order/7201',
          namespace: 'custom',
          key: 'gift',
          type: 'single_line_text_field',
          value: 'no',
        },
      ],
      billingAddress: null,
      shippingAddress: null,
      subtotalPriceSet: {
        shopMoney: {
          amount: '14.0',
          currencyCode: 'CAD',
        },
      },
      currentTotalPriceSet: {
        shopMoney: {
          amount: '14.0',
          currencyCode: 'CAD',
        },
      },
      totalPriceSet: {
        shopMoney: {
          amount: '14.0',
          currencyCode: 'CAD',
        },
      },
      totalRefundedSet: {
        shopMoney: {
          amount: '0.0',
          currencyCode: 'CAD',
        },
      },
      customer: {
        id: 'gid://shopify/Customer/7203',
        email: 'before@example.com',
        displayName: 'Before Customer',
      },
      shippingLines: [],
      lineItems: [
        {
          id: 'gid://shopify/LineItem/7204',
          title: 'Expanded order line item',
          quantity: 1,
          sku: 'expanded-order-line',
          variantTitle: null,
          originalUnitPriceSet: {
            shopMoney: {
              amount: '14.0',
              currencyCode: 'CAD',
            },
          },
        },
      ],
      transactions: [],
      refunds: [],
      returns: [],
    };

    store.upsertBaseOrders([baseOrder]);

    const app = createApp(snapshotConfig).callback();
    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation ExpandedOrderUpdate($input: OrderInput!) {
          orderUpdate(input: $input) {
            order {
              id
              updatedAt
              email
              phone
              poNumber
              note
              tags
              customer {
                id
                email
                displayName
              }
              customAttributes {
                key
                value
              }
              shippingAddress {
                firstName
                lastName
                address1
                address2
                company
                city
                province
                provinceCode
                country
                countryCodeV2
                zip
                phone
              }
              gift: metafield(namespace: "custom", key: "gift") {
                id
                namespace
                key
                type
                value
              }
              metafields(first: 10) {
                nodes {
                  id
                  namespace
                  key
                  type
                  value
                }
                pageInfo {
                  hasNextPage
                  hasPreviousPage
                  startCursor
                  endCursor
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
            id: baseOrder.id,
            email: 'after@example.com',
            phone: '+16135551111',
            poNumber: 'PO-7201',
            note: 'expanded order update note',
            tags: ['vip', 'expanded'],
            customAttributes: [
              { key: 'source', value: 'updated-order' },
              { key: 'delivery_window', value: 'morning' },
            ],
            shippingAddress: {
              firstName: 'Ada',
              lastName: 'Lovelace',
              address1: '190 MacLaren',
              address2: 'Suite 200',
              company: 'Analytical Engines Ltd',
              city: 'Sudbury',
              province: 'Ontario',
              provinceCode: 'ON',
              country: 'Canada',
              countryCodeV2: 'CA',
              zip: 'K2P0V6',
              phone: '+16135552222',
            },
            metafields: [
              {
                namespace: 'custom',
                key: 'gift',
                type: 'single_line_text_field',
                value: 'yes',
              },
              {
                namespace: 'delivery',
                key: 'window',
                type: 'single_line_text_field',
                value: 'morning',
              },
            ],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    const updatedOrder = updateResponse.body.data.orderUpdate.order;
    expect(updatedOrder).toEqual({
      id: baseOrder.id,
      updatedAt: '2024-01-01T00:00:01.000Z',
      email: 'after@example.com',
      phone: '+16135551111',
      poNumber: 'PO-7201',
      note: 'expanded order update note',
      tags: ['expanded', 'vip'],
      customer: {
        id: 'gid://shopify/Customer/7203',
        email: 'before@example.com',
        displayName: 'Before Customer',
      },
      customAttributes: [
        { key: 'source', value: 'updated-order' },
        { key: 'delivery_window', value: 'morning' },
      ],
      shippingAddress: {
        firstName: 'Ada',
        lastName: 'Lovelace',
        address1: '190 MacLaren',
        address2: 'Suite 200',
        company: 'Analytical Engines Ltd',
        city: 'Sudbury',
        province: 'Ontario',
        provinceCode: 'ON',
        country: 'Canada',
        countryCodeV2: 'CA',
        zip: 'K2P0V6',
        phone: '+16135552222',
      },
      gift: {
        id: 'gid://shopify/Metafield/7202',
        namespace: 'custom',
        key: 'gift',
        type: 'single_line_text_field',
        value: 'yes',
      },
      metafields: {
        nodes: [
          {
            id: 'gid://shopify/Metafield/7202',
            namespace: 'custom',
            key: 'gift',
            type: 'single_line_text_field',
            value: 'yes',
          },
          {
            id: expect.stringMatching(/^gid:\/\/shopify\/Metafield\//),
            namespace: 'delivery',
            key: 'window',
            type: 'single_line_text_field',
            value: 'morning',
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: 'cursor:gid://shopify/Metafield/7202',
          endCursor: expect.stringMatching(/^cursor:gid:\/\/shopify\/Metafield\//),
        },
      },
    });
    expect(updateResponse.body.data.orderUpdate.userErrors).toEqual([]);

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query ExpandedOrderAfterUpdate($id: ID!) {
          order(id: $id) {
            id
            email
            phone
            poNumber
            note
            tags
            customer {
              email
            }
            shippingAddress {
              address1
              address2
              city
              province
              country
              zip
            }
            gift: metafield(namespace: "custom", key: "gift") {
              value
            }
            metafields(first: 10) {
              nodes {
                namespace
                key
                value
              }
            }
          }
        }`,
        variables: { id: baseOrder.id },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        order: {
          id: baseOrder.id,
          email: 'after@example.com',
          phone: '+16135551111',
          poNumber: 'PO-7201',
          note: 'expanded order update note',
          tags: ['expanded', 'vip'],
          customer: {
            email: 'before@example.com',
          },
          shippingAddress: {
            address1: '190 MacLaren',
            address2: 'Suite 200',
            city: 'Sudbury',
            province: 'Ontario',
            country: 'Canada',
            zip: 'K2P0V6',
          },
          gift: {
            value: 'yes',
          },
          metafields: {
            nodes: [
              {
                namespace: 'custom',
                key: 'gift',
                value: 'yes',
              },
              {
                namespace: 'delivery',
                key: 'window',
                value: 'morning',
              },
            ],
          },
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages a happy-path orderUpdate locally in snapshot mode for a synthetic order and replays the edited note/tags through downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderUpdate happy-path parity should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const orderId = await createLocalOrder(app);

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UpdateSyntheticOrder($input: OrderInput!) {
          orderUpdate(input: $input) {
            order {
              id
              name
              updatedAt
              note
              tags
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            id: orderId,
            note: 'snapshot-updated-order-note',
            tags: ['vip', 'edited'],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body).toEqual({
      data: {
        orderUpdate: {
          order: {
            id: orderId,
            name: '#1',
            updatedAt: '2024-01-01T00:00:03.000Z',
            note: 'snapshot-updated-order-note',
            tags: ['edited', 'vip'],
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query OrderAfterUpdate($id: ID!) {
          order(id: $id) {
            id
            note
            tags
          }
          orders(first: 5) {
            nodes {
              id
              note
              tags
            }
          }
          ordersCount {
            count
            precision
          }
        }`,
        variables: { id: orderId },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        order: {
          id: orderId,
          note: 'snapshot-updated-order-note',
          tags: ['edited', 'vip'],
        },
        orders: {
          nodes: [
            {
              id: orderId,
              note: 'snapshot-updated-order-note',
              tags: ['edited', 'vip'],
            },
          ],
        },
        ordersCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages a happy-path orderUpdate locally in live-hybrid mode for a synthetic order without proxying upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderUpdate happy-path parity should not hit upstream in live-hybrid mode for synthetic orders');
    });

    const app = createApp(liveHybridConfig).callback();
    const orderId = await createLocalOrder(app, 'test-token');

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'test-token')
      .send({
        query: `mutation UpdateSyntheticOrderLiveHybrid($input: OrderInput!) {
          orderUpdate(input: $input) {
            order {
              id
              updatedAt
              note
              tags
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            id: orderId,
            note: 'live-hybrid-updated-order-note',
            tags: ['hybrid', 'edited'],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body).toEqual({
      data: {
        orderUpdate: {
          order: {
            id: orderId,
            updatedAt: '2024-01-01T00:00:02.000Z',
            note: 'live-hybrid-updated-order-note',
            tags: ['edited', 'hybrid'],
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'test-token')
      .send({
        query: `query OrderAfterLiveHybridUpdate($id: ID!) {
          order(id: $id) {
            id
            note
            tags
          }
        }`,
        variables: { id: orderId },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        order: {
          id: orderId,
          note: 'live-hybrid-updated-order-note',
          tags: ['edited', 'hybrid'],
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages order-edit begin/add/set/commit locally in snapshot mode and applies the committed calculated line back onto the order', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('order edit flow should not hit upstream in snapshot mode');
    });

    const variant = seedBaseVariantCatalog();
    const app = createApp(snapshotConfig).callback();
    const orderId = await createLocalOrder(app);

    const beginResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation BeginOrderEdit($id: ID!) {
          orderEditBegin(id: $id) {
            calculatedOrder {
              id
              addedLineItems: lineItems(first: 10) {
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
        variables: { id: orderId },
      });

    expect(beginResponse.status).toBe(200);
    expect(beginResponse.body.data.orderEditBegin.userErrors).toEqual([]);
    expect(beginResponse.body.data.orderEditBegin.calculatedOrder.id).toMatch(/^gid:\/\/shopify\/CalculatedOrder\//);
    expect(beginResponse.body.data.orderEditBegin.calculatedOrder.addedLineItems.nodes).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/CalculatedLineItem\//),
        title: 'Original staged line item',
        quantity: 1,
      },
    ]);

    const calculatedOrderId = beginResponse.body.data.orderEditBegin.calculatedOrder.id as string;

    const addVariantResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation AddVariantToOrderEdit($id: ID!, $variantId: ID!, $quantity: Int!) {
          orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity) {
            calculatedOrder {
              id
              lineItems(first: 10) {
                nodes {
                  id
                  title
                  quantity
                  sku
                  variantTitle
                }
              }
            }
            calculatedLineItem {
              id
              title
              quantity
              sku
              variantTitle
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: calculatedOrderId,
          variantId: variant.id,
          quantity: 2,
        },
      });

    expect(addVariantResponse.status).toBe(200);
    expect(addVariantResponse.body.data.orderEditAddVariant.userErrors).toEqual([]);
    expect(addVariantResponse.body.data.orderEditAddVariant.calculatedOrder.lineItems.nodes).toHaveLength(2);
    expect(addVariantResponse.body.data.orderEditAddVariant.calculatedLineItem).toEqual({
      id: expect.stringMatching(/^gid:\/\/shopify\/CalculatedLineItem\//),
      title: 'Hermes Winter Jacket',
      quantity: 2,
      sku: 'hermes-jacket-blue-large',
      variantTitle: 'Blue / Large',
    });

    const calculatedLineItemId = addVariantResponse.body.data.orderEditAddVariant.calculatedLineItem.id as string;

    const setQuantityResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation SetOrderEditQuantity($id: ID!, $lineItemId: ID!, $quantity: Int!) {
          orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: $quantity) {
            calculatedOrder {
              id
              lineItems(first: 10) {
                nodes {
                  id
                  title
                  quantity
                }
              }
            }
            calculatedLineItem {
              id
              title
              quantity
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: calculatedOrderId,
          lineItemId: calculatedLineItemId,
          quantity: 5,
        },
      });

    expect(setQuantityResponse.status).toBe(200);
    expect(setQuantityResponse.body.data.orderEditSetQuantity.userErrors).toEqual([]);
    expect(setQuantityResponse.body.data.orderEditSetQuantity.calculatedLineItem).toEqual({
      id: calculatedLineItemId,
      title: 'Hermes Winter Jacket',
      quantity: 5,
    });

    const commitResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CommitOrderEdit($id: ID!, $notifyCustomer: Boolean, $staffNote: String) {
          orderEditCommit(id: $id, notifyCustomer: $notifyCustomer, staffNote: $staffNote) {
            order {
              id
              name
              note
              lineItems(first: 10) {
                nodes {
                  title
                  quantity
                  sku
                  variantTitle
                }
              }
            }
            userErrors {
              field
              message
            }
            successMessages
          }
        }`,
        variables: {
          id: calculatedOrderId,
          notifyCustomer: false,
          staffNote: 'committed local order edit',
        },
      });

    expect(commitResponse.status).toBe(200);
    expect(commitResponse.body.data.orderEditCommit.userErrors).toEqual([]);
    expect(commitResponse.body.data.orderEditCommit.successMessages).toEqual(['Order updated']);
    expect(commitResponse.body.data.orderEditCommit.order).toEqual({
      id: orderId,
      name: '#1',
      note: 'pre-edit order',
      lineItems: {
        nodes: [
          {
            title: 'Original staged line item',
            quantity: 1,
            sku: 'original-line',
            variantTitle: null,
          },
          {
            title: 'Hermes Winter Jacket',
            quantity: 5,
            sku: 'hermes-jacket-blue-large',
            variantTitle: 'Blue / Large',
          },
        ],
      },
    });

    const orderReadResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query ReadCommittedOrder($id: ID!) {
          order(id: $id) {
            id
            note
            lineItems(first: 10) {
              nodes {
                title
                quantity
                sku
                variantTitle
              }
            }
          }
        }`,
        variables: { id: orderId },
      });

    expect(orderReadResponse.status).toBe(200);
    expect(orderReadResponse.body.data.order.note).toBe('pre-edit order');
    expect(orderReadResponse.body.data.order.lineItems.nodes).toEqual([
      {
        title: 'Original staged line item',
        quantity: 1,
        sku: 'original-line',
        variantTitle: null,
      },
      {
        title: 'Hermes Winter Jacket',
        quantity: 5,
        sku: 'hermes-jacket-blue-large',
        variantTitle: 'Blue / Large',
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('short-circuits the local order-edit flow in live-hybrid mode for synthetic local orders without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('local order edit flow should not hit upstream in live-hybrid mode for staged ids');
    });

    const variant = seedBaseVariantCatalog();
    const app = createApp(liveHybridConfig).callback();
    const orderId = await createLocalOrder(app, 'shpat_test_token');

    const beginResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `mutation BeginOrderEdit($id: ID!) {
          orderEditBegin(id: $id) {
            calculatedOrder {
              id
            }
            userErrors {
              field
              message
            }
            successMessages
          }
        }`,
        variables: { id: orderId },
      });

    expect(beginResponse.status).toBe(200);
    expect(beginResponse.body.data.orderEditBegin.userErrors).toEqual([]);

    const calculatedOrderId = beginResponse.body.data.orderEditBegin.calculatedOrder.id as string;

    const addVariantResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `mutation AddVariantToOrderEdit($id: ID!, $variantId: ID!, $quantity: Int!) {
          orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity) {
            calculatedLineItem {
              id
              quantity
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: calculatedOrderId,
          variantId: variant.id,
          quantity: 2,
        },
      });

    const calculatedLineItemId = addVariantResponse.body.data.orderEditAddVariant.calculatedLineItem.id as string;
    expect(addVariantResponse.body.data.orderEditAddVariant.userErrors).toEqual([]);
    expect(addVariantResponse.body.data.orderEditAddVariant.calculatedLineItem.quantity).toBe(2);

    const setQuantityResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `mutation SetOrderEditQuantity($id: ID!, $lineItemId: ID!, $quantity: Int!) {
          orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: $quantity) {
            calculatedLineItem {
              id
              quantity
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: calculatedOrderId,
          lineItemId: calculatedLineItemId,
          quantity: 4,
        },
      });

    expect(setQuantityResponse.status).toBe(200);
    expect(setQuantityResponse.body.data.orderEditSetQuantity.userErrors).toEqual([]);
    expect(setQuantityResponse.body.data.orderEditSetQuantity.calculatedLineItem.quantity).toBe(4);

    const commitResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `mutation CommitOrderEdit($id: ID!, $notifyCustomer: Boolean, $staffNote: String) {
          orderEditCommit(id: $id, notifyCustomer: $notifyCustomer, staffNote: $staffNote) {
            order {
              id
              note
            }
            userErrors {
              field
              message
            }
            successMessages
          }
        }`,
        variables: {
          id: calculatedOrderId,
          notifyCustomer: true,
          staffNote: 'live-hybrid commit',
        },
      });

    expect(commitResponse.status).toBe(200);
    expect(commitResponse.body.data.orderEditCommit.userErrors).toEqual([]);
    expect(commitResponse.body.data.orderEditCommit.successMessages).toEqual(['Order updated']);
    expect(commitResponse.body.data.orderEditCommit.order).toEqual({
      id: orderId,
      note: 'pre-edit order',
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
