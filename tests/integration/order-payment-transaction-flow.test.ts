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

const moneySelection = `{
  shopMoney {
    amount
    currencyCode
  }
}`;

const transactionSelection = `{
  id
  kind
  status
  gateway
  paymentId
  paymentReferenceId
  processedAt
  parentTransaction {
    id
    kind
    status
  }
  amountSet ${moneySelection}
}`;

const paymentOrderSelection = `{
  id
  displayFinancialStatus
  capturable
  totalCapturable
  totalCapturableSet ${moneySelection}
  totalOutstandingSet ${moneySelection}
  totalReceivedSet ${moneySelection}
  netPaymentSet ${moneySelection}
  paymentGatewayNames
  transactions ${transactionSelection}
}`;

async function createOrder(
  app: ReturnType<typeof createApp>['callback'] extends () => infer T ? T : never,
  transactions: unknown[],
) {
  return request(app)
    .post('/admin/api/2026-04/graphql.json')
    .send({
      query: `mutation CreateOrder($order: OrderCreateOrderInput!) {
        orderCreate(order: $order) {
          order ${paymentOrderSelection}
          userErrors { field message }
        }
      }`,
      variables: {
        order: {
          currency: 'CAD',
          transactions,
          lineItems: [
            {
              title: 'HAR-226 payment item',
              quantity: 1,
              priceSet: { shopMoney: { amount: '25.00', currencyCode: 'CAD' } },
            },
          ],
        },
      },
    });
}

