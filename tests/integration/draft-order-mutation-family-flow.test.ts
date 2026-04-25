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

const draftOrderSelection = `{
  id
  name
  invoiceUrl
  status
  ready
  email
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
}`;

async function createDraftOrder(app: ReturnType<typeof createApp>['callback'] extends () => infer T ? T : never) {
  return request(app)
    .post('/admin/api/2026-04/graphql.json')
    .send({
      query: `mutation DraftOrderCreate($input: DraftOrderInput!) {
        draftOrderCreate(input: $input) {
          draftOrder ${draftOrderSelection}
          userErrors {
            field
            message
          }
        }
      }`,
      variables: {
        input: {
          email: 'draft-family@example.com',
          note: 'initial note',
          tags: ['initial', 'draft'],
          customAttributes: [{ key: 'source', value: 'har-118' }],
          billingAddress: {
            firstName: 'Draft',
            lastName: 'Family',
            address1: '123 Queen St W',
            city: 'Toronto',
            provinceCode: 'ON',
            countryCode: 'CA',
            zip: 'M5H 2M9',
            phone: '+14165550101',
          },
          shippingAddress: {
            firstName: 'Ship',
            lastName: 'Family',
            address1: '456 King St W',
            city: 'Toronto',
            provinceCode: 'ON',
            countryCode: 'CA',
            zip: 'M5V 1K4',
            phone: '+14165550102',
          },
          lineItems: [
            {
              title: 'Initial custom item',
              quantity: 1,
              originalUnitPrice: '10.00',
              sku: 'HAR-118-INITIAL',
            },
          ],
        },
      },
    });
}

