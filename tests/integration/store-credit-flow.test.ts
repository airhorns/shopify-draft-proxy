import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { CustomerRecord, StoreCreditAccountRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeCustomer(overrides: Partial<CustomerRecord> = {}): CustomerRecord {
  return {
    id: 'gid://shopify/Customer/8801',
    firstName: 'Credit',
    lastName: 'Holder',
    displayName: 'Credit Holder',
    email: 'credit-holder@example.com',
    legacyResourceId: '8801',
    locale: 'en',
    note: null,
    canDelete: true,
    verifiedEmail: true,
    taxExempt: false,
    state: 'DISABLED',
    tags: [],
    numberOfOrders: 0,
    amountSpent: { amount: '0.00', currencyCode: 'USD' },
    defaultEmailAddress: { emailAddress: 'credit-holder@example.com' },
    defaultPhoneNumber: null,
    defaultAddress: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...overrides,
  };
}

function makeStoreCreditAccount(overrides: Partial<StoreCreditAccountRecord> = {}): StoreCreditAccountRecord {
  return {
    id: 'gid://shopify/StoreCreditAccount/9901',
    customerId: 'gid://shopify/Customer/8801',
    cursor: 'store-credit-account-9901',
    balance: { amount: '10.00', currencyCode: 'USD' },
    ...overrides,
  };
}

describe('store credit account draft flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('does not invent sensitive store credit accounts in snapshot mode', async () => {
    store.upsertBaseCustomers([makeCustomer()]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('snapshot store credit reads should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query StoreCreditAbsent($customerId: ID!, $accountId: ID!) {
          customer(id: $customerId) {
            id
            storeCreditAccounts(first: 2) {
              nodes { id balance { amount currencyCode } }
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          storeCreditAccount(id: $accountId) {
            id
            balance { amount currencyCode }
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/8801',
          accountId: 'gid://shopify/StoreCreditAccount/404',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        customer: {
          id: 'gid://shopify/Customer/8801',
          storeCreditAccounts: {
            nodes: [],
            edges: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: null,
              endCursor: null,
            },
          },
        },
        storeCreditAccount: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('reads seeded store credit accounts from direct and customer roots', async () => {
    store.upsertBaseCustomers([makeCustomer()]);
    store.upsertBaseStoreCreditAccounts([makeStoreCreditAccount()]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('snapshot store credit account reads should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query StoreCreditRead($customerId: ID!, $accountId: ID!) {
          direct: storeCreditAccount(id: $accountId) {
            id
            balance { amount currencyCode }
            owner {
              __typename
              ... on Customer { id email displayName }
            }
            transactions(first: 1) {
              nodes { amount { amount currencyCode } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          customer(id: $customerId) {
            storeCreditAccounts(first: 1) {
              nodes { id balance { amount currencyCode } }
              edges { cursor node { id } }
            }
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/8801',
          accountId: 'gid://shopify/StoreCreditAccount/9901',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        direct: {
          id: 'gid://shopify/StoreCreditAccount/9901',
          balance: { amount: '10.00', currencyCode: 'USD' },
          owner: {
            __typename: 'Customer',
            id: 'gid://shopify/Customer/8801',
            email: 'credit-holder@example.com',
            displayName: 'Credit Holder',
          },
          transactions: {
            nodes: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: null,
              endCursor: null,
            },
          },
        },
        customer: {
          storeCreditAccounts: {
            nodes: [
              {
                id: 'gid://shopify/StoreCreditAccount/9901',
                balance: { amount: '10.00', currencyCode: 'USD' },
              },
            ],
            edges: [
              {
                cursor: 'cursor:store-credit-account-9901',
                node: { id: 'gid://shopify/StoreCreditAccount/9901' },
              },
            ],
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages credit and debit mutations locally and exposes downstream balance changes', async () => {
    store.upsertBaseCustomers([makeCustomer()]);
    store.upsertBaseStoreCreditAccounts([makeStoreCreditAccount()]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('supported store credit mutations should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const creditResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation StoreCreditCredit($id: ID!, $creditInput: StoreCreditAccountCreditInput!) {
          storeCreditAccountCredit(id: $id, creditInput: $creditInput) {
            storeCreditAccountTransaction {
              amount { amount currencyCode }
              balanceAfterTransaction { amount currencyCode }
              createdAt
              event
              origin { __typename }
              account { id balance { amount currencyCode } }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: 'gid://shopify/StoreCreditAccount/9901',
          creditInput: { creditAmount: { amount: '5.00', currencyCode: 'USD' } },
        },
      });

    expect(creditResponse.status).toBe(200);
    expect(creditResponse.body.data.storeCreditAccountCredit.userErrors).toEqual([]);
    expect(creditResponse.body.data.storeCreditAccountCredit.storeCreditAccountTransaction).toMatchObject({
      amount: { amount: '5.00', currencyCode: 'USD' },
      balanceAfterTransaction: { amount: '15.00', currencyCode: 'USD' },
      event: 'ADJUSTMENT',
      origin: null,
      account: {
        id: 'gid://shopify/StoreCreditAccount/9901',
        balance: { amount: '15.00', currencyCode: 'USD' },
      },
    });

    const debitResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation StoreCreditDebit($id: ID!, $debitInput: StoreCreditAccountDebitInput!) {
          storeCreditAccountDebit(id: $id, debitInput: $debitInput) {
            storeCreditAccountTransaction {
              amount { amount currencyCode }
              balanceAfterTransaction { amount currencyCode }
              event
              account { id balance { amount currencyCode } }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: 'gid://shopify/StoreCreditAccount/9901',
          debitInput: { debitAmount: { amount: '3.00', currencyCode: 'USD' } },
        },
      });

    expect(debitResponse.status).toBe(200);
    expect(debitResponse.body.data.storeCreditAccountDebit).toEqual({
      storeCreditAccountTransaction: {
        amount: { amount: '-3.00', currencyCode: 'USD' },
        balanceAfterTransaction: { amount: '12.00', currencyCode: 'USD' },
        event: 'ADJUSTMENT',
        account: {
          id: 'gid://shopify/StoreCreditAccount/9901',
          balance: { amount: '12.00', currencyCode: 'USD' },
        },
      },
      userErrors: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query StoreCreditReadback($id: ID!) {
          storeCreditAccount(id: $id) {
            id
            balance { amount currencyCode }
            transactions(first: 2) {
              nodes {
                amount { amount currencyCode }
                balanceAfterTransaction { amount currencyCode }
                event
              }
            }
          }
        }`,
        variables: { id: 'gid://shopify/StoreCreditAccount/9901' },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        storeCreditAccount: {
          id: 'gid://shopify/StoreCreditAccount/9901',
          balance: { amount: '12.00', currencyCode: 'USD' },
          transactions: {
            nodes: [
              {
                amount: { amount: '-3.00', currencyCode: 'USD' },
                balanceAfterTransaction: { amount: '12.00', currencyCode: 'USD' },
                event: 'ADJUSTMENT',
              },
              {
                amount: { amount: '5.00', currencyCode: 'USD' },
                balanceAfterTransaction: { amount: '15.00', currencyCode: 'USD' },
                event: 'ADJUSTMENT',
              },
            ],
          },
        },
      },
    });
    expect(store.getLog().map((entry) => entry.status)).toEqual(['staged', 'staged']);
    expect(store.getLog().map((entry) => entry.operationName)).toEqual([
      'storeCreditAccountCredit',
      'storeCreditAccountDebit',
    ]);
    expect(store.getLog()[0]?.requestBody).toMatchObject({
      variables: {
        id: 'gid://shopify/StoreCreditAccount/9901',
        creditInput: { creditAmount: { amount: '5.00', currencyCode: 'USD' } },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns captured not-found userErrors locally for unknown store credit accounts', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('store credit validation branches should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation StoreCreditUnknown($id: ID!, $creditInput: StoreCreditAccountCreditInput!) {
          storeCreditAccountCredit(id: $id, creditInput: $creditInput) {
            storeCreditAccountTransaction { amount { amount currencyCode } }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: 'gid://shopify/StoreCreditAccount/404',
          creditInput: { creditAmount: { amount: '1.00', currencyCode: 'USD' } },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        storeCreditAccountCredit: {
          storeCreditAccountTransaction: null,
          userErrors: [
            {
              field: ['id'],
              message: 'Store credit account does not exist',
              code: 'ACCOUNT_NOT_FOUND',
            },
          ],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