describe('order payment capture, void, and mandate flows', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages multi-capture orderCapture locally and exposes downstream financial and capturable state', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderCapture should not hit upstream in snapshot mode');
    });
    const app = createApp(snapshotConfig).callback();
    const createResponse = await createOrder(app, [
      {
        kind: 'AUTHORIZATION',
        status: 'SUCCESS',
        gateway: 'manual',
        amountSet: { shopMoney: { amount: '25.00', currencyCode: 'CAD' } },
      },
    ]);
    const orderId = createResponse.body.data.orderCreate.order.id;
    const authorizationId = createResponse.body.data.orderCreate.order.transactions[0].id;

    const firstCapture = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Capture($input: OrderCaptureInput!) {
          orderCapture(input: $input) {
            transaction ${transactionSelection}
            order ${paymentOrderSelection}
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            id: orderId,
            parentTransactionId: authorizationId,
            amount: '10.00',
            currency: 'CAD',
            finalCapture: false,
          },
        },
      });

    expect(firstCapture.status).toBe(200);
    expect(firstCapture.body.data.orderCapture.userErrors).toEqual([]);
    expect(firstCapture.body.data.orderCapture.transaction).toMatchObject({
      kind: 'CAPTURE',
      status: 'SUCCESS',
      gateway: 'manual',
      parentTransaction: { id: authorizationId, kind: 'AUTHORIZATION', status: 'SUCCESS' },
      amountSet: { shopMoney: { amount: '10.0', currencyCode: 'CAD' } },
      paymentId: expect.stringMatching(/^gid:\/\/shopify\/Payment\//u),
      paymentReferenceId: expect.stringMatching(/^gid:\/\/shopify\/PaymentReference\//u),
    });
    expect(firstCapture.body.data.orderCapture.order).toMatchObject({
      displayFinancialStatus: 'PARTIALLY_PAID',
      capturable: true,
      totalCapturable: '15.0',
      totalOutstandingSet: { shopMoney: { amount: '15.0', currencyCode: 'CAD' } },
      totalReceivedSet: { shopMoney: { amount: '10.0', currencyCode: 'CAD' } },
      paymentGatewayNames: ['manual'],
    });

    const finalCapture = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Capture($input: OrderCaptureInput!) {
          orderCapture(input: $input) {
            transaction ${transactionSelection}
            order ${paymentOrderSelection}
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            id: orderId,
            parentTransactionId: authorizationId,
            amount: '15.00',
            currency: 'CAD',
            finalCapture: true,
          },
        },
      });

    expect(finalCapture.status).toBe(200);
    expect(finalCapture.body.data.orderCapture.userErrors).toEqual([]);
    expect(finalCapture.body.data.orderCapture.order).toMatchObject({
      displayFinancialStatus: 'PAID',
      capturable: false,
      totalCapturable: '0.0',
      totalOutstandingSet: { shopMoney: { amount: '0.0', currencyCode: 'CAD' } },
      totalReceivedSet: { shopMoney: { amount: '25.0', currencyCode: 'CAD' } },
    });

    const downstreamRead = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query OrderAfterCapture($id: ID!) {
          order(id: $id) ${paymentOrderSelection}
        }`,
        variables: { id: orderId },
      });

    expect(downstreamRead.body.data.order).toMatchObject({
      displayFinancialStatus: 'PAID',
      capturable: false,
      totalCapturableSet: { shopMoney: { amount: '0.0', currencyCode: 'CAD' } },
      transactions: [
        expect.objectContaining({ id: authorizationId, kind: 'AUTHORIZATION' }),
        expect.objectContaining({ kind: 'CAPTURE', amountSet: { shopMoney: { amount: '10.0', currencyCode: 'CAD' } } }),
        expect.objectContaining({ kind: 'CAPTURE', amountSet: { shopMoney: { amount: '15.0', currencyCode: 'CAD' } } }),
      ],
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'orderCreate',
      'orderCapture',
      'orderCapture',
    ]);
    expect(
      logResponse.body.entries
        .slice(1)
        .map(
          (entry: { requestBody: { variables: { input: { amount: string } } } }) =>
            entry.requestBody.variables.input.amount,
        ),
    ).toEqual(['10.00', '15.00']);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages transactionVoid locally and returns validation userErrors for invalid or already-voided transactions', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('transactionVoid should not hit upstream in snapshot mode');
    });
    const app = createApp(snapshotConfig).callback();
    const createResponse = await createOrder(app, [
      {
        kind: 'AUTHORIZATION',
        status: 'SUCCESS',
        gateway: 'manual',
        amountSet: { shopMoney: { amount: '25.00', currencyCode: 'CAD' } },
      },
    ]);
    const orderId = createResponse.body.data.orderCreate.order.id;
    const authorizationId = createResponse.body.data.orderCreate.order.transactions[0].id;

    const missingResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Void($id: ID!) {
          transactionVoid(id: $id) {
            transaction ${transactionSelection}
            userErrors { field message }
          }
        }`,
        variables: { id: 'gid://shopify/OrderTransaction/999999' },
      });
    expect(missingResponse.body.data.transactionVoid).toEqual({
      transaction: null,
      userErrors: [{ field: ['id'], message: 'Transaction does not exist' }],
    });

    const voidResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Void($id: ID!) {
          transactionVoid(id: $id) {
            transaction ${transactionSelection}
            userErrors { field message }
          }
        }`,
        variables: { id: authorizationId },
      });
    expect(voidResponse.status).toBe(200);
    expect(voidResponse.body.data.transactionVoid.userErrors).toEqual([]);
    expect(voidResponse.body.data.transactionVoid.transaction).toMatchObject({
      kind: 'VOID',
      status: 'SUCCESS',
      parentTransaction: { id: authorizationId, kind: 'AUTHORIZATION', status: 'SUCCESS' },
      amountSet: { shopMoney: { amount: '25.0', currencyCode: 'CAD' } },
    });

    const duplicateVoid = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Void($id: ID!) {
          transactionVoid(id: $id) {
            transaction ${transactionSelection}
            userErrors { field message }
          }
        }`,
        variables: { id: authorizationId },
      });
    expect(duplicateVoid.body.data.transactionVoid).toEqual({
      transaction: null,
      userErrors: [{ field: ['id'], message: 'Transaction has already been voided' }],
    });

    const downstreamRead = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query OrderAfterVoid($id: ID!) {
          order(id: $id) ${paymentOrderSelection}
        }`,
        variables: { id: orderId },
      });

    expect(downstreamRead.body.data.order).toMatchObject({
      displayFinancialStatus: 'VOIDED',
      capturable: false,
      totalCapturable: '0.0',
      totalOutstandingSet: { shopMoney: { amount: '25.0', currencyCode: 'CAD' } },
      transactions: [
        expect.objectContaining({ id: authorizationId, kind: 'AUTHORIZATION' }),
        expect.objectContaining({ kind: 'VOID' }),
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns userErrors for over-capture without mutating downstream payment state', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderCapture validation should not hit upstream in snapshot mode');
    });
    const app = createApp(snapshotConfig).callback();
    const createResponse = await createOrder(app, [
      {
        kind: 'AUTHORIZATION',
        status: 'SUCCESS',
        gateway: 'manual',
        amountSet: { shopMoney: { amount: '25.00', currencyCode: 'CAD' } },
      },
    ]);
    const orderId = createResponse.body.data.orderCreate.order.id;
    const authorizationId = createResponse.body.data.orderCreate.order.transactions[0].id;

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Capture($input: OrderCaptureInput!) {
          orderCapture(input: $input) {
            transaction ${transactionSelection}
            order ${paymentOrderSelection}
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            id: orderId,
            parentTransactionId: authorizationId,
            amount: '30.00',
            currency: 'CAD',
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.orderCapture.transaction).toBeNull();
    expect(response.body.data.orderCapture.userErrors).toEqual([
      { field: ['input', 'amount'], message: 'Amount exceeds capturable amount' },
    ]);
    expect(response.body.data.orderCapture.order).toMatchObject({
      displayFinancialStatus: 'AUTHORIZED',
      capturable: true,
      totalCapturable: '25.0',
      transactions: [expect.objectContaining({ id: authorizationId, kind: 'AUTHORIZATION' })],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages orderCreateMandatePayment locally with stable idempotency results', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('orderCreateMandatePayment should not hit upstream in snapshot mode');
    });
    const app = createApp(snapshotConfig).callback();
    const createResponse = await createOrder(app, []);
    const orderId = createResponse.body.data.orderCreate.order.id;
    const mutation = `mutation Mandate($id: ID!, $idempotencyKey: String!, $amount: MoneyInput) {
      orderCreateMandatePayment(id: $id, idempotencyKey: $idempotencyKey, amount: $amount) {
        job { id done }
        paymentReferenceId
        order ${paymentOrderSelection}
        userErrors { field message }
      }
    }`;
    const variables = {
      id: orderId,
      idempotencyKey: 'har-226-idempotent-payment',
      amount: { amount: '25.00', currencyCode: 'CAD' },
    };

    const firstResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({ query: mutation, variables });
    const secondResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({ query: mutation, variables });

    expect(firstResponse.status).toBe(200);
    expect(firstResponse.body.data.orderCreateMandatePayment.userErrors).toEqual([]);
    expect(secondResponse.body.data.orderCreateMandatePayment.job).toEqual(
      firstResponse.body.data.orderCreateMandatePayment.job,
    );
    expect(secondResponse.body.data.orderCreateMandatePayment.paymentReferenceId).toBe(
      firstResponse.body.data.orderCreateMandatePayment.paymentReferenceId,
    );
    expect(secondResponse.body.data.orderCreateMandatePayment.order).toMatchObject({
      displayFinancialStatus: 'PAID',
      totalOutstandingSet: { shopMoney: { amount: '0.0', currencyCode: 'CAD' } },
      transactions: [
        expect.objectContaining({
          kind: 'MANDATE_PAYMENT',
          status: 'SUCCESS',
          gateway: 'mandate',
          paymentReferenceId: firstResponse.body.data.orderCreateMandatePayment.paymentReferenceId,
          amountSet: { shopMoney: { amount: '25.0', currencyCode: 'CAD' } },
        }),
      ],
    });
    expect(secondResponse.body.data.orderCreateMandatePayment.order.transactions).toHaveLength(1);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
