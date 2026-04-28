import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { defaultPaymentTermsTemplates } from '../../src/state/types.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const liveHybridConfig: AppConfig = {
  ...snapshotConfig,
  readMode: 'live-hybrid',
};

const paymentTermsSelection = `
  id
  due
  overdue
  dueInDays
  paymentTermsName
  paymentTermsType
  translatedName
  paymentSchedules(first: 1) {
    nodes {
      id
      dueAt
      issuedAt
      completedAt
      completed
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
    edges {
      cursor
      node {
        id
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

describe('payment terms query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it.each([
    ['snapshot', snapshotConfig],
    ['live-hybrid', liveHybridConfig],
  ])(
    'serializes paymentTermsTemplates catalog and enum filtering in %s mode without upstream access',
    async (_mode, config) => {
      const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
        throw new Error('payment terms template reads should not hit upstream fetch');
      });

      const app = createApp(config).callback();
      const response = await request(app)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query: `query PaymentTermsTemplatesRead($type: PaymentTermsType) {
          all: paymentTermsTemplates {
            id
            name
            description
            dueInDays
            paymentTermsType
            translatedName
            __typename
          }
          filtered: paymentTermsTemplates(paymentTermsType: $type) {
            id
            name
            dueInDays
            paymentTermsType
          }
        }`,
          variables: {
            type: 'NET',
          },
        });

      expect(response.status).toBe(200);
      expect(response.body).toEqual({
        data: {
          all: defaultPaymentTermsTemplates.map((template) => ({
            id: template.id,
            name: template.name,
            description: template.description,
            dueInDays: template.dueInDays,
            paymentTermsType: template.paymentTermsType,
            translatedName: template.translatedName,
            __typename: 'PaymentTermsTemplate',
          })),
          filtered: defaultPaymentTermsTemplates
            .filter((template) => template.paymentTermsType === 'NET')
            .map((template) => ({
              id: template.id,
              name: template.name,
              dueInDays: template.dueInDays,
              paymentTermsType: template.paymentTermsType,
            })),
        },
      });
      expect(fetchSpy).not.toHaveBeenCalled();
    },
  );

  it('serializes null and normalized paymentTerms graphs for draft-order and order reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('payment terms order reads should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const orderCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation OrderWithoutPaymentTerms($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order {
              id
              paymentTerms {
                id
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
            email: 'no-payment-terms-order@example.com',
            lineItems: [{ title: 'No terms order line', quantity: 1, priceSet: { shopMoney: { amount: '4.00' } } }],
          },
        },
      });

    expect(orderCreateResponse.status).toBe(200);
    expect(orderCreateResponse.body.data.orderCreate).toEqual({
      order: {
        id: expect.stringMatching(/^gid:\/\/shopify\/Order\/\d+$/),
        paymentTerms: null,
      },
      userErrors: [],
    });

    const blockedPaymentTermsCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftWithPaymentTermsPermissionBranch($input: DraftOrderInput!) {
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
            paymentTerms: {
              paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
            },
            lineItems: [{ title: 'Blocked terms draft line', quantity: 1, originalUnitPrice: '10.00' }],
          },
        },
      });

    expect(blockedPaymentTermsCreateResponse.status).toBe(200);
    expect(blockedPaymentTermsCreateResponse.body.data.draftOrderCreate).toEqual({
      draftOrder: null,
      userErrors: [{ field: null, message: 'The user must have access to set payment terms.' }],
    });

    const draftCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DraftWithoutPaymentTerms($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              paymentTerms {
                id
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
            email: 'payment-terms-draft@example.com',
            lineItems: [{ title: 'Terms draft line', quantity: 1, originalUnitPrice: '10.00' }],
          },
        },
      });

    expect(draftCreateResponse.status).toBe(200);
    expect(draftCreateResponse.body.data.draftOrderCreate.userErrors).toEqual([]);
    expect(draftCreateResponse.body.data.draftOrderCreate.draftOrder.paymentTerms).toBeNull();

    const draftOrderId = draftCreateResponse.body.data.draftOrderCreate.draftOrder.id as string;
    const draftOrder = store.getDraftOrderById(draftOrderId);
    if (!draftOrder) {
      throw new Error(`Expected staged draft order ${draftOrderId}`);
    }

    store.updateDraftOrder({
      ...draftOrder,
      paymentTerms: {
        id: 'gid://shopify/PaymentTerms/500',
        due: false,
        overdue: false,
        dueInDays: 30,
        paymentTermsName: 'Net 30',
        paymentTermsType: 'NET',
        translatedName: 'Net 30',
        paymentSchedules: [
          {
            id: 'gid://shopify/PaymentSchedule/501',
            dueAt: '2026-05-22T12:00:00Z',
            issuedAt: '2026-04-22T12:00:00Z',
            completedAt: null,
            completed: false,
            due: false,
            amount: { amount: '10.0', currencyCode: 'CAD' },
            balanceDue: { amount: '10.0', currencyCode: 'CAD' },
            totalBalance: { amount: '10.0', currencyCode: 'CAD' },
          },
        ],
      },
    });

    const expectedPaymentTerms = {
      id: 'gid://shopify/PaymentTerms/500',
      due: false,
      overdue: false,
      dueInDays: 30,
      paymentTermsName: 'Net 30',
      paymentTermsType: 'NET',
      translatedName: 'Net 30',
      paymentSchedules: {
        nodes: [
          {
            id: 'gid://shopify/PaymentSchedule/501',
            dueAt: '2026-05-22T12:00:00Z',
            issuedAt: '2026-04-22T12:00:00Z',
            completedAt: null,
            completed: false,
            due: false,
            amount: { amount: '10.0', currencyCode: 'CAD' },
            balanceDue: { amount: '10.0', currencyCode: 'CAD' },
            totalBalance: { amount: '10.0', currencyCode: 'CAD' },
          },
        ],
        edges: [
          {
            cursor: 'cursor:gid://shopify/PaymentSchedule/501',
            node: {
              id: 'gid://shopify/PaymentSchedule/501',
            },
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: 'cursor:gid://shopify/PaymentSchedule/501',
          endCursor: 'cursor:gid://shopify/PaymentSchedule/501',
        },
      },
    };

    const draftReadResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query DraftPaymentTerms($id: ID!) {
          draftOrder(id: $id) {
            id
            paymentTerms {
              ${paymentTermsSelection}
            }
          }
        }`,
        variables: {
          id: draftOrderId,
        },
      });

    expect(draftReadResponse.status).toBe(200);
    expect(draftReadResponse.body.data.draftOrder.paymentTerms).toEqual(expectedPaymentTerms);

    const completeResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CompleteDraftWithPaymentTerms($id: ID!) {
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
          id: draftOrderId,
        },
      });

    expect(completeResponse.status).toBe(200);
    expect(completeResponse.body.data.draftOrderComplete.userErrors).toEqual([]);

    const orderReadResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CompletedOrderPaymentTerms {
          orders(first: 1, query: "email:payment-terms-draft@example.com") {
            nodes {
              id
              paymentTerms {
                ${paymentTermsSelection}
              }
            }
          }
        }`,
      });

    expect(orderReadResponse.status).toBe(200);
    expect(orderReadResponse.body.data.orders.nodes).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/Order\/\d+$/),
        paymentTerms: expectedPaymentTerms,
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
