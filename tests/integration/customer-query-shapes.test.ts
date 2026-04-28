import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

describe('customer query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns null for direct customer lookups in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { customer: { id: 'gid://shopify/Customer/1' } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query ($id: ID!) { customer(id: $id) { id email firstName lastName } }',
        variables: { id: 'gid://shopify/Customer/999' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        customer: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns a selection-aware empty customers connection in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { customers: { nodes: [{ id: 'gid://shopify/Customer/1' }] } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomersSnapshot($id: ID!) {
          lookup: customer(id: $id) { id email }
          customerCatalog: customers(first: 5) {
            edges { cursor node { id email } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: { id: 'gid://shopify/Customer/999' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        lookup: null,
        customerCatalog: {
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('serves customer account page reads from normalized snapshot state', async () => {
    store.upsertBaseCustomerAccountPages([
      {
        id: 'gid://shopify/CustomerAccountPage/221353705778',
        title: 'Orders',
        handle: 'orders',
        defaultCursor: 'orders-default-cursor',
        cursor: 'orders-edge-cursor',
      },
      {
        id: 'gid://shopify/CustomerAccountPage/221353738546',
        title: 'Profile',
        handle: 'profile',
        defaultCursor: 'profile-default-cursor',
        cursor: 'profile-edge-cursor',
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customer account page snapshot reads should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerAccountPages($id: ID!, $missingId: ID!) {
          page: customerAccountPage(id: $id) {
            __typename
            id
            title
            handle
            defaultCursor
          }
          missing: customerAccountPage(id: $missingId) { id title }
          pages: customerAccountPages(first: 1) {
            nodes { id title handle defaultCursor }
            edges { cursor node { id title } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: {
          id: 'gid://shopify/CustomerAccountPage/221353705778',
          missingId: 'gid://shopify/CustomerAccountPage/999999999999999',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        page: {
          __typename: 'CustomerAccountPage',
          id: 'gid://shopify/CustomerAccountPage/221353705778',
          title: 'Orders',
          handle: 'orders',
          defaultCursor: 'orders-default-cursor',
        },
        missing: null,
        pages: {
          nodes: [
            {
              id: 'gid://shopify/CustomerAccountPage/221353705778',
              title: 'Orders',
              handle: 'orders',
              defaultCursor: 'orders-default-cursor',
            },
          ],
          edges: [
            {
              cursor: 'orders-edge-cursor',
              node: {
                id: 'gid://shopify/CustomerAccountPage/221353705778',
                title: 'Orders',
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: 'orders-edge-cursor',
            endCursor: 'orders-edge-cursor',
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns Shopify-like empty customer account page shapes when snapshot state is absent', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('empty customer account page snapshot reads should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query EmptyCustomerAccountPages($id: ID!) {
          page: customerAccountPage(id: $id) { id title }
          pages: customerAccountPages(first: 5) {
            nodes { id }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: { id: 'gid://shopify/CustomerAccountPage/999999999999999' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        page: null,
        pages: {
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
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns Shopify-like empty directly related customer sub-resource shapes in snapshot mode', async () => {
    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/777',
        firstName: 'Nested',
        lastName: 'Probe',
        displayName: 'Nested Probe',
        email: 'nested@example.com',
        legacyResourceId: '777',
        locale: 'en',
        note: null,
        canDelete: true,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: [],
        numberOfOrders: 0,
        amountSpent: { amount: '0.0', currencyCode: 'USD' },
        defaultEmailAddress: { emailAddress: 'nested@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-01T00:00:00.000Z',
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('snapshot customer sub-resource reads should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerNestedEmpty($id: ID!) {
          customer(id: $id) {
            id
            addresses { address1 city }
            addressesV2(first: 2) {
              nodes { address1 city }
              edges { cursor node { address1 city } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            companyContactProfiles { id }
            orders(first: 2) {
              nodes { id }
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            events(first: 2) {
              nodes { id }
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            metafield(namespace: "custom", key: "tier") { id namespace key }
            metafields(first: 2) {
              nodes { id namespace key }
              edges { cursor node { id namespace key } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            storeCreditAccounts(first: 2) {
              nodes { id }
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            paymentMethods(first: 2) {
              nodes { id }
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            subscriptionContracts(first: 2) {
              nodes { id }
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            lastOrder { id }
          }
        }`,
        variables: { id: 'gid://shopify/Customer/777' },
      });

    const emptyConnection = {
      nodes: [],
      edges: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    };
    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        customer: {
          id: 'gid://shopify/Customer/777',
          addresses: [],
          addressesV2: emptyConnection,
          companyContactProfiles: [],
          orders: emptyConnection,
          events: emptyConnection,
          metafield: null,
          metafields: emptyConnection,
          storeCreditAccounts: emptyConnection,
          paymentMethods: emptyConnection,
          subscriptionContracts: emptyConnection,
          lastOrder: null,
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('serializes seeded customer payment method roots and customer-owned connections with revoked filtering', async () => {
    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/8801',
        firstName: 'Payment',
        lastName: 'Holder',
        displayName: 'Payment Holder',
        email: 'payment-holder@example.com',
        legacyResourceId: '8801',
        locale: 'en',
        note: null,
        canDelete: true,
        verifiedEmail: true,
        taxExempt: false,
        state: 'ENABLED',
        tags: [],
        numberOfOrders: 0,
        amountSpent: { amount: '0.0', currencyCode: 'USD' },
        defaultEmailAddress: { emailAddress: 'payment-holder@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-01T00:00:00.000Z',
      },
    ]);
    store.upsertBaseCustomerPaymentMethods([
      {
        id: 'gid://shopify/CustomerPaymentMethod/active-card',
        customerId: 'gid://shopify/Customer/8801',
        cursor: 'active-card-cursor',
        revokedAt: null,
        revokedReason: null,
        instrument: {
          typeName: 'CustomerCreditCard',
          data: {
            __typename: 'CustomerCreditCard',
            brand: 'VISA',
            lastDigits: '4242',
            expiryMonth: 12,
            expiryYear: 2030,
            name: 'Payment Holder',
            maskedNumber: '**** **** **** 4242',
          },
        },
        subscriptionContracts: [
          {
            id: 'gid://shopify/SubscriptionContract/contract-1',
            cursor: 'contract-cursor',
            data: {
              __typename: 'SubscriptionContract',
              status: 'ACTIVE',
            },
          },
        ],
      },
      {
        id: 'gid://shopify/CustomerPaymentMethod/revoked-paypal',
        customerId: 'gid://shopify/Customer/8801',
        cursor: 'revoked-paypal-cursor',
        revokedAt: '2024-02-02T00:00:00.000Z',
        revokedReason: 'CUSTOMER_REVOKED',
        instrument: {
          typeName: 'CustomerPaypalBillingAgreement',
          data: {
            __typename: 'CustomerPaypalBillingAgreement',
            paypalAccountEmail: 'paypal@example.com',
            inactive: true,
          },
        },
        subscriptionContracts: [],
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('seeded customer payment method snapshot reads should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query PaymentMethodReads($customerId: ID!, $activeId: ID!, $revokedId: ID!) {
          active: customerPaymentMethod(id: $activeId) {
            id
            revokedAt
            instrument {
              __typename
              ... on CustomerCreditCard {
                brand
                lastDigits
                expiryMonth
                expiryYear
                name
                maskedNumber
              }
            }
            customer { id email }
            subscriptionContracts(first: 2) {
              nodes { id status }
              edges { cursor node { id status } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          revokedHidden: customerPaymentMethod(id: $revokedId) { id revokedAt }
          revokedShown: customerPaymentMethod(id: $revokedId, showRevoked: true) {
            id
            revokedAt
            revokedReason
            instrument {
              __typename
              ... on CustomerPaypalBillingAgreement {
                paypalAccountEmail
                inactive
              }
            }
          }
          customer(id: $customerId) {
            paymentMethods(first: 5) {
              nodes { id revokedAt }
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            allPaymentMethods: paymentMethods(first: 5, showRevoked: true) {
              nodes { id revokedAt }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/8801',
          activeId: 'gid://shopify/CustomerPaymentMethod/active-card',
          revokedId: 'gid://shopify/CustomerPaymentMethod/revoked-paypal',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        active: {
          id: 'gid://shopify/CustomerPaymentMethod/active-card',
          revokedAt: null,
          instrument: {
            __typename: 'CustomerCreditCard',
            brand: 'VISA',
            lastDigits: '4242',
            expiryMonth: 12,
            expiryYear: 2030,
            name: 'Payment Holder',
            maskedNumber: '**** **** **** 4242',
          },
          customer: {
            id: 'gid://shopify/Customer/8801',
            email: 'payment-holder@example.com',
          },
          subscriptionContracts: {
            nodes: [
              {
                id: 'gid://shopify/SubscriptionContract/contract-1',
                status: 'ACTIVE',
              },
            ],
            edges: [
              {
                cursor: 'cursor:contract-cursor',
                node: {
                  id: 'gid://shopify/SubscriptionContract/contract-1',
                  status: 'ACTIVE',
                },
              },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: 'cursor:contract-cursor',
              endCursor: 'cursor:contract-cursor',
            },
          },
        },
        revokedHidden: null,
        revokedShown: {
          id: 'gid://shopify/CustomerPaymentMethod/revoked-paypal',
          revokedAt: '2024-02-02T00:00:00.000Z',
          revokedReason: 'CUSTOMER_REVOKED',
          instrument: {
            __typename: 'CustomerPaypalBillingAgreement',
            paypalAccountEmail: 'paypal@example.com',
            inactive: true,
          },
        },
        customer: {
          paymentMethods: {
            nodes: [
              {
                id: 'gid://shopify/CustomerPaymentMethod/active-card',
                revokedAt: null,
              },
            ],
            edges: [
              {
                cursor: 'cursor:active-card-cursor',
                node: {
                  id: 'gid://shopify/CustomerPaymentMethod/active-card',
                },
              },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: 'cursor:active-card-cursor',
              endCursor: 'cursor:active-card-cursor',
            },
          },
          allPaymentMethods: {
            nodes: [
              {
                id: 'gid://shopify/CustomerPaymentMethod/active-card',
                revokedAt: null,
              },
              {
                id: 'gid://shopify/CustomerPaymentMethod/revoked-paypal',
                revokedAt: '2024-02-02T00:00:00.000Z',
              },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: 'cursor:active-card-cursor',
              endCursor: 'cursor:revoked-paypal-cursor',
            },
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('logs sensitive customer payment method mutations as unsupported passthrough boundaries', async () => {
    const upstreamBody = {
      data: {
        customerPaymentMethodRevoke: {
          revokedCustomerPaymentMethodId: 'gid://shopify/CustomerPaymentMethod/remote',
          userErrors: [],
        },
      },
    };
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify(upstreamBody), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation RevokeCustomerPaymentMethod($id: ID!) {
          customerPaymentMethodRevoke(customerPaymentMethodId: $id) {
            revokedCustomerPaymentMethodId
            userErrors { field message }
          }
        }`,
        variables: { id: 'gid://shopify/CustomerPaymentMethod/remote' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual(upstreamBody);
    expect(fetchSpy).toHaveBeenCalledTimes(1);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.status).toBe(200);
    expect(logResponse.body.entries).toHaveLength(1);
    expect(logResponse.body.entries[0]).toMatchObject({
      operationName: 'RevokeCustomerPaymentMethod',
      status: 'proxied',
      interpreted: {
        operationType: 'mutation',
        rootFields: ['customerPaymentMethodRevoke'],
        primaryRootField: 'customerPaymentMethodRevoke',
        registeredOperation: {
          name: 'customerPaymentMethodRevoke',
          domain: 'payments',
          execution: 'stage-locally',
          implemented: false,
        },
      },
    });
    expect(logResponse.body.entries[0].interpreted.registeredOperation.supportNotes).toContain(
      'write_customer_payment_methods',
    );
  });

  it('serves customerByIdentifier by id, email, and phone from snapshot state without trusting the operation name', async () => {
    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/301',
        firstName: 'Ada',
        lastName: 'Lovelace',
        displayName: 'Ada Lovelace',
        email: 'ada@example.com',
        legacyResourceId: '301',
        locale: 'en',
        note: null,
        canDelete: true,
        verifiedEmail: true,
        taxExempt: false,
        state: 'ENABLED',
        tags: ['vip'],
        numberOfOrders: 0,
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'ada@example.com',
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          marketingUpdatedAt: '2026-04-25T01:00:00Z',
        },
        defaultPhoneNumber: {
          phoneNumber: '+15550101',
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'CONFIRMED_OPT_IN',
          marketingUpdatedAt: '2026-04-25T01:05:00Z',
          marketingCollectedFrom: 'OTHER',
        },
        emailMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T01:00:00Z',
        },
        smsMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'CONFIRMED_OPT_IN',
          consentUpdatedAt: '2026-04-25T01:05:00Z',
          consentCollectedFrom: 'OTHER',
        },
        defaultAddress: null,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
      },
    ]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customerByIdentifier snapshot lookup should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query Customer(
          $idIdentifier: CustomerIdentifierInput!
          $emailIdentifier: CustomerIdentifierInput!
          $phoneIdentifier: CustomerIdentifierInput!
          $missingIdentifier: CustomerIdentifierInput!
        ) {
          byId: customerByIdentifier(identifier: $idIdentifier) { id email }
          byEmail: customerByIdentifier(identifier: $emailIdentifier) {
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
          byPhone: customerByIdentifier(identifier: $phoneIdentifier) {
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
          missing: customerByIdentifier(identifier: $missingIdentifier) { id }
        }`,
        variables: {
          idIdentifier: { id: 'gid://shopify/Customer/301' },
          emailIdentifier: { emailAddress: 'ADA@example.com' },
          phoneIdentifier: { phoneNumber: '+15550101' },
          missingIdentifier: { emailAddress: 'missing@example.com' },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        byId: {
          id: 'gid://shopify/Customer/301',
          email: 'ada@example.com',
        },
        byEmail: {
          id: 'gid://shopify/Customer/301',
          defaultEmailAddress: {
            emailAddress: 'ada@example.com',
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
        byPhone: {
          id: 'gid://shopify/Customer/301',
          defaultPhoneNumber: {
            phoneNumber: '+15550101',
            marketingState: 'SUBSCRIBED',
            marketingOptInLevel: 'CONFIRMED_OPT_IN',
            marketingUpdatedAt: '2026-04-25T01:05:00Z',
            marketingCollectedFrom: 'OTHER',
          },
          smsMarketingConsent: {
            marketingState: 'SUBSCRIBED',
            marketingOptInLevel: 'CONFIRMED_OPT_IN',
            consentUpdatedAt: '2026-04-25T01:05:00Z',
            consentCollectedFrom: 'OTHER',
          },
        },
        missing: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns Shopify-like customerByIdentifier validation errors for unsupported identifier shapes', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customerByIdentifier validation should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const customIdResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query CustomerByIdentifierCustom($identifier: CustomerIdentifierInput!) {
          customId: customerByIdentifier(identifier: $identifier) { id }
        }`,
        variables: {
          identifier: {
            customId: {
              namespace: 'custom',
              key: 'har_150_missing',
              value: 'missing',
            },
          },
        },
      });

    expect(customIdResponse.status).toBe(200);
    expect(customIdResponse.body).toEqual({
      data: {
        customId: null,
      },
      errors: [
        {
          message: "Metafield definition of type 'id' is required when using custom ids.",
          path: ['customId'],
          extensions: {
            code: 'NOT_FOUND',
          },
        },
      ],
    });

    const emptyIdentifierResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query CustomerByIdentifierEmpty($identifier: CustomerIdentifierInput!) {
          customerByIdentifier(identifier: $identifier) { id }
        }`,
        variables: { identifier: {} },
      });

    expect(emptyIdentifierResponse.status).toBe(200);
    expect(emptyIdentifierResponse.body).toEqual({
      errors: [
        {
          message: 'Variable $identifier of type CustomerIdentifierInput! was provided invalid value',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: {},
            problems: [
              {
                path: [],
                explanation: "'CustomerIdentifierInput' requires exactly one argument, but 0 were provided.",
              },
            ],
          },
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('treats unsupported customersCount query fields as no-op filters in snapshot mode without hitting upstream', async () => {
    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/301',
        firstName: 'Ada',
        lastName: 'Lovelace',
        displayName: 'Ada Lovelace',
        email: 'ada@example.com',
        legacyResourceId: '301',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['vip'],
        numberOfOrders: 3,
        amountSpent: null,
        defaultEmailAddress: null,
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-02-01T00:00:00.000Z',
      },
      {
        id: 'gid://shopify/Customer/302',
        firstName: 'Grace',
        lastName: 'Hopper',
        displayName: 'Grace Hopper',
        email: 'grace@example.com',
        legacyResourceId: '302',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'ENABLED',
        tags: ['wholesale'],
        numberOfOrders: 8,
        amountSpent: null,
        defaultEmailAddress: null,
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-01-02T00:00:00.000Z',
        updatedAt: '2024-02-02T00:00:00.000Z',
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { customersCount: { count: 999, precision: 'AT_LEAST' } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerCounts($disabledQuery: String!, $emailQuery: String!) {
          total: customersCount { count precision }
          disabled: customersCount(query: $disabledQuery) { count precision }
          byEmail: customersCount(query: $emailQuery) { count }
        }`,
        variables: {
          disabledQuery: 'state:DISABLED',
          emailQuery: 'email:grace@example.com',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        total: {
          count: 2,
          precision: 'EXACT',
        },
        disabled: {
          count: 2,
          precision: 'EXACT',
        },
        byEmail: {
          count: 2,
        },
      },
      extensions: {
        search: [
          {
            path: ['disabled'],
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
          {
            path: ['byEmail'],
            query: 'email:grace@example.com',
            parsed: {
              field: 'email',
              match_all: 'grace@example.com',
            },
            warnings: [
              {
                field: 'email',
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

  it('hydrates customer detail from a live-hybrid catalog read and serves a later customer lookup from local state', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementationOnce(
        async () =>
          new Response(
            JSON.stringify({
              data: {
                customers: {
                  edges: [
                    {
                      cursor: 'cursor:gid://shopify/Customer/101',
                      node: {
                        id: 'gid://shopify/Customer/101',
                        firstName: 'Ada',
                        lastName: 'Lovelace',
                        displayName: 'Ada Lovelace',
                        email: 'ada@example.com',
                        legacyResourceId: '101',
                        locale: 'en',
                        note: 'Prefers email updates',
                        canDelete: false,
                        verifiedEmail: true,
                        taxExempt: false,
                        taxExemptions: ['CA_BC_RESELLER_EXEMPTION'],
                        state: 'ENABLED',
                        tags: ['vip', 'wholesale'],
                        numberOfOrders: 3,
                        amountSpent: {
                          amount: '125.50',
                          currencyCode: 'USD',
                        },
                        defaultEmailAddress: {
                          emailAddress: 'ada@example.com',
                        },
                        defaultPhoneNumber: {
                          phoneNumber: '+1*******0101',
                        },
                        defaultAddress: {
                          address1: '123 Analytical Engine Way',
                          city: 'London',
                          province: null,
                          country: 'United Kingdom',
                          zip: 'SW1A 1AA',
                          formattedArea: 'London, United Kingdom',
                        },
                        metafields: {
                          nodes: [
                            {
                              id: 'gid://shopify/Metafield/101',
                              namespace: 'custom',
                              key: 'loyalty',
                              type: 'single_line_text_field',
                              value: 'gold',
                            },
                          ],
                          pageInfo: {
                            hasNextPage: false,
                            hasPreviousPage: false,
                            startCursor: 'metafield-cursor-101',
                            endCursor: 'metafield-cursor-101',
                          },
                        },
                        createdAt: '2024-01-01T00:00:00.000Z',
                        updatedAt: '2024-01-02T00:00:00.000Z',
                      },
                    },
                  ],
                  pageInfo: {
                    hasNextPage: false,
                    hasPreviousPage: false,
                    startCursor: 'cursor:gid://shopify/Customer/101',
                    endCursor: 'cursor:gid://shopify/Customer/101',
                  },
                },
              },
            }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          ),
      )
      .mockImplementationOnce(
        async () =>
          new Response(JSON.stringify({ data: { customer: null } }), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          }),
      );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerCatalogHydration {
          customers(first: 5) {
            edges {
              cursor
              node {
                id
                firstName
                lastName
                displayName
                email
                legacyResourceId
                locale
                note
                canDelete
                verifiedEmail
                taxExempt
                taxExemptions
                state
                tags
                numberOfOrders
                amountSpent { amount currencyCode }
                defaultEmailAddress { emailAddress }
                defaultPhoneNumber { phoneNumber }
                defaultAddress {
                  address1
                  city
                  province
                  country
                  zip
                  formattedArea
                }
                metafields(first: 5) {
                  nodes { id namespace key type value }
                  pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                }
                createdAt
                updatedAt
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
      });

    expect(hydrateResponse.status).toBe(200);
    expect(hydrateResponse.body.data.customers.edges).toHaveLength(1);

    const detailResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerDetail($id: ID!) {
          customer(id: $id) {
            id
            firstName
            lastName
            displayName
            email
            legacyResourceId
            locale
            note
            canDelete
            verifiedEmail
            taxExempt
            taxExemptions
            state
            tags
            numberOfOrders
            loyalty: metafield(namespace: "custom", key: "loyalty") {
              id
              namespace
              key
              type
              value
            }
            metafields(first: 5) {
              nodes { id namespace key type value }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            amountSpent { amount currencyCode }
            defaultEmailAddress { emailAddress }
            defaultPhoneNumber { phoneNumber }
            defaultAddress {
              address1
              city
              province
              country
              zip
              formattedArea
            }
            createdAt
            updatedAt
          }
        }`,
        variables: { id: 'gid://shopify/Customer/101' },
      });

    expect(detailResponse.status).toBe(200);
    expect(detailResponse.body).toEqual({
      data: {
        customer: {
          id: 'gid://shopify/Customer/101',
          firstName: 'Ada',
          lastName: 'Lovelace',
          displayName: 'Ada Lovelace',
          email: 'ada@example.com',
          legacyResourceId: '101',
          locale: 'en',
          note: 'Prefers email updates',
          canDelete: false,
          verifiedEmail: true,
          taxExempt: false,
          taxExemptions: ['CA_BC_RESELLER_EXEMPTION'],
          state: 'ENABLED',
          tags: ['vip', 'wholesale'],
          numberOfOrders: 3,
          loyalty: {
            id: 'gid://shopify/Metafield/101',
            namespace: 'custom',
            key: 'loyalty',
            type: 'single_line_text_field',
            value: 'gold',
          },
          metafields: {
            nodes: [
              {
                id: 'gid://shopify/Metafield/101',
                namespace: 'custom',
                key: 'loyalty',
                type: 'single_line_text_field',
                value: 'gold',
              },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: 'cursor:gid://shopify/Metafield/101',
              endCursor: 'cursor:gid://shopify/Metafield/101',
            },
          },
          amountSpent: {
            amount: '125.50',
            currencyCode: 'USD',
          },
          defaultEmailAddress: {
            emailAddress: 'ada@example.com',
          },
          defaultPhoneNumber: {
            phoneNumber: '+1*******0101',
          },
          defaultAddress: {
            address1: '123 Analytical Engine Way',
            city: 'London',
            province: null,
            country: 'United Kingdom',
            zip: 'SW1A 1AA',
            formattedArea: 'London, United Kingdom',
          },
          createdAt: '2024-01-01T00:00:00.000Z',
          updatedAt: '2024-01-02T00:00:00.000Z',
        },
      },
    });
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('hydrates customers from a live-hybrid detail read and serves a later catalog query from local state', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementationOnce(
        async () =>
          new Response(
            JSON.stringify({
              data: {
                customer: {
                  id: 'gid://shopify/Customer/202',
                  firstName: 'Grace',
                  lastName: 'Hopper',
                  displayName: 'Grace Hopper',
                  email: 'grace@example.com',
                  legacyResourceId: '202',
                  locale: 'en',
                  note: 'Wholesale net terms',
                  canDelete: false,
                  verifiedEmail: true,
                  taxExempt: true,
                  state: 'ENABLED',
                  tags: ['returning'],
                  numberOfOrders: 8,
                  amountSpent: {
                    amount: '900.00',
                    currencyCode: 'USD',
                  },
                  defaultEmailAddress: {
                    emailAddress: 'grace@example.com',
                  },
                  defaultPhoneNumber: {
                    phoneNumber: '+1*******0202',
                  },
                  defaultAddress: {
                    address1: '1701 Compiler Ave',
                    city: 'Arlington',
                    province: 'Virginia',
                    country: 'United States',
                    zip: '22201',
                    formattedArea: 'Arlington VA, United States',
                  },
                  createdAt: '2024-02-01T00:00:00.000Z',
                  updatedAt: '2024-02-03T00:00:00.000Z',
                },
              },
            }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          ),
      )
      .mockImplementationOnce(
        async () =>
          new Response(JSON.stringify({ data: { customers: { edges: [], pageInfo: { hasNextPage: false } } } }), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          }),
      );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerHydration($id: ID!) {
          customer(id: $id) {
            id
            firstName
            lastName
            displayName
            email
            legacyResourceId
            locale
            note
            canDelete
            verifiedEmail
            taxExempt
            state
            tags
            numberOfOrders
            amountSpent { amount currencyCode }
            defaultEmailAddress { emailAddress }
            defaultPhoneNumber { phoneNumber }
            defaultAddress {
              address1
              city
              province
              country
              zip
              formattedArea
            }
            createdAt
            updatedAt
          }
        }`,
        variables: { id: 'gid://shopify/Customer/202' },
      });

    expect(hydrateResponse.status).toBe(200);
    expect(hydrateResponse.body.data.customer.id).toBe('gid://shopify/Customer/202');

    const catalogResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomersCatalog {
          customers(first: 5) {
            edges {
              cursor
              node {
                id
                displayName
                email
                legacyResourceId
                locale
                note
                canDelete
                verifiedEmail
                taxExempt
                amountSpent { amount currencyCode }
                defaultPhoneNumber { phoneNumber }
                defaultAddress {
                  address1
                  city
                  province
                  country
                  zip
                  formattedArea
                }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
      });

    expect(catalogResponse.status).toBe(200);
    expect(catalogResponse.body).toEqual({
      data: {
        customers: {
          edges: [
            {
              cursor: 'cursor:gid://shopify/Customer/202',
              node: {
                id: 'gid://shopify/Customer/202',
                displayName: 'Grace Hopper',
                email: 'grace@example.com',
                legacyResourceId: '202',
                locale: 'en',
                note: 'Wholesale net terms',
                canDelete: false,
                verifiedEmail: true,
                taxExempt: true,
                amountSpent: {
                  amount: '900.00',
                  currencyCode: 'USD',
                },
                defaultPhoneNumber: {
                  phoneNumber: '+1*******0202',
                },
                defaultAddress: {
                  address1: '1701 Compiler Ave',
                  city: 'Arlington',
                  province: 'Virginia',
                  country: 'United States',
                  zip: '22201',
                  formattedArea: 'Arlington VA, United States',
                },
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'cursor:gid://shopify/Customer/202',
            endCursor: 'cursor:gid://shopify/Customer/202',
          },
        },
      },
    });
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('treats unsupported customersCount query fields as no-op filters after live hydration', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(
      async () =>
        new Response(
          JSON.stringify({
            data: {
              customers: {
                edges: [
                  {
                    cursor: 'cursor:gid://shopify/Customer/401',
                    node: {
                      id: 'gid://shopify/Customer/401',
                      firstName: 'Ada',
                      lastName: 'Lovelace',
                      displayName: 'Ada Lovelace',
                      email: 'ada@example.com',
                      legacyResourceId: '401',
                      locale: 'en',
                      note: null,
                      canDelete: false,
                      verifiedEmail: true,
                      taxExempt: false,
                      state: 'DISABLED',
                      tags: ['vip'],
                      numberOfOrders: '3',
                      amountSpent: null,
                      defaultEmailAddress: null,
                      defaultPhoneNumber: null,
                      defaultAddress: {
                        address1: '123 Analytical Engine Way',
                        city: 'London',
                        province: null,
                        country: 'United Kingdom',
                        zip: 'SW1A 1AA',
                        formattedArea: 'London, United Kingdom',
                      },
                      createdAt: '2024-01-01T00:00:00.000Z',
                      updatedAt: '2024-02-01T00:00:00.000Z',
                    },
                  },
                  {
                    cursor: 'cursor:gid://shopify/Customer/402',
                    node: {
                      id: 'gid://shopify/Customer/402',
                      firstName: 'Grace',
                      lastName: 'Hopper',
                      displayName: 'Grace Hopper',
                      email: 'grace@example.com',
                      legacyResourceId: '402',
                      locale: 'en',
                      note: null,
                      canDelete: false,
                      verifiedEmail: true,
                      taxExempt: false,
                      state: 'ENABLED',
                      tags: ['referral'],
                      numberOfOrders: '8',
                      amountSpent: null,
                      defaultEmailAddress: null,
                      defaultPhoneNumber: null,
                      defaultAddress: {
                        address1: '1701 Compiler Ave',
                        city: 'Arlington',
                        province: 'Virginia',
                        country: 'United States',
                        zip: '22201',
                        formattedArea: 'Arlington VA, United States',
                      },
                      createdAt: '2024-01-02T00:00:00.000Z',
                      updatedAt: '2024-02-02T00:00:00.000Z',
                    },
                  },
                ],
                pageInfo: {
                  hasNextPage: false,
                  hasPreviousPage: false,
                  startCursor: 'cursor:gid://shopify/Customer/401',
                  endCursor: 'cursor:gid://shopify/Customer/402',
                },
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
    );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomersCountHydration {
          customers(first: 10) {
            edges {
              cursor
              node {
                id
                firstName
                lastName
                displayName
                email
                legacyResourceId
                locale
                note
                canDelete
                verifiedEmail
                taxExempt
                state
                tags
                numberOfOrders
                amountSpent { amount currencyCode }
                defaultEmailAddress { emailAddress }
                defaultPhoneNumber { phoneNumber }
                defaultAddress {
                  address1
                  city
                  province
                  country
                  zip
                  formattedArea
                }
                createdAt
                updatedAt
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
      });

    expect(hydrateResponse.status).toBe(200);

    const countResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query HydratedCustomerCounts($emailQuery: String!, $disabledQuery: String!) {
          total: customersCount { count precision }
          matching: customersCount(query: $emailQuery) { count precision }
          disabled: customersCount(query: $disabledQuery) { count }
        }`,
        variables: {
          emailQuery: 'email:grace@example.com',
          disabledQuery: 'state:DISABLED',
        },
      });

    expect(countResponse.status).toBe(200);
    expect(countResponse.body).toEqual({
      data: {
        total: {
          count: 2,
          precision: 'EXACT',
        },
        matching: {
          count: 2,
          precision: 'EXACT',
        },
        disabled: {
          count: 2,
        },
      },
      extensions: {
        search: [
          {
            path: ['matching'],
            query: 'email:grace@example.com',
            parsed: {
              field: 'email',
              match_all: 'grace@example.com',
            },
            warnings: [
              {
                field: 'email',
                message: 'Invalid search field for this query.',
                code: 'invalid_field',
              },
            ],
          },
          {
            path: ['disabled'],
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
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('replays live Shopify customer cursors and string count fields from hydrated catalog state', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementationOnce(
        async () =>
          new Response(
            JSON.stringify({
              data: {
                customers: {
                  edges: [
                    {
                      cursor: 'opaque-customer-cursor-1',
                      node: {
                        id: 'gid://shopify/Customer/6157654556905',
                        displayName: 'Henry Short',
                        email: 'egnition_sample_29@egnition.com',
                        legacyResourceId: '6157654556905',
                        locale: 'en',
                        note: null,
                        canDelete: false,
                        verifiedEmail: true,
                        taxExempt: false,
                        state: 'DISABLED',
                        numberOfOrders: '12',
                        amountSpent: {
                          amount: '1.6',
                          currencyCode: 'CAD',
                        },
                        defaultEmailAddress: {
                          emailAddress: 'egnition_sample_29@egnition.com',
                        },
                        defaultPhoneNumber: {
                          phoneNumber: '+291****0123',
                        },
                        defaultAddress: {
                          address1: 'Ap #147-5705 Nonummy Street',
                          city: 'Maubeuge',
                          province: null,
                          country: 'Eritrea',
                          zip: '7759',
                          formattedArea: 'Maubeuge, Eritrea',
                        },
                        createdAt: '2022-04-06T14:51:36Z',
                        updatedAt: '2024-03-14T01:55:09Z',
                      },
                    },
                    {
                      cursor: 'opaque-customer-cursor-2',
                      node: {
                        id: 'gid://shopify/Customer/6157654589673',
                        displayName: 'Xander Holloway',
                        email: 'egnition_sample_92@egnition.com',
                        state: 'DISABLED',
                        numberOfOrders: '14',
                        amountSpent: {
                          amount: '2.2',
                          currencyCode: 'CAD',
                        },
                        defaultEmailAddress: {
                          emailAddress: 'egnition_sample_92@egnition.com',
                        },
                        createdAt: '2022-04-06T14:51:37Z',
                        updatedAt: '2022-04-11T22:24:05Z',
                      },
                    },
                    {
                      cursor: 'opaque-customer-cursor-3',
                      node: {
                        id: 'gid://shopify/Customer/6157654622441',
                        displayName: 'Isaiah Marquez',
                        email: 'egnition_sample_99@egnition.com',
                        state: 'DISABLED',
                        numberOfOrders: '6',
                        amountSpent: {
                          amount: '0.6',
                          currencyCode: 'CAD',
                        },
                        defaultEmailAddress: {
                          emailAddress: 'egnition_sample_99@egnition.com',
                        },
                        createdAt: '2022-04-06T14:51:38Z',
                        updatedAt: '2022-04-06T21:24:10Z',
                      },
                    },
                  ],
                  pageInfo: {
                    hasNextPage: true,
                    hasPreviousPage: false,
                    startCursor: 'opaque-customer-cursor-1',
                    endCursor: 'opaque-customer-cursor-3',
                  },
                },
              },
            }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          ),
      )
      .mockImplementationOnce(
        async () =>
          new Response(
            JSON.stringify({
              data: {
                customers: {
                  edges: [],
                  pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
                },
              },
            }),
            {
              status: 200,
              headers: { 'content-type': 'application/json' },
            },
          ),
      )
      .mockImplementationOnce(
        async () =>
          new Response(JSON.stringify({ data: { customer: null } }), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          }),
      );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomersCatalogHydration {
          customers(first: 3) {
            edges {
              cursor
              node {
                id
                displayName
                email
                state
                numberOfOrders
                amountSpent { amount currencyCode }
                defaultEmailAddress { emailAddress }
                createdAt
                updatedAt
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
      });

    expect(hydrateResponse.status).toBe(200);
    expect(hydrateResponse.body.data.customers.edges).toHaveLength(3);

    const localCatalogResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomersCatalogReplay($first: Int!, $after: String) {
          customers(first: $first, after: $after) {
            edges {
              cursor
              node {
                id
                displayName
                numberOfOrders
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: {
          first: 1,
          after: 'opaque-customer-cursor-2',
        },
      });

    expect(localCatalogResponse.status).toBe(200);
    expect(localCatalogResponse.body).toEqual({
      data: {
        customers: {
          edges: [
            {
              cursor: 'opaque-customer-cursor-3',
              node: {
                id: 'gid://shopify/Customer/6157654622441',
                displayName: 'Isaiah Marquez',
                numberOfOrders: '6',
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: true,
            startCursor: 'opaque-customer-cursor-3',
            endCursor: 'opaque-customer-cursor-3',
          },
        },
      },
    });

    const localDetailResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerDetail($id: ID!) {
          customer(id: $id) {
            id
            displayName
            legacyResourceId
            locale
            note
            canDelete
            verifiedEmail
            taxExempt
            numberOfOrders
            defaultEmailAddress { emailAddress }
            defaultPhoneNumber { phoneNumber }
            defaultAddress {
              address1
              city
              province
              country
              zip
              formattedArea
            }
          }
        }`,
        variables: { id: 'gid://shopify/Customer/6157654556905' },
      });

    expect(localDetailResponse.status).toBe(200);
    expect(localDetailResponse.body).toEqual({
      data: {
        customer: {
          id: 'gid://shopify/Customer/6157654556905',
          displayName: 'Henry Short',
          legacyResourceId: '6157654556905',
          locale: 'en',
          note: null,
          canDelete: false,
          verifiedEmail: true,
          taxExempt: false,
          numberOfOrders: '12',
          defaultEmailAddress: {
            emailAddress: 'egnition_sample_29@egnition.com',
          },
          defaultPhoneNumber: {
            phoneNumber: '+291****0123',
          },
          defaultAddress: {
            address1: 'Ap #147-5705 Nonummy Street',
            city: 'Maubeuge',
            province: null,
            country: 'Eritrea',
            zip: '7759',
            formattedArea: 'Maubeuge, Eritrea',
          },
        },
      },
    });
    expect(fetchSpy).toHaveBeenCalledTimes(3);
  });

  it('filters customers by query terms and sorts them by updatedAt in live replay mode', async () => {
    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/401',
        firstName: 'Ada',
        lastName: 'Byron',
        displayName: 'Ada Byron',
        email: 'ada@example.com',
        legacyResourceId: '401',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'ENABLED',
        tags: ['vip', 'newsletter'],
        numberOfOrders: '5',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'ada@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-03-01T00:00:00.000Z',
        updatedAt: '2024-03-01T10:00:00.000Z',
      },
      {
        id: 'gid://shopify/Customer/402',
        firstName: 'Ada',
        lastName: 'Lovelace',
        displayName: 'Ada Lovelace',
        email: 'ada.lovelace@example.com',
        legacyResourceId: '402',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'ENABLED',
        tags: ['vip'],
        numberOfOrders: '8',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'ada.lovelace@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-03-02T00:00:00.000Z',
        updatedAt: '2024-03-02T10:00:00.000Z',
      },
      {
        id: 'gid://shopify/Customer/403',
        firstName: 'Grace',
        lastName: 'Hopper',
        displayName: 'Grace Hopper',
        email: 'grace@example.com',
        legacyResourceId: '403',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['vip'],
        numberOfOrders: '3',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'grace@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-03-03T00:00:00.000Z',
        updatedAt: '2024-03-03T10:00:00.000Z',
      },
    ]);
    store.setBaseCustomerCatalogConnection({
      orderedCustomerIds: ['gid://shopify/Customer/401', 'gid://shopify/Customer/402', 'gid://shopify/Customer/403'],
      cursorByCustomerId: {
        'gid://shopify/Customer/401': 'opaque-cursor-401',
        'gid://shopify/Customer/402': 'opaque-cursor-402',
        'gid://shopify/Customer/403': 'opaque-cursor-403',
      },
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'opaque-cursor-401',
        endCursor: 'opaque-cursor-403',
      },
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query FilteredCustomers($first: Int!, $query: String!) {
          customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true) {
            edges {
              cursor
              node {
                id
                displayName
                email
                state
                tags
                updatedAt
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: { first: 5, query: 'state:ENABLED tag:vip ada' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        customers: {
          edges: [
            {
              cursor: 'opaque-cursor-402',
              node: {
                id: 'gid://shopify/Customer/402',
                displayName: 'Ada Lovelace',
                email: 'ada.lovelace@example.com',
                state: 'ENABLED',
                tags: ['vip'],
                updatedAt: '2024-03-02T10:00:00.000Z',
              },
            },
            {
              cursor: 'opaque-cursor-401',
              node: {
                id: 'gid://shopify/Customer/401',
                displayName: 'Ada Byron',
                email: 'ada@example.com',
                state: 'ENABLED',
                tags: ['vip', 'newsletter'],
                updatedAt: '2024-03-01T10:00:00.000Z',
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'opaque-cursor-402',
            endCursor: 'opaque-cursor-401',
          },
        },
      },
    });
  });

  it('supports OR groups, prefix filters, and grouped negation in customer search overlay reads', async () => {
    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/601',
        firstName: 'Howard',
        lastName: 'Hahn',
        displayName: 'Howard Hahn',
        email: 'howard@example.com',
        legacyResourceId: '601',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['egnition-sample-data', 'VIP'],
        numberOfOrders: '12',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'howard@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2022-04-06T14:51:36Z',
        updatedAt: '2024-03-14T01:59:43Z',
      },
      {
        id: 'gid://shopify/Customer/602',
        firstName: 'Brennan',
        lastName: 'Lynch',
        displayName: 'Brennan Lynch',
        email: 'brennan@example.com',
        legacyResourceId: '602',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['egnition-sample-data', 'referral'],
        numberOfOrders: '9',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'brennan@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2022-04-06T14:51:37Z',
        updatedAt: '2024-03-14T01:59:37Z',
      },
      {
        id: 'gid://shopify/Customer/603',
        firstName: 'Kasimir',
        lastName: 'Richardson',
        displayName: 'Kasimir Richardson',
        email: 'kasimir@example.com',
        legacyResourceId: '603',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['egnition-sample-data'],
        numberOfOrders: '7',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'kasimir@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2022-04-06T14:51:38Z',
        updatedAt: '2024-03-14T01:56:22Z',
      },
      {
        id: 'gid://shopify/Customer/604',
        firstName: 'Channing',
        lastName: 'Guerrero',
        displayName: 'Channing Guerrero',
        email: 'channing@example.com',
        legacyResourceId: '604',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['egnition-sample-data'],
        numberOfOrders: '5',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'channing@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2022-04-06T14:51:39Z',
        updatedAt: '2024-03-14T01:55:55Z',
      },
      {
        id: 'gid://shopify/Customer/605',
        firstName: 'Ada',
        lastName: 'Lovelace',
        displayName: 'Ada Lovelace',
        email: 'ada@example.com',
        legacyResourceId: '605',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'ENABLED',
        tags: ['egnition-sample-data', 'VIP'],
        numberOfOrders: '4',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'ada@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2022-04-06T14:51:40Z',
        updatedAt: '2024-03-14T01:55:40Z',
      },
    ]);
    store.setBaseCustomerCatalogConnection({
      orderedCustomerIds: [
        'gid://shopify/Customer/601',
        'gid://shopify/Customer/602',
        'gid://shopify/Customer/603',
        'gid://shopify/Customer/604',
        'gid://shopify/Customer/605',
      ],
      cursorByCustomerId: {
        'gid://shopify/Customer/601': 'opaque-cursor-601',
        'gid://shopify/Customer/602': 'opaque-cursor-602',
        'gid://shopify/Customer/603': 'opaque-cursor-603',
        'gid://shopify/Customer/604': 'opaque-cursor-604',
        'gid://shopify/Customer/605': 'opaque-cursor-605',
      },
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'opaque-cursor-601',
        endCursor: 'opaque-cursor-605',
      },
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query AdvancedCustomers($prefixQuery: String!, $orQuery: String!, $groupedQuery: String!) {
          prefix: customers(first: 5, query: $prefixQuery, sortKey: UPDATED_AT, reverse: true) {
            edges {
              cursor
              node {
                id
                displayName
                tags
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          orMatches: customers(first: 5, query: $orQuery, sortKey: UPDATED_AT, reverse: true) {
            edges {
              cursor
              node {
                id
                displayName
                tags
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          groupedExclusion: customers(first: 5, query: $groupedQuery, sortKey: UPDATED_AT, reverse: true) {
            edges {
              cursor
              node {
                id
                displayName
                tags
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: {
          prefixQuery: 'How*',
          orQuery: '(tag:VIP OR tag:referral) state:DISABLED',
          groupedQuery: 'state:DISABLED -(tag:VIP OR tag:referral)',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        prefix: {
          edges: [
            {
              cursor: 'opaque-cursor-601',
              node: {
                id: 'gid://shopify/Customer/601',
                displayName: 'Howard Hahn',
                tags: ['egnition-sample-data', 'VIP'],
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'opaque-cursor-601',
            endCursor: 'opaque-cursor-601',
          },
        },
        orMatches: {
          edges: [
            {
              cursor: 'opaque-cursor-601',
              node: {
                id: 'gid://shopify/Customer/601',
                displayName: 'Howard Hahn',
                tags: ['egnition-sample-data', 'VIP'],
              },
            },
            {
              cursor: 'opaque-cursor-602',
              node: {
                id: 'gid://shopify/Customer/602',
                displayName: 'Brennan Lynch',
                tags: ['egnition-sample-data', 'referral'],
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'opaque-cursor-601',
            endCursor: 'opaque-cursor-602',
          },
        },
        groupedExclusion: {
          edges: [
            {
              cursor: 'opaque-cursor-603',
              node: {
                id: 'gid://shopify/Customer/603',
                displayName: 'Kasimir Richardson',
                tags: ['egnition-sample-data'],
              },
            },
            {
              cursor: 'opaque-cursor-604',
              node: {
                id: 'gid://shopify/Customer/604',
                displayName: 'Channing Guerrero',
                tags: ['egnition-sample-data'],
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'opaque-cursor-603',
            endCursor: 'opaque-cursor-604',
          },
        },
      },
    });
  });

  it('supports deterministic customer sort keys NAME, ID, and LOCATION with cursor-stable replay', async () => {
    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/920',
        firstName: 'Ada',
        lastName: 'Lovelace',
        displayName: 'Ada Lovelace',
        email: 'ada@example.com',
        legacyResourceId: '920',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'ENABLED',
        tags: ['vip'],
        numberOfOrders: '10',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'ada@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: {
          address1: '1 Ave A',
          city: 'Campinas',
          province: 'Sao Paulo',
          country: 'Brazil',
          zip: '13000',
          formattedArea: 'Campinas, Brazil',
        },
        createdAt: '2024-04-01T00:00:00.000Z',
        updatedAt: '2024-04-01T00:00:00.000Z',
      },
      {
        id: 'gid://shopify/Customer/905',
        firstName: 'Alan',
        lastName: 'Turing',
        displayName: 'Alan Turing',
        email: 'alan@example.com',
        legacyResourceId: '905',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'ENABLED',
        tags: ['referral'],
        numberOfOrders: '8',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'alan@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: {
          address1: '2 Ave B',
          city: 'Rio de Janeiro',
          province: 'Rio de Janeiro',
          country: 'Brazil',
          zip: '20000',
          formattedArea: 'Rio de Janeiro, Brazil',
        },
        createdAt: '2024-04-02T00:00:00.000Z',
        updatedAt: '2024-04-02T00:00:00.000Z',
      },
      {
        id: 'gid://shopify/Customer/930',
        firstName: 'Barbara',
        lastName: 'Liskov',
        displayName: 'Barbara Liskov',
        email: 'barbara@example.com',
        legacyResourceId: '930',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'DISABLED',
        tags: ['wholesale'],
        numberOfOrders: '4',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'barbara@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: {
          address1: '3 Ave C',
          city: 'Calgary',
          province: 'Alberta',
          country: 'Canada',
          zip: 'T2P',
          formattedArea: 'Calgary, Canada',
        },
        createdAt: '2024-04-04T00:00:00.000Z',
        updatedAt: '2024-04-04T00:00:00.000Z',
      },
      {
        id: 'gid://shopify/Customer/910',
        firstName: 'Grace',
        lastName: 'Hopper',
        displayName: 'Grace Hopper',
        email: 'grace@example.com',
        legacyResourceId: '910',
        locale: 'en',
        note: null,
        canDelete: false,
        verifiedEmail: true,
        taxExempt: false,
        state: 'ENABLED',
        tags: ['vip'],
        numberOfOrders: '3',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'grace@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: {
          address1: '4 Ave D',
          city: 'Toronto',
          province: 'Ontario',
          country: 'Canada',
          zip: 'M5H',
          formattedArea: 'Toronto, Canada',
        },
        createdAt: '2024-04-03T00:00:00.000Z',
        updatedAt: '2024-04-03T00:00:00.000Z',
      },
    ]);
    store.setBaseCustomerCatalogConnection({
      orderedCustomerIds: [
        'gid://shopify/Customer/920',
        'gid://shopify/Customer/905',
        'gid://shopify/Customer/930',
        'gid://shopify/Customer/910',
      ],
      cursorByCustomerId: {
        'gid://shopify/Customer/920': 'opaque-cursor-920',
        'gid://shopify/Customer/905': 'opaque-cursor-905',
        'gid://shopify/Customer/930': 'opaque-cursor-930',
        'gid://shopify/Customer/910': 'opaque-cursor-910',
      },
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'opaque-cursor-920',
        endCursor: 'opaque-cursor-910',
      },
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerSortKeys {
          nameOrder: customers(first: 4, sortKey: NAME) {
            edges {
              cursor
              node {
                id
                displayName
                defaultAddress { country province city }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          idOrder: customers(first: 4, sortKey: ID) {
            edges {
              cursor
              node {
                id
                displayName
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          locationOrder: customers(first: 4, sortKey: LOCATION) {
            edges {
              cursor
              node {
                id
                displayName
                defaultAddress { country province city }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        nameOrder: {
          edges: [
            {
              cursor: 'opaque-cursor-910',
              node: {
                id: 'gid://shopify/Customer/910',
                displayName: 'Grace Hopper',
                defaultAddress: {
                  country: 'Canada',
                  province: 'Ontario',
                  city: 'Toronto',
                },
              },
            },
            {
              cursor: 'opaque-cursor-930',
              node: {
                id: 'gid://shopify/Customer/930',
                displayName: 'Barbara Liskov',
                defaultAddress: {
                  country: 'Canada',
                  province: 'Alberta',
                  city: 'Calgary',
                },
              },
            },
            {
              cursor: 'opaque-cursor-920',
              node: {
                id: 'gid://shopify/Customer/920',
                displayName: 'Ada Lovelace',
                defaultAddress: {
                  country: 'Brazil',
                  province: 'Sao Paulo',
                  city: 'Campinas',
                },
              },
            },
            {
              cursor: 'opaque-cursor-905',
              node: {
                id: 'gid://shopify/Customer/905',
                displayName: 'Alan Turing',
                defaultAddress: {
                  country: 'Brazil',
                  province: 'Rio de Janeiro',
                  city: 'Rio de Janeiro',
                },
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'opaque-cursor-910',
            endCursor: 'opaque-cursor-905',
          },
        },
        idOrder: {
          edges: [
            {
              cursor: 'opaque-cursor-905',
              node: {
                id: 'gid://shopify/Customer/905',
                displayName: 'Alan Turing',
              },
            },
            {
              cursor: 'opaque-cursor-910',
              node: {
                id: 'gid://shopify/Customer/910',
                displayName: 'Grace Hopper',
              },
            },
            {
              cursor: 'opaque-cursor-920',
              node: {
                id: 'gid://shopify/Customer/920',
                displayName: 'Ada Lovelace',
              },
            },
            {
              cursor: 'opaque-cursor-930',
              node: {
                id: 'gid://shopify/Customer/930',
                displayName: 'Barbara Liskov',
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'opaque-cursor-905',
            endCursor: 'opaque-cursor-930',
          },
        },
        locationOrder: {
          edges: [
            {
              cursor: 'opaque-cursor-905',
              node: {
                id: 'gid://shopify/Customer/905',
                displayName: 'Alan Turing',
                defaultAddress: {
                  country: 'Brazil',
                  province: 'Rio de Janeiro',
                  city: 'Rio de Janeiro',
                },
              },
            },
            {
              cursor: 'opaque-cursor-920',
              node: {
                id: 'gid://shopify/Customer/920',
                displayName: 'Ada Lovelace',
                defaultAddress: {
                  country: 'Brazil',
                  province: 'Sao Paulo',
                  city: 'Campinas',
                },
              },
            },
            {
              cursor: 'opaque-cursor-930',
              node: {
                id: 'gid://shopify/Customer/930',
                displayName: 'Barbara Liskov',
                defaultAddress: {
                  country: 'Canada',
                  province: 'Alberta',
                  city: 'Calgary',
                },
              },
            },
            {
              cursor: 'opaque-cursor-910',
              node: {
                id: 'gid://shopify/Customer/910',
                displayName: 'Grace Hopper',
                defaultAddress: {
                  country: 'Canada',
                  province: 'Ontario',
                  city: 'Toronto',
                },
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'opaque-cursor-905',
            endCursor: 'opaque-cursor-910',
          },
        },
      },
    });
  });

  it('replays hydrated relevance-ranked customer searches with captured order, opaque cursors, and pageInfo', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementationOnce(
        async () =>
          new Response(
            JSON.stringify({
              data: {
                customers: {
                  edges: [
                    {
                      cursor: 'relevance-cursor-910',
                      node: {
                        id: 'gid://shopify/Customer/910',
                        firstName: 'Grace',
                        lastName: 'Hopper',
                        displayName: 'Grace Hopper',
                        email: 'grace@example.com',
                        legacyResourceId: '910',
                        locale: 'en',
                        note: null,
                        canDelete: false,
                        verifiedEmail: true,
                        taxExempt: false,
                        state: 'ENABLED',
                        tags: ['vip'],
                        numberOfOrders: '3',
                        amountSpent: null,
                        defaultEmailAddress: { emailAddress: 'grace@example.com' },
                        defaultPhoneNumber: null,
                        defaultAddress: {
                          address1: '4 Ave D',
                          city: 'Toronto',
                          province: 'Ontario',
                          country: 'Canada',
                          zip: 'M5H',
                          formattedArea: 'Toronto, Canada',
                        },
                        createdAt: '2024-04-03T00:00:00.000Z',
                        updatedAt: '2024-04-03T00:00:00.000Z',
                      },
                    },
                    {
                      cursor: 'relevance-cursor-930',
                      node: {
                        id: 'gid://shopify/Customer/930',
                        firstName: 'Barbara',
                        lastName: 'Liskov',
                        displayName: 'Barbara Liskov',
                        email: 'barbara@example.com',
                        legacyResourceId: '930',
                        locale: 'en',
                        note: null,
                        canDelete: false,
                        verifiedEmail: true,
                        taxExempt: false,
                        state: 'DISABLED',
                        tags: ['vip', 'wholesale'],
                        numberOfOrders: '4',
                        amountSpent: null,
                        defaultEmailAddress: { emailAddress: 'barbara@example.com' },
                        defaultPhoneNumber: null,
                        defaultAddress: {
                          address1: '3 Ave C',
                          city: 'Calgary',
                          province: 'Alberta',
                          country: 'Canada',
                          zip: 'T2P',
                          formattedArea: 'Calgary, Canada',
                        },
                        createdAt: '2024-04-04T00:00:00.000Z',
                        updatedAt: '2024-04-04T00:00:00.000Z',
                      },
                    },
                    {
                      cursor: 'relevance-cursor-920',
                      node: {
                        id: 'gid://shopify/Customer/920',
                        firstName: 'Ada',
                        lastName: 'Lovelace',
                        displayName: 'Ada Lovelace',
                        email: 'ada@example.com',
                        legacyResourceId: '920',
                        locale: 'en',
                        note: null,
                        canDelete: false,
                        verifiedEmail: true,
                        taxExempt: false,
                        state: 'ENABLED',
                        tags: ['vip'],
                        numberOfOrders: '10',
                        amountSpent: null,
                        defaultEmailAddress: { emailAddress: 'ada@example.com' },
                        defaultPhoneNumber: null,
                        defaultAddress: {
                          address1: '1 Ave A',
                          city: 'Campinas',
                          province: 'Sao Paulo',
                          country: 'Brazil',
                          zip: '13000',
                          formattedArea: 'Campinas, Brazil',
                        },
                        createdAt: '2024-04-01T00:00:00.000Z',
                        updatedAt: '2024-04-01T00:00:00.000Z',
                      },
                    },
                  ],
                  pageInfo: {
                    hasNextPage: true,
                    hasPreviousPage: false,
                    startCursor: 'relevance-cursor-910',
                    endCursor: 'relevance-cursor-920',
                  },
                },
              },
            }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          ),
      )
      .mockImplementationOnce(
        async () =>
          new Response(
            JSON.stringify({
              data: {
                customers: {
                  edges: [],
                  pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
                },
              },
            }),
            {
              status: 200,
              headers: { 'content-type': 'application/json' },
            },
          ),
      );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();
    const requestBody = {
      query: `query CustomerRelevanceReplay($query: String!) {
        customers(first: 3, query: $query, sortKey: RELEVANCE) {
          edges {
            cursor
            node {
              id
              displayName
              legacyResourceId
              tags
            }
          }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }`,
      variables: { query: 'vip' },
    };

    const firstResponse = await request(app).post('/admin/api/2025-01/graphql.json').send(requestBody);

    expect(firstResponse.status).toBe(200);
    expect(firstResponse.body).toEqual({
      data: {
        customers: {
          edges: [
            {
              cursor: 'relevance-cursor-910',
              node: {
                id: 'gid://shopify/Customer/910',
                displayName: 'Grace Hopper',
                legacyResourceId: '910',
                tags: ['vip'],
              },
            },
            {
              cursor: 'relevance-cursor-930',
              node: {
                id: 'gid://shopify/Customer/930',
                displayName: 'Barbara Liskov',
                legacyResourceId: '930',
                tags: ['vip', 'wholesale'],
              },
            },
            {
              cursor: 'relevance-cursor-920',
              node: {
                id: 'gid://shopify/Customer/920',
                displayName: 'Ada Lovelace',
                legacyResourceId: '920',
                tags: ['vip'],
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: 'relevance-cursor-910',
            endCursor: 'relevance-cursor-920',
          },
        },
      },
    });

    const replayResponse = await request(app).post('/admin/api/2025-01/graphql.json').send(requestBody);

    expect(replayResponse.status).toBe(200);
    expect(replayResponse.body).toEqual(firstResponse.body);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('supports backward customer pagination over filtered catalog results', async () => {
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
        state: 'ENABLED',
        tags: ['vip'],
        numberOfOrders: '1',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'alan@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-04-01T00:00:00.000Z',
        updatedAt: '2024-04-01T00:00:00.000Z',
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
        state: 'ENABLED',
        tags: ['vip'],
        numberOfOrders: '2',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'barbara@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-04-02T00:00:00.000Z',
        updatedAt: '2024-04-02T00:00:00.000Z',
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
        state: 'ENABLED',
        tags: ['vip'],
        numberOfOrders: '3',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'claude@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-04-03T00:00:00.000Z',
        updatedAt: '2024-04-03T00:00:00.000Z',
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
        state: 'ENABLED',
        tags: ['vip'],
        numberOfOrders: '4',
        amountSpent: null,
        defaultEmailAddress: {
          emailAddress: 'donald@example.com',
        },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-04-04T00:00:00.000Z',
        updatedAt: '2024-04-04T00:00:00.000Z',
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

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query BackwardCustomers($before: String!, $query: String!) {
          customers(last: 2, before: $before, query: $query) {
            edges {
              cursor
              node {
                id
                displayName
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: {
          before: 'opaque-cursor-504',
          query: 'state:ENABLED tag:vip',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        customers: {
          edges: [
            {
              cursor: 'opaque-cursor-502',
              node: {
                id: 'gid://shopify/Customer/502',
                displayName: 'Barbara Liskov',
              },
            },
            {
              cursor: 'opaque-cursor-503',
              node: {
                id: 'gid://shopify/Customer/503',
                displayName: 'Claude Shannon',
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: true,
            startCursor: 'opaque-cursor-502',
            endCursor: 'opaque-cursor-503',
          },
        },
      },
    });
  });
});
