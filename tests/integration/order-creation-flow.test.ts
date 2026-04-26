import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import type { ProductRecord, ProductVariantRecord } from '../../src/state/types.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const liveHybridConfig: AppConfig = {
  ...snapshotConfig,
  readMode: 'live-hybrid',
};

const draftOrderCompleteNormalizedSourceName = '347082227713';

function makeProduct(id: string, title: string): ProductRecord {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? null,
    title,
    handle: title.toLowerCase().replace(/\s+/g, '-'),
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '2025-01-01T00:00:00.000Z',
    updatedAt: '2025-01-01T00:00:00.000Z',
    vendor: null,
    productType: null,
    tags: [],
    totalInventory: 8,
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
}

function makeVariant(productId: string, variantId: string): ProductVariantRecord {
  return {
    id: variantId,
    productId,
    title: 'Red / Small',
    sku: 'RICH-VARIANT-RED-SMALL',
    barcode: 'custom',
    price: '20.00',
    compareAtPrice: null,
    taxable: true,
    inventoryPolicy: 'DENY',
    inventoryQuantity: 8,
    selectedOptions: [{ name: 'Color', value: 'Red' }],
    inventoryItem: {
      id: 'gid://shopify/InventoryItem/9101',
      tracked: true,
      requiresShipping: true,
      measurement: null,
      countryCodeOfOrigin: null,
      provinceCodeOfOrigin: null,
      harmonizedSystemCode: null,
      inventoryLevels: [],
    },
  };
}

