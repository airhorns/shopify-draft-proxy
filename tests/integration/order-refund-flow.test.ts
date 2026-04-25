import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

async function createRefundableOrder(app: ReturnType<typeof createApp>['callback'] extends () => infer T ? T : never) {
  return request(app)
    .post('/admin/api/2025-01/graphql.json')
    .send({
      query: `mutation CreateRefundableOrder($order: OrderCreateOrderInput!) {
        orderCreate(order: $order) {
          order {
            id
            name
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
              gateway: 'manual',
              amountSet: {
                shopMoney: {
                  amount: '25.00',
                  currencyCode: 'CAD',
                },
              },
            },
          ],
          lineItems: [
            {
              title: 'Hermes refundable item',
              quantity: 2,
              originalUnitPriceSet: {
                shopMoney: {
                  amount: '10.00',
                  currencyCode: 'CAD',
                },
              },
              sku: 'hermes-refundable',
            },
          ],
        },
      },
    });
}

describe('order refund flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns Shopify-like empty refund and return structures for staged orders without refunds', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('staged order refund reads should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await createRefundableOrder(app);
    const orderId = createResponse.body.data.orderCreate.order.id;

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query OrderRefundEmptyState($id: ID!) {
          order(id: $id) {
            id
            refunds {
              id
            }
            returns(first: 5) {
              nodes {
                id
                status
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            transactions {
              id
              kind
              status
              gateway
              amountSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
            totalRefundedSet {
              shopMoney {
                amount
                currencyCode
              }
            }
          }
        }`,
        variables: {
          id: orderId,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        order: {
          id: 'gid://shopify/Order/2',
          refunds: [],
          returns: {
            nodes: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: null,
              endCursor: null,
            },
          },
          transactions: [
            {
              id: 'gid://shopify/OrderTransaction/4',
              kind: 'SALE',
              status: 'SUCCESS',
              gateway: 'manual',
              amountSet: {
                shopMoney: {
                  amount: '25.0',
                  currencyCode: 'CAD',
                },
              },
            },
          ],
          totalRefundedSet: {
            shopMoney: {
              amount: '0.0',
              currencyCode: 'CAD',
            },
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages refundCreate locally and replays refund totals through downstream order reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('refundCreate should stage locally in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await createRefundableOrder(app);
    const order = createResponse.body.data.orderCreate.order;
    const lineItemId = order.lineItems.nodes[0].id;

    const refundResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation RefundCreateLocal($input: RefundInput!) {
          refundCreate(input: $input) {
            refund {
              id
              note
              createdAt
              updatedAt
              totalRefundedSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              refundLineItems(first: 5) {
                nodes {
                  id
                  quantity
                  restockType
                  lineItem {
                    id
                    title
                  }
                  subtotalSet {
                    shopMoney {
                      amount
                      currencyCode
                    }
                  }
                }
              }
              transactions(first: 5) {
                nodes {
                  id
                  kind
                  status
                  gateway
                  amountSet {
                    shopMoney {
                      amount
                      currencyCode
                    }
                  }
                }
              }
            }
            order {
              id
              displayFinancialStatus
              totalRefundedSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
              totalRefundedShippingSet {
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
          input: {
            orderId: order.id,
            note: 'partial line item and shipping refund',
            notify: false,
            refundLineItems: [
              {
                lineItemId,
                quantity: 1,
                restockType: 'RETURN',
                locationId: 'gid://shopify/Location/1',
              },
            ],
            shipping: {
              amount: '5.00',
            },
            transactions: [
              {
                kind: 'REFUND',
                status: 'SUCCESS',
                gateway: 'manual',
                amountSet: {
                  shopMoney: {
                    amount: '15.00',
                    currencyCode: 'CAD',
                  },
                },
              },
            ],
          },
        },
      });

    expect(refundResponse.status).toBe(200);
    expect(refundResponse.body).toEqual({
      data: {
        refundCreate: {
          refund: {
            id: 'gid://shopify/Refund/6',
            note: 'partial line item and shipping refund',
            createdAt: '2024-01-01T00:00:03.000Z',
            updatedAt: '2024-01-01T00:00:03.000Z',
            totalRefundedSet: {
              shopMoney: {
                amount: '15.0',
                currencyCode: 'CAD',
              },
            },
            refundLineItems: {
              nodes: [
                {
                  id: 'gid://shopify/RefundLineItem/7',
                  quantity: 1,
                  restockType: 'RETURN',
                  lineItem: {
                    id: lineItemId,
                    title: 'Hermes refundable item',
                  },
                  subtotalSet: {
                    shopMoney: {
                      amount: '10.0',
                      currencyCode: 'CAD',
                    },
                  },
                },
              ],
            },
            transactions: {
              nodes: [
                {
                  id: 'gid://shopify/OrderTransaction/8',
                  kind: 'REFUND',
                  status: 'SUCCESS',
                  gateway: 'manual',
                  amountSet: {
                    shopMoney: {
                      amount: '15.0',
                      currencyCode: 'CAD',
                    },
                  },
                },
              ],
            },
          },
          order: {
            id: order.id,
            displayFinancialStatus: 'PARTIALLY_REFUNDED',
            totalRefundedSet: {
              shopMoney: {
                amount: '15.0',
                currencyCode: 'CAD',
              },
            },
            totalRefundedShippingSet: {
              shopMoney: {
                amount: '5.0',
                currencyCode: 'CAD',
              },
            },
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query OrderReadAfterRefund($id: ID!) {
          order(id: $id) {
            id
            displayFinancialStatus
            refunds {
              id
              note
              totalRefundedSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
            transactions {
              id
              kind
              status
              gateway
              amountSet {
                shopMoney {
                  amount
                  currencyCode
                }
              }
            }
            returns(first: 5) {
              nodes {
                id
                status
              }
            }
            totalRefundedSet {
              shopMoney {
                amount
                currencyCode
              }
            }
            totalRefundedShippingSet {
              shopMoney {
                amount
                currencyCode
              }
            }
          }
        }`,
        variables: {
          id: order.id,
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.order).toMatchObject({
      id: order.id,
      displayFinancialStatus: 'PARTIALLY_REFUNDED',
      refunds: [
        {
          id: 'gid://shopify/Refund/6',
          note: 'partial line item and shipping refund',
          totalRefundedSet: {
            shopMoney: {
              amount: '15.0',
              currencyCode: 'CAD',
            },
          },
        },
      ],
      returns: {
        nodes: [],
      },
      totalRefundedSet: {
        shopMoney: {
          amount: '15.0',
          currencyCode: 'CAD',
        },
      },
      totalRefundedShippingSet: {
        shopMoney: {
          amount: '5.0',
          currencyCode: 'CAD',
        },
      },
    });
    expect(readResponse.body.data.order.transactions).toHaveLength(2);
    expect(readResponse.body.data.order.transactions[1]).toMatchObject({
      id: 'gid://shopify/OrderTransaction/8',
      kind: 'REFUND',
      status: 'SUCCESS',
      gateway: 'manual',
      amountSet: {
        shopMoney: {
          amount: '15.0',
          currencyCode: 'CAD',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns a Shopify-like userError when refundCreate would over-refund the staged order', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid refundCreate should not hit upstream in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await createRefundableOrder(app);
    const order = createResponse.body.data.orderCreate.order;

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation RefundCreateOverRefund($input: RefundInput!) {
          refundCreate(input: $input) {
            refund {
              id
            }
            order {
              id
              totalRefundedSet {
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
          input: {
            orderId: order.id,
            transactions: [
              {
                kind: 'REFUND',
                status: 'SUCCESS',
                gateway: 'manual',
                amountSet: {
                  shopMoney: {
                    amount: '30.00',
                    currencyCode: 'CAD',
                  },
                },
              },
            ],
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        refundCreate: {
          refund: null,
          order: {
            id: order.id,
            totalRefundedSet: {
              shopMoney: {
                amount: '0.0',
                currencyCode: 'CAD',
              },
            },
          },
          userErrors: [
            {
              field: null,
              message: 'Refund amount $30.00 is greater than net payment received $25.00',
            },
          ],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
