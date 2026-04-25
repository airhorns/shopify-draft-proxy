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

describe('customer draft flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages customerCreate locally and exposes the created customer on downstream reads without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customerCreate should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id
              firstName
              lastName
              displayName
              email
              locale
              note
              verifiedEmail
              taxExempt
              tags
              state
              canDelete
              defaultEmailAddress { emailAddress }
              defaultPhoneNumber { phoneNumber }
              defaultAddress { address1 }
              createdAt
              updatedAt
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'draft-customer@example.com',
            firstName: 'Draft',
            lastName: 'Customer',
            locale: 'en',
            note: 'created locally',
            phone: '+14155550123',
            tags: ['vip', 'draft'],
            taxExempt: true,
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.customerCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.customerCreate.customer).toMatchObject({
      firstName: 'Draft',
      lastName: 'Customer',
      displayName: 'Draft Customer',
      email: 'draft-customer@example.com',
      locale: 'en',
      note: 'created locally',
      verifiedEmail: true,
      taxExempt: true,
      tags: ['draft', 'vip'],
      state: 'DISABLED',
      canDelete: true,
      defaultEmailAddress: { emailAddress: 'draft-customer@example.com' },
      defaultPhoneNumber: { phoneNumber: '+14155550123' },
      defaultAddress: null,
    });
    expect(createResponse.body.data.customerCreate.customer.id).toMatch(/^gid:\/\/shopify\/Customer\//);
    expect(createResponse.body.data.customerCreate.customer.createdAt).toBeTruthy();
    expect(createResponse.body.data.customerCreate.customer.updatedAt).toBeTruthy();

    const customerId = createResponse.body.data.customerCreate.customer.id;
    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerReadback($id: ID!) {
          detail: customer(id: $id) {
            id
            displayName
            email
            note
            defaultPhoneNumber { phoneNumber }
            orders(first: 1) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            events(first: 1) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            metafields(first: 1) {
              nodes { id namespace key }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            storeCreditAccounts(first: 1) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            paymentMethods(first: 1) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            subscriptionContracts(first: 1) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            lastOrder { id }
          }
          catalog: customers(first: 10) {
            nodes {
              id
              email
              tags
            }
          }
          counts: customersCount {
            count
            precision
          }
        }`,
        variables: { id: customerId },
      });

    expect(readResponse.status).toBe(200);
    const emptyConnection = {
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    };
    expect(readResponse.body).toEqual({
      data: {
        detail: {
          id: customerId,
          displayName: 'Draft Customer',
          email: 'draft-customer@example.com',
          note: 'created locally',
          defaultPhoneNumber: { phoneNumber: '+14155550123' },
          orders: emptyConnection,
          events: emptyConnection,
          metafields: emptyConnection,
          storeCreditAccounts: emptyConnection,
          paymentMethods: emptyConnection,
          subscriptionContracts: emptyConnection,
          lastOrder: null,
        },
        catalog: {
          nodes: [
            {
              id: customerId,
              email: 'draft-customer@example.com',
              tags: ['draft', 'vip'],
            },
          ],
        },
        counts: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages customerUpdate locally and keeps later customer and customers replays aligned', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customerUpdate should not hit upstream fetch');
    });

    const createdAt = '2024-01-01T00:00:00.000Z';
    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/401',
        firstName: 'Ada',
        lastName: 'Lovelace',
        displayName: 'Ada Lovelace',
        email: 'ada@example.com',
        legacyResourceId: '401',
        locale: 'en',
        note: 'before update',
        canDelete: true,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['founder'],
        numberOfOrders: '3',
        amountSpent: null,
        defaultEmailAddress: { emailAddress: 'ada@example.com' },
        defaultPhoneNumber: { phoneNumber: '+141****0001' },
        defaultAddress: null,
        createdAt,
        updatedAt: createdAt,
      },
    ]);

    const app = createApp(snapshotConfig).callback();
    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerUpdate($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer {
              id
              firstName
              lastName
              displayName
              email
              note
              locale
              taxExempt
              tags
              defaultEmailAddress { emailAddress }
              updatedAt
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            id: 'gid://shopify/Customer/401',
            firstName: 'Ada',
            lastName: 'Byron',
            note: 'after update',
            tags: ['vip', 'newsletter'],
            taxExempt: true,
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.customerUpdate.userErrors).toEqual([]);
    expect(updateResponse.body.data.customerUpdate.customer).toMatchObject({
      id: 'gid://shopify/Customer/401',
      firstName: 'Ada',
      lastName: 'Byron',
      displayName: 'Ada Byron',
      email: 'ada@example.com',
      note: 'after update',
      locale: 'en',
      taxExempt: true,
      tags: ['newsletter', 'vip'],
      defaultEmailAddress: { emailAddress: 'ada@example.com' },
    });
    expect(updateResponse.body.data.customerUpdate.customer.updatedAt).not.toBe(createdAt);

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query UpdatedCustomer($id: ID!) {
          customer(id: $id) {
            id
            displayName
            note
            taxExempt
            tags
          }
          customers(first: 5, query: "tag:vip") {
            nodes {
              id
              displayName
              tags
            }
          }
        }`,
        variables: { id: 'gid://shopify/Customer/401' },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        customer: {
          id: 'gid://shopify/Customer/401',
          displayName: 'Ada Byron',
          note: 'after update',
          taxExempt: true,
          tags: ['newsletter', 'vip'],
        },
        customers: {
          nodes: [
            {
              id: 'gid://shopify/Customer/401',
              displayName: 'Ada Byron',
              tags: ['newsletter', 'vip'],
            },
          ],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages customerDelete locally and removes the customer from downstream reads without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customerDelete should not hit upstream fetch');
    });

    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/402',
        firstName: 'Grace',
        lastName: 'Hopper',
        displayName: 'Grace Hopper',
        email: 'grace@example.com',
        legacyResourceId: '402',
        locale: 'en',
        note: null,
        canDelete: true,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['newsletter'],
        numberOfOrders: '8',
        amountSpent: null,
        defaultEmailAddress: { emailAddress: 'grace@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
      },
    ]);

    const app = createApp(snapshotConfig).callback();
    const deleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerDelete($input: CustomerDeleteInput!) {
          customerDelete(input: $input) {
            deletedCustomerId
            userErrors { field message }
          }
        }`,
        variables: { input: { id: 'gid://shopify/Customer/402' } },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body).toEqual({
      data: {
        customerDelete: {
          deletedCustomerId: 'gid://shopify/Customer/402',
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query DeletedCustomer($id: ID!) {
          customer(id: $id) { id }
          customers(first: 5) { nodes { id } }
          customersCount { count precision }
        }`,
        variables: { id: 'gid://shopify/Customer/402' },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        customer: null,
        customers: { nodes: [] },
        customersCount: { count: 0, precision: 'EXACT' },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured customer mutation validation userErrors in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customer mutation validation should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerCreateValidation($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: { input: { email: '' } },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body).toEqual({
      data: {
        customerCreate: {
          customer: null,
          userErrors: [
            {
              field: null,
              message: 'A name, phone number, or email address must be present',
            },
          ],
        },
      },
    });

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerUpdateValidation($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: { input: { id: 'gid://shopify/Customer/999999999999999', firstName: 'Ghost' } },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body).toEqual({
      data: {
        customerUpdate: {
          customer: null,
          userErrors: [
            {
              field: ['id'],
              message: 'Customer does not exist',
            },
          ],
        },
      },
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerDeleteValidation($input: CustomerDeleteInput!) {
          customerDelete(input: $input) {
            deletedCustomerId
            userErrors { field message }
          }
        }`,
        variables: { input: { id: 'gid://shopify/Customer/999999999999999' } },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body).toEqual({
      data: {
        customerDelete: {
          deletedCustomerId: null,
          userErrors: [
            {
              field: ['id'],
              message: "Customer can't be found",
            },
          ],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
