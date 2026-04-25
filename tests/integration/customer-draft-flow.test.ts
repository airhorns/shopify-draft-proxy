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
    expect(readResponse.body).toEqual({
      data: {
        detail: {
          id: customerId,
          displayName: 'Draft Customer',
          email: 'draft-customer@example.com',
          note: 'created locally',
          defaultPhoneNumber: { phoneNumber: '+14155550123' },
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
