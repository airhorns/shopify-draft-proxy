import request, { type Response } from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const orderSelection = `{
  id
  name
  closed
  closedAt
  cancelledAt
  cancelReason
  displayFinancialStatus
  paymentGatewayNames
  totalOutstandingSet {
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
  transactions {
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
}`;

type TestApp = ReturnType<typeof createApp>['callback'] extends () => infer T ? T : never;

async function postGraphql(app: TestApp, query: string, variables: Record<string, unknown>) {
  return request(app).post('/admin/api/2026-04/graphql.json').send({ query, variables });
}

async function snapshotMeta(app: TestApp) {
  const [stateResponse, logResponse] = await Promise.all([
    request(app).get('/__meta/state'),
    request(app).get('/__meta/log'),
  ]);

  return {
    state: stateResponse.body,
    log: logResponse.body,
  };
}

async function expectUnchangedAfter(app: TestApp, action: () => Promise<Response>): Promise<Response> {
  const before = await snapshotMeta(app);
  const response = await action();
  const after = await snapshotMeta(app);

  expect(after).toEqual(before);
  return response;
}

describe('order lifecycle, payment, and customer mutations', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages captured lifecycle, payment, customer, invoice, tax, and cancel behavior without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('HAR-120 order management mutations should not hit upstream in snapshot mode');
    });
    store.stageCreateCustomer({
      id: 'gid://shopify/Customer/100',
      firstName: 'HAR',
      lastName: 'Customer',
      displayName: 'HAR Customer',
      email: 'har-customer@example.com',
      legacyResourceId: null,
      locale: null,
      note: null,
      canDelete: true,
      verifiedEmail: true,
      taxExempt: false,
      state: 'ENABLED',
      tags: [],
      numberOfOrders: '0',
      amountSpent: { amount: '0.0', currencyCode: 'CAD' },
      defaultEmailAddress: { emailAddress: 'har-customer@example.com' },
      defaultPhoneNumber: null,
      defaultAddress: null,
      createdAt: '2024-01-01T00:00:00.000Z',
      updatedAt: '2024-01-01T00:00:00.000Z',
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        variables: {
          order: {
            currency: 'CAD',
            note: 'HAR-120 snapshot order',
            tags: ['har-120'],
            lineItems: [
              {
                title: 'HAR-120 local item',
                quantity: 1,
                priceSet: { shopMoney: { amount: '19.00', currencyCode: 'CAD' } },
                sku: 'har-120-local',
              },
            ],
          },
        },
      });

    const orderId = createResponse.body.data.orderCreate.order.id;
    expect(createResponse.body.data.orderCreate.order).toMatchObject({
      closed: false,
      closedAt: null,
      cancelledAt: null,
      cancelReason: null,
      displayFinancialStatus: 'PENDING',
      totalOutstandingSet: { shopMoney: { amount: '19.0', currencyCode: 'CAD' } },
    });

    const closeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Close($input: OrderCloseInput!) {
          orderClose(input: $input) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        variables: { input: { id: orderId } },
      });

    expect(closeResponse.body.data.orderClose.order.closed).toBe(true);
    expect(closeResponse.body.data.orderClose.order.closedAt).toBeTruthy();
    expect(closeResponse.body.data.orderClose.userErrors).toEqual([]);

    const openResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Open($input: OrderOpenInput!) {
          orderOpen(input: $input) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        variables: { input: { id: orderId } },
      });

    expect(openResponse.body.data.orderOpen.order).toMatchObject({ closed: false, closedAt: null });
    expect(openResponse.body.data.orderOpen.userErrors).toEqual([]);

    const markPaidResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation MarkPaid($input: OrderMarkAsPaidInput!) {
          orderMarkAsPaid(input: $input) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        variables: { input: { id: orderId } },
      });

    expect(markPaidResponse.body.data.orderMarkAsPaid.order).toMatchObject({
      displayFinancialStatus: 'PAID',
      paymentGatewayNames: ['manual'],
      totalOutstandingSet: { shopMoney: { amount: '0.0', currencyCode: 'CAD' } },
    });
    expect(markPaidResponse.body.data.orderMarkAsPaid.order.transactions).toEqual([
      {
        kind: 'SALE',
        status: 'SUCCESS',
        gateway: 'manual',
        amountSet: { shopMoney: { amount: '19.0', currencyCode: 'CAD' } },
      },
    ]);

    const manualPaymentResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ManualPayment($id: ID!, $amount: MoneyInput) {
          orderCreateManualPayment(id: $id, amount: $amount) {
            order { id }
            userErrors { field message }
          }
        }`,
        variables: { id: orderId, amount: { amount: '19.00', currencyCode: 'CAD' } },
      });

    expect(manualPaymentResponse.body.data.orderCreateManualPayment.userErrors).toEqual([
      { field: ['id'], message: 'Order is already paid' },
    ]);

    const setCustomerResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSet($orderId: ID!, $customerId: ID!) {
          orderCustomerSet(orderId: $orderId, customerId: $customerId) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        variables: { orderId, customerId: 'gid://shopify/Customer/100' },
      });

    expect(setCustomerResponse.body.data.orderCustomerSet.order.customer).toEqual({
      id: 'gid://shopify/Customer/100',
      email: 'har-customer@example.com',
      displayName: 'HAR Customer',
    });

    const linkedCustomerReadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query LinkedCustomerOrderSummary($id: ID!) {
          customer(id: $id) {
            id
            numberOfOrders
            amountSpent { amount currencyCode }
            lastOrder { id name }
            orders(first: 5) {
              nodes {
                id
                name
                currentTotalPriceSet { shopMoney { amount currencyCode } }
              }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }`,
        variables: { id: 'gid://shopify/Customer/100' },
      });

    expect(linkedCustomerReadResponse.body.data.customer).toEqual({
      id: 'gid://shopify/Customer/100',
      numberOfOrders: '0',
      amountSpent: { amount: '0.0', currencyCode: 'CAD' },
      lastOrder: null,
      orders: {
        nodes: [
          {
            id: orderId,
            name: '#1',
            currentTotalPriceSet: { shopMoney: { amount: '19.0', currencyCode: 'CAD' } },
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: `cursor:${orderId}`,
          endCursor: `cursor:${orderId}`,
        },
      },
    });

    const removeCustomerResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerRemove($orderId: ID!) {
          orderCustomerRemove(orderId: $orderId) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        variables: { orderId },
      });

    expect(removeCustomerResponse.body.data.orderCustomerRemove.order.customer).toBeNull();

    const unlinkedCustomerReadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query UnlinkedCustomerOrderSummary($id: ID!) {
          customer(id: $id) {
            id
            numberOfOrders
            amountSpent { amount currencyCode }
            lastOrder { id }
            orders(first: 5) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }`,
        variables: { id: 'gid://shopify/Customer/100' },
      });

    expect(unlinkedCustomerReadResponse.body.data.customer).toEqual({
      id: 'gid://shopify/Customer/100',
      numberOfOrders: '0',
      amountSpent: { amount: '0.0', currencyCode: 'CAD' },
      lastOrder: null,
      orders: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
    });

    const invoiceResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Invoice($id: ID!) {
          orderInvoiceSend(id: $id) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        variables: { id: orderId },
      });

    expect(invoiceResponse.body.data.orderInvoiceSend.order.id).toBe(orderId);
    expect(invoiceResponse.body.data.orderInvoiceSend.userErrors).toEqual([]);

    const taxResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Tax($orderId: ID) {
          taxSummaryCreate(orderId: $orderId) {
            enqueuedOrders { id }
            userErrors { field message }
          }
        }`,
        variables: { orderId },
      });

    expect(taxResponse.body).toMatchObject({
      data: { taxSummaryCreate: null },
      errors: [{ extensions: { code: 'ACCESS_DENIED' }, path: ['taxSummaryCreate'] }],
    });

    const cancelResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Cancel($orderId: ID!, $restock: Boolean!, $reason: OrderCancelReason!) {
          orderCancel(orderId: $orderId, restock: $restock, reason: $reason) {
            job { id done }
            orderCancelUserErrors { field message }
            userErrors { field message }
          }
        }`,
        variables: { orderId, restock: false, reason: 'OTHER' },
      });

    expect(cancelResponse.body.data.orderCancel).toEqual({
      job: { id: expect.stringMatching(/^gid:\/\/shopify\/Job\//u), done: false },
      orderCancelUserErrors: [],
      userErrors: [],
    });

    const readAfterCancelResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query Read($id: ID!) { order(id: $id) ${orderSelection} }`,
        variables: { id: orderId },
      });

    expect(readAfterCancelResponse.body.data.order).toMatchObject({
      id: orderId,
      closed: true,
      cancelledAt: expect.any(String),
      cancelReason: 'OTHER',
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('rejects invalid lifecycle and customer transitions without mutating state or logs', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('HAR-278 order transition validation should not hit upstream in snapshot mode');
    });
    store.stageCreateCustomer({
      id: 'gid://shopify/Customer/278',
      firstName: 'HAR',
      lastName: 'Validation',
      displayName: 'HAR Validation',
      email: 'har-278@example.com',
      legacyResourceId: null,
      locale: null,
      note: null,
      canDelete: true,
      verifiedEmail: true,
      taxExempt: false,
      state: 'ENABLED',
      tags: [],
      numberOfOrders: 0,
      amountSpent: { amount: '0.0', currencyCode: 'CAD' },
      defaultEmailAddress: { emailAddress: 'har-278@example.com' },
      defaultPhoneNumber: null,
      defaultAddress: null,
      createdAt: '2024-01-01T00:00:00.000Z',
      updatedAt: '2024-01-01T00:00:00.000Z',
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await postGraphql(
      app,
      `mutation CreateOrder($order: OrderCreateOrderInput!) {
        orderCreate(order: $order) {
          order ${orderSelection}
          userErrors { field message }
        }
      }`,
      {
        order: {
          currency: 'CAD',
          lineItems: [
            {
              title: 'HAR-278 validation item',
              quantity: 1,
              priceSet: { shopMoney: { amount: '19.00', currencyCode: 'CAD' } },
            },
          ],
        },
      },
    );
    const orderId = createResponse.body.data.orderCreate.order.id;

    const customerRemoveEmpty = await expectUnchangedAfter(app, () =>
      postGraphql(
        app,
        `mutation CustomerRemove($orderId: ID!) {
          orderCustomerRemove(orderId: $orderId) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        { orderId },
      ),
    );
    expect(customerRemoveEmpty.body.data.orderCustomerRemove.userErrors).toEqual([
      { field: ['orderId'], message: 'Order does not have a customer' },
    ]);

    const customerSetUnknown = await expectUnchangedAfter(app, () =>
      postGraphql(
        app,
        `mutation CustomerSet($orderId: ID!, $customerId: ID!) {
          orderCustomerSet(orderId: $orderId, customerId: $customerId) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        { orderId, customerId: 'gid://shopify/Customer/999999' },
      ),
    );
    expect(customerSetUnknown.body.data.orderCustomerSet.userErrors).toEqual([
      { field: ['customerId'], message: 'Customer does not exist' },
    ]);

    const customerSetResponse = await postGraphql(
      app,
      `mutation CustomerSet($orderId: ID!, $customerId: ID!) {
        orderCustomerSet(orderId: $orderId, customerId: $customerId) {
          order ${orderSelection}
          userErrors { field message }
        }
      }`,
      { orderId, customerId: 'gid://shopify/Customer/278' },
    );
    expect(customerSetResponse.body.data.orderCustomerSet.userErrors).toEqual([]);

    const customerSetRepeat = await expectUnchangedAfter(app, () =>
      postGraphql(
        app,
        `mutation CustomerSet($orderId: ID!, $customerId: ID!) {
          orderCustomerSet(orderId: $orderId, customerId: $customerId) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        { orderId, customerId: 'gid://shopify/Customer/278' },
      ),
    );
    expect(customerSetRepeat.body.data.orderCustomerSet.userErrors).toEqual([
      { field: ['customerId'], message: 'Order already has this customer' },
    ]);

    const closeResponse = await postGraphql(
      app,
      `mutation Close($input: OrderCloseInput!) {
        orderClose(input: $input) {
          order ${orderSelection}
          userErrors { field message }
        }
      }`,
      { input: { id: orderId } },
    );
    expect(closeResponse.body.data.orderClose.userErrors).toEqual([]);

    const repeatedClose = await expectUnchangedAfter(app, () =>
      postGraphql(
        app,
        `mutation Close($input: OrderCloseInput!) {
          orderClose(input: $input) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        { input: { id: orderId } },
      ),
    );
    expect(repeatedClose.body.data.orderClose.userErrors).toEqual([
      { field: ['id'], message: 'Order is already closed' },
    ]);

    const openResponse = await postGraphql(
      app,
      `mutation Open($input: OrderOpenInput!) {
        orderOpen(input: $input) {
          order ${orderSelection}
          userErrors { field message }
        }
      }`,
      { input: { id: orderId } },
    );
    expect(openResponse.body.data.orderOpen.userErrors).toEqual([]);

    const repeatedOpen = await expectUnchangedAfter(app, () =>
      postGraphql(
        app,
        `mutation Open($input: OrderOpenInput!) {
          orderOpen(input: $input) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        { input: { id: orderId } },
      ),
    );
    expect(repeatedOpen.body.data.orderOpen.userErrors).toEqual([{ field: ['id'], message: 'Order is already open' }]);

    const markPaidResponse = await postGraphql(
      app,
      `mutation MarkPaid($input: OrderMarkAsPaidInput!) {
        orderMarkAsPaid(input: $input) {
          order ${orderSelection}
          userErrors { field message }
        }
      }`,
      { input: { id: orderId } },
    );
    expect(markPaidResponse.body.data.orderMarkAsPaid.userErrors).toEqual([]);

    const repeatedMarkPaid = await expectUnchangedAfter(app, () =>
      postGraphql(
        app,
        `mutation MarkPaid($input: OrderMarkAsPaidInput!) {
          orderMarkAsPaid(input: $input) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        { input: { id: orderId } },
      ),
    );
    expect(repeatedMarkPaid.body.data.orderMarkAsPaid.userErrors).toEqual([
      { field: ['id'], message: 'Order is already paid' },
    ]);

    const cancelResponse = await postGraphql(
      app,
      `mutation Cancel($orderId: ID!, $restock: Boolean!, $reason: OrderCancelReason!) {
        orderCancel(orderId: $orderId, restock: $restock, reason: $reason) {
          orderCancelUserErrors { field message }
        }
      }`,
      { orderId, restock: false, reason: 'OTHER' },
    );
    expect(cancelResponse.body.data.orderCancel.orderCancelUserErrors).toEqual([]);

    const repeatedCancel = await expectUnchangedAfter(app, () =>
      postGraphql(
        app,
        `mutation Cancel($orderId: ID!, $restock: Boolean!, $reason: OrderCancelReason!) {
          orderCancel(orderId: $orderId, restock: $restock, reason: $reason) {
            orderCancelUserErrors { field message }
          }
        }`,
        { orderId, restock: false, reason: 'OTHER' },
      ),
    );
    expect(repeatedCancel.body.data.orderCancel.orderCancelUserErrors).toEqual([
      { field: ['orderId'], message: 'Order is already canceled' },
    ]);

    const openCanceled = await expectUnchangedAfter(app, () =>
      postGraphql(
        app,
        `mutation Open($input: OrderOpenInput!) {
          orderOpen(input: $input) {
            order ${orderSelection}
            userErrors { field message }
          }
        }`,
        { input: { id: orderId } },
      ),
    );
    expect(openCanceled.body.data.orderOpen.userErrors).toEqual([
      { field: ['id'], message: 'Canceled orders cannot be opened' },
    ]);

    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