function seedDraftOrderVariantCatalog(): ProductVariantRecord {
  const product = {
    ...makeProduct('gid://shopify/Product/9001', 'Hermes stocked product'),
    legacyResourceId: null,
    handle: 'hermes-stocked-product',
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    vendor: 'Hermes',
    productType: 'Test fixture',
    tags: ['draft-order-variant'],
    totalInventory: 6,
  };
  const baseVariant = makeVariant(product.id, 'gid://shopify/ProductVariant/9002');
  const variant = {
    ...baseVariant,
    title: 'Medium / Black',
    sku: 'HERMES-STOCKED-M-BLACK',
    price: '15.50',
    inventoryQuantity: 6,
    selectedOptions: [
      { name: 'Size', value: 'Medium' },
      { name: 'Color', value: 'Black' },
    ],
    inventoryItem: {
      id: 'gid://shopify/InventoryItem/9003',
      tracked: true,
      requiresShipping: true,
      measurement: null,
      countryCodeOfOrigin: null,
      provinceCodeOfOrigin: null,
      harmonizedSystemCode: null,
      inventoryLevels: [],
    },
  };

  store.stageCreateProduct(product);
  store.replaceStagedVariantsForProduct(product.id, [variant]);
  return variant;
}

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

  it('stages rich orderCreate options, taxes, discounts, currencies, and variant-backed line items locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('rich orderCreate parity should not hit upstream in snapshot mode');
    });
    const product = makeProduct('gid://shopify/Product/9100', 'Inventory-backed coat');
    const variant = makeVariant(product.id, 'gid://shopify/ProductVariant/9100');
    store.upsertBaseProducts([product]);
    store.replaceBaseVariantsForProduct(product.id, [variant]);

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation RichOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
          orderCreate(order: $order, options: $options) {
            order {
              id
              email
              displayFinancialStatus
              displayFulfillmentStatus
              paymentGatewayNames
              subtotalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
              totalTaxSet { shopMoney { amount currencyCode } }
              totalDiscountsSet { shopMoney { amount currencyCode } }
              currentTotalPriceSet { shopMoney { amount currencyCode } }
              discountCodes
              discountApplications(first: 5) {
                nodes {
                  code
                  value {
                    ... on MoneyV2 {
                      amount
                      currencyCode
                    }
                    ... on PricingPercentageValue {
                      percentage
                    }
                  }
                }
              }
              shippingLines(first: 5) {
                nodes {
                  title
                  code
                  source
                  originalPriceSet { shopMoney { amount currencyCode } }
                  taxLines { title rate priceSet { shopMoney { amount currencyCode } } }
                }
              }
              lineItems(first: 5) {
                nodes {
                  id
                  title
                  quantity
                  sku
                  variantTitle
                  variant { id }
                  originalUnitPriceSet {
                    shopMoney { amount currencyCode }
                    presentmentMoney { amount currencyCode }
                  }
                  taxLines {
                    title
                    rate
                    priceSet { shopMoney { amount currencyCode } }
                  }
                }
              }
              transactions {
                kind
                status
                gateway
                amountSet { shopMoney { amount currencyCode } }
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          order: {
            currency: 'USD',
            presentmentCurrency: 'CAD',
            email: 'rich-order@example.com',
            fulfillmentStatus: 'FULFILLED',
            discountCode: {
              itemFixedDiscountCode: {
                code: 'SAVE5',
                amountSet: {
                  shopMoney: {
                    amount: '5.00',
                    currencyCode: 'USD',
                  },
                },
              },
            },
            shippingLines: [
              {
                title: 'Standard',
                code: 'STANDARD',
                source: 'hermes-rich-parity',
                priceSet: {
                  shopMoney: {
                    amount: '5.00',
                    currencyCode: 'USD',
                  },
                },
                taxLines: [
                  {
                    title: 'Shipping tax',
                    rate: 0.1,
                    priceSet: {
                      shopMoney: {
                        amount: '0.50',
                        currencyCode: 'USD',
                      },
                    },
                  },
                ],
              },
            ],
            lineItems: [
              {
                variantId: variant.id,
                quantity: 2,
                priceSet: {
                  shopMoney: {
                    amount: '20.00',
                    currencyCode: 'USD',
                  },
                  presentmentMoney: {
                    amount: '27.00',
                    currencyCode: 'CAD',
                  },
                },
                taxLines: [
                  {
                    title: 'Line tax',
                    rate: 0.05,
                    priceSet: {
                      shopMoney: {
                        amount: '2.00',
                        currencyCode: 'USD',
                      },
                    },
                  },
                ],
              },
            ],
            transactions: [
              {
                kind: 'SALE',
                status: 'SUCCESS',
                gateway: 'manual',
                amountSet: {
                  shopMoney: {
                    amount: '42.50',
                    currencyCode: 'USD',
                  },
                },
              },
            ],
          },
          options: {
            inventoryBehaviour: 'BYPASS',
            sendReceipt: false,
            sendFulfillmentReceipt: false,
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.orderCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.orderCreate.order).toMatchObject({
      email: 'rich-order@example.com',
      displayFinancialStatus: 'PAID',
      displayFulfillmentStatus: 'FULFILLED',
      paymentGatewayNames: ['manual'],
      subtotalPriceSet: {
        shopMoney: {
          amount: '40.0',
          currencyCode: 'USD',
        },
        presentmentMoney: null,
      },
      totalTaxSet: {
        shopMoney: {
          amount: '2.5',
          currencyCode: 'USD',
        },
      },
      totalDiscountsSet: {
        shopMoney: {
          amount: '5.0',
          currencyCode: 'USD',
        },
      },
      currentTotalPriceSet: {
        shopMoney: {
          amount: '42.5',
          currencyCode: 'USD',
        },
      },
      discountCodes: ['SAVE5'],
      discountApplications: {
        nodes: [
          {
            code: 'SAVE5',
            value: {
              amount: '5.0',
              currencyCode: 'USD',
              percentage: null,
            },
          },
        ],
      },
      shippingLines: {
        nodes: [
          {
            title: 'Standard',
            code: 'STANDARD',
            source: 'hermes-rich-parity',
            originalPriceSet: {
              shopMoney: {
                amount: '5.0',
                currencyCode: 'USD',
              },
            },
            taxLines: [
              {
                title: 'Shipping tax',
                rate: 0.1,
                priceSet: {
                  shopMoney: {
                    amount: '0.5',
                    currencyCode: 'USD',
                  },
                },
              },
            ],
          },
        ],
      },
      lineItems: {
        nodes: [
          {
            title: 'Inventory-backed coat',
            quantity: 2,
            sku: 'RICH-VARIANT-RED-SMALL',
            variantTitle: 'Red / Small',
            variant: {
              id: variant.id,
            },
            originalUnitPriceSet: {
              shopMoney: {
                amount: '20.0',
                currencyCode: 'USD',
              },
              presentmentMoney: {
                amount: '27.0',
                currencyCode: 'CAD',
              },
            },
            taxLines: [
              {
                title: 'Line tax',
                rate: 0.05,
                priceSet: {
                  shopMoney: {
                    amount: '2.0',
                    currencyCode: 'USD',
                  },
                },
              },
            ],
          },
        ],
      },
      transactions: [
        {
          kind: 'SALE',
          status: 'SUCCESS',
          gateway: 'manual',
          amountSet: {
            shopMoney: {
              amount: '42.5',
              currencyCode: 'USD',
            },
          },
        },
      ],
    });

    const orderId = createResponse.body.data.orderCreate.order.id;
    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query RichOrderCreateRead($id: ID!) {
          order(id: $id) {
            id
            totalTaxSet { shopMoney { amount currencyCode } }
            totalDiscountsSet { shopMoney { amount currencyCode } }
            discountCodes
            lineItems(first: 1) {
              nodes {
                variant { id }
                taxLines { title }
              }
            }
          }
          orders(first: 5) {
            nodes { id discountCodes }
          }
          ordersCount { count precision }
        }`,
        variables: {
          id: orderId,
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        order: {
          id: orderId,
          totalTaxSet: {
            shopMoney: {
              amount: '2.5',
              currencyCode: 'USD',
            },
          },
          totalDiscountsSet: {
            shopMoney: {
              amount: '5.0',
              currencyCode: 'USD',
            },
          },
          discountCodes: ['SAVE5'],
          lineItems: {
            nodes: [
              {
                variant: {
                  id: variant.id,
                },
                taxLines: [{ title: 'Line tax' }],
              },
            ],
          },
        },
        orders: {
          nodes: [
            {
              id: orderId,
              discountCodes: ['SAVE5'],
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

  it('returns local orderCreate userErrors for unsupported conflicting tax-line inputs without proxying upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid orderCreate tax-line request should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation InvalidOrderCreate($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id }
            userErrors { field message }
          }
        }`,
        variables: {
          order: {
            lineItems: [
              {
                title: 'Taxed line',
                quantity: 1,
                priceSet: {
                  shopMoney: {
                    amount: '10.00',
                    currencyCode: 'USD',
                  },
                },
                taxLines: [{ title: 'Line tax', rate: 0.05 }],
              },
            ],
            taxLines: [{ title: 'Order tax', rate: 0.05 }],
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        orderCreate: {
          order: null,
          userErrors: [
            {
              field: ['order', 'taxLines'],
              message: 'Tax lines can be specified on the order or on line items and shipping lines, but not both.',
            },
          ],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns the captured orderCreate no-line-items userError without staging or logging', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid orderCreate no-line-items request should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation OrderCreateNoLineItems($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id }
            userErrors { field message }
          }
        }`,
        variables: {
          order: {
            email: 'hermes-order-no-line-items@example.com',
            lineItems: [],
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        orderCreate: {
          order: null,
          userErrors: [
            {
              field: ['order', 'lineItems'],
              message: 'Line items must have at least one line item',
            },
          ],
        },
      },
    });
    expect((await request(app).get('/__meta/log')).body.entries).toEqual([]);
    expect((await request(app).get('/__meta/state')).body.stagedState.orders).toEqual({});
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages a created order locally in live-hybrid mode and serves immediate order/order(s) replay without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error(
        'orderCreate happy-path parity should not hit upstream in live-hybrid mode for supported order roots',
      );
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
            shippingLine: {
              title: 'Standard',
              code: 'custom',
              originalPriceSet: {
                shopMoney: {
                  amount: '5.0',
                  currencyCode: 'CAD',
                },
              },
            },
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
                amount: '15.0',
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

  it('stages merchant-realistic draftOrderCreate fields and replays them through detail/catalog/count locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('merchant-realistic draft-order create parity should not hit upstream in snapshot mode');
    });
    const seededVariant = seedDraftOrderVariantCatalog();

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCreateMerchantRealistic($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              name
              invoiceUrl
              status
              ready
              email
              customer {
                id
                email
                displayName
              }
              taxExempt
              taxesIncluded
              reserveInventoryUntil
              paymentTerms {
                id
                due
                overdue
                dueInDays
                paymentTermsName
                paymentTermsType
                translatedName
              }
              tags
              customAttributes {
                key
                value
              }
              appliedDiscount {
                title
                description
                value
                valueType
                amountSet {
                  shopMoney {
                    amount
                    currencyCode
                  }
                }
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
                custom
                originalPriceSet {
                  shopMoney {
                    amount
                    currencyCode
                  }
                }
                discountedPriceSet {
                  shopMoney {
                    amount
                    currencyCode
                  }
                }
              }
              subtotalPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              totalDiscountsSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              totalShippingPriceSet {
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
              totalQuantityOfLineItems
              lineItems(first: 5) {
                nodes {
                  id
                  title
                  name
                  quantity
                  sku
                  variantTitle
                  custom
                  requiresShipping
                  taxable
                  customAttributes {
                    key
                    value
                  }
                  appliedDiscount {
                    title
                    description
                    value
                    valueType
                    amountSet {
                      shopMoney {
                        amount
                        currencyCode
                      }
                    }
                  }
                  originalUnitPriceSet {
                    shopMoney {
                      amount
                      currencyCode
                    }
                  }
                  originalTotalSet {
                    shopMoney {
                      amount
                      currencyCode
                    }
                  }
                  discountedTotalSet {
                    shopMoney {
                      amount
                      currencyCode
                    }
                  }
                  totalDiscountSet {
                    shopMoney {
                      amount
                      currencyCode
                    }
                  }
                  variant {
                    id
                    title
                    sku
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
            purchasingEntity: {
              customerId: 'gid://shopify/Customer/7001',
            },
            email: 'merchant-realistic-draft@example.com',
            note: 'merchant realistic draft order create parity',
            taxExempt: true,
            reserveInventoryUntil: '2026-05-23T12:00:00Z',
            tags: ['merchant-realistic', 'draft-order'],
            customAttributes: [
              { key: 'source', value: 'phone-order' },
              { key: 'purchase-order', value: 'PO-117' },
            ],
            appliedDiscount: {
              title: 'Loyalty credit',
              description: 'merchant order-level discount',
              value: 5,
              amount: 5,
              valueType: 'FIXED_AMOUNT',
            },
            billingAddress: {
              firstName: 'Hermes',
              lastName: 'Buyer',
              address1: '123 Queen St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCode: 'CA',
              zip: 'M5H 2M9',
              phone: '+14165550101',
            },
            shippingAddress: {
              firstName: 'Hermes',
              lastName: 'Buyer',
              address1: '500 King St W',
              city: 'Toronto',
              provinceCode: 'ON',
              countryCode: 'CA',
              zip: 'M5V 1L9',
              phone: '+14165550102',
            },
            shippingLine: {
              title: 'Merchant Courier',
              priceWithCurrency: {
                amount: '7.25',
                currencyCode: 'CAD',
              },
            },
            lineItems: [
              {
                title: 'Custom installation service',
                quantity: 2,
                originalUnitPrice: '20.00',
                requiresShipping: false,
                taxable: false,
                sku: 'CUSTOM-INSTALL',
                appliedDiscount: {
                  title: 'Service discount',
                  description: '10 percent off service',
                  value: 10,
                  amount: 4,
                  valueType: 'PERCENTAGE',
                },
                customAttributes: [{ key: 'appointment', value: 'morning' }],
              },
              {
                variantId: seededVariant.id,
                quantity: 1,
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.draftOrderCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.draftOrderCreate.draftOrder).toEqual({
      id: 'gid://shopify/DraftOrder/2',
      name: '#D1',
      invoiceUrl: 'https://example.myshopify.com/draft_orders/2/invoice',
      status: 'OPEN',
      ready: true,
      email: 'merchant-realistic-draft@example.com',
      customer: {
        id: 'gid://shopify/Customer/7001',
        email: 'merchant-realistic-draft@example.com',
        displayName: 'Hermes Buyer',
      },
      taxExempt: true,
      taxesIncluded: false,
      reserveInventoryUntil: '2026-05-23T12:00:00Z',
      paymentTerms: null,
      tags: ['draft-order', 'merchant-realistic'],
      customAttributes: [
        { key: 'source', value: 'phone-order' },
        { key: 'purchase-order', value: 'PO-117' },
      ],
      appliedDiscount: {
        title: 'Loyalty credit',
        description: 'merchant order-level discount',
        value: 5,
        valueType: 'FIXED_AMOUNT',
        amountSet: {
          shopMoney: {
            amount: '5.0',
            currencyCode: 'CAD',
          },
        },
      },
      billingAddress: {
        firstName: 'Hermes',
        lastName: 'Buyer',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCodeV2: 'CA',
        zip: 'M5H 2M9',
        phone: '+14165550101',
      },
      shippingAddress: {
        firstName: 'Hermes',
        lastName: 'Buyer',
        address1: '500 King St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCodeV2: 'CA',
        zip: 'M5V 1L9',
        phone: '+14165550102',
      },
      shippingLine: {
        title: 'Merchant Courier',
        code: 'custom',
        custom: true,
        originalPriceSet: {
          shopMoney: {
            amount: '7.25',
            currencyCode: 'CAD',
          },
        },
        discountedPriceSet: {
          shopMoney: {
            amount: '7.25',
            currencyCode: 'CAD',
          },
        },
      },
      subtotalPriceSet: {
        shopMoney: {
          amount: '46.5',
          currencyCode: 'CAD',
        },
      },
      totalDiscountsSet: {
        shopMoney: {
          amount: '9.0',
          currencyCode: 'CAD',
        },
      },
      totalShippingPriceSet: {
        shopMoney: {
          amount: '7.25',
          currencyCode: 'CAD',
        },
      },
      totalPriceSet: {
        shopMoney: {
          amount: '53.75',
          currencyCode: 'CAD',
        },
      },
      totalQuantityOfLineItems: 3,
      lineItems: {
        nodes: [
          {
            id: 'gid://shopify/DraftOrderLineItem/3',
            title: 'Custom installation service',
            name: 'Custom installation service',
            quantity: 2,
            sku: 'CUSTOM-INSTALL',
            variantTitle: null,
            custom: true,
            requiresShipping: false,
            taxable: false,
            customAttributes: [{ key: 'appointment', value: 'morning' }],
            appliedDiscount: {
              title: 'Service discount',
              description: '10 percent off service',
              value: 10,
              valueType: 'PERCENTAGE',
              amountSet: {
                shopMoney: {
                  amount: '4.0',
                  currencyCode: 'CAD',
                },
              },
            },
            originalUnitPriceSet: {
              shopMoney: {
                amount: '20.0',
                currencyCode: 'CAD',
              },
            },
            originalTotalSet: {
              shopMoney: {
                amount: '40.0',
                currencyCode: 'CAD',
              },
            },
            discountedTotalSet: {
              shopMoney: {
                amount: '36.0',
                currencyCode: 'CAD',
              },
            },
            totalDiscountSet: {
              shopMoney: {
                amount: '4.0',
                currencyCode: 'CAD',
              },
            },
            variant: null,
          },
          {
            id: 'gid://shopify/DraftOrderLineItem/4',
            title: 'Hermes stocked product',
            name: 'Hermes stocked product',
            quantity: 1,
            sku: 'HERMES-STOCKED-M-BLACK',
            variantTitle: 'Medium / Black',
            custom: false,
            requiresShipping: true,
            taxable: true,
            customAttributes: [],
            appliedDiscount: null,
            originalUnitPriceSet: {
              shopMoney: {
                amount: '15.5',
                currencyCode: 'CAD',
              },
            },
            originalTotalSet: {
              shopMoney: {
                amount: '15.5',
                currencyCode: 'CAD',
              },
            },
            discountedTotalSet: {
              shopMoney: {
                amount: '15.5',
                currencyCode: 'CAD',
              },
            },
            totalDiscountSet: {
              shopMoney: {
                amount: '0.0',
                currencyCode: 'CAD',
              },
            },
            variant: {
              id: 'gid://shopify/ProductVariant/9002',
              title: 'Medium / Black',
              sku: 'HERMES-STOCKED-M-BLACK',
            },
          },
        ],
      },
    });

    const draftOrderId = createResponse.body.data.draftOrderCreate.draftOrder.id;
    const detailResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query DraftOrderMerchantRealisticDetail($id: ID!) {
          draftOrder(id: $id) {
            id
            customer {
              id
              email
              displayName
            }
            shippingLine {
              title
              code
            }
            totalPriceSet {
              shopMoney {
                amount
                currencyCode
              }
            }
            totalQuantityOfLineItems
          }
          draftOrders(first: 10) {
            nodes {
              id
              email
              totalPriceSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
          }
          draftOrdersCount {
            count
            precision
          }
        }`,
        variables: { id: draftOrderId },
      });

    expect(detailResponse.status).toBe(200);
    expect(detailResponse.body).toEqual({
      data: {
        draftOrder: {
          id: draftOrderId,
          customer: {
            id: 'gid://shopify/Customer/7001',
            email: 'merchant-realistic-draft@example.com',
            displayName: 'Hermes Buyer',
          },
          shippingLine: {
            title: 'Merchant Courier',
            code: 'custom',
          },
          totalPriceSet: {
            shopMoney: {
              amount: '53.75',
              currencyCode: 'CAD',
            },
          },
          totalQuantityOfLineItems: 3,
        },
        draftOrders: {
          nodes: [
            {
              id: draftOrderId,
              email: 'merchant-realistic-draft@example.com',
              totalPriceSet: {
                shopMoney: {
                  amount: '53.75',
                  currencyCode: 'CAD',
                },
              },
            },
          ],
        },
        draftOrdersCount: {
          count: 1,
          precision: 'EXACT',
        },
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

  it('returns Shopify-like userErrors for draftOrderCreate with no line items without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order no-line-items validation should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCreateNoLineItems($input: DraftOrderInput!) {
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
        variables: {
          input: {
            lineItems: [],
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        draftOrderCreate: {
          draftOrder: null,
          userErrors: [
            {
              field: null,
              message: 'Add at least 1 product',
            },
          ],
        },
      },
    });
    expect((await request(app).get('/__meta/log')).body.entries).toEqual([]);
    expect((await request(app).get('/__meta/state')).body.stagedState.draftOrders).toEqual({});
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('accepts variant-backed draftOrderCreate line items even when custom title and price fields are present', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order variant/custom line item parity should not hit upstream in snapshot mode');
    });
    const seededVariant = seedDraftOrderVariantCatalog();

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCreateUnsupportedLineItem($input: DraftOrderInput!) {
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
        variables: {
          input: {
            lineItems: [
              {
                variantId: seededVariant.id,
                title: 'Should not be mixed with a variant',
                originalUnitPrice: '1.00',
                quantity: 1,
              },
            ],
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        draftOrderCreate: {
          draftOrder: {
            id: 'gid://shopify/DraftOrder/2',
          },
          userErrors: [],
        },
      },
    });

    const countResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query DraftOrderCountAfterRejectedCreate {
          draftOrdersCount {
            count
            precision
          }
        }`,
      });

    expect(countResponse.status).toBe(200);
    expect(countResponse.body).toEqual({
      data: {
        draftOrdersCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns captured draftOrderCreate validation userErrors without staging or logging rejected creates', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order validation matrix should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderCreateValidationMatrix(
          $noLineItems: DraftOrderInput!
          $unknownVariant: DraftOrderInput!
          $customMissingTitle: DraftOrderInput!
          $zeroQuantity: DraftOrderInput!
          $paymentTerms: DraftOrderInput!
          $negativePrice: DraftOrderInput!
          $pastReserve: DraftOrderInput!
          $badEmail: DraftOrderInput!
        ) {
          noLineItems: draftOrderCreate(input: $noLineItems) {
            draftOrder { id }
            userErrors { field message }
          }
          unknownVariant: draftOrderCreate(input: $unknownVariant) {
            draftOrder { id }
            userErrors { field message }
          }
          customMissingTitle: draftOrderCreate(input: $customMissingTitle) {
            draftOrder { id }
            userErrors { field message }
          }
          zeroQuantity: draftOrderCreate(input: $zeroQuantity) {
            draftOrder { id }
            userErrors { field message }
          }
          paymentTerms: draftOrderCreate(input: $paymentTerms) {
            draftOrder { id }
            userErrors { field message }
          }
          negativePrice: draftOrderCreate(input: $negativePrice) {
            draftOrder { id }
            userErrors { field message }
          }
          pastReserve: draftOrderCreate(input: $pastReserve) {
            draftOrder { id }
            userErrors { field message }
          }
          badEmail: draftOrderCreate(input: $badEmail) {
            draftOrder { id }
            userErrors { field message }
          }
        }`,
        variables: {
          noLineItems: { lineItems: [] },
          unknownVariant: {
            lineItems: [{ variantId: 'gid://shopify/ProductVariant/999999999999999999', quantity: 1 }],
          },
          customMissingTitle: {
            lineItems: [{ quantity: 1, originalUnitPrice: '10.00' }],
          },
          zeroQuantity: {
            lineItems: [{ title: 'Zero quantity', quantity: 0, originalUnitPrice: '10.00' }],
          },
          paymentTerms: {
            paymentTerms: { paymentSchedules: [{ dueAt: '2026-05-22T12:00:00Z' }] },
            lineItems: [{ title: 'Payment terms', quantity: 1, originalUnitPrice: '10.00' }],
          },
          negativePrice: {
            lineItems: [{ title: 'Negative price', quantity: 1, originalUnitPrice: '-1.00' }],
          },
          pastReserve: {
            reserveInventoryUntil: '2020-01-01T00:00:00Z',
            lineItems: [{ title: 'Past reserve', quantity: 1, originalUnitPrice: '10.00' }],
          },
          badEmail: {
            email: 'not-an-email',
            lineItems: [{ title: 'Bad email', quantity: 1, originalUnitPrice: '10.00' }],
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        noLineItems: {
          draftOrder: null,
          userErrors: [{ field: null, message: 'Add at least 1 product' }],
        },
        unknownVariant: {
          draftOrder: null,
          userErrors: [{ field: null, message: 'Product with ID 999999999999999999 is no longer available.' }],
        },
        customMissingTitle: {
          draftOrder: null,
          userErrors: [{ field: null, message: 'Merchandise title is empty.' }],
        },
        zeroQuantity: {
          draftOrder: null,
          userErrors: [
            { field: ['lineItems', '0', 'quantity'], message: 'Quantity must be greater than or equal to 1' },
          ],
        },
        paymentTerms: {
          draftOrder: null,
          userErrors: [{ field: null, message: 'Payment terms template id can not be empty.' }],
        },
        negativePrice: {
          draftOrder: null,
          userErrors: [{ field: null, message: 'Cannot send negative price for line_item' }],
        },
        pastReserve: {
          draftOrder: null,
          userErrors: [{ field: null, message: "Reserve until can't be in the past" }],
        },
        badEmail: {
          draftOrder: null,
          userErrors: [{ field: ['email'], message: 'Email is invalid' }],
        },
      },
    });

    expect((await request(app).get('/__meta/log')).body.entries).toEqual([]);
    expect((await request(app).get('/__meta/state')).body.stagedState.draftOrders).toEqual({});
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

  it('filters staged draftOrders by captured query fields and applies count limits in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order filtered catalog/count replay should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createdDraftOrders: Array<{ id: string; createdAt: string; updatedAt: string }> = [];
    for (const input of [
      {
        email: 'older-filtered-draft-orders@example.com',
        tags: ['priority', 'source-alpha'],
        customAttributes: [{ key: 'source', value: 'alpha' }],
        lineItems: [
          {
            title: 'Older filtered staged draft-order row',
            quantity: 1,
            originalUnitPrice: '8.00',
            requiresShipping: false,
            taxable: false,
            sku: 'older-filtered-draft-order',
          },
        ],
      },
      {
        email: 'middle-filtered-draft-orders@example.com',
        tags: ['priority', 'source-beta'],
        customAttributes: [{ key: 'source', value: 'beta' }],
        lineItems: [
          {
            title: 'Middle filtered staged draft-order row',
            quantity: 1,
            originalUnitPrice: '9.00',
            requiresShipping: false,
            taxable: false,
            sku: 'middle-filtered-draft-order',
          },
        ],
      },
      {
        email: 'newer-filtered-draft-orders@example.com',
        tags: ['priority', 'source-alpha'],
        customAttributes: [{ key: 'source', value: 'alpha' }],
        lineItems: [
          {
            title: 'Newer filtered staged draft-order row',
            quantity: 1,
            originalUnitPrice: '10.00',
            requiresShipping: false,
            taxable: false,
            sku: 'newer-filtered-draft-order',
          },
        ],
      },
    ]) {
      const createResponse = await request(app)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query: `mutation DraftOrderCreateForFilteredReads($input: DraftOrderInput!) {
            draftOrderCreate(input: $input) {
              draftOrder {
                id
                createdAt
                updatedAt
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
      createdDraftOrders.push(createResponse.body.data.draftOrderCreate.draftOrder);
    }

    const idTail = createdDraftOrders[1]!.id.split('/').at(-1);
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query DraftOrdersFilteredSnapshot(
          $sourceQuery: String!
          $statusTagQuery: String!
          $idQuery: String!
          $createdAtQuery: String!
          $updatedAtQuery: String!
          $customerIdQuery: String!
          $limit: Int!
          $savedSearchId: ID!
        ) {
          sourceMatches: draftOrders(first: 10, query: $sourceQuery, sortKey: CREATED_AT, reverse: true) {
            edges { cursor node { id email tags createdAt updatedAt } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          statusTagMatches: draftOrders(first: 10, query: $statusTagQuery, sortKey: CREATED_AT, reverse: true) {
            nodes { id email tags }
          }
          idMatches: draftOrders(first: 2, query: $idQuery) {
            nodes { id email }
          }
          createdAtMatches: draftOrders(first: 10, query: $createdAtQuery, sortKey: CREATED_AT, reverse: true) {
            nodes { id email createdAt }
          }
          updatedAtMatches: draftOrders(first: 10, query: $updatedAtQuery, sortKey: UPDATED_AT, reverse: true) {
            nodes { id email updatedAt }
          }
          emptyCustomer: draftOrders(first: 5, query: $customerIdQuery) {
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          savedSearchEmpty: draftOrders(first: 5, savedSearchId: $savedSearchId) {
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          openLimitCount: draftOrdersCount(query: "status:open", limit: $limit) {
            count
            precision
          }
          sourceCount: draftOrdersCount(query: $sourceQuery) {
            count
            precision
          }
          emptyCustomerCount: draftOrdersCount(query: $customerIdQuery) {
            count
            precision
          }
          savedSearchCount: draftOrdersCount(savedSearchId: $savedSearchId) {
            count
            precision
          }
        }`,
        variables: {
          sourceQuery: 'source:alpha',
          statusTagQuery: 'status:open tag:priority',
          idQuery: `id:${idTail}`,
          createdAtQuery: `created_at:>=${createdDraftOrders[1]!.createdAt}`,
          updatedAtQuery: `updated_at:>=${createdDraftOrders[1]!.updatedAt}`,
          customerIdQuery: 'customer_id:0',
          limit: 2,
          savedSearchId: 'gid://shopify/SavedSearch/1',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        sourceMatches: {
          edges: [
            {
              cursor: `cursor:${createdDraftOrders[2]!.id}`,
              node: {
                id: createdDraftOrders[2]!.id,
                email: 'newer-filtered-draft-orders@example.com',
                tags: ['priority', 'source-alpha'],
                createdAt: createdDraftOrders[2]!.createdAt,
                updatedAt: createdDraftOrders[2]!.updatedAt,
              },
            },
            {
              cursor: `cursor:${createdDraftOrders[0]!.id}`,
              node: {
                id: createdDraftOrders[0]!.id,
                email: 'older-filtered-draft-orders@example.com',
                tags: ['priority', 'source-alpha'],
                createdAt: createdDraftOrders[0]!.createdAt,
                updatedAt: createdDraftOrders[0]!.updatedAt,
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: `cursor:${createdDraftOrders[2]!.id}`,
            endCursor: `cursor:${createdDraftOrders[0]!.id}`,
          },
        },
        statusTagMatches: {
          nodes: [
            {
              id: createdDraftOrders[2]!.id,
              email: 'newer-filtered-draft-orders@example.com',
              tags: ['priority', 'source-alpha'],
            },
            {
              id: createdDraftOrders[1]!.id,
              email: 'middle-filtered-draft-orders@example.com',
              tags: ['priority', 'source-beta'],
            },
            {
              id: createdDraftOrders[0]!.id,
              email: 'older-filtered-draft-orders@example.com',
              tags: ['priority', 'source-alpha'],
            },
          ],
        },
        idMatches: {
          nodes: [
            {
              id: createdDraftOrders[1]!.id,
              email: 'middle-filtered-draft-orders@example.com',
            },
          ],
        },
        createdAtMatches: {
          nodes: [
            {
              id: createdDraftOrders[2]!.id,
              email: 'newer-filtered-draft-orders@example.com',
              createdAt: createdDraftOrders[2]!.createdAt,
            },
            {
              id: createdDraftOrders[1]!.id,
              email: 'middle-filtered-draft-orders@example.com',
              createdAt: createdDraftOrders[1]!.createdAt,
            },
          ],
        },
        updatedAtMatches: {
          nodes: [
            {
              id: createdDraftOrders[2]!.id,
              email: 'newer-filtered-draft-orders@example.com',
              updatedAt: createdDraftOrders[2]!.updatedAt,
            },
            {
              id: createdDraftOrders[1]!.id,
              email: 'middle-filtered-draft-orders@example.com',
              updatedAt: createdDraftOrders[1]!.updatedAt,
            },
          ],
        },
        emptyCustomer: {
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        savedSearchEmpty: {
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        openLimitCount: {
          count: 2,
          precision: 'AT_LEAST',
        },
        sourceCount: {
          count: 2,
          precision: 'EXACT',
        },
        emptyCustomerCount: {
          count: 0,
          precision: 'EXACT',
        },
        savedSearchCount: {
          count: 0,
          precision: 'EXACT',
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
      throw new Error(
        'draft-order invalid query warning replay should not hit upstream in live-hybrid mode when staged draft orders exist',
      );
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

  it('serves supported staged draft-order query filters locally in live-hybrid mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('supported draft-order filter replay should not hit upstream in live-hybrid mode');
    });

    const app = createApp(liveHybridConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `mutation DraftOrderCreateForLiveHybridFilteredRead($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              email
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
            email: 'live-hybrid-filtered-draft-orders@example.com',
            tags: ['live-filter'],
            customAttributes: [{ key: 'source', value: 'live-hybrid' }],
            lineItems: [
              {
                title: 'Live-hybrid filtered staged draft-order row',
                quantity: 1,
                originalUnitPrice: '10.00',
                requiresShipping: false,
                taxable: false,
                sku: 'live-hybrid-filtered-draft-order',
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.draftOrderCreate.userErrors).toEqual([]);
    const draftOrderId = createResponse.body.data.draftOrderCreate.draftOrder.id;

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `query DraftOrdersLiveHybridSupportedSearch($query: String!) {
          draftOrders(first: 5, query: $query) {
            nodes {
              id
              email
              tags
            }
          }
          draftOrdersCount(query: $query) {
            count
            precision
          }
        }`,
        variables: {
          query: 'source:live-hybrid tag:live-filter status:open',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        draftOrders: {
          nodes: [
            {
              id: draftOrderId,
              email: 'live-hybrid-filtered-draft-orders@example.com',
              tags: ['live-filter'],
            },
          ],
        },
        draftOrdersCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured orderCreate missing-order INVALID_VARIABLE branch in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error(
        'orderCreate should not hit upstream in snapshot mode when the required $order variable is missing',
      );
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
      throw new Error(
        'orderCreate should not hit upstream in live-hybrid mode when the required $order variable is missing',
      );
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
          message:
            "Argument 'order' on Field 'orderCreate' has an invalid value (null). Expected type 'OrderCreateOrderInput!'.",
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
      throw new Error(
        'draftOrderCreate should not hit upstream in snapshot mode when the inline input argument is omitted',
      );
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
      throw new Error(
        'draftOrderCreate should not hit upstream in snapshot mode when the inline input argument is null',
      );
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
          message:
            "Argument 'input' on Field 'draftOrderCreate' has an invalid value (null). Expected type 'DraftOrderInput!'.",
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
      throw new Error(
        'draftOrderCreate should not hit upstream in snapshot mode when the required $input variable is missing',
      );
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
      throw new Error(
        'draftOrderComplete should not hit upstream in snapshot mode when the required $id variable is missing',
      );
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
      throw new Error(
        'draftOrderComplete should not hit upstream in live-hybrid mode when the required $id variable is missing',
      );
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
      throw new Error(
        'draftOrderComplete should not hit upstream in snapshot mode when the inline id argument is omitted',
      );
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
      throw new Error(
        'draftOrderComplete should not hit upstream in snapshot mode when the inline id argument is null',
      );
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
    const createRequest = request(app).post('/admin/api/2025-01/graphql.json');

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
          customAttributes: [{ key: 'source', value: 'draft-order-complete-test' }],
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

    const completeRequest = request(app).post('/admin/api/2025-01/graphql.json');

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
            completedAt
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
            order {
              id
              name
              sourceName
              paymentGatewayNames
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
              currentTotalPriceSet {
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
    const completedPayload = completeResponse.body.data.draftOrderComplete;
    expect(completedPayload.userErrors).toEqual([]);
    expect(completedPayload.draftOrder).toEqual({
      id: createdDraftOrderId,
      name: '#D1',
      status: 'COMPLETED',
      ready: true,
      invoiceUrl: createdInvoiceUrl,
      completedAt: expect.stringMatching(/^2024-01-01T00:00:0[1-9]\.000Z$/u),
      totalPriceSet: {
        shopMoney: {
          amount: '25.0',
          currencyCode: 'CAD',
        },
      },
      lineItems: {
        nodes: [
          {
            id: expect.any(String),
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
      order: {
        id: expect.stringMatching(/^gid:\/\/shopify\/Order\/\d+$/u),
        name: '#1',
        sourceName: draftOrderCompleteNormalizedSourceName,
        paymentGatewayNames: ['manual'],
        displayFinancialStatus: 'PAID',
        displayFulfillmentStatus: 'UNFULFILLED',
        note: 'complete this staged draft locally',
        tags: ['draft-complete', mode],
        customAttributes: [{ key: 'source', value: 'draft-order-complete-test' }],
        billingAddress: {
          firstName: 'Hermes',
          lastName: 'Closer',
          address1: '123 Queen St W',
          city: 'Toronto',
          provinceCode: 'ON',
          countryCodeV2: 'CA',
          zip: 'M5H 2M9',
          phone: '+141****0101',
        },
        shippingAddress: {
          firstName: 'Hermes',
          lastName: 'Closer',
          address1: '123 Queen St W',
          city: 'Toronto',
          provinceCode: 'ON',
          countryCodeV2: 'CA',
          zip: 'M5H 2M9',
          phone: '+141****0101',
        },
        currentTotalPriceSet: {
          shopMoney: {
            amount: '25.0',
            currencyCode: 'CAD',
          },
        },
        lineItems: {
          nodes: [
            {
              id: expect.stringMatching(/^gid:\/\/shopify\/LineItem\/\d+$/u),
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
    });
    const completedOrderId = completedPayload.draftOrder.order.id;
    const completedAt = completedPayload.draftOrder.completedAt;

    const detailRequest = request(app).post('/admin/api/2025-01/graphql.json');

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
          completedAt
          order {
            id
            name
            sourceName
            displayFinancialStatus
          }
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
          completedAt,
          order: {
            id: completedOrderId,
            name: '#1',
            sourceName: draftOrderCompleteNormalizedSourceName,
            displayFinancialStatus: 'PAID',
          },
        },
      },
    });

    const orderReadRequest = request(app).post('/admin/api/2025-01/graphql.json');

    if (mode === 'live-hybrid') {
      orderReadRequest.set('x-shopify-access-token', 'shpat_test_token');
    }

    const orderReadResponse = await orderReadRequest.send({
      query: `query DraftOrderCompletedOrderVisibility($id: ID!, $first: Int!) {
        order(id: $id) {
          id
          name
          sourceName
          displayFinancialStatus
          displayFulfillmentStatus
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
            sourceName
            displayFinancialStatus
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
        id: completedOrderId,
        first: 5,
      },
    });

    expect(orderReadResponse.status).toBe(200);
    expect(orderReadResponse.body).toEqual({
      data: {
        order: {
          id: completedOrderId,
          name: '#1',
          sourceName: draftOrderCompleteNormalizedSourceName,
          displayFinancialStatus: 'PAID',
          displayFulfillmentStatus: 'UNFULFILLED',
          note: 'complete this staged draft locally',
          tags: ['draft-complete', mode],
          currentTotalPriceSet: {
            shopMoney: {
              amount: '25.0',
              currencyCode: 'CAD',
            },
          },
        },
        orders: {
          nodes: [
            {
              id: completedOrderId,
              name: '#1',
              sourceName: draftOrderCompleteNormalizedSourceName,
              displayFinancialStatus: 'PAID',
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: `cursor:${completedOrderId}`,
            endCursor: `cursor:${completedOrderId}`,
          },
        },
        ordersCount: {
          count: 1,
          precision: 'EXACT',
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

  it('stages payment-pending draft order completion as a pending regular order in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('payment-pending draftOrderComplete should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCreateForPendingCompletion($input: DraftOrderInput!) {
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
        variables: {
          input: {
            email: 'draft-complete-pending@example.com',
            lineItems: [
              {
                title: 'Payment pending completion line item',
                quantity: 1,
                originalUnitPrice: '30.00',
                sku: 'draft-complete-pending',
              },
            ],
          },
        },
      });

    const draftOrderId = createResponse.body.data.draftOrderCreate.draftOrder.id;
    const completeResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCompletePending($id: ID!, $sourceName: String, $paymentPending: Boolean) {
          draftOrderComplete(id: $id, sourceName: $sourceName, paymentPending: $paymentPending) {
            draftOrder {
              id
              status
              ready
              order {
                id
                sourceName
                displayFinancialStatus
                currentTotalPriceSet {
                  shopMoney {
                    amount
                    currencyCode
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
          id: draftOrderId,
          sourceName: 'hermes-payment-terms',
          paymentPending: true,
        },
      });

    expect(completeResponse.status).toBe(200);
    const completedDraftOrder = completeResponse.body.data.draftOrderComplete.draftOrder;
    expect(completeResponse.body.data.draftOrderComplete.userErrors).toEqual([]);
    expect(completedDraftOrder).toEqual({
      id: draftOrderId,
      status: 'COMPLETED',
      ready: true,
      order: {
        id: expect.stringMatching(/^gid:\/\/shopify\/Order\/\d+$/u),
        sourceName: draftOrderCompleteNormalizedSourceName,
        displayFinancialStatus: 'PENDING',
        currentTotalPriceSet: {
          shopMoney: {
            amount: '30.0',
            currencyCode: 'CAD',
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns the captured invalid payment gateway userError without completing the staged draft order', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid paymentGatewayId draftOrderComplete should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCreateForInvalidGateway($input: DraftOrderInput!) {
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
        variables: {
          input: {
            email: 'draft-complete-invalid-gateway@example.com',
            lineItems: [
              {
                title: 'Invalid gateway completion line item',
                quantity: 1,
                originalUnitPrice: '20.00',
                sku: 'draft-complete-invalid-gateway',
              },
            ],
          },
        },
      });

    const draftOrderId = createResponse.body.data.draftOrderCreate.draftOrder.id;
    const completeResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftOrderCompleteInvalidGateway($id: ID!, $paymentGatewayId: ID) {
          draftOrderComplete(id: $id, paymentGatewayId: $paymentGatewayId) {
            draftOrder {
              id
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: draftOrderId,
          paymentGatewayId: 'gid://shopify/PaymentGateway/12121213',
        },
      });

    expect(completeResponse.status).toBe(200);
    expect(completeResponse.body).toEqual({
      data: {
        draftOrderComplete: {
          draftOrder: null,
          userErrors: [
            {
              field: null,
              message: 'Invalid payment gateway',
            },
          ],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query DraftOrderAfterInvalidGateway($id: ID!) {
          draftOrder(id: $id) {
            id
            status
            ready
            order {
              id
            }
          }
          ordersCount {
            count
            precision
          }
        }`,
        variables: {
          id: draftOrderId,
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        draftOrder: {
          id: draftOrderId,
          status: 'OPEN',
          ready: true,
          order: null,
        },
        ordersCount: {
          count: 0,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
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
            shippingLine: {
              title: 'Standard',
              code: 'custom',
            },
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
          shippingLine: {
            title: 'Standard',
            code: 'custom',
          },
          createdAt: '2024-01-01T00:00:01.000Z',
          updatedAt: '2024-01-01T00:00:01.000Z',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('replays locally staged draft orders through draftOrders and draftOrdersCount in live-hybrid mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error(
        'draft-order catalog/count parity should not hit upstream in live-hybrid mode for staged synthetic drafts',
      );
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
