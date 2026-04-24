import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

describe('customer email side-effect safety', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('suppresses customer email side-effect roots locally without calling upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customer email side-effect roots must not hit upstream fetch');
    });

    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/401',
        firstName: 'Ada',
        lastName: 'Lovelace',
        displayName: 'Ada Lovelace',
        email: 'ada@example.com',
        legacyResourceId: '401',
        locale: 'en',
        note: null,
        canDelete: true,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: [],
        numberOfOrders: 0,
        amountSpent: null,
        defaultEmailAddress: { emailAddress: 'ada@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-01T00:00:00.000Z',
      },
    ]);

    const app = createApp(config).callback();

    const inviteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation SendInvite($customerId: ID!) {
          customerSendAccountInviteEmail(customerId: $customerId) {
            customer { id email }
            userErrors { field message }
          }
        }`,
        variables: { customerId: 'gid://shopify/Customer/401' },
      });
    const paymentMethodResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation SendPaymentMethodUpdate($customerPaymentMethodId: ID!) {
          customerPaymentMethodSendUpdateEmail(customerPaymentMethodId: $customerPaymentMethodId) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: { customerPaymentMethodId: 'gid://shopify/CustomerPaymentMethod/pm_401' },
      });

    expect(inviteResponse.status).toBe(200);
    expect(inviteResponse.body.data.customerSendAccountInviteEmail).toEqual({
      customer: {
        id: 'gid://shopify/Customer/401',
        email: 'ada@example.com',
      },
      userErrors: [
        {
          field: ['customerId'],
          message:
            'customerSendAccountInviteEmail is intentionally suppressed by the local proxy because it sends email.',
        },
      ],
    });
    expect(paymentMethodResponse.status).toBe(200);
    expect(paymentMethodResponse.body.data.customerPaymentMethodSendUpdateEmail).toEqual({
      customer: null,
      userErrors: [
        {
          field: ['customerPaymentMethodId'],
          message:
            'customerPaymentMethodSendUpdateEmail is intentionally suppressed by the local proxy because it sends email.',
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.status).toBe(200);
    expect(logResponse.body.entries).toEqual([
      expect.objectContaining({
        operationName: 'customerSendAccountInviteEmail',
        status: 'suppressed',
        interpreted: expect.objectContaining({
          primaryRootField: 'customerSendAccountInviteEmail',
          capability: {
            operationName: 'customerSendAccountInviteEmail',
            domain: 'customers',
            execution: 'suppress-locally',
          },
        }),
        notes: 'Locally suppressed customerSendAccountInviteEmail without sending customer email upstream.',
      }),
      expect.objectContaining({
        operationName: 'customerPaymentMethodSendUpdateEmail',
        status: 'suppressed',
        interpreted: expect.objectContaining({
          primaryRootField: 'customerPaymentMethodSendUpdateEmail',
          capability: {
            operationName: 'customerPaymentMethodSendUpdateEmail',
            domain: 'customers',
            execution: 'suppress-locally',
          },
        }),
        notes: 'Locally suppressed customerPaymentMethodSendUpdateEmail without sending customer email upstream.',
      }),
    ]);

    const commitResponse = await request(app).post('/__meta/commit').set('x-shopify-access-token', 'shpat_commit_test');
    expect(commitResponse.status).toBe(200);
    expect(commitResponse.body).toEqual({
      ok: true,
      stopIndex: null,
      attempts: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('keeps intentionally unsupported customer roots visibly proxied', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            customerMerge: {
              resultingCustomerId: 'gid://shopify/Customer/401',
              job: null,
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

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_customer_merge_test')
      .send({
        query: `mutation MergeCustomers($customerOneId: ID!, $customerTwoId: ID!) {
          customerMerge(customerOneId: $customerOneId, customerTwoId: $customerTwoId) {
            resultingCustomerId
            job { id }
            userErrors { field message }
          }
        }`,
        variables: {
          customerOneId: 'gid://shopify/Customer/401',
          customerTwoId: 'gid://shopify/Customer/402',
        },
      });

    expect(response.status).toBe(200);
    expect(fetchSpy).toHaveBeenCalledTimes(1);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries).toEqual([
      expect.objectContaining({
        operationName: 'customerMerge',
        status: 'proxied',
        interpreted: expect.objectContaining({
          primaryRootField: 'customerMerge',
          capability: {
            operationName: 'customerMerge',
            domain: 'customers',
            execution: 'passthrough',
          },
        }),
        notes: 'Mutation passthrough placeholder until supported local staging is implemented.',
      }),
    ]);
  });
});
