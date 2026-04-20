import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const liveHybridConfig: AppConfig = {
  ...snapshotConfig,
  readMode: 'live-hybrid',
};

describe('order creation flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages a created order locally in snapshot mode and replays it through order/order(s) reads without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderCreate happy-path parity should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation OrderCreateHappyPath($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              name
              createdAt
              updatedAt
              displayFinancialStatus
              displayFulfillmentStatus
              note
              tags
              customAttributes {
                key
                value
              }
              billingAddress {
                firstName
                lastName
                address1
                city
                provinceCode
                countryCodeV2
                zip
                phone
              }
              shippingAddress {
                firstName
                lastName
                address1
                city
                provinceCode
                countryCodeV2
                zip
                phone
              }
              subtotalPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              currentTotalPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              totalPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              customer {
                id
                email
                displayName
              }
              shippingLines(first: 5) {
                nodes {
                  title
                  code
                  originalPriceSet {
                    shopMoney {
                      amount
                      currencyCode
                    }
                  }
                }
              }
              lineItems(first: 5) {
                nodes {
                  id
                  title
                  quantity
                  sku
                  variantTitle
                  originalUnitPriceSet {
                    shopMoney {
                      amount
                      currencyCode
                    }
                  }
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
            email: 'hermes-order-snapshot@example.com',
            note: 'order create parity probe',
            tags: ['order-create', 'parity-probe'],
            customAttributes: [
              { key: 'source', value: 'hermes-parity-plan' },
              { key: 'channel', value: 'cron-orders-bootstrap' },
            ],
            billingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCode: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCode: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingLines: [
              {
                title: 'Standard',
                code: 'STANDARD',
                priceSet: {
                  shopMoney: {
                    amount: '5.00',
                    currencyCode: 'CAD',
                  },
                },
              },
            ],
            transactions: [
              {
                kind: 'SALE',
                status: 'SUCCESS',
                amountSet: {
                  shopMoney: {
                    amount: '15.00',
                    currencyCode: 'CAD',
                  },
                },
              },
            ],
            lineItems: [
              {
                title: 'Hermes custom order line item',
                quantity: 1,
                originalUnitPriceSet: {
                  shopMoney: {
                    amount: '10.00',
                    currencyCode: 'CAD',
                  },
                },
                sku: 'hermes-order-snapshot',
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body).toEqual({
      data: {
        orderCreate: {
          order: {
            id: 'gid://shopify/Order/2',
            name: '#1',
            createdAt: '2024-01-01T00:00:01.000Z',
            updatedAt: '2024-01-01T00:00:01.000Z',
            displayFinancialStatus: 'PAID',
            displayFulfillmentStatus: 'UNFULFILLED',
            note: 'order create parity probe',
            tags: ['order-create', 'parity-probe'],
            customAttributes: [
              { key: 'source', value: 'hermes-parity-plan' },
              { key: 'channel', value: 'cron-orders-bootstrap' },
            ],
            billingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCodeV2: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCodeV2: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            subtotalPriceSet: {
              shopMoney: {
                amount: '10.0',
                currencyCode: 'CAD',
              },
            },
            currentTotalPriceSet: {
              shopMoney: {
                amount: '15.0',
                currencyCode: 'CAD',
              },
            },
            totalPriceSet: {
              shopMoney: {
                amount: '15.0',
                currencyCode: 'CAD',
              },
            },
            customer: {
              id: 'gid://shopify/Customer/4',
              email: 'hermes-order-snapshot@example.com',
              displayName: 'Hermes Operator',
            },
            shippingLines: {
              nodes: [
                {
                  title: 'Standard',
                  code: 'STANDARD',
                  originalPriceSet: {
                    shopMoney: {
                      amount: '5.0',
                      currencyCode: 'CAD',
                    },
                  },
                },
              ],
            },
            lineItems: {
              nodes: [
                {
                  id: 'gid://shopify/LineItem/3',
                  title: 'Hermes custom order line item',
                  quantity: 1,
                  sku: 'hermes-order-snapshot',
                  variantTitle: null,
                  originalUnitPriceSet: {
                    shopMoney: {
                      amount: '10.0',
                      currencyCode: 'CAD',
                    },
                  },
                },
              ],
            },
          },
          userErrors: [],
        },
      },
    });

    const orderId = createResponse.body.data.orderCreate.order.id;
    const detailResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query OrderReadAfterCreate($id: ID!, $first: Int!) {
          order(id: $id) {
            id
            name
            note
            tags
            currentTotalPriceSet {
              shopMoney {
                amount
                currencyCode
              }
            }
          }
          orders(first: $first, sortKey: CREATED_AT, reverse: true) {
            nodes {
              id
              name
              note
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          ordersCount {
            count
            precision
          }
        }`,
        variables: {
          id: orderId,
          first: 5,
        },
      });

    expect(detailResponse.status).toBe(200);
    expect(detailResponse.body).toEqual({
      data: {
        order: {
          id: 'gid://shopify/Order/2',
          name: '#1',
          note: 'order create parity probe',
          tags: ['order-create', 'parity-probe'],
          currentTotalPriceSet: {
            shopMoney: {
              amount: '15.0',
              currencyCode: 'CAD',
            },
          },
        },
        orders: {
          nodes: [
            {
              id: 'gid://shopify/Order/2',
              name: '#1',
              note: 'order create parity probe',
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'cursor:gid://shopify/Order/2',
            endCursor: 'cursor:gid://shopify/Order/2',
          },
        },
        ordersCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages a created order locally in live-hybrid mode and serves immediate order/order(s) replay without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderCreate happy-path parity should not hit upstream in live-hybrid mode for supported order roots');
    });

    const app = createApp(liveHybridConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `mutation OrderCreateHappyPath($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              name
              note
              currentTotalPriceSet {
                shopMoney {
                  amount
                  currencyCode
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
            email: 'hermes-order-live-hybrid@example.com',
            note: 'live-hybrid order create parity probe',
            tags: ['order-create', 'live-hybrid'],
            billingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCode: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCode: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingLines: [
              {
                title: 'Standard',
                code: 'STANDARD',
                priceSet: {
                  shopMoney: {
                    amount: '5.00',
                    currencyCode: 'CAD',
                  },
                },
              },
            ],
            transactions: [
              {
                kind: 'SALE',
                status: 'SUCCESS',
                amountSet: {
                  shopMoney: {
                    amount: '15.00',
                    currencyCode: 'CAD',
                  },
                },
              },
            ],
            lineItems: [
              {
                title: 'Hermes live-hybrid custom order line item',
                quantity: 1,
                originalUnitPriceSet: {
                  shopMoney: {
                    amount: '10.00',
                    currencyCode: 'CAD',
                  },
                },
                sku: 'hermes-order-live-hybrid',
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body).toEqual({
      data: {
        orderCreate: {
          order: {
            id: 'gid://shopify/Order/1',
            name: '#1',
            note: 'live-hybrid order create parity probe',
            currentTotalPriceSet: {
              shopMoney: {
                amount: '15.0',
                currencyCode: 'CAD',
              },
            },
          },
          userErrors: [],
        },
      },
    });

    const orderId = createResponse.body.data.orderCreate.order.id;
    const detailResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `query OrderReadAfterCreate($id: ID!, $first: Int!) {
          order(id: $id) {
            id
            name
            note
          }
          orders(first: $first, sortKey: CREATED_AT, reverse: true) {
            nodes {
              id
              name
              note
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          ordersCount {
            count
            precision
          }
        }`,
        variables: {
          id: orderId,
          first: 5,
        },
      });

    expect(detailResponse.status).toBe(200);
    expect(detailResponse.body).toEqual({
      data: {
        order: {
          id: 'gid://shopify/Order/1',
          name: '#1',
          note: 'live-hybrid order create parity probe',
        },
        orders: {
          nodes: [
            {
              id: 'gid://shopify/Order/1',
              name: '#1',
              note: 'live-hybrid order create parity probe',
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'cursor:gid://shopify/Order/1',
            endCursor: 'cursor:gid://shopify/Order/1',
          },
        },
        ordersCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages a created draft order locally in snapshot mode and replays the same draft through draftOrder detail reads without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order create/detail parity should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCreateHappyPath($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              name
              invoiceUrl
              status
              email
              tags
              customAttributes {
                key
                value
              }
              billingAddress {
                firstName
                lastName
                address1
                city
                provinceCode
                countryCodeV2
                zip
                phone
              }
              shippingAddress {
                firstName
                lastName
                address1
                city
                provinceCode
                countryCodeV2
                zip
                phone
              }
              shippingLine {
                title
                code
                originalPriceSet {
                  shopMoney {
                    amount
                    currencyCode
                  }
                }
              }
              createdAt
              updatedAt
              subtotalPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              totalPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              lineItems(first: 5) {
                nodes {
                  id
                  title
                  quantity
                  sku
                  variantTitle
                  originalUnitPriceSet {
                    shopMoney {
                      amount
                      currencyCode
                    }
                  }
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
            email: 'hermes-draft-order-snapshot@example.com',
            note: 'snapshot draft order create parity',
            tags: ['parity-plan', 'draft-order'],
            customAttributes: [
              { key: 'source', value: 'hermes-parity-plan' },
              { key: 'channel', value: 'cron-orders-bootstrap' },
            ],
            billingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCode: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCode: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingLine: {
              title: 'Standard',
              priceWithCurrency: {
                amount: '5.00',
                currencyCode: 'CAD',
              },
            },
            lineItems: [
              {
                title: 'Hermes custom draft-order item',
                quantity: 1,
                originalUnitPrice: '10.00',
                requiresShipping: false,
                taxable: false,
                sku: 'hermes-draft-order-snapshot',
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body).toEqual({
      data: {
        draftOrderCreate: {
          draftOrder: {
            id: 'gid://shopify/DraftOrder/2',
            name: '#D1',
            invoiceUrl: 'https://example.myshopify.com/draft_orders/2/invoice',
            status: 'OPEN',
            email: 'hermes-draft-order-snapshot@example.com',
            tags: ['draft-order', 'parity-plan'],
            customAttributes: [
              { key: 'source', value: 'hermes-parity-plan' },
              { key: 'channel', value: 'cron-orders-bootstrap' },
            ],
            billingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCodeV2: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCodeV2: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingLine: null,
            createdAt: '2024-01-01T00:00:01.000Z',
            updatedAt: '2024-01-01T00:00:01.000Z',
            subtotalPriceSet: {
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
            lineItems: {
              nodes: [
                {
                  id: 'gid://shopify/DraftOrderLineItem/3',
                  title: 'Hermes custom draft-order item',
                  quantity: 1,
                  sku: 'hermes-draft-order-snapshot',
                  variantTitle: null,
                  originalUnitPriceSet: {
                    shopMoney: {
                      amount: '10.0',
                      currencyCode: 'CAD',
                    },
                  },
                },
              ],
            },
          },
          userErrors: [],
        },
      },
    });

    const draftOrderId = createResponse.body.data.draftOrderCreate.draftOrder.id;
    const detailResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query DraftOrderDetail($id: ID!) {
          draftOrder(id: $id) {
            id
            name
            invoiceUrl
            status
            email
            tags
            customAttributes {
              key
              value
            }
            billingAddress {
              firstName
              lastName
              address1
              city
              provinceCode
              countryCodeV2
              zip
              phone
            }
            shippingAddress {
              firstName
              lastName
              address1
              city
              provinceCode
              countryCodeV2
              zip
              phone
            }
            shippingLine {
              title
              code
              originalPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
            createdAt
            updatedAt
            subtotalPriceSet {
              shopMoney {
                amount
                currencyCode
              }
            }
            totalPriceSet {
              shopMoney {
                amount
                currencyCode
              }
            }
            lineItems(first: 5) {
              nodes {
                id
                title
                quantity
                sku
                variantTitle
                originalUnitPriceSet {
                  shopMoney {
                    amount
                    currencyCode
                  }
                }
              }
            }
          }
        }`,
        variables: { id: draftOrderId },
      });

    expect(detailResponse.status).toBe(200);
    expect(detailResponse.body).toEqual({
      data: {
        draftOrder: createResponse.body.data.draftOrderCreate.draftOrder,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('replays locally staged draft orders through draftOrders and draftOrdersCount in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order catalog/count parity should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCreateForCatalog($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              name
              status
              email
              tags
              createdAt
              updatedAt
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            email: 'snapshot-draft-orders@example.com',
            tags: ['draft-order', 'catalog'],
            lineItems: [
              {
                title: 'Snapshot draft catalog item',
                quantity: 1,
                originalUnitPrice: '10.00',
                requiresShipping: false,
                taxable: false,
                sku: 'snapshot-draft-orders',
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query DraftOrdersSnapshotCatalog {
          draftOrders(first: 10) {
            edges {
              cursor
              node {
                id
                name
                status
                email
                tags
                createdAt
                updatedAt
              }
            }
            nodes {
              id
              name
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          draftOrdersCount {
            count
            precision
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        draftOrders: {
          edges: [
            {
              cursor: 'cursor:gid://shopify/DraftOrder/2',
              node: {
                id: 'gid://shopify/DraftOrder/2',
                name: '#D1',
                status: 'OPEN',
                email: 'snapshot-draft-orders@example.com',
                tags: ['catalog', 'draft-order'],
                createdAt: '2024-01-01T00:00:01.000Z',
                updatedAt: '2024-01-01T00:00:01.000Z',
              },
            },
          ],
          nodes: [
            {
              id: 'gid://shopify/DraftOrder/2',
              name: '#D1',
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'cursor:gid://shopify/DraftOrder/2',
            endCursor: 'cursor:gid://shopify/DraftOrder/2',
          },
        },
        draftOrdersCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('respects first-window slicing for staged draftOrders connections in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order first-window replay should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createdDraftOrderIds: string[] = [];
    for (const input of [
      {
        email: 'older-draft-orders@example.com',
        tags: ['draft-order', 'catalog', 'older'],
        lineItems: [
          {
            title: 'Older staged draft-order row',
            quantity: 1,
            originalUnitPrice: '10.00',
            requiresShipping: false,
            taxable: false,
            sku: 'older-draft-order',
          },
        ],
      },
      {
        email: 'newer-draft-orders@example.com',
        tags: ['draft-order', 'catalog', 'newer'],
        lineItems: [
          {
            title: 'Newer staged draft-order row',
            quantity: 1,
            originalUnitPrice: '12.00',
            requiresShipping: false,
            taxable: false,
            sku: 'newer-draft-order',
          },
        ],
      },
    ]) {
      const createResponse = await request(app)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query: `mutation DraftOrderCreateForFirstWindow($input: DraftOrderInput!) {
            draftOrderCreate(input: $input) {
              draftOrder {
                id
              }
              userErrors {
                field
                message
              }
            }
          }`,
          variables: { input },
        });

      expect(createResponse.status).toBe(200);
      expect(createResponse.body.data.draftOrderCreate.userErrors).toEqual([]);
      createdDraftOrderIds.push(createResponse.body.data.draftOrderCreate.draftOrder.id as string);
    }

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query DraftOrdersFirstWindowSnapshot($first: Int!) {
          draftOrders(first: $first) {
            edges {
              cursor
              node {
                id
                email
                createdAt
              }
            }
            nodes {
              id
              email
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          draftOrdersCount {
            count
            precision
          }
        }`,
        variables: { first: 1 },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        draftOrders: {
          edges: [
            {
              cursor: `cursor:${createdDraftOrderIds[1]}`,
              node: {
                id: createdDraftOrderIds[1],
                email: 'newer-draft-orders@example.com',
                createdAt: '2024-01-01T00:00:03.000Z',
              },
            },
          ],
          nodes: [
            {
              id: createdDraftOrderIds[1],
              email: 'newer-draft-orders@example.com',
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: `cursor:${createdDraftOrderIds[1]}`,
            endCursor: `cursor:${createdDraftOrderIds[1]}`,
          },
        },
        draftOrdersCount: {
          count: 2,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('respects after-cursor slicing for staged draftOrders connections in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order after-cursor replay should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createdDraftOrderIds: string[] = [];
    for (const input of [
      {
        email: 'oldest-after-draft-orders@example.com',
        tags: ['draft-order', 'catalog', 'oldest'],
        lineItems: [
          {
            title: 'Oldest staged draft-order row',
            quantity: 1,
            originalUnitPrice: '8.00',
            requiresShipping: false,
            taxable: false,
            sku: 'oldest-draft-order',
          },
        ],
      },
      {
        email: 'middle-after-draft-orders@example.com',
        tags: ['draft-order', 'catalog', 'middle'],
        lineItems: [
          {
            title: 'Middle staged draft-order row',
            quantity: 1,
            originalUnitPrice: '9.00',
            requiresShipping: false,
            taxable: false,
            sku: 'middle-draft-order',
          },
        ],
      },
      {
        email: 'newest-after-draft-orders@example.com',
        tags: ['draft-order', 'catalog', 'newest'],
        lineItems: [
          {
            title: 'Newest staged draft-order row',
            quantity: 1,
            originalUnitPrice: '10.00',
            requiresShipping: false,
            taxable: false,
            sku: 'newest-draft-order',
          },
        ],
      },
    ]) {
      const createResponse = await request(app)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query: `mutation DraftOrderCreateForAfterWindow($input: DraftOrderInput!) {
            draftOrderCreate(input: $input) {
              draftOrder {
                id
              }
              userErrors {
                field
                message
              }
            }
          }`,
          variables: { input },
        });

      expect(createResponse.status).toBe(200);
      expect(createResponse.body.data.draftOrderCreate.userErrors).toEqual([]);
      createdDraftOrderIds.push(createResponse.body.data.draftOrderCreate.draftOrder.id as string);
    }

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query DraftOrdersAfterWindowSnapshot($first: Int!, $after: String!) {
          draftOrders(first: $first, after: $after) {
            edges {
              cursor
              node {
                id
                email
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
        }`,
        variables: {
          first: 1,
          after: `cursor:${createdDraftOrderIds[2]}`,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        draftOrders: {
          edges: [
            {
              cursor: `cursor:${createdDraftOrderIds[1]}`,
              node: {
                id: createdDraftOrderIds[1],
                email: 'middle-after-draft-orders@example.com',
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: true,
            startCursor: `cursor:${createdDraftOrderIds[1]}`,
            endCursor: `cursor:${createdDraftOrderIds[1]}`,
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('respects backward before/last slicing for staged draftOrders connections in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order backward-window replay should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createdDraftOrderIds: string[] = [];
    for (const input of [
      {
        email: 'oldest-before-draft-orders@example.com',
        tags: ['draft-order', 'catalog', 'oldest'],
        lineItems: [
          {
            title: 'Oldest backward staged draft-order row',
            quantity: 1,
            originalUnitPrice: '8.00',
            requiresShipping: false,
            taxable: false,
            sku: 'oldest-before-draft-order',
          },
        ],
      },
      {
        email: 'middle-before-draft-orders@example.com',
        tags: ['draft-order', 'catalog', 'middle'],
        lineItems: [
          {
            title: 'Middle backward staged draft-order row',
            quantity: 1,
            originalUnitPrice: '9.00',
            requiresShipping: false,
            taxable: false,
            sku: 'middle-before-draft-order',
          },
        ],
      },
      {
        email: 'newest-before-draft-orders@example.com',
        tags: ['draft-order', 'catalog', 'newest'],
        lineItems: [
          {
            title: 'Newest backward staged draft-order row',
            quantity: 1,
            originalUnitPrice: '10.00',
            requiresShipping: false,
            taxable: false,
            sku: 'newest-before-draft-order',
          },
        ],
      },
    ]) {
      const createResponse = await request(app)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query: `mutation DraftOrderCreateForBackwardWindow($input: DraftOrderInput!) {
            draftOrderCreate(input: $input) {
              draftOrder {
                id
              }
              userErrors {
                field
                message
              }
            }
          }`,
          variables: { input },
        });

      expect(createResponse.status).toBe(200);
      expect(createResponse.body.data.draftOrderCreate.userErrors).toEqual([]);
      createdDraftOrderIds.push(createResponse.body.data.draftOrderCreate.draftOrder.id as string);
    }

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query DraftOrdersBackwardWindowSnapshot($last: Int!, $before: String!) {
          draftOrders(last: $last, before: $before) {
            edges {
              cursor
              node {
                id
                email
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
        }`,
        variables: {
          last: 1,
          before: `cursor:${createdDraftOrderIds[1]}`,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        draftOrders: {
          edges: [
            {
              cursor: `cursor:${createdDraftOrderIds[2]}`,
              node: {
                id: createdDraftOrderIds[2],
                email: 'newest-before-draft-orders@example.com',
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: `cursor:${createdDraftOrderIds[2]}`,
            endCursor: `cursor:${createdDraftOrderIds[2]}`,
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('replays the captured draft-order email query warning locally in snapshot mode without filtering staged draft orders', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order invalid query warning replay should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createdDraftOrderIds: string[] = [];
    for (const input of [
      {
        email: 'older-email-warning-draft-orders@example.com',
        tags: ['draft-order', 'warning', 'older'],
        lineItems: [
          {
            title: 'Older invalid query warning draft order',
            quantity: 1,
            originalUnitPrice: '8.00',
            requiresShipping: false,
            taxable: false,
            sku: 'older-email-warning-draft-order',
          },
        ],
      },
      {
        email: 'newer-email-warning-draft-orders@example.com',
        tags: ['draft-order', 'warning', 'newer'],
        lineItems: [
          {
            title: 'Newer invalid query warning draft order',
            quantity: 1,
            originalUnitPrice: '9.00',
            requiresShipping: false,
            taxable: false,
            sku: 'newer-email-warning-draft-order',
          },
        ],
      },
    ]) {
      const createResponse = await request(app)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query: `mutation DraftOrderCreateForEmailWarning($input: DraftOrderInput!) {
            draftOrderCreate(input: $input) {
              draftOrder {
                id
              }
              userErrors {
                field
                message
              }
            }
          }`,
          variables: { input },
        });

      expect(createResponse.status).toBe(200);
      expect(createResponse.body.data.draftOrderCreate.userErrors).toEqual([]);
      createdDraftOrderIds.push(createResponse.body.data.draftOrderCreate.draftOrder.id as string);
    }

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query DraftOrdersInvalidEmailSearch($first: Int!, $query: String!) {
          draftOrders(first: $first, query: $query) {
            edges {
              cursor
              node {
                id
                email
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          draftOrdersCount(query: $query) {
            count
            precision
          }
        }`,
        variables: {
          first: 2,
          query: 'email:hermes@example.com',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        draftOrders: {
          edges: [
            {
              cursor: `cursor:${createdDraftOrderIds[1]}`,
              node: {
                id: createdDraftOrderIds[1],
                email: 'newer-email-warning-draft-orders@example.com',
              },
            },
            {
              cursor: `cursor:${createdDraftOrderIds[0]}`,
              node: {
                id: createdDraftOrderIds[0],
                email: 'older-email-warning-draft-orders@example.com',
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: `cursor:${createdDraftOrderIds[1]}`,
            endCursor: `cursor:${createdDraftOrderIds[0]}`,
          },
        },
        draftOrdersCount: {
          count: 2,
          precision: 'EXACT',
        },
      },
      extensions: {
        search: [
          {
            path: ['draftOrders'],
            query: 'email:hermes@example.com',
            parsed: {
              field: 'email',
              match_all: 'hermes@example.com',
            },
            warnings: [
              {
                field: 'email',
                message: 'Invalid search field for this query.',
                code: 'invalid_field',
              },
            ],
          },
          {
            path: ['draftOrdersCount'],
            query: 'email:hermes@example.com',
            parsed: {
              field: 'email',
              match_all: 'hermes@example.com',
            },
            warnings: [
              {
                field: 'email',
                message: 'Invalid search field for this query.',
                code: 'invalid_field',
              },
            ],
          },
        ],
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('replays the captured draft-order email query warning locally in live-hybrid mode without filtering staged draft orders', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order invalid query warning replay should not hit upstream in live-hybrid mode when staged draft orders exist');
    });

    const app = createApp(liveHybridConfig).callback();
    const createdDraftOrderIds: string[] = [];
    for (const input of [
      {
        email: 'older-live-hybrid-email-warning-draft-orders@example.com',
        tags: ['draft-order', 'warning', 'older'],
        lineItems: [
          {
            title: 'Older live-hybrid invalid query warning draft order',
            quantity: 1,
            originalUnitPrice: '8.00',
            requiresShipping: false,
            taxable: false,
            sku: 'older-live-hybrid-email-warning-draft-order',
          },
        ],
      },
      {
        email: 'newer-live-hybrid-email-warning-draft-orders@example.com',
        tags: ['draft-order', 'warning', 'newer'],
        lineItems: [
          {
            title: 'Newer live-hybrid invalid query warning draft order',
            quantity: 1,
            originalUnitPrice: '9.00',
            requiresShipping: false,
            taxable: false,
            sku: 'newer-live-hybrid-email-warning-draft-order',
          },
        ],
      },
    ]) {
      const createResponse = await request(app)
        .post('/admin/api/2025-01/graphql.json')
        .set('x-shopify-access-token', 'shpat_test_token')
        .send({
          query: `mutation DraftOrderCreateForLiveHybridEmailWarning($input: DraftOrderInput!) {
            draftOrderCreate(input: $input) {
              draftOrder {
                id
              }
              userErrors {
                field
                message
              }
            }
          }`,
          variables: { input },
        });

      expect(createResponse.status).toBe(200);
      expect(createResponse.body.data.draftOrderCreate.userErrors).toEqual([]);
      createdDraftOrderIds.push(createResponse.body.data.draftOrderCreate.draftOrder.id as string);
    }

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `query DraftOrdersInvalidEmailSearchLiveHybrid($first: Int!, $query: String!) {
          draftOrders(first: $first, query: $query) {
            edges {
              cursor
              node {
                id
                email
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          draftOrdersCount(query: $query) {
            count
            precision
          }
        }`,
        variables: {
          first: 2,
          query: 'email:hermes@example.com',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        draftOrders: {
          edges: [
            {
              cursor: `cursor:${createdDraftOrderIds[1]}`,
              node: {
                id: createdDraftOrderIds[1],
                email: 'newer-live-hybrid-email-warning-draft-orders@example.com',
              },
            },
            {
              cursor: `cursor:${createdDraftOrderIds[0]}`,
              node: {
                id: createdDraftOrderIds[0],
                email: 'older-live-hybrid-email-warning-draft-orders@example.com',
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: `cursor:${createdDraftOrderIds[1]}`,
            endCursor: `cursor:${createdDraftOrderIds[0]}`,
          },
        },
        draftOrdersCount: {
          count: 2,
          precision: 'EXACT',
        },
      },
      extensions: {
        search: [
          {
            path: ['draftOrders'],
            query: 'email:hermes@example.com',
            parsed: {
              field: 'email',
              match_all: 'hermes@example.com',
            },
            warnings: [
              {
                field: 'email',
                message: 'Invalid search field for this query.',
                code: 'invalid_field',
              },
            ],
          },
          {
            path: ['draftOrdersCount'],
            query: 'email:hermes@example.com',
            parsed: {
              field: 'email',
              match_all: 'hermes@example.com',
            },
            warnings: [
              {
                field: 'email',
                message: 'Invalid search field for this query.',
                code: 'invalid_field',
              },
            ],
          },
        ],
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderCreate missing-order INVALID_VARIABLE branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderCreate should not hit upstream in snapshot mode when the required $order variable is missing');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation OrderCreateMissingOrderParity($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {},
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $order of type OrderCreateOrderInput! was provided invalid value',
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

  it('mirrors the captured orderCreate missing-order INVALID_VARIABLE branch in live-hybrid mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderCreate should not hit upstream in live-hybrid mode when the required $order variable is missing');
    });

    const app = createApp(liveHybridConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `mutation OrderCreateMissingOrderParity($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {},
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $order of type OrderCreateOrderInput! was provided invalid value',
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

  it('mirrors the captured orderCreate inline missing-order-argument GraphQL validation branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderCreate should not hit upstream in snapshot mode when the inline order argument is omitted');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InlineMissingOrderArg {
          orderCreate {
            order {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: "Field 'orderCreate' is missing required arguments: order",
          path: ['mutation', 'orderCreate'],
          extensions: {
            code: 'missingRequiredArguments',
            className: 'Field',
            name: 'orderCreate',
            arguments: 'order',
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderCreate inline null-order-argument GraphQL validation branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderCreate should not hit upstream in snapshot mode when the inline order argument is null');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InlineNullOrderArg {
          orderCreate(order: null) {
            order {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: "Argument 'order' on Field 'orderCreate' has an invalid value (null). Expected type 'OrderCreateOrderInput!'.",
          path: ['mutation', 'orderCreate', 'order'],
          extensions: {
            code: 'argumentLiteralsIncompatible',
            typeName: 'Field',
            argumentName: 'order',
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured draftOrderCreate inline missing-input-argument GraphQL validation branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draftOrderCreate should not hit upstream in snapshot mode when the inline input argument is omitted');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InlineMissingDraftOrderInput {
          draftOrderCreate {
            draftOrder {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: "Field 'draftOrderCreate' is missing required arguments: input",
          path: ['mutation', 'draftOrderCreate'],
          extensions: {
            code: 'missingRequiredArguments',
            className: 'Field',
            name: 'draftOrderCreate',
            arguments: 'input',
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured draftOrderCreate inline null-input-argument GraphQL validation branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draftOrderCreate should not hit upstream in snapshot mode when the inline input argument is null');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InlineNullDraftOrderInput {
          draftOrderCreate(input: null) {
            draftOrder {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: "Argument 'input' on Field 'draftOrderCreate' has an invalid value (null). Expected type 'DraftOrderInput!'.",
          path: ['mutation', 'draftOrderCreate', 'input'],
          extensions: {
            code: 'argumentLiteralsIncompatible',
            typeName: 'Field',
            argumentName: 'input',
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured draftOrderCreate missing-input INVALID_VARIABLE branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draftOrderCreate should not hit upstream in snapshot mode when the required $input variable is missing');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCreateMissingInputParity($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {},
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: 'Variable $input of type DraftOrderInput! was provided invalid value',
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

  it('mirrors the captured draftOrderComplete missing-id INVALID_VARIABLE branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draftOrderComplete should not hit upstream in snapshot mode when the required $id variable is missing');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCompleteMissingIdParity($id: ID!, $paymentGatewayId: ID, $sourceName: String) {
          draftOrderComplete(id: $id, paymentGatewayId: $paymentGatewayId, sourceName: $sourceName) {
            draftOrder {
              id
              name
              status
              ready
              invoiceUrl
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          paymentGatewayId: null,
          sourceName: 'hermes-cron-orders',
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

  it('mirrors the captured draftOrderComplete missing-id INVALID_VARIABLE branch in live-hybrid mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draftOrderComplete should not hit upstream in live-hybrid mode when the required $id variable is missing');
    });

    const app = createApp(liveHybridConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `mutation DraftOrderCompleteMissingIdParity($id: ID!, $paymentGatewayId: ID, $sourceName: String) {
          draftOrderComplete(id: $id, paymentGatewayId: $paymentGatewayId, sourceName: $sourceName) {
            draftOrder {
              id
              name
              status
              ready
              invoiceUrl
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          paymentGatewayId: null,
          sourceName: 'hermes-cron-orders-live-hybrid',
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

  it('mirrors the captured draftOrderComplete inline missing-id-argument GraphQL validation branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draftOrderComplete should not hit upstream in snapshot mode when the inline id argument is omitted');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCompleteInlineMissingIdParity {
          draftOrderComplete(paymentGatewayId: null, sourceName: "hermes-cron-orders") {
            draftOrder {
              id
              name
              status
              ready
              invoiceUrl
            }
            userErrors {
              field
              message
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: "Field 'draftOrderComplete' is missing required arguments: id",
          path: ['mutation', 'draftOrderComplete'],
          extensions: {
            code: 'missingRequiredArguments',
            className: 'Field',
            name: 'draftOrderComplete',
            arguments: 'id',
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured draftOrderComplete inline null-id-argument GraphQL validation branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draftOrderComplete should not hit upstream in snapshot mode when the inline id argument is null');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCompleteInlineNullIdParity {
          draftOrderComplete(id: null, paymentGatewayId: null, sourceName: "hermes-cron-orders") {
            draftOrder {
              id
              name
              status
              ready
              invoiceUrl
            }
            userErrors {
              field
              message
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: "Argument 'id' on Field 'draftOrderComplete' has an invalid value (null). Expected type 'ID!'.",
          path: ['mutation', 'draftOrderComplete', 'id'],
          extensions: {
            code: 'argumentLiteralsIncompatible',
            typeName: 'Field',
            argumentName: 'id',
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  const assertLocalDraftOrderCompletion = async (mode: 'snapshot' | 'live-hybrid') => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error(`draftOrderComplete local synthetic completion should not hit upstream in ${mode} mode`);
    });

    const app = createApp(mode === 'snapshot' ? snapshotConfig : liveHybridConfig).callback();
    const createRequest = request(app)
      .post('/admin/api/2025-01/graphql.json');

    if (mode === 'live-hybrid') {
      createRequest.set('x-shopify-access-token', 'shpat_test_token');
    }

    const createResponse = await createRequest.send({
      query: `mutation DraftOrderCreateForCompletion($input: DraftOrderInput!) {
        draftOrderCreate(input: $input) {
          draftOrder {
            id
            invoiceUrl
          }
          userErrors {
            field
            message
          }
        }
      }`,
      variables: {
        input: {
          email: `draft-complete-${mode}@example.com`,
          note: 'complete this staged draft locally',
          tags: ['draft-complete', mode],
          customAttributes: [
            { key: 'source', value: 'draft-order-complete-test' },
          ],
          billingAddress: {
            firstName: 'Hermes',
            lastName: 'Closer',
            address1: '123 Queen St W',
            city: 'Toronto',
            provinceCode: 'ON',
            countryCode: 'CA',
            zip: 'M5H 2M9',
            phone: '+141****0101',
          },
          shippingAddress: {
            firstName: 'Hermes',
            lastName: 'Closer',
            address1: '123 Queen St W',
            city: 'Toronto',
            provinceCode: 'ON',
            countryCode: 'CA',
            zip: 'M5H 2M9',
            phone: '+141****0101',
          },
          lineItems: [
            {
              title: 'Hermes completion test line item',
              quantity: 2,
              originalUnitPrice: '12.50',
              sku: `draft-complete-${mode}`,
            },
          ],
        },
      },
    });

    const createdDraftOrder = createResponse.body['data']['draftOrderCreate']['draftOrder'];
    const createdDraftOrderId = createdDraftOrder['id'];
    const createdInvoiceUrl = createdDraftOrder['invoiceUrl'];

    const completeRequest = request(app)
      .post('/admin/api/2025-01/graphql.json');

    if (mode === 'live-hybrid') {
      completeRequest.set('x-shopify-access-token', 'shpat_test_token');
    }

    const completeResponse = await completeRequest.send({
      query: `mutation DraftOrderCompleteHappyPath($id: ID!, $paymentGatewayId: ID, $sourceName: String) {
        draftOrderComplete(id: $id, paymentGatewayId: $paymentGatewayId, sourceName: $sourceName) {
          draftOrder {
            id
            name
            status
            ready
            invoiceUrl
            totalPriceSet {
              shopMoney {
                amount
                currencyCode
              }
            }
            lineItems(first: 5) {
              nodes {
                id
                title
                quantity
                sku
                variantTitle
                originalUnitPriceSet {
                  shopMoney {
                    amount
                    currencyCode
                  }
                }
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
        id: createdDraftOrderId,
        paymentGatewayId: null,
        sourceName: 'hermes-cron-orders',
      },
    });

    expect(completeResponse.status).toBe(200);
    expect(completeResponse.body).toEqual({
      data: {
        draftOrderComplete: {
          draftOrder: {
            id: createdDraftOrderId,
            name: '#D1',
            status: 'COMPLETED',
            ready: true,
            invoiceUrl: createdInvoiceUrl,
            totalPriceSet: {
              shopMoney: {
                amount: '25.0',
                currencyCode: 'CAD',
              },
            },
            lineItems: {
              nodes: [
                {
                  id: 'gid://shopify/DraftOrderLineItem/3',
                  title: 'Hermes completion test line item',
                  quantity: 2,
                  sku: `draft-complete-${mode}`,
                  variantTitle: null,
                  originalUnitPriceSet: {
                    shopMoney: {
                      amount: '12.5',
                      currencyCode: 'CAD',
                    },
                  },
                },
              ],
            },
          },
          userErrors: [],
        },
      },
    });

    const detailRequest = request(app)
      .post('/admin/api/2025-01/graphql.json');

    if (mode === 'live-hybrid') {
      detailRequest.set('x-shopify-access-token', 'shpat_test_token');
    }

    const detailResponse = await detailRequest.send({
      query: `query DraftOrderCompletedDetail($id: ID!) {
        draftOrder(id: $id) {
          id
          status
          ready
          invoiceUrl
        }
      }`,
      variables: {
        id: createdDraftOrderId,
      },
    });

    expect(detailResponse.status).toBe(200);
    expect(detailResponse.body).toEqual({
      data: {
        draftOrder: {
          id: createdDraftOrderId,
          status: 'COMPLETED',
          ready: true,
          invoiceUrl: createdInvoiceUrl,
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  };

  it('completes a locally staged draft order in snapshot mode and replays the completed draft detail without hitting upstream', async () => {
    await assertLocalDraftOrderCompletion('snapshot');
  });

  it('completes a locally staged draft order in live-hybrid mode and replays the completed draft detail without hitting upstream', async () => {
    await assertLocalDraftOrderCompletion('live-hybrid');
  });

  it('stages draftOrderCreate locally in live-hybrid mode and serves immediate draftOrder detail replay without hitting upstream for supported order roots', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('supported draft-order create/detail parity should not hit upstream in live-hybrid mode');
    });

    const app = createApp(liveHybridConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `mutation DraftOrderCreateLiveHybrid($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              name
              invoiceUrl
              status
              email
              tags
              customAttributes {
                key
                value
              }
              billingAddress {
                firstName
                lastName
                address1
                city
                provinceCode
                countryCodeV2
                zip
                phone
              }
              shippingAddress {
                firstName
                lastName
                address1
                city
                provinceCode
                countryCodeV2
                zip
                phone
              }
              shippingLine {
                title
                code
              }
              createdAt
              updatedAt
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            email: 'hermes-live-hybrid-draft-order@example.com',
            note: 'live-hybrid draft order create parity',
            tags: ['parity-plan', 'draft-order', 'live-hybrid'],
            customAttributes: [
              { key: 'source', value: 'hermes-parity-plan' },
              { key: 'channel', value: 'live-hybrid-orders-bootstrap' },
            ],
            billingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCode: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCode: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingLine: {
              title: 'Standard',
              priceWithCurrency: {
                amount: '5.00',
                currencyCode: 'CAD',
              },
            },
            lineItems: [
              {
                title: 'Hermes live-hybrid draft-order item',
                quantity: 1,
                originalUnitPrice: '10.00',
                requiresShipping: false,
                taxable: false,
                sku: 'hermes-live-hybrid-draft-order',
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body).toEqual({
      data: {
        draftOrderCreate: {
          draftOrder: {
            id: 'gid://shopify/DraftOrder/2',
            name: '#D1',
            invoiceUrl: 'https://example.myshopify.com/draft_orders/2/invoice',
            status: 'OPEN',
            email: 'hermes-live-hybrid-draft-order@example.com',
            tags: ['draft-order', 'live-hybrid', 'parity-plan'],
            customAttributes: [
              { key: 'source', value: 'hermes-parity-plan' },
              { key: 'channel', value: 'live-hybrid-orders-bootstrap' },
            ],
            billingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCodeV2: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingAddress: {
              firstName: 'Hermes',
              lastName: 'Operator',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCodeV2: 'CA',
              zip: 'M5H 2M9',
              phone: '+141****0101',
            },
            shippingLine: null,
            createdAt: '2024-01-01T00:00:01.000Z',
            updatedAt: '2024-01-01T00:00:01.000Z',
          },
          userErrors: [],
        },
      },
    });

    const draftOrderId = createResponse.body.data.draftOrderCreate.draftOrder.id;
    const detailResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `query DraftOrderLiveHybridDetail($id: ID!) {
          draftOrder(id: $id) {
            id
            name
            invoiceUrl
            status
            email
            tags
            customAttributes {
              key
              value
            }
            billingAddress {
              firstName
              lastName
              address1
              city
              provinceCode
              countryCodeV2
              zip
              phone
            }
            shippingAddress {
              firstName
              lastName
              address1
              city
              provinceCode
              countryCodeV2
              zip
              phone
            }
            shippingLine {
              title
              code
            }
            createdAt
            updatedAt
          }
        }`,
        variables: {
          id: draftOrderId,
        },
      });

    expect(detailResponse.status).toBe(200);
    expect(detailResponse.body).toEqual({
      data: {
        draftOrder: {
          id: 'gid://shopify/DraftOrder/2',
          name: '#D1',
          invoiceUrl: 'https://example.myshopify.com/draft_orders/2/invoice',
          status: 'OPEN',
          email: 'hermes-live-hybrid-draft-order@example.com',
          tags: ['draft-order', 'live-hybrid', 'parity-plan'],
          customAttributes: [
            { key: 'source', value: 'hermes-parity-plan' },
            { key: 'channel', value: 'live-hybrid-orders-bootstrap' },
          ],
          billingAddress: {
            firstName: 'Hermes',
            lastName: 'Operator',
            address1: '123 Queen St W',
            city: 'Toronto',
            provinceCode: 'ON',
            countryCodeV2: 'CA',
            zip: 'M5H 2M9',
            phone: '+141****0101',
          },
          shippingAddress: {
            firstName: 'Hermes',
            lastName: 'Operator',
            address1: '123 Queen St W',
            city: 'Toronto',
            provinceCode: 'ON',
            countryCodeV2: 'CA',
            zip: 'M5H 2M9',
            phone: '+141****0101',
          },
          shippingLine: null,
          createdAt: '2024-01-01T00:00:01.000Z',
          updatedAt: '2024-01-01T00:00:01.000Z',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('replays locally staged draft orders through draftOrders and draftOrdersCount in live-hybrid mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order catalog/count parity should not hit upstream in live-hybrid mode for staged synthetic drafts');
    });

    const app = createApp(liveHybridConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `mutation DraftOrderCreateLiveHybridCatalog($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              name
              status
              email
              tags
              createdAt
              updatedAt
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            email: 'live-hybrid-draft-orders@example.com',
            tags: ['draft-order', 'catalog', 'live-hybrid'],
            lineItems: [
              {
                title: 'Live-hybrid draft catalog item',
                quantity: 1,
                originalUnitPrice: '10.00',
                requiresShipping: false,
                taxable: false,
                sku: 'live-hybrid-draft-orders',
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `query DraftOrdersLiveHybridCatalog {
          draftOrders(first: 10) {
            edges {
              cursor
              node {
                id
                name
                status
                email
                tags
                createdAt
                updatedAt
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          draftOrdersCount {
            count
            precision
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        draftOrders: {
          edges: [
            {
              cursor: 'cursor:gid://shopify/DraftOrder/2',
              node: {
                id: 'gid://shopify/DraftOrder/2',
                name: '#D1',
                status: 'OPEN',
                email: 'live-hybrid-draft-orders@example.com',
                tags: ['catalog', 'draft-order', 'live-hybrid'],
                createdAt: '2024-01-01T00:00:01.000Z',
                updatedAt: '2024-01-01T00:00:01.000Z',
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'cursor:gid://shopify/DraftOrder/2',
            endCursor: 'cursor:gid://shopify/DraftOrder/2',
          },
        },
        draftOrdersCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