async function createNoRecipientDraftOrder(
  app: ReturnType<typeof createApp>['callback'] extends () => infer T ? T : never,
  label: string,
) {
  return request(app)
    .post('/admin/api/2026-04/graphql.json')
    .send({
      query: `mutation DraftOrderCreate($input: DraftOrderInput!) {
        draftOrderCreate(input: $input) {
          draftOrder {
            id
            status
            email
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
          note: `invoice safety ${label}`,
          tags: ['invoice-safety', label],
          lineItems: [
            {
              title: `Invoice safety item ${label}`,
              quantity: 1,
              originalUnitPrice: '1.00',
            },
          ],
        },
      },
    });
}

describe('draft-order mutation family flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages draftOrderUpdate, draftOrderDuplicate, and draftOrderDelete locally for synthetic drafts', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order mutation family should not hit upstream in snapshot mode');
    });
    const app = createApp(snapshotConfig).callback();

    const createResponse = await createDraftOrder(app);
    expect(createResponse.status).toBe(200);
    const draftOrderId = createResponse.body.data.draftOrderCreate.draftOrder.id as string;

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderUpdate($id: ID!, $input: DraftOrderInput!) {
          draftOrderUpdate(id: $id, input: $input) {
            draftOrder ${draftOrderSelection}
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: draftOrderId,
          input: {
            email: 'updated-draft-family@example.com',
            note: 'updated note',
            tags: ['updated', 'draft'],
            customAttributes: [{ key: 'source', value: 'har-118-update' }],
            shippingLine: {
              title: 'Standard',
              code: 'STD',
              priceWithCurrency: {
                amount: '5.00',
                currencyCode: 'CAD',
              },
            },
            lineItems: [
              {
                title: 'Updated custom item',
                quantity: 2,
                originalUnitPrice: '12.50',
                sku: 'HAR-118-UPDATED',
              },
            ],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.draftOrderUpdate.userErrors).toEqual([]);
    expect(updateResponse.body.data.draftOrderUpdate.draftOrder).toMatchObject({
      id: draftOrderId,
      email: 'updated-draft-family@example.com',
      note: 'updated note',
      tags: ['draft', 'updated'],
      customAttributes: [{ key: 'source', value: 'har-118-update' }],
      shippingLine: {
        title: 'Standard',
        code: 'STD',
        originalPriceSet: {
          shopMoney: {
            amount: '5.0',
            currencyCode: 'CAD',
          },
        },
      },
      subtotalPriceSet: {
        shopMoney: {
          amount: '25.0',
          currencyCode: 'CAD',
        },
      },
      totalPriceSet: {
        shopMoney: {
          amount: '30.0',
          currencyCode: 'CAD',
        },
      },
    });
    expect(updateResponse.body.data.draftOrderUpdate.draftOrder.lineItems.nodes).toEqual([
      expect.objectContaining({
        title: 'Updated custom item',
        quantity: 2,
        sku: 'HAR-118-UPDATED',
      }),
    ]);

    const duplicateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderDuplicate($id: ID) {
          draftOrderDuplicate(id: $id) {
            draftOrder ${draftOrderSelection}
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: draftOrderId,
        },
      });

    expect(duplicateResponse.status).toBe(200);
    const duplicatedDraftOrder = duplicateResponse.body.data.draftOrderDuplicate.draftOrder;
    expect(duplicateResponse.body.data.draftOrderDuplicate.userErrors).toEqual([]);
    expect(duplicatedDraftOrder).toMatchObject({
      status: 'OPEN',
      ready: true,
      email: 'updated-draft-family@example.com',
      note: 'updated note',
      tags: ['draft', 'updated'],
    });
    expect(duplicatedDraftOrder.id).not.toBe(draftOrderId);
    expect(duplicatedDraftOrder.lineItems.nodes[0].id).not.toBe(
      updateResponse.body.data.draftOrderUpdate.draftOrder.lineItems.nodes[0].id,
    );

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderDelete($input: DraftOrderDeleteInput!) {
          draftOrderDelete(input: $input) {
            deletedId
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            id: draftOrderId,
          },
        },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.draftOrderDelete).toEqual({
      deletedId: draftOrderId,
      userErrors: [],
    });

    const detailResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query DraftOrderDetail($id: ID!) {
          draftOrder(id: $id) {
            id
          }
        }`,
        variables: {
          id: draftOrderId,
        },
      });

    expect(detailResponse.body.data.draftOrder).toBeNull();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('keeps draftOrderInvoiceSend local and explicit instead of sending email upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draftOrderInvoiceSend should not hit upstream in live-hybrid mode');
    });
    const app = createApp(liveHybridConfig).callback();
    const createResponse = await createDraftOrder(app);
    const draftOrderId = createResponse.body.data.draftOrderCreate.draftOrder.id as string;

    const invoiceResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderInvoiceSend($id: ID!) {
          draftOrderInvoiceSend(id: $id) {
            draftOrder {
              id
              status
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: draftOrderId,
        },
      });

    expect(invoiceResponse.status).toBe(200);
    expect(invoiceResponse.body.data.draftOrderInvoiceSend).toEqual({
      draftOrder: {
        id: draftOrderId,
        status: 'OPEN',
      },
      userErrors: [
        {
          field: ['id'],
          message: 'draftOrderInvoiceSend is intentionally not executed by the local proxy because it sends email.',
        },
      ],
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.status).toBe(200);
    const invoiceLogEntry = (
      logResponse.body.entries as Array<{
        interpreted?: { primaryRootField?: string };
        requestBody?: { query?: string; variables?: unknown };
        status?: string;
        notes?: string;
      }>
    ).find((entry) => entry.interpreted?.primaryRootField === 'draftOrderInvoiceSend');
    expect(invoiceLogEntry).toMatchObject({
      requestBody: {
        variables: { id: draftOrderId },
      },
      status: 'staged',
      notes: 'Locally handled draftOrderInvoiceSend in live-hybrid mode without sending invoice email.',
    });
    expect(invoiceLogEntry?.requestBody?.query).toContain('draftOrderInvoiceSend');
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors safe draftOrderInvoiceSend no-recipient and lifecycle validation branches locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draftOrderInvoiceSend safe validation branches should not hit upstream');
    });
    const app = createApp(liveHybridConfig).callback();

    const openCreateResponse = await createNoRecipientDraftOrder(app, 'open-no-recipient');
    expect(openCreateResponse.status).toBe(200);
    const openDraftOrderId = openCreateResponse.body.data.draftOrderCreate.draftOrder.id as string;

    const openInvoiceResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderInvoiceSend($id: ID!) {
          draftOrderInvoiceSend(id: $id) {
            draftOrder {
              id
              status
              email
              invoiceUrl
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: openDraftOrderId,
        },
      });

    expect(openInvoiceResponse.status).toBe(200);
    expect(openInvoiceResponse.body.data.draftOrderInvoiceSend).toMatchObject({
      draftOrder: {
        id: openDraftOrderId,
        status: 'OPEN',
        email: null,
      },
      userErrors: [{ field: null, message: "To can't be blank" }],
    });

    const completedCreateResponse = await createNoRecipientDraftOrder(app, 'completed-no-recipient');
    const completedDraftOrderId = completedCreateResponse.body.data.draftOrderCreate.draftOrder.id as string;
    const completeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderComplete($id: ID!) {
          draftOrderComplete(id: $id, paymentPending: true) {
            draftOrder {
              id
              status
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: completedDraftOrderId,
        },
      });
    expect(completeResponse.status).toBe(200);
    expect(completeResponse.body.data.draftOrderComplete.userErrors).toEqual([]);

    const completedInvoiceResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderInvoiceSend($id: ID!) {
          draftOrderInvoiceSend(id: $id) {
            draftOrder {
              id
              status
              email
            }
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          id: completedDraftOrderId,
        },
      });

    expect(completedInvoiceResponse.status).toBe(200);
    expect(completedInvoiceResponse.body.data.draftOrderInvoiceSend).toEqual({
      draftOrder: {
        id: completedDraftOrderId,
        status: 'COMPLETED',
        email: null,
      },
      userErrors: [
        { field: null, message: "To can't be blank" },
        {
          field: null,
          message: "Draft order Invoice can't be sent. This draft order is already paid.",
        },
      ],
    });

    const deletedCreateResponse = await createNoRecipientDraftOrder(app, 'deleted');
    const deletedDraftOrderId = deletedCreateResponse.body.data.draftOrderCreate.draftOrder.id as string;
    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderDelete($input: DraftOrderDeleteInput!) {
          draftOrderDelete(input: $input) {
            deletedId
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          input: {
            id: deletedDraftOrderId,
          },
        },
      });
    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.draftOrderDelete.userErrors).toEqual([]);

    const deletedInvoiceResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderInvoiceSend($id: ID!) {
          draftOrderInvoiceSend(id: $id) {
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
          id: deletedDraftOrderId,
        },
      });

    expect(deletedInvoiceResponse.status).toBe(200);
    expect(deletedInvoiceResponse.body.data.draftOrderInvoiceSend).toEqual({
      draftOrder: null,
      userErrors: [{ field: null, message: 'Draft order not found' }],
    });

    const unknownInvoiceResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderInvoiceSend($id: ID!) {
          draftOrderInvoiceSend(id: $id) {
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
          id: 'gid://shopify/DraftOrder/999999999999999',
        },
      });

    expect(unknownInvoiceResponse.status).toBe(200);
    expect(unknownInvoiceResponse.body.data.draftOrderInvoiceSend).toEqual({
      draftOrder: null,
      userErrors: [{ field: null, message: 'Draft order not found' }],
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.status).toBe(200);
    expect(
      (
        logResponse.body.entries as Array<{
          interpreted?: { primaryRootField?: string };
        }>
      ).filter((entry) => entry.interpreted?.primaryRootField === 'draftOrderInvoiceSend'),
    ).toHaveLength(4);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages draftOrderCreateFromOrder from a synthetic local order in live-hybrid mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draftOrderCreateFromOrder should not hit upstream for synthetic orders in live-hybrid mode');
    });
    const app = createApp(liveHybridConfig).callback();

    const orderCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation OrderCreate($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              note
              tags
              customer {
                email
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
            email: 'source-order@example.com',
            note: 'source order note',
            tags: ['source-order'],
            lineItems: [
              {
                title: 'Source item',
                quantity: 1,
                originalUnitPriceSet: {
                  shopMoney: {
                    amount: '20.00',
                    currencyCode: 'CAD',
                  },
                },
                sku: 'HAR-118-SOURCE',
              },
            ],
          },
        },
      });

    expect(orderCreateResponse.status).toBe(200);
    const orderId = orderCreateResponse.body.data.orderCreate.order.id as string;

    const createFromOrderResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderCreateFromOrder($orderId: ID!) {
          draftOrderCreateFromOrder(orderId: $orderId) {
            draftOrder ${draftOrderSelection}
            userErrors {
              field
              message
            }
          }
        }`,
        variables: {
          orderId,
        },
      });

    expect(createFromOrderResponse.status).toBe(200);
    expect(createFromOrderResponse.body.data.draftOrderCreateFromOrder.userErrors).toEqual([]);
    expect(createFromOrderResponse.body.data.draftOrderCreateFromOrder.draftOrder).toMatchObject({
      status: 'OPEN',
      email: 'source-order@example.com',
      note: 'source order note',
      tags: ['source-order'],
      lineItems: {
        nodes: [
          expect.objectContaining({
            title: 'Source item',
            quantity: 1,
            sku: 'HAR-118-SOURCE',
          }),
        ],
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('short-circuits safe draft-order mutation validation branches locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft-order validation branches should not hit upstream in snapshot mode');
    });
    const app = createApp(snapshotConfig).callback();

    const cases = [
      {
        operationName: 'draftOrderUpdate',
        query: `mutation DraftOrderUpdate($id: ID!, $input: DraftOrderInput!) {
          draftOrderUpdate(id: $id, input: $input) {
            draftOrder { id }
            userErrors { field message }
          }
        }`,
        expectedMessage: 'Variable $id of type ID! was provided invalid value',
      },
      {
        operationName: 'draftOrderDelete',
        query: `mutation DraftOrderDelete($input: DraftOrderDeleteInput!) {
          draftOrderDelete(input: $input) {
            deletedId
            userErrors { field message }
          }
        }`,
        expectedMessage: 'Variable $input of type DraftOrderDeleteInput! was provided invalid value',
      },
      {
        operationName: 'draftOrderInvoiceSend',
        query: `mutation DraftOrderInvoiceSend($id: ID!) {
          draftOrderInvoiceSend(id: $id) {
            draftOrder { id }
            userErrors { field message }
          }
        }`,
        expectedMessage: 'Variable $id of type ID! was provided invalid value',
      },
      {
        operationName: 'draftOrderCreateFromOrder',
        query: `mutation DraftOrderCreateFromOrder($orderId: ID!) {
          draftOrderCreateFromOrder(orderId: $orderId) {
            draftOrder { id }
            userErrors { field message }
          }
        }`,
        expectedMessage: 'Variable $orderId of type ID! was provided invalid value',
      },
    ];

    for (const testCase of cases) {
      const response = await request(app).post('/admin/api/2026-04/graphql.json').send({ query: testCase.query });
      expect(response.status, testCase.operationName).toBe(200);
      expect(response.body.errors[0].message, testCase.operationName).toBe(testCase.expectedMessage);
      expect(response.body.errors[0].extensions.code, testCase.operationName).toBe('INVALID_VARIABLE');
    }

    const duplicateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftOrderDuplicate($id: ID) {
          draftOrderDuplicate(id: $id) {
            draftOrder { id }
            userErrors { field message }
          }
        }`,
      });

    expect(duplicateResponse.status).toBe(200);
    expect(duplicateResponse.body.data.draftOrderDuplicate).toEqual({
      draftOrder: null,
      userErrors: [{ field: ['id'], message: 'Draft order does not exist' }],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
