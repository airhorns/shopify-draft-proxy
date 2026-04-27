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

const paymentTermsSelection = `
  id
  due
  overdue
  dueInDays
  paymentTermsName
  paymentTermsType
  translatedName
  paymentSchedules(first: 2) {
    nodes {
      id
      dueAt
      issuedAt
      completedAt
      due
      amount {
        amount
        currencyCode
      }
      balanceDue {
        amount
        currencyCode
      }
      totalBalance {
        amount
        currencyCode
      }
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
`;

describe('payment terms lifecycle staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages paymentTermsCreate/update/delete for orders without upstream access and preserves raw commit order', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('payment terms lifecycle mutations should not hit upstream fetch in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const orderCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateOrder($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              paymentTerms { id }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          order: {
            email: 'payment-terms-order@example.com',
            lineItems: [{ title: 'Terms order line', quantity: 1, priceSet: { shopMoney: { amount: '18.50' } } }],
          },
        },
      });

    expect(orderCreateResponse.status).toBe(200);
    expect(orderCreateResponse.body.data.orderCreate.userErrors).toEqual([]);
    const orderId = orderCreateResponse.body.data.orderCreate.order.id as string;

    const createMutation = `mutation PaymentTermsCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
      paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
        paymentTerms {
          ${paymentTermsSelection}
        }
        userErrors { field message code }
      }
    }`;
    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: createMutation,
        variables: {
          referenceId: orderId,
          attrs: {
            paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
            paymentSchedules: [{ issuedAt: '2026-04-27T12:00:00Z' }],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.paymentTermsCreate.userErrors).toEqual([]);
    const paymentTermsId = createResponse.body.data.paymentTermsCreate.paymentTerms.id as string;
    const paymentScheduleId = createResponse.body.data.paymentTermsCreate.paymentTerms.paymentSchedules.nodes[0]
      .id as string;
    expect(createResponse.body.data.paymentTermsCreate.paymentTerms).toEqual({
      id: expect.stringMatching(/^gid:\/\/shopify\/PaymentTerms\/\d+$/),
      due: false,
      overdue: false,
      dueInDays: 30,
      paymentTermsName: 'Net 30',
      paymentTermsType: 'NET',
      translatedName: 'Net 30',
      paymentSchedules: {
        nodes: [
          {
            id: expect.stringMatching(/^gid:\/\/shopify\/PaymentSchedule\/\d+$/),
            dueAt: '2026-05-27T12:00:00Z',
            issuedAt: '2026-04-27T12:00:00Z',
            completedAt: null,
            due: false,
            amount: { amount: '18.5', currencyCode: 'CAD' },
            balanceDue: { amount: '18.5', currencyCode: 'CAD' },
            totalBalance: { amount: '18.5', currencyCode: 'CAD' },
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: `cursor:${paymentScheduleId}`,
          endCursor: `cursor:${paymentScheduleId}`,
        },
      },
    });

    const orderReadAfterCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query OrderPaymentTerms($id: ID!) {
          order(id: $id) {
            id
            paymentTerms {
              ${paymentTermsSelection}
            }
          }
        }`,
        variables: { id: orderId },
      });
    expect(orderReadAfterCreate.status).toBe(200);
    expect(orderReadAfterCreate.body.data.order.paymentTerms.id).toBe(paymentTermsId);

    const updateMutation = `mutation PaymentTermsUpdate($input: PaymentTermsUpdateInput!) {
      paymentTermsUpdate(input: $input) {
        paymentTerms {
          ${paymentTermsSelection}
        }
        userErrors { field message code }
      }
    }`;
    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: updateMutation,
        variables: {
          input: {
            paymentTermsId,
            paymentTermsAttributes: {
              paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/7',
              paymentSchedules: [{ dueAt: '2026-05-27T12:00:00Z' }],
            },
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.paymentTermsUpdate.userErrors).toEqual([]);
    expect(updateResponse.body.data.paymentTermsUpdate.paymentTerms).toMatchObject({
      id: paymentTermsId,
      dueInDays: null,
      paymentTermsName: 'Fixed',
      paymentTermsType: 'FIXED',
      translatedName: 'Fixed',
    });
    expect(updateResponse.body.data.paymentTermsUpdate.paymentTerms.paymentSchedules.nodes).toEqual([
      expect.objectContaining({
        id: expect.stringMatching(/^gid:\/\/shopify\/PaymentSchedule\/\d+$/),
        dueAt: '2026-05-27T12:00:00Z',
        issuedAt: null,
      }),
    ]);
    expect(updateResponse.body.data.paymentTermsUpdate.paymentTerms.paymentSchedules.nodes[0].id).not.toBe(
      paymentScheduleId,
    );

    const deleteMutation = `mutation PaymentTermsDelete($input: PaymentTermsDeleteInput!) {
      paymentTermsDelete(input: $input) {
        deletedId
        userErrors { field message code }
      }
    }`;
    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: deleteMutation,
        variables: { input: { paymentTermsId } },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.paymentTermsDelete).toEqual({
      deletedId: paymentTermsId,
      userErrors: [],
    });

    const orderReadAfterDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query OrderPaymentTermsDeleted($id: ID!) {
          order(id: $id) {
            id
            paymentTerms { id }
          }
        }`,
        variables: { id: orderId },
      });
    expect(orderReadAfterDelete.status).toBe(200);
    expect(orderReadAfterDelete.body.data.order.paymentTerms).toBeNull();

    const log = (await request(app).get('/__meta/log')).body.entries;
    expect(log.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'orderCreate',
      'paymentTermsCreate',
      'paymentTermsUpdate',
      'paymentTermsDelete',
    ]);
    expect(log.slice(1).map((entry: { requestBody: { query: string } }) => entry.requestBody.query)).toEqual([
      createMutation,
      updateMutation,
      deleteMutation,
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages paymentTermsCreate/update/delete for draft orders and keeps draft reads consistent', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('draft payment terms lifecycle mutations should not hit upstream fetch in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const draftCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateDraft($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder { id paymentTerms { id } }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'payment-terms-draft-lifecycle@example.com',
            lineItems: [{ title: 'Terms draft line', quantity: 1, originalUnitPrice: '12.00' }],
          },
        },
      });

    expect(draftCreateResponse.status).toBe(200);
    expect(draftCreateResponse.body.data.draftOrderCreate.userErrors).toEqual([]);
    const draftOrderId = draftCreateResponse.body.data.draftOrderCreate.draftOrder.id as string;

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftPaymentTermsCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
          paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
            paymentTerms { id paymentTermsName paymentTermsType dueInDays }
            userErrors { field message }
          }
        }`,
        variables: {
          referenceId: draftOrderId,
          attrs: {
            paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/2',
            paymentSchedules: [{ issuedAt: '2026-04-28T12:00:00Z' }],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.paymentTermsCreate).toEqual({
      paymentTerms: {
        id: expect.stringMatching(/^gid:\/\/shopify\/PaymentTerms\/\d+$/),
        paymentTermsName: 'Net 7',
        paymentTermsType: 'NET',
        dueInDays: 7,
      },
      userErrors: [],
    });
    const paymentTermsId = createResponse.body.data.paymentTermsCreate.paymentTerms.id as string;

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftPaymentTermsUpdate($input: PaymentTermsUpdateInput!) {
          paymentTermsUpdate(input: $input) {
            paymentTerms { id paymentTermsName paymentTermsType dueInDays }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            paymentTermsId,
            paymentTermsAttributes: {
              paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/1',
            },
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.paymentTermsUpdate).toEqual({
      paymentTerms: {
        id: paymentTermsId,
        paymentTermsName: 'Due on receipt',
        paymentTermsType: 'RECEIPT',
        dueInDays: null,
      },
      userErrors: [],
    });

    const draftReadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query DraftPaymentTerms($id: ID!) {
          draftOrder(id: $id) {
            id
            paymentTerms { id paymentTermsName paymentTermsType dueInDays }
          }
        }`,
        variables: { id: draftOrderId },
      });
    expect(draftReadResponse.status).toBe(200);
    expect(draftReadResponse.body.data.draftOrder.paymentTerms).toEqual({
      id: paymentTermsId,
      paymentTermsName: 'Due on receipt',
      paymentTermsType: 'RECEIPT',
      dueInDays: null,
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DraftPaymentTermsDelete($input: PaymentTermsDeleteInput!) {
          paymentTermsDelete(input: $input) {
            deletedId
            userErrors { field message }
          }
        }`,
        variables: { input: { paymentTermsId } },
      });
    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.paymentTermsDelete).toEqual({
      deletedId: paymentTermsId,
      userErrors: [],
    });

    const draftReadAfterDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query DraftPaymentTermsDeleted($id: ID!) {
          draftOrder(id: $id) {
            paymentTerms { id }
          }
        }`,
        variables: { id: draftOrderId },
      });
    expect(draftReadAfterDelete.status).toBe(200);
    expect(draftReadAfterDelete.body.data.draftOrder.paymentTerms).toBeNull();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns local payment terms userErrors for unknown targets, invalid schedules, and duplicate deletes', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('payment terms validation branches should not hit upstream fetch in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation PaymentTermsValidation(
          $missingTarget: ID!
          $validAttrs: PaymentTermsCreateInput!
          $missingTemplateAttrs: PaymentTermsCreateInput!
          $unknownTemplateAttrs: PaymentTermsCreateInput!
          $netAttrs: PaymentTermsCreateInput!
          $fixedAttrs: PaymentTermsCreateInput!
          $missingTerms: PaymentTermsUpdateInput!
          $missingDelete: PaymentTermsDeleteInput!
        ) {
          missingTarget: paymentTermsCreate(referenceId: $missingTarget, paymentTermsAttributes: $validAttrs) {
            paymentTerms { id }
            userErrors { field message code }
          }
          missingTemplate: paymentTermsCreate(referenceId: "gid://shopify/Order/1", paymentTermsAttributes: $missingTemplateAttrs) {
            paymentTerms { id }
            userErrors { field message code }
          }
          unknownTemplate: paymentTermsCreate(referenceId: "gid://shopify/Order/1", paymentTermsAttributes: $unknownTemplateAttrs) {
            paymentTerms { id }
            userErrors { field message code }
          }
          invalidNet: paymentTermsCreate(referenceId: "gid://shopify/Order/1", paymentTermsAttributes: $netAttrs) {
            paymentTerms { id }
            userErrors { field message code }
          }
          invalidFixed: paymentTermsCreate(referenceId: "gid://shopify/Order/1", paymentTermsAttributes: $fixedAttrs) {
            paymentTerms { id }
            userErrors { field message code }
          }
          missingUpdate: paymentTermsUpdate(input: $missingTerms) {
            paymentTerms { id }
            userErrors { field message code }
          }
          missingDelete: paymentTermsDelete(input: $missingDelete) {
            deletedId
            userErrors { field message code }
          }
        }`,
        variables: {
          missingTarget: 'gid://shopify/Order/404',
          validAttrs: {
            paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/1',
          },
          missingTemplateAttrs: { paymentSchedules: [{ issuedAt: '2026-04-27T12:00:00Z' }] },
          unknownTemplateAttrs: { paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/404' },
          netAttrs: { paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4', paymentSchedules: [{}] },
          fixedAttrs: { paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/7', paymentSchedules: [{}] },
          missingTerms: {
            paymentTermsId: 'gid://shopify/PaymentTerms/404',
            paymentTermsAttributes: { paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/1' },
          },
          missingDelete: { paymentTermsId: 'gid://shopify/PaymentTerms/404' },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      missingTarget: {
        paymentTerms: null,
        userErrors: [
          { field: ['referenceId'], message: 'Reference order or draft order does not exist.', code: 'NOT_FOUND' },
        ],
      },
      missingTemplate: {
        paymentTerms: null,
        userErrors: [
          {
            field: ['paymentTermsAttributes', 'paymentTermsTemplateId'],
            message: 'Payment terms template id can not be empty.',
            code: 'PAYMENT_TERMS_TEMPLATE_ID_EMPTY',
          },
        ],
      },
      unknownTemplate: {
        paymentTerms: null,
        userErrors: [
          {
            field: ['paymentTermsAttributes', 'paymentTermsTemplateId'],
            message: 'Payment terms template does not exist.',
            code: 'PAYMENT_TERMS_TEMPLATE_NOT_FOUND',
          },
        ],
      },
      invalidNet: {
        paymentTerms: null,
        userErrors: [
          {
            field: ['paymentTermsAttributes', 'paymentSchedules', '0', 'issuedAt'],
            message: 'Issued at must be provided for net payment terms.',
            code: 'PAYMENT_SCHEDULE_INVALID',
          },
        ],
      },
      invalidFixed: {
        paymentTerms: null,
        userErrors: [
          {
            field: ['paymentTermsAttributes', 'paymentSchedules', '0', 'dueAt'],
            message: 'Due at must be provided for fixed payment terms.',
            code: 'PAYMENT_SCHEDULE_INVALID',
          },
        ],
      },
      missingUpdate: {
        paymentTerms: null,
        userErrors: [
          {
            field: ['paymentTermsId'],
            message: 'Payment terms do not exist.',
            code: 'PAYMENT_TERMS_NOT_FOUND',
          },
        ],
      },
      missingDelete: {
        deletedId: null,
        userErrors: [
          {
            field: ['paymentTermsId'],
            message: 'Payment terms do not exist.',
            code: 'PAYMENT_TERMS_DELETE_UNSUCCESSFUL',
          },
        ],
      },
    });
    expect((await request(app).get('/__meta/log')).body.entries).toEqual([]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
