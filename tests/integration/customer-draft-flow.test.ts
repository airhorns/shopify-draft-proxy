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
          byIdentifier: customerByIdentifier(identifier: { emailAddress: "draft-customer@example.com" }) {
            id
            email
            defaultPhoneNumber { phoneNumber }
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
        byIdentifier: {
          id: customerId,
          email: 'draft-customer@example.com',
          defaultPhoneNumber: { phoneNumber: '+14155550123' },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages customer address lifecycle mutations and overlays downstream address reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customer address mutations should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const createCustomerResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'address-flow@example.com',
            firstName: 'Address',
            lastName: 'Flow',
          },
        },
      });
    const customerId = createCustomerResponse.body.data.customerCreate.customer.id;

    const createAddressResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation AddressCreate($customerId: ID!) {
          customerAddressCreate(
            customerId: $customerId
            address: {
              firstName: "Ada"
              lastName: "Lovelace"
              address1: "1 Main"
              city: "Ottawa"
              countryCode: CA
              provinceCode: "ON"
              zip: "K1A 0B1"
            }
            setAsDefault: true
          ) {
            address { id address1 city country countryCodeV2 provinceCode zip }
            userErrors { field message }
          }
        }`,
        variables: { customerId },
      });
    expect(createAddressResponse.status).toBe(200);
    expect(createAddressResponse.body.data.customerAddressCreate.userErrors).toEqual([]);
    const firstAddressId = createAddressResponse.body.data.customerAddressCreate.address.id;

    const createSecondAddressResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation AddressCreateSecond($customerId: ID!) {
          customerAddressCreate(
            customerId: $customerId
            address: { address1: "2 Side", city: "Toronto", countryCode: CA, provinceCode: "ON", zip: "M5H 2N2" }
          ) {
            address { id address1 city countryCodeV2 provinceCode zip }
            userErrors { field message }
          }
        }`,
        variables: { customerId },
      });
    const secondAddressId = createSecondAddressResponse.body.data.customerAddressCreate.address.id;

    const updateSecondAddressResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation AddressUpdate($customerId: ID!, $addressId: ID!) {
          customerAddressUpdate(
            customerId: $customerId
            addressId: $addressId
            address: { city: "Montreal", provinceCode: "QC", zip: "H2Y 1C6" }
          ) {
            address { id address1 city provinceCode zip }
            userErrors { field message }
          }
        }`,
        variables: { customerId, addressId: secondAddressId },
      });
    expect(updateSecondAddressResponse.body.data.customerAddressUpdate).toEqual({
      address: {
        id: secondAddressId,
        address1: '2 Side',
        city: 'Montreal',
        provinceCode: 'QC',
        zip: 'H2Y 1C6',
      },
      userErrors: [],
    });

    const defaultAddressResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DefaultAddress($customerId: ID!, $addressId: ID!) {
          customerUpdateDefaultAddress(customerId: $customerId, addressId: $addressId) {
            customer { id defaultAddress { id address1 city provinceCode zip } }
            userErrors { field message }
          }
        }`,
        variables: { customerId, addressId: secondAddressId },
      });
    expect(defaultAddressResponse.body.data.customerUpdateDefaultAddress).toEqual({
      customer: {
        id: customerId,
        defaultAddress: {
          id: secondAddressId,
          address1: '2 Side',
          city: 'Montreal',
          provinceCode: 'QC',
          zip: 'H2Y 1C6',
        },
      },
      userErrors: [],
    });

    const deleteFirstAddressResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation AddressDelete($customerId: ID!, $addressId: ID!) {
          customerAddressDelete(customerId: $customerId, addressId: $addressId) {
            deletedAddressId
            userErrors { field message }
          }
        }`,
        variables: { customerId, addressId: firstAddressId },
      });
    expect(deleteFirstAddressResponse.body.data.customerAddressDelete).toEqual({
      deletedAddressId: firstAddressId,
      userErrors: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query AddressReadback($id: ID!) {
          detail: customer(id: $id) {
            id
            defaultAddress { id address1 city provinceCode zip }
            addressesV2(first: 5) {
              nodes { id address1 city provinceCode zip }
              edges { cursor node { id city } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          byIdentifier: customerByIdentifier(identifier: { emailAddress: "address-flow@example.com" }) {
            id
            defaultAddress { id city }
            addressesV2(first: 5) { nodes { id city } }
          }
          catalog: customers(first: 5, query: "email:address-flow@example.com") {
            nodes {
              id
              defaultAddress { id city }
              addressesV2(first: 5) { nodes { id city } }
            }
          }
        }`,
        variables: { id: customerId },
      });

    expect(readResponse.body.data).toEqual({
      detail: {
        id: customerId,
        defaultAddress: {
          id: secondAddressId,
          address1: '2 Side',
          city: 'Montreal',
          provinceCode: 'QC',
          zip: 'H2Y 1C6',
        },
        addressesV2: {
          nodes: [
            {
              id: secondAddressId,
              address1: '2 Side',
              city: 'Montreal',
              provinceCode: 'QC',
              zip: 'H2Y 1C6',
            },
          ],
          edges: [
            {
              cursor: `customer-address-${customerId}-1`,
              node: {
                id: secondAddressId,
                city: 'Montreal',
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: `customer-address-${customerId}-1`,
            endCursor: `customer-address-${customerId}-1`,
          },
        },
      },
      byIdentifier: {
        id: customerId,
        defaultAddress: {
          id: secondAddressId,
          city: 'Montreal',
        },
        addressesV2: {
          nodes: [
            {
              id: secondAddressId,
              city: 'Montreal',
            },
          ],
        },
      },
      catalog: {
        nodes: [
          {
            id: customerId,
            defaultAddress: {
              id: secondAddressId,
              city: 'Montreal',
            },
            addressesV2: {
              nodes: [
                {
                  id: secondAddressId,
                  city: 'Montreal',
                },
              ],
            },
          },
        ],
      },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'CustomerCreate',
      'customerAddressCreate',
      'customerAddressCreate',
      'customerAddressUpdate',
      'customerUpdateDefaultAddress',
      'customerAddressDelete',
    ]);
    expect(logResponse.body.entries.at(-1).requestBody.variables).toEqual({
      customerId,
      addressId: firstAddressId,
    });

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.customerAddresses[secondAddressId]).toMatchObject({
      id: secondAddressId,
      customerId,
      address1: '2 Side',
      city: 'Montreal',
      provinceCode: 'QC',
      zip: 'H2Y 1C6',
    });
    expect(stateResponse.body.stagedState.deletedCustomerAddressIds[firstAddressId]).toBe(true);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors fixture-backed customer address validation branches locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customer address validation should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const createCustomerResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'address-validation@example.com',
            firstName: 'Address',
            lastName: 'Validation',
          },
        },
      });
    const customerId = createCustomerResponse.body.data.customerCreate.customer.id;

    const unknownCustomerResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UnknownCustomerAddressCreate($customerId: ID!) {
          customerAddressCreate(
            customerId: $customerId
            address: { address1: "1 Main", city: "Ottawa", countryCode: CA, provinceCode: "ON", zip: "K1A 0B1" }
            setAsDefault: true
          ) {
            address { id }
            userErrors { field message }
          }
        }`,
        variables: { customerId: 'gid://shopify/Customer/999999999999999' },
      });
    expect(unknownCustomerResponse.body).toEqual({
      data: {
        customerAddressCreate: {
          address: null,
          userErrors: [
            {
              field: ['customerId'],
              message: 'Customer does not exist',
            },
          ],
        },
      },
    });

    const unknownAddressId = 'gid://shopify/MailingAddress/999999999999999';
    const unknownUpdateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UnknownAddressUpdate($customerId: ID!, $addressId: ID!) {
          customerAddressUpdate(customerId: $customerId, addressId: $addressId, address: { city: "Ghost" }) {
            address { id }
            userErrors { field message }
          }
        }`,
        variables: { customerId, addressId: unknownAddressId },
      });
    expect(unknownUpdateResponse.body).toEqual({
      data: { customerAddressUpdate: null },
      errors: [
        {
          message: 'invalid id',
          path: ['customerAddressUpdate'],
          extensions: { code: 'RESOURCE_NOT_FOUND' },
        },
      ],
    });

    const unknownDefaultResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UnknownDefaultAddress($customerId: ID!, $addressId: ID!) {
          customerUpdateDefaultAddress(customerId: $customerId, addressId: $addressId) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: { customerId, addressId: unknownAddressId },
      });
    expect(unknownDefaultResponse.body).toEqual({
      data: { customerUpdateDefaultAddress: null },
      errors: [
        {
          message: 'invalid id',
          path: ['customerUpdateDefaultAddress'],
          extensions: { code: 'RESOURCE_NOT_FOUND' },
        },
      ],
    });

    const unknownDeleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UnknownAddressDelete($customerId: ID!, $addressId: ID!) {
          customerAddressDelete(customerId: $customerId, addressId: $addressId) {
            deletedAddressId
            userErrors { field message }
          }
        }`,
        variables: { customerId, addressId: unknownAddressId },
      });
    expect(unknownDeleteResponse.body).toEqual({
      data: { customerAddressDelete: null },
      errors: [
        {
          message: 'invalid id',
          path: ['customerAddressDelete'],
          extensions: { code: 'RESOURCE_NOT_FOUND' },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('exposes staged customerCreate rows through filtered reverse and backward customer windows', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('staged customer connection replay should not hit upstream fetch');
    });
    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/501',
        firstName: 'Alan',
        lastName: 'Turing',
        displayName: 'Alan Turing',
        email: 'alan@example.com',
        legacyResourceId: '501',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['vip'],
        numberOfOrders: '1',
        amountSpent: null,
        defaultEmailAddress: { emailAddress: 'alan@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2023-12-01T00:00:00.000Z',
        updatedAt: '2023-12-01T00:00:00.000Z',
      },
      {
        id: 'gid://shopify/Customer/502',
        firstName: 'Barbara',
        lastName: 'Liskov',
        displayName: 'Barbara Liskov',
        email: 'barbara@example.com',
        legacyResourceId: '502',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['vip'],
        numberOfOrders: '2',
        amountSpent: null,
        defaultEmailAddress: { emailAddress: 'barbara@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2023-12-02T00:00:00.000Z',
        updatedAt: '2023-12-02T00:00:00.000Z',
      },
      {
        id: 'gid://shopify/Customer/503',
        firstName: 'Claude',
        lastName: 'Shannon',
        displayName: 'Claude Shannon',
        email: 'claude@example.com',
        legacyResourceId: '503',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['vip'],
        numberOfOrders: '3',
        amountSpent: null,
        defaultEmailAddress: { emailAddress: 'claude@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2023-12-03T00:00:00.000Z',
        updatedAt: '2023-12-03T00:00:00.000Z',
      },
      {
        id: 'gid://shopify/Customer/504',
        firstName: 'Donald',
        lastName: 'Knuth',
        displayName: 'Donald Knuth',
        email: 'donald@example.com',
        legacyResourceId: '504',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['vip'],
        numberOfOrders: '4',
        amountSpent: null,
        defaultEmailAddress: { emailAddress: 'donald@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2023-12-04T00:00:00.000Z',
        updatedAt: '2023-12-04T00:00:00.000Z',
      },
    ]);
    store.setBaseCustomerCatalogConnection({
      orderedCustomerIds: [
        'gid://shopify/Customer/501',
        'gid://shopify/Customer/502',
        'gid://shopify/Customer/503',
        'gid://shopify/Customer/504',
      ],
      cursorByCustomerId: {
        'gid://shopify/Customer/501': 'opaque-cursor-501',
        'gid://shopify/Customer/502': 'opaque-cursor-502',
        'gid://shopify/Customer/503': 'opaque-cursor-503',
        'gid://shopify/Customer/504': 'opaque-cursor-504',
      },
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'opaque-cursor-501',
        endCursor: 'opaque-cursor-504',
      },
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id displayName email state tags createdAt updatedAt }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'zeta-vip@example.com',
            firstName: 'Zeta',
            lastName: 'Vip',
            tags: ['vip', 'draft'],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.customerCreate.userErrors).toEqual([]);
    const customerId = createResponse.body.data.customerCreate.customer.id;
    const syntheticCursor = `cursor:${customerId}`;

    const connectionResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query StagedCustomerWindows($query: String!, $before: String!) {
          firstPage: customers(first: 2, query: $query, sortKey: UPDATED_AT, reverse: true) {
            edges {
              cursor
              node { id displayName email tags updatedAt }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          previousPage: customers(last: 2, before: $before, query: $query, sortKey: UPDATED_AT, reverse: true) {
            edges {
              cursor
              node { id displayName email tags updatedAt }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          counts: customersCount(query: "state:DISABLED") {
            count
            precision
          }
        }`,
        variables: {
          query: 'state:DISABLED tag:vip',
          before: 'opaque-cursor-502',
        },
      });

    expect(connectionResponse.status).toBe(200);
    expect(connectionResponse.body).toEqual({
      data: {
        firstPage: {
          edges: [
            {
              cursor: syntheticCursor,
              node: {
                id: customerId,
                displayName: 'Zeta Vip',
                email: 'zeta-vip@example.com',
                tags: ['draft', 'vip'],
                updatedAt: '2024-01-01T00:00:01.000Z',
              },
            },
            {
              cursor: 'opaque-cursor-504',
              node: {
                id: 'gid://shopify/Customer/504',
                displayName: 'Donald Knuth',
                email: 'donald@example.com',
                tags: ['vip'],
                updatedAt: '2023-12-04T00:00:00.000Z',
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: syntheticCursor,
            endCursor: 'opaque-cursor-504',
          },
        },
        previousPage: {
          edges: [
            {
              cursor: 'opaque-cursor-504',
              node: {
                id: 'gid://shopify/Customer/504',
                displayName: 'Donald Knuth',
                email: 'donald@example.com',
                tags: ['vip'],
                updatedAt: '2023-12-04T00:00:00.000Z',
              },
            },
            {
              cursor: 'opaque-cursor-503',
              node: {
                id: 'gid://shopify/Customer/503',
                displayName: 'Claude Shannon',
                email: 'claude@example.com',
                tags: ['vip'],
                updatedAt: '2023-12-03T00:00:00.000Z',
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: true,
            startCursor: 'opaque-cursor-504',
            endCursor: 'opaque-cursor-503',
          },
        },
        counts: {
          count: 5,
          precision: 'EXACT',
        },
      },
      extensions: {
        search: [
          {
            path: ['counts'],
            query: 'state:DISABLED',
            parsed: {
              field: 'state',
              match_all: 'DISABLED',
            },
            warnings: [
              {
                field: 'state',
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

  it('stages customerMerge locally and keeps downstream customer reads aligned', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customerMerge should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const createMutation = `mutation CustomerCreate($input: CustomerInput!) {
      customerCreate(input: $input) {
        customer { id firstName lastName displayName email note tags defaultEmailAddress { emailAddress } createdAt updatedAt }
        userErrors { field message }
      }
    }`;

    const createOneResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: createMutation,
        variables: {
          input: {
            email: 'merge-one@example.com',
            firstName: 'Merge',
            lastName: 'One',
            note: 'one note',
            tags: ['merge-one'],
          },
        },
      });
    const createTwoResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: createMutation,
        variables: {
          input: {
            email: 'merge-two@example.com',
            firstName: 'Merge',
            lastName: 'Two',
            note: 'two note',
            tags: ['merge-two'],
          },
        },
      });

    const customerOneId = createOneResponse.body.data.customerCreate.customer.id;
    const customerTwoId = createTwoResponse.body.data.customerCreate.customer.id;

    const previewResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerMergePreview($one: ID!, $two: ID!) {
          customerMergePreview(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            defaultFields {
              firstName
              lastName
              email { emailAddress }
              note
              tags
            }
            alternateFields {
              firstName
              lastName
              email { emailAddress }
            }
            blockingFields { note tags }
            customerMergeErrors { errorFields message }
          }
        }`,
        variables: { one: customerOneId, two: customerTwoId },
      });

    expect(previewResponse.status).toBe(200);
    expect(previewResponse.body.data.customerMergePreview).toEqual({
      resultingCustomerId: customerTwoId,
      defaultFields: {
        firstName: 'Merge',
        lastName: 'Two',
        email: { emailAddress: 'merge-two@example.com' },
        note: 'two note one note',
        tags: ['merge-one', 'merge-two'],
      },
      alternateFields: {
        firstName: 'Merge',
        lastName: 'One',
        email: { emailAddress: 'merge-one@example.com' },
      },
      blockingFields: null,
      customerMergeErrors: null,
    });

    const mergeResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerMerge($one: ID!, $two: ID!, $override: CustomerMergeOverrideFields) {
          customerMerge(customerOneId: $one, customerTwoId: $two, overrideFields: $override) {
            resultingCustomerId
            job { id done }
            userErrors { field message code }
          }
        }`,
        variables: {
          one: customerOneId,
          two: customerTwoId,
          override: {
            customerIdOfEmailToKeep: customerTwoId,
            customerIdOfFirstNameToKeep: customerOneId,
            customerIdOfLastNameToKeep: customerTwoId,
            note: 'merged note',
            tags: ['merged'],
          },
        },
      });

    expect(mergeResponse.status).toBe(200);
    expect(mergeResponse.body.data.customerMerge).toMatchObject({
      resultingCustomerId: customerTwoId,
      job: { done: false },
      userErrors: [],
    });
    expect(mergeResponse.body.data.customerMerge.job.id).toMatch(/^gid:\/\/shopify\/Job\//);
    const jobId = mergeResponse.body.data.customerMerge.job.id;

    const downstreamResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerMergeDownstream($one: ID!, $two: ID!, $jobId: ID!) {
          source: customer(id: $one) { id email }
          result: customer(id: $two) { id firstName lastName displayName email note tags defaultEmailAddress { emailAddress } }
          oldEmail: customerByIdentifier(identifier: { emailAddress: "merge-one@example.com" }) { id email }
          newEmail: customerByIdentifier(identifier: { emailAddress: "merge-two@example.com" }) { id email }
          catalog: customers(first: 10) { nodes { id email note tags } }
          counts: customersCount { count precision }
          mergeStatus: customerMergeJobStatus(jobId: $jobId) {
            jobId
            resultingCustomerId
            status
            customerMergeErrors { errorFields message }
          }
        }`,
        variables: { one: customerOneId, two: customerTwoId, jobId },
      });

    expect(downstreamResponse.status).toBe(200);
    expect(downstreamResponse.body).toEqual({
      data: {
        source: null,
        result: {
          id: customerTwoId,
          firstName: 'Merge',
          lastName: 'Two',
          displayName: 'Merge Two',
          email: 'merge-two@example.com',
          note: 'merged note',
          tags: ['merged'],
          defaultEmailAddress: { emailAddress: 'merge-two@example.com' },
        },
        oldEmail: null,
        newEmail: {
          id: customerTwoId,
          email: 'merge-two@example.com',
        },
        catalog: {
          nodes: [
            {
              id: customerTwoId,
              email: 'merge-two@example.com',
              note: 'merged note',
              tags: ['merged'],
            },
          ],
        },
        counts: {
          count: 1,
          precision: 'EXACT',
        },
        mergeStatus: {
          jobId,
          resultingCustomerId: customerTwoId,
          status: 'COMPLETED',
          customerMergeErrors: [],
        },
      },
    });

    const metaState = await request(app).get('/__meta/state');
    expect(metaState.body.stagedState.deletedCustomerIds).toEqual({ [customerOneId]: true });
    expect(metaState.body.stagedState.mergedCustomerIds).toEqual({ [customerOneId]: customerTwoId });
    expect(metaState.body.stagedState.customerMergeRequests[jobId]).toMatchObject({
      jobId,
      resultingCustomerId: customerTwoId,
      status: 'COMPLETED',
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'CustomerCreate',
      'CustomerCreate',
      'CustomerMerge',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns fixture-backed customerMerge validation errors locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customerMerge validation should not hit upstream fetch');
    });
    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/901',
        firstName: 'Validation',
        lastName: 'Customer',
        displayName: 'Validation Customer',
        email: 'merge-validation@example.com',
        legacyResourceId: '901',
        locale: 'en',
        note: null,
        canDelete: true,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: [],
        numberOfOrders: 0,
        amountSpent: null,
        defaultEmailAddress: { emailAddress: 'merge-validation@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-01T00:00:00.000Z',
      },
    ]);

    const app = createApp(snapshotConfig).callback();
    const selfMergeResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation SelfMerge($id: ID!) {
          customerMerge(customerOneId: $id, customerTwoId: $id) {
            resultingCustomerId
            job { id done }
            userErrors { field message code }
          }
        }`,
        variables: { id: 'gid://shopify/Customer/901' },
      });
    expect(selfMergeResponse.body.data.customerMerge).toEqual({
      resultingCustomerId: null,
      job: null,
      userErrors: [
        {
          field: null,
          message: 'Customers IDs should not match',
          code: 'INVALID_CUSTOMER_ID',
        },
      ],
    });

    const unknownMergeResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UnknownMerge($one: ID!, $two: ID!) {
          customerMerge(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            job { id done }
            userErrors { field message code }
          }
        }`,
        variables: {
          one: 'gid://shopify/Customer/901',
          two: 'gid://shopify/Customer/999999999999999',
        },
      });
    expect(unknownMergeResponse.body.data.customerMerge.userErrors).toEqual([
      {
        field: ['customerTwoId'],
        message: 'Customer does not exist with ID 999999999999999',
        code: 'INVALID_CUSTOMER_ID',
      },
    ]);

    const missingArgumentResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation MissingMerge($one: ID!) {
          customerMerge(customerOneId: $one) {
            resultingCustomerId
            userErrors { field message code }
          }
        }`,
        variables: { one: 'gid://shopify/Customer/901' },
      });
    expect(missingArgumentResponse.body.errors).toEqual([
      {
        message: "Field 'customerMerge' is missing required arguments: customerTwoId",
        path: ['customerMerge'],
        extensions: {
          code: 'missingRequiredArguments',
          className: 'Field',
          name: 'customerMerge',
          arguments: 'customerTwoId',
        },
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('overlays staged customerByIdentifier lookups in live-hybrid mode when Shopify has no match yet', async () => {
    const app = createApp({ ...snapshotConfig, readMode: 'live-hybrid' }).callback();
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { customerByIdentifier: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email defaultPhoneNumber { phoneNumber } }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'live-hybrid-draft@example.com',
            firstName: 'Live',
            lastName: 'Hybrid',
            phone: '+14155550999',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    const customerId = createResponse.body.data.customerCreate.customer.id;

    const identifierResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query CustomerByIdentifier($identifier: CustomerIdentifierInput!) {
          customerByIdentifier(identifier: $identifier) {
            id
            email
            defaultPhoneNumber { phoneNumber }
          }
        }`,
        variables: { identifier: { emailAddress: 'live-hybrid-draft@example.com' } },
      });

    expect(identifierResponse.status).toBe(200);
    expect(identifierResponse.body).toEqual({
      data: {
        customerByIdentifier: {
          id: customerId,
          email: 'live-hybrid-draft@example.com',
          defaultPhoneNumber: { phoneNumber: '+14155550999' },
        },
      },
    });
    expect(fetchSpy).toHaveBeenCalledTimes(1);
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
            email: 'ada-updated@example.com',
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
      email: 'ada-updated@example.com',
      note: 'after update',
      locale: 'en',
      taxExempt: true,
      tags: ['newsletter', 'vip'],
      defaultEmailAddress: { emailAddress: 'ada-updated@example.com' },
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
          byIdentifier: customerByIdentifier(identifier: { emailAddress: "ada-updated@example.com" }) {
            id
            displayName
            email
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
        byIdentifier: {
          id: 'gid://shopify/Customer/401',
          displayName: 'Ada Byron',
          email: 'ada-updated@example.com',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages customerSet create, update, and identifier upsert slices locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customerSet should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetCreate($input: CustomerSetInput!) {
          customerSet(input: $input) {
            customer {
              id
              firstName
              lastName
              displayName
              email
              note
              taxExempt
              taxExemptions
              tags
              defaultEmailAddress { emailAddress }
              defaultPhoneNumber { phoneNumber }
              defaultAddress { address1 }
              addressesV2(first: 5) {
                nodes { id address1 }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
              createdAt
              updatedAt
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'customer-set-create@example.com',
            firstName: 'Set',
            lastName: 'Create',
            note: 'created by customerSet',
            phone: '+14155550123',
            tags: ['set', 'create'],
            taxExempt: true,
            taxExemptions: ['CA_BC_RESELLER_EXEMPTION'],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.customerSet.userErrors).toEqual([]);
    expect(createResponse.body.data.customerSet.customer).toMatchObject({
      firstName: 'Set',
      lastName: 'Create',
      displayName: 'Set Create',
      email: 'customer-set-create@example.com',
      note: 'created by customerSet',
      taxExempt: true,
      taxExemptions: ['CA_BC_RESELLER_EXEMPTION'],
      tags: ['create', 'set'],
      defaultEmailAddress: { emailAddress: 'customer-set-create@example.com' },
      defaultPhoneNumber: { phoneNumber: '+14155550123' },
      defaultAddress: null,
      addressesV2: {
        nodes: [],
        pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
      },
    });
    const createdCustomerId = createResponse.body.data.customerSet.customer.id;

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetUpdate($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer {
              id
              displayName
              email
              note
              taxExempt
              taxExemptions
              tags
              defaultAddress { address1 city province country zip formattedArea }
              addressesV2(first: 5) {
                nodes { id address1 city province country zip formattedArea }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
              updatedAt
            }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { id: createdCustomerId },
          input: {
            email: 'customer-set-create@example.com',
            firstName: 'Set',
            lastName: 'Updated',
            note: 'updated by customerSet',
            tags: ['set', 'updated'],
            taxExempt: false,
            taxExemptions: [],
            addresses: [
              { address1: '10 Set St', city: 'Ottawa', countryCode: 'CA', provinceCode: 'ON', zip: 'K1A 0B1' },
            ],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.customerSet.userErrors).toEqual([]);
    expect(updateResponse.body.data.customerSet.customer).toMatchObject({
      id: createdCustomerId,
      displayName: 'Set Updated',
      email: 'customer-set-create@example.com',
      note: 'updated by customerSet',
      taxExempt: false,
      taxExemptions: [],
      tags: ['set', 'updated'],
      defaultAddress: {
        address1: '10 Set St',
        city: 'Ottawa',
        province: 'Ontario',
        country: 'Canada',
        zip: 'K1A 0B1',
        formattedArea: 'Ottawa ON, Canada',
      },
    });
    expect(updateResponse.body.data.customerSet.customer.addressesV2.nodes).toHaveLength(1);
    expect(updateResponse.body.data.customerSet.customer.addressesV2.nodes[0]).toMatchObject({
      address1: '10 Set St',
      city: 'Ottawa',
      province: 'Ontario',
      country: 'Canada',
      zip: 'K1A 0B1',
      formattedArea: 'Ottawa ON, Canada',
    });

    const upsertResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetUpsert($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer { id email displayName tags }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { email: 'customer-set-upsert@example.com' },
          input: {
            email: 'customer-set-upsert@example.com',
            firstName: 'Set',
            lastName: 'Upsert',
            tags: ['set', 'upsert'],
          },
        },
      });

    expect(upsertResponse.status).toBe(200);
    expect(upsertResponse.body.data.customerSet.userErrors).toEqual([]);
    expect(upsertResponse.body.data.customerSet.customer).toMatchObject({
      email: 'customer-set-upsert@example.com',
      displayName: 'Set Upsert',
      tags: ['set', 'upsert'],
    });
    const upsertedCustomerId = upsertResponse.body.data.customerSet.customer.id;

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query CustomerSetReadback($id: ID!, $upsertId: ID!) {
          detail: customer(id: $id) {
            id
            displayName
            defaultAddress { address1 }
            addressesV2(first: 5) { nodes { address1 } }
          }
          byIdentifier: customerByIdentifier(identifier: { emailAddress: "customer-set-upsert@example.com" }) {
            id
            email
          }
          catalog: customers(first: 10, query: "tag:set") {
            nodes { id email tags }
          }
          counts: customersCount { count precision }
          upsertDetail: customer(id: $upsertId) { id email displayName }
        }`,
        variables: { id: createdCustomerId, upsertId: upsertedCustomerId },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.detail).toEqual({
      id: createdCustomerId,
      displayName: 'Set Updated',
      defaultAddress: { address1: '10 Set St' },
      addressesV2: { nodes: [{ address1: '10 Set St' }] },
    });
    expect(readResponse.body.data.byIdentifier).toEqual({
      id: upsertedCustomerId,
      email: 'customer-set-upsert@example.com',
    });
    expect(readResponse.body.data.catalog.nodes).toEqual([
      { id: upsertedCustomerId, email: 'customer-set-upsert@example.com', tags: ['set', 'upsert'] },
      { id: createdCustomerId, email: 'customer-set-create@example.com', tags: ['set', 'updated'] },
    ]);
    expect(readResponse.body.data.counts).toEqual({ count: 2, precision: 'EXACT' });
    expect(readResponse.body.data.upsertDetail).toEqual({
      id: upsertedCustomerId,
      email: 'customer-set-upsert@example.com',
      displayName: 'Set Upsert',
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'customerSet',
      'customerSet',
      'customerSet',
    ]);
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
    ]);
    expect(logResponse.body.entries[0].requestBody.query).toContain('mutation CustomerSetCreate');
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('rejects unsupported customerSet fields locally without proxying upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('unsupported customerSet fields should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const unsupportedInputResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetUnsupported($input: CustomerSetInput!) {
          customerSet(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'customer-set-unsupported@example.com',
            metafields: [{ namespace: 'custom', key: 'loyalty', type: 'single_line_text_field', value: 'gold' }],
          },
        },
      });

    expect(unsupportedInputResponse.status).toBe(200);
    expect(unsupportedInputResponse.body.data.customerSet).toEqual({
      customer: null,
      userErrors: [
        {
          field: ['input', 'metafields'],
          message: "customerSet input field 'metafields' is not supported by local staging yet",
        },
      ],
    });

    const customIdResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetCustomId($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { customId: { namespace: 'custom', key: 'external_id', value: 'unsupported' } },
          input: { firstName: 'Custom' },
        },
      });

    expect(customIdResponse.status).toBe(200);
    expect(customIdResponse.body).toEqual({
      data: { customerSet: null },
      errors: [
        {
          message: "Metafield definition of type 'id' is required when using custom ids.",
          path: ['customerSet'],
          extensions: { code: 'NOT_FOUND' },
        },
      ],
    });

    const unknownIdResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetUnknown($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { id: 'gid://shopify/Customer/999999999999999' },
          input: { firstName: 'Ghost' },
        },
      });

    expect(unknownIdResponse.status).toBe(200);
    expect(unknownIdResponse.body.data.customerSet).toEqual({
      customer: null,
      userErrors: [{ field: ['input'], message: 'Resource matching the identifier was not found.' }],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages customer tax exemptions and metafields on customerUpdate and exposes them on downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customerUpdate tax/metafield staging should not hit upstream fetch');
    });

    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/154',
        firstName: 'Tax',
        lastName: 'Customer',
        displayName: 'Tax Customer',
        email: 'tax-customer@example.com',
        legacyResourceId: '154',
        locale: 'en',
        note: null,
        canDelete: true,
        verifiedEmail: true,
        taxExempt: false,
        taxExemptions: [],
        state: 'DISABLED',
        tags: ['baseline'],
        numberOfOrders: 0,
        amountSpent: null,
        defaultEmailAddress: { emailAddress: 'tax-customer@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-01T00:00:00.000Z',
      },
    ]);

    const app = createApp(snapshotConfig).callback();
    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerTaxAndMetafields($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer {
              id
              taxExempt
              taxExemptions
              loyalty: metafield(namespace: "custom", key: "loyalty") {
                id
                namespace
                key
                type
                value
              }
              metafields(first: 5) {
                nodes {
                  id
                  namespace
                  key
                  type
                  value
                }
                pageInfo {
                  hasNextPage
                  hasPreviousPage
                  startCursor
                  endCursor
                }
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            id: 'gid://shopify/Customer/154',
            taxExempt: true,
            taxExemptions: ['CA_BC_RESELLER_EXEMPTION'],
            metafields: [
              {
                namespace: 'custom',
                key: 'loyalty',
                type: 'single_line_text_field',
                value: 'gold',
              },
            ],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.customerUpdate.userErrors).toEqual([]);
    expect(updateResponse.body.data.customerUpdate.customer).toMatchObject({
      id: 'gid://shopify/Customer/154',
      taxExempt: true,
      taxExemptions: ['CA_BC_RESELLER_EXEMPTION'],
      loyalty: {
        namespace: 'custom',
        key: 'loyalty',
        type: 'single_line_text_field',
        value: 'gold',
      },
      metafields: {
        nodes: [
          {
            namespace: 'custom',
            key: 'loyalty',
            type: 'single_line_text_field',
            value: 'gold',
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
        },
      },
    });
    expect(updateResponse.body.data.customerUpdate.customer.loyalty.id).toMatch(/^gid:\/\/shopify\/Metafield\//);
    expect(updateResponse.body.data.customerUpdate.customer.metafields.nodes[0].id).toBe(
      updateResponse.body.data.customerUpdate.customer.loyalty.id,
    );
    expect(updateResponse.body.data.customerUpdate.customer.metafields.pageInfo.startCursor).toBe(
      `cursor:${updateResponse.body.data.customerUpdate.customer.loyalty.id}`,
    );
    expect(updateResponse.body.data.customerUpdate.customer.metafields.pageInfo.endCursor).toBe(
      `cursor:${updateResponse.body.data.customerUpdate.customer.loyalty.id}`,
    );

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query CustomerTaxMetafieldRead($id: ID!) {
          customer(id: $id) {
            id
            taxExempt
            taxExemptions
            loyalty: metafield(namespace: "custom", key: "loyalty") {
              id
              namespace
              key
              type
              value
            }
          }
          customers(first: 5) {
            nodes {
              id
              taxExemptions
              metafields(first: 5) {
                nodes { id namespace key type value }
              }
            }
          }
        }`,
        variables: { id: 'gid://shopify/Customer/154' },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.customer).toEqual({
      id: 'gid://shopify/Customer/154',
      taxExempt: true,
      taxExemptions: ['CA_BC_RESELLER_EXEMPTION'],
      loyalty: updateResponse.body.data.customerUpdate.customer.loyalty,
    });
    expect(readResponse.body.data.customers.nodes).toEqual([
      {
        id: 'gid://shopify/Customer/154',
        taxExemptions: ['CA_BC_RESELLER_EXEMPTION'],
        metafields: {
          nodes: [updateResponse.body.data.customerUpdate.customer.loyalty],
        },
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns userErrors for invalid customer tax exemption and metafield update inputs', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid customerUpdate tax/metafield inputs should not hit upstream fetch');
    });

    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/155',
        firstName: 'Invalid',
        lastName: 'Input',
        displayName: 'Invalid Input',
        email: 'invalid-input@example.com',
        legacyResourceId: '155',
        locale: 'en',
        note: null,
        canDelete: true,
        verifiedEmail: true,
        taxExempt: false,
        taxExemptions: [],
        state: 'DISABLED',
        tags: [],
        numberOfOrders: 0,
        amountSpent: null,
        defaultEmailAddress: { emailAddress: 'invalid-input@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-01T00:00:00.000Z',
      },
    ]);

    const app = createApp(snapshotConfig).callback();
    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation InvalidCustomerTaxMetafields($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id taxExemptions metafields(first: 5) { nodes { id } } }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            id: 'gid://shopify/Customer/155',
            taxExemptions: ['NOT_A_TAX_EXEMPTION'],
            metafields: [{ namespace: 'custom', key: 'bad_type', type: 'not_a_type', value: 'bad' }],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body).toEqual({
      data: {
        customerUpdate: {
          customer: null,
          userErrors: [
            {
              field: ['taxExemptions', '0'],
              message: 'Tax exemption is not a valid value',
            },
            {
              field: ['metafields', '0', 'type'],
              message: expect.stringContaining('Type must be one of the following:'),
            },
          ],
        },
      },
    });
    expect(store.getEffectiveCustomerById('gid://shopify/Customer/155')?.taxExemptions).toEqual([]);
    expect(store.getEffectiveMetafieldsByCustomerId('gid://shopify/Customer/155')).toEqual([]);
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

  it('stages customer marketing consent updates locally and exposes them on downstream customer reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customer consent updates should not hit upstream fetch');
    });

    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/403',
        firstName: 'Katherine',
        lastName: 'Johnson',
        displayName: 'Katherine Johnson',
        email: 'katherine@example.com',
        legacyResourceId: '403',
        locale: 'en',
        note: null,
        canDelete: true,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['newsletter'],
        numberOfOrders: '2',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'katherine@example.com',
          marketingState: 'NOT_SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          marketingUpdatedAt: null,
        },
        defaultPhoneNumber: {
          phoneNumber: '+14155550124',
          marketingState: 'NOT_SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          marketingUpdatedAt: null,
          marketingCollectedFrom: null,
        },
        emailMarketingConsent: {
          marketingState: 'NOT_SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: null,
        },
        smsMarketingConsent: {
          marketingState: 'NOT_SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: null,
          consentCollectedFrom: null,
        },
        defaultAddress: null,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
      },
    ]);

    const app = createApp(snapshotConfig).callback();
    const emailResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation EmailConsent($input: CustomerEmailMarketingConsentUpdateInput!) {
          customerEmailMarketingConsentUpdate(input: $input) {
            customer {
              id
              defaultEmailAddress {
                emailAddress
                marketingState
                marketingOptInLevel
                marketingUpdatedAt
              }
              emailMarketingConsent {
                marketingState
                marketingOptInLevel
                consentUpdatedAt
              }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/403',
            emailMarketingConsent: {
              marketingState: 'SUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
              consentUpdatedAt: '2026-04-25T01:00:00Z',
            },
          },
        },
      });

    expect(emailResponse.status).toBe(200);
    expect(emailResponse.body.data.customerEmailMarketingConsentUpdate).toEqual({
      customer: {
        id: 'gid://shopify/Customer/403',
        defaultEmailAddress: {
          emailAddress: 'katherine@example.com',
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          marketingUpdatedAt: '2026-04-25T01:00:00Z',
        },
        emailMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T01:00:00Z',
        },
      },
      userErrors: [],
    });

    const smsResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation SmsConsent($input: CustomerSmsMarketingConsentUpdateInput!) {
          customerSmsMarketingConsentUpdate(input: $input) {
            customer {
              id
              defaultPhoneNumber {
                phoneNumber
                marketingState
                marketingOptInLevel
                marketingUpdatedAt
                marketingCollectedFrom
              }
              smsMarketingConsent {
                marketingState
                marketingOptInLevel
                consentUpdatedAt
                consentCollectedFrom
              }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/403',
            smsMarketingConsent: {
              marketingState: 'SUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
              consentUpdatedAt: '2026-04-25T01:05:00Z',
            },
          },
        },
      });

    expect(smsResponse.status).toBe(200);
    expect(smsResponse.body.data.customerSmsMarketingConsentUpdate).toEqual({
      customer: {
        id: 'gid://shopify/Customer/403',
        defaultPhoneNumber: {
          phoneNumber: '+14155550124',
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          marketingUpdatedAt: '2026-04-25T01:05:00Z',
          marketingCollectedFrom: 'OTHER',
        },
        smsMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T01:05:00Z',
          consentCollectedFrom: 'OTHER',
        },
      },
      userErrors: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ConsentReadback($id: ID!, $identifier: CustomerIdentifierInput!) {
          customer(id: $id) {
            id
            defaultEmailAddress { marketingState marketingOptInLevel marketingUpdatedAt }
            defaultPhoneNumber { marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom }
          }
          customerByIdentifier(identifier: $identifier) {
            id
            emailMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt }
            smsMarketingConsent { marketingState marketingOptInLevel consentUpdatedAt consentCollectedFrom }
          }
          customers(first: 5, query: "email:katherine@example.com") {
            nodes {
              id
              defaultEmailAddress { marketingState marketingUpdatedAt }
              defaultPhoneNumber { marketingState marketingUpdatedAt marketingCollectedFrom }
            }
          }
        }`,
        variables: {
          id: 'gid://shopify/Customer/403',
          identifier: { emailAddress: 'katherine@example.com' },
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        customer: {
          id: 'gid://shopify/Customer/403',
          defaultEmailAddress: {
            marketingState: 'SUBSCRIBED',
            marketingOptInLevel: 'SINGLE_OPT_IN',
            marketingUpdatedAt: '2026-04-25T01:00:00Z',
          },
          defaultPhoneNumber: {
            marketingState: 'SUBSCRIBED',
            marketingOptInLevel: 'SINGLE_OPT_IN',
            marketingUpdatedAt: '2026-04-25T01:05:00Z',
            marketingCollectedFrom: 'OTHER',
          },
        },
        customerByIdentifier: {
          id: 'gid://shopify/Customer/403',
          emailMarketingConsent: {
            marketingState: 'SUBSCRIBED',
            marketingOptInLevel: 'SINGLE_OPT_IN',
            consentUpdatedAt: '2026-04-25T01:00:00Z',
          },
          smsMarketingConsent: {
            marketingState: 'SUBSCRIBED',
            marketingOptInLevel: 'SINGLE_OPT_IN',
            consentUpdatedAt: '2026-04-25T01:05:00Z',
            consentCollectedFrom: 'OTHER',
          },
        },
        customers: {
          nodes: [
            {
              id: 'gid://shopify/Customer/403',
              defaultEmailAddress: {
                marketingState: 'SUBSCRIBED',
                marketingUpdatedAt: '2026-04-25T01:00:00Z',
              },
              defaultPhoneNumber: {
                marketingState: 'SUBSCRIBED',
                marketingUpdatedAt: '2026-04-25T01:05:00Z',
                marketingCollectedFrom: 'OTHER',
              },
            },
          ],
        },
      },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.status).toBe(200);
    expect(logResponse.body.entries).toMatchObject([
      {
        operationName: 'customerEmailMarketingConsentUpdate',
        interpreted: {
          operationName: 'EmailConsent',
          primaryRootField: 'customerEmailMarketingConsentUpdate',
        },
        requestBody: {
          variables: {
            input: {
              customerId: 'gid://shopify/Customer/403',
            },
          },
        },
        status: 'staged',
      },
      {
        operationName: 'customerSmsMarketingConsentUpdate',
        interpreted: {
          operationName: 'SmsConsent',
          primaryRootField: 'customerSmsMarketingConsentUpdate',
        },
        requestBody: {
          variables: {
            input: {
              customerId: 'gid://shopify/Customer/403',
            },
          },
        },
        status: 'staged',
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('buffers customer account activation and outbound email roots locally for commit replay', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customer outbound side effects should be buffered locally');
    });

    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/404',
        firstName: 'Dorothy',
        lastName: 'Vaughan',
        displayName: 'Dorothy Vaughan',
        email: 'dorothy@example.com',
        legacyResourceId: '404',
        locale: 'en',
        note: null,
        canDelete: true,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['invite'],
        numberOfOrders: '0',
        amountSpent: null,
        defaultEmailAddress: { emailAddress: 'dorothy@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
      },
    ]);
    store.upsertBaseCustomerPaymentMethods([
      {
        id: 'gid://shopify/CustomerPaymentMethod/local-payment-method',
        customerId: 'gid://shopify/Customer/404',
      },
    ]);

    const app = createApp(snapshotConfig).callback();
    const activationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Activation($customerId: ID!) {
          customerGenerateAccountActivationUrl(customerId: $customerId) {
            accountActivationUrl
            userErrors { field message }
          }
        }`,
        variables: { customerId: 'gid://shopify/Customer/404' },
      });

    expect(activationResponse.status).toBe(200);
    expect(activationResponse.body.data.customerGenerateAccountActivationUrl.userErrors).toEqual([]);
    expect(activationResponse.body.data.customerGenerateAccountActivationUrl.accountActivationUrl).toMatch(
      /^https:\/\/shopify-draft-proxy\.local\/customer-activation\/404\?token=/,
    );

    const inviteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation Invite($customerId: ID!, $email: EmailInput) {
          customerSendAccountInviteEmail(customerId: $customerId, email: $email) {
            customer { id state }
            userErrors { field message }
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/404',
          email: {
            subject: 'Activate your account',
            customMessage: 'Welcome',
          },
        },
      });

    expect(inviteResponse.status).toBe(200);
    expect(inviteResponse.body).toEqual({
      data: {
        customerSendAccountInviteEmail: {
          customer: {
            id: 'gid://shopify/Customer/404',
            state: 'INVITED',
          },
          userErrors: [],
        },
      },
    });

    const paymentEmailResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation PaymentEmail($customerPaymentMethodId: ID!) {
          customerPaymentMethodSendUpdateEmail(customerPaymentMethodId: $customerPaymentMethodId) {
            customer { id state }
            userErrors { field message }
          }
        }`,
        variables: {
          customerPaymentMethodId: 'gid://shopify/CustomerPaymentMethod/local-payment-method',
        },
      });

    expect(paymentEmailResponse.status).toBe(200);
    expect(paymentEmailResponse.body).toEqual({
      data: {
        customerPaymentMethodSendUpdateEmail: {
          customer: {
            id: 'gid://shopify/Customer/404',
            state: 'INVITED',
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query CustomerInviteReadback($id: ID!) {
          customer(id: $id) { id state }
        }`,
        variables: { id: 'gid://shopify/Customer/404' },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        customer: {
          id: 'gid://shopify/Customer/404',
          state: 'INVITED',
        },
      },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.status).toBe(200);
    expect(logResponse.body.entries).toMatchObject([
      {
        operationName: 'customerGenerateAccountActivationUrl',
        interpreted: {
          operationName: 'Activation',
          primaryRootField: 'customerGenerateAccountActivationUrl',
        },
        requestBody: {
          variables: { customerId: 'gid://shopify/Customer/404' },
        },
        status: 'staged',
      },
      {
        operationName: 'customerSendAccountInviteEmail',
        interpreted: {
          operationName: 'Invite',
          primaryRootField: 'customerSendAccountInviteEmail',
        },
        requestBody: {
          variables: {
            customerId: 'gid://shopify/Customer/404',
            email: {
              subject: 'Activate your account',
            },
          },
        },
        status: 'staged',
      },
      {
        operationName: 'customerPaymentMethodSendUpdateEmail',
        interpreted: {
          operationName: 'PaymentEmail',
          primaryRootField: 'customerPaymentMethodSendUpdateEmail',
        },
        requestBody: {
          variables: {
            customerPaymentMethodId: 'gid://shopify/CustomerPaymentMethod/local-payment-method',
          },
        },
        status: 'staged',
      },
    ]);
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

    const emailConsentResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerEmailConsentValidation($input: CustomerEmailMarketingConsentUpdateInput!) {
          customerEmailMarketingConsentUpdate(input: $input) {
            customer { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/999999999999999',
            emailMarketingConsent: {
              marketingState: 'SUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
            },
          },
        },
      });

    expect(emailConsentResponse.status).toBe(200);
    expect(emailConsentResponse.body).toEqual({
      data: {
        customerEmailMarketingConsentUpdate: {
          customer: null,
          userErrors: [
            {
              field: ['input', 'customerId'],
              message: 'Customer not found',
              code: 'INVALID',
            },
          ],
        },
      },
    });

    const smsConsentResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSmsConsentValidation($input: CustomerSmsMarketingConsentUpdateInput!) {
          customerSmsMarketingConsentUpdate(input: $input) {
            customer { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/999999999999999',
            smsMarketingConsent: {
              marketingState: 'SUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
            },
          },
        },
      });

    expect(smsConsentResponse.status).toBe(200);
    expect(smsConsentResponse.body).toEqual({
      data: {
        customerSmsMarketingConsentUpdate: {
          customer: null,
          userErrors: [
            {
              field: null,
              message: 'Customer not found',
              code: null,
            },
          ],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
