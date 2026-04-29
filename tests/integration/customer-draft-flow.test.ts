import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import type { CustomerRecord } from '../../src/state/types.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeConsentCustomer(overrides: Partial<CustomerRecord> = {}): CustomerRecord {
  const base: CustomerRecord = {
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
  };

  return { ...base, ...overrides };
}

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

  it('models captured customerCreate input validation and normalization without mutating state on failures', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customerCreate validation should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const createMutation = `mutation CustomerCreate($input: CustomerInput!) {
      customerCreate(input: $input) {
        customer {
          id
          email
          firstName
          lastName
          locale
          note
          tags
          defaultPhoneNumber { phoneNumber }
        }
        userErrors { field message }
      }
    }`;

    const invalidEmailResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: createMutation,
        variables: { input: { email: 'not-an-email' } },
      });
    expect(invalidEmailResponse.body.data.customerCreate).toEqual({
      customer: null,
      userErrors: [{ field: ['email'], message: 'Email is invalid' }],
    });
    expect(store.listEffectiveCustomers()).toEqual([]);
    expect(store.getLog().at(-1)?.stagedResourceIds).toEqual([]);

    const validResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: createMutation,
        variables: {
          input: {
            email: 'validation-create@example.com',
            firstName: '   ',
            lastName: '',
            note: '',
            phone: '',
            tags: ['Zulu', 'alpha', 'alpha', ' spaced tag ', ''],
          },
        },
      });
    expect(validResponse.body.data.customerCreate.userErrors).toEqual([]);
    expect(validResponse.body.data.customerCreate.customer).toMatchObject({
      email: 'validation-create@example.com',
      firstName: null,
      lastName: null,
      locale: 'en',
      note: '',
      tags: ['alpha', 'spaced tag', 'Zulu'],
      defaultPhoneNumber: null,
    });
    const customerId = validResponse.body.data.customerCreate.customer.id;

    const duplicateEmailResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: createMutation,
        variables: { input: { email: 'validation-create@example.com', firstName: 'Duplicate' } },
      });
    expect(duplicateEmailResponse.body.data.customerCreate).toEqual({
      customer: null,
      userErrors: [{ field: ['email'], message: 'Email has already been taken' }],
    });

    const phoneSeedResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: createMutation,
        variables: { input: { phone: '+14155550120' } },
      });
    expect(phoneSeedResponse.body.data.customerCreate.userErrors).toEqual([]);

    const duplicatePhoneResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: createMutation,
        variables: { input: { phone: '+14155550120', firstName: 'Duplicate' } },
      });
    expect(duplicatePhoneResponse.body.data.customerCreate).toEqual({
      customer: null,
      userErrors: [{ field: ['phone'], message: 'Phone has already been taken' }],
    });

    const invalidPhoneResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: createMutation,
        variables: { input: { phone: 'abc' } },
      });
    expect(invalidPhoneResponse.body.data.customerCreate).toEqual({
      customer: null,
      userErrors: [{ field: ['phone'], message: 'Phone is invalid' }],
    });

    const invalidLocaleResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: createMutation,
        variables: { input: { email: 'validation-locale@example.com', locale: 'not-a-locale' } },
      });
    expect(invalidLocaleResponse.body.data.customerCreate).toEqual({
      customer: null,
      userErrors: [{ field: ['locale'], message: 'Locale is invalid' }],
    });

    const oversizedResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: createMutation,
        variables: {
          input: {
            email: 'validation-oversized@example.com',
            firstName: 'F'.repeat(300),
            lastName: 'L'.repeat(300),
            note: 'N'.repeat(5001),
            tags: ['T'.repeat(256)],
          },
        },
      });
    expect(oversizedResponse.body.data.customerCreate).toEqual({
      customer: null,
      userErrors: [
        { field: ['firstName'], message: 'First name is too long (maximum is 255 characters)' },
        { field: ['lastName'], message: 'Last name is too long (maximum is 255 characters)' },
        { field: ['note'], message: 'Note is too long (maximum is 5000 characters)' },
        { field: ['tags'], message: 'Tags is too long (maximum is 255 characters)' },
      ],
    });

    expect(new Set(store.listEffectiveCustomers().map((customer) => customer.id))).toEqual(
      new Set([customerId, phoneSeedResponse.body.data.customerCreate.customer.id]),
    );
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('models captured customerUpdate validation without mutating customer state or metafields on failure', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customerUpdate validation should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id
              email
              note
              tags
              metafield(namespace: "custom", key: "loyalty") { namespace key type value }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'validation-update@example.com',
            firstName: 'Validation',
            lastName: 'Update',
            phone: '+14155550123',
            note: 'before failure',
            tags: ['before'],
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
    const customerId = createResponse.body.data.customerCreate.customer.id;

    const duplicateSeedResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id email defaultPhoneNumber { phoneNumber } }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'validation-duplicate@example.com',
            phone: '+14155550124',
          },
        },
      });
    expect(duplicateSeedResponse.body.data.customerCreate.userErrors).toEqual([]);

    const updateMutation = `mutation CustomerUpdate($input: CustomerInput!) {
      customerUpdate(input: $input) {
        customer {
          id
          email
          firstName
          lastName
          locale
          note
          tags
          defaultPhoneNumber { phoneNumber }
          metafield(namespace: "custom", key: "loyalty") { namespace key type value }
        }
        userErrors { field message }
      }
    }`;

    const invalidUpdateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: updateMutation,
        variables: {
          input: {
            id: customerId,
            email: 'not-an-email',
            phone: 'abc',
            locale: 'not-a-locale',
            note: 'should not apply',
            tags: ['should-not-apply'],
            metafields: [
              {
                namespace: 'custom',
                key: 'loyalty',
                type: 'single_line_text_field',
                value: 'platinum',
              },
            ],
          },
        },
      });
    expect(invalidUpdateResponse.body.data.customerUpdate).toEqual({
      customer: null,
      userErrors: [
        { field: ['email'], message: 'Email is invalid' },
        { field: ['phone'], message: 'Phone is invalid' },
        { field: ['locale'], message: 'Locale is invalid' },
      ],
    });
    expect(store.getLog().at(-1)?.stagedResourceIds).toEqual([]);

    const failedDuplicateUpdateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: updateMutation,
        variables: {
          input: {
            id: customerId,
            email: 'validation-duplicate@example.com',
            phone: '+14155550124',
          },
        },
      });
    expect(failedDuplicateUpdateResponse.body.data.customerUpdate).toEqual({
      customer: null,
      userErrors: [
        { field: ['email'], message: 'Email has already been taken' },
        { field: ['phone'], message: 'Phone has already been taken' },
      ],
    });

    const readAfterFailureResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerAfterFailedUpdate($id: ID!) {
          customer(id: $id) {
            id
            email
            note
            tags
            defaultPhoneNumber { phoneNumber }
            metafield(namespace: "custom", key: "loyalty") { namespace key type value }
          }
          customerByIdentifier(identifier: { emailAddress: "validation-update@example.com" }) {
            id
            email
          }
          customers(first: 10, query: "email:validation-update@example.com") {
            nodes { id email note tags }
          }
        }`,
        variables: { id: customerId },
      });

    expect(readAfterFailureResponse.body.data).toEqual({
      customer: {
        id: customerId,
        email: 'validation-update@example.com',
        note: 'before failure',
        tags: ['before'],
        defaultPhoneNumber: { phoneNumber: '+14155550123' },
        metafield: {
          namespace: 'custom',
          key: 'loyalty',
          type: 'single_line_text_field',
          value: 'gold',
        },
      },
      customerByIdentifier: {
        id: customerId,
        email: 'validation-update@example.com',
      },
      customers: {
        nodes: [
          {
            id: customerId,
            email: 'validation-update@example.com',
            note: 'before failure',
            tags: ['before'],
          },
        ],
      },
    });

    const normalizationResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: updateMutation,
        variables: {
          input: {
            id: customerId,
            firstName: '   ',
            lastName: '',
            note: '',
            phone: '',
            tags: ['Zulu', 'alpha', 'alpha', ' spaced tag ', ''],
          },
        },
      });
    expect(normalizationResponse.body.data.customerUpdate.userErrors).toEqual([]);
    expect(normalizationResponse.body.data.customerUpdate.customer).toMatchObject({
      id: customerId,
      email: 'validation-update@example.com',
      firstName: null,
      lastName: null,
      note: '',
      tags: ['alpha', 'spaced tag', 'Zulu'],
      defaultPhoneNumber: null,
      metafield: {
        namespace: 'custom',
        key: 'loyalty',
        type: 'single_line_text_field',
        value: 'gold',
      },
    });

    const readAfterNormalizationResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerAfterNormalizedUpdate($id: ID!) {
          customer(id: $id) {
            id
            firstName
            lastName
            note
            tags
            defaultPhoneNumber { phoneNumber }
          }
          customerByIdentifier(identifier: { emailAddress: "validation-update@example.com" }) {
            id
            firstName
            lastName
            note
            tags
            defaultPhoneNumber { phoneNumber }
          }
        }`,
        variables: { id: customerId },
      });
    expect(readAfterNormalizationResponse.body.data).toEqual({
      customer: {
        id: customerId,
        firstName: null,
        lastName: null,
        note: '',
        tags: ['alpha', 'spaced tag', 'Zulu'],
        defaultPhoneNumber: null,
      },
      customerByIdentifier: {
        id: customerId,
        firstName: null,
        lastName: null,
        note: '',
        tags: ['alpha', 'spaced tag', 'Zulu'],
        defaultPhoneNumber: null,
      },
    });

    const nullScalarResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: updateMutation,
        variables: {
          input: {
            id: customerId,
            firstName: null,
            lastName: null,
            note: null,
            phone: null,
          },
        },
      });
    expect(nullScalarResponse.body.data.customerUpdate.userErrors).toEqual([]);
    expect(nullScalarResponse.body.data.customerUpdate.customer).toMatchObject({
      id: customerId,
      firstName: null,
      lastName: null,
      note: null,
      defaultPhoneNumber: null,
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages dataSaleOptOut locally and overlays downstream customer privacy reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('dataSaleOptOut should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id
              email
              dataSaleOptOut
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'privacy-opt-out@example.com',
            firstName: 'Privacy',
            lastName: 'Optout',
          },
        },
      });
    const customerId = createResponse.body.data.customerCreate.customer.id;
    expect(createResponse.body.data.customerCreate.customer.dataSaleOptOut).toBe(false);

    const optOutResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DataSaleOptOut($email: String!) {
          dataSaleOptOut(email: $email) {
            customerId
            userErrors { field message code }
          }
        }`,
        variables: { email: 'privacy-opt-out@example.com' },
      });

    expect(optOutResponse.status).toBe(200);
    expect(optOutResponse.body).toEqual({
      data: {
        dataSaleOptOut: {
          customerId,
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerPrivacyRead($id: ID!, $identifier: CustomerIdentifierInput!) {
          customer(id: $id) {
            id
            email
            dataSaleOptOut
          }
          customerByIdentifier(identifier: $identifier) {
            id
            email
            dataSaleOptOut
          }
        }`,
        variables: { id: customerId, identifier: { id: customerId } },
      });

    expect(readResponse.body).toEqual({
      data: {
        customer: {
          id: customerId,
          email: 'privacy-opt-out@example.com',
          dataSaleOptOut: true,
        },
        customerByIdentifier: {
          id: customerId,
          email: 'privacy-opt-out@example.com',
          dataSaleOptOut: true,
        },
      },
    });

    const unknownOptOutResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DataSaleOptOut($email: String!) {
          dataSaleOptOut(email: $email) {
            customerId
            userErrors { field message code }
          }
        }`,
        variables: { email: 'new-opt-out@example.com' },
      });
    const createdOptOutCustomerId = unknownOptOutResponse.body.data.dataSaleOptOut.customerId;
    expect(createdOptOutCustomerId).toMatch(/^gid:\/\/shopify\/Customer\//);
    expect(unknownOptOutResponse.body.data.dataSaleOptOut.userErrors).toEqual([]);

    const unknownReadResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerPrivacyRead($id: ID!) {
          customer(id: $id) {
            id
            email
            dataSaleOptOut
          }
        }`,
        variables: { id: createdOptOutCustomerId },
      });
    expect(unknownReadResponse.body.data.customer).toEqual({
      id: createdOptOutCustomerId,
      email: 'new-opt-out@example.com',
      dataSaleOptOut: true,
    });

    const invalidOptOutResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DataSaleOptOut($email: String!) {
          dataSaleOptOut(email: $email) {
            customerId
            userErrors { field message code }
          }
        }`,
        variables: { email: 'not-an-email' },
      });

    expect(invalidOptOutResponse.body).toEqual({
      data: {
        dataSaleOptOut: {
          customerId: null,
          userErrors: [
            {
              field: null,
              message: 'Data sale opt out failed.',
              code: 'FAILED',
            },
          ],
        },
      },
    });
    expect(store.getLog().map((entry) => entry.operationName)).toEqual([
      'CustomerCreate',
      'DataSaleOptOut',
      'DataSaleOptOut',
      'DataSaleOptOut',
    ]);
    expect(store.getLog()[1]).toMatchObject({
      status: 'staged',
      interpreted: {
        capability: {
          domain: 'privacy',
          execution: 'stage-locally',
        },
      },
      notes: 'Staged locally in the in-memory customer privacy draft store.',
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages CustomerInput address lists on create and update read-after-write paths', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('CustomerInput address lists should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerCreateWithAddresses($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id
              defaultAddress { id address1 city provinceCode countryCodeV2 formattedArea }
              addressesV2(first: 5) {
                nodes { id address1 city provinceCode countryCodeV2 formattedArea }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'customer-input-addresses@example.com',
            firstName: 'Address',
            lastName: 'Input',
            addresses: [
              {
                address1: '10 Input St',
                city: 'Ottawa',
                countryCode: 'CA',
                provinceCode: 'ON',
                zip: 'K1A 0B1',
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.customerCreate.userErrors).toEqual([]);
    const customerId = createResponse.body.data.customerCreate.customer.id;
    const createdAddress = createResponse.body.data.customerCreate.customer.addressesV2.nodes[0];
    expect(createdAddress).toMatchObject({
      id: expect.stringMatching(/^gid:\/\/shopify\/CustomerAddress\//),
      address1: '10 Input St',
      city: 'Ottawa',
      provinceCode: 'ON',
      countryCodeV2: 'CA',
      formattedArea: 'Ottawa ON, Canada',
    });
    expect(createResponse.body.data.customerCreate.customer.defaultAddress).toEqual(createdAddress);
    expect(createResponse.body.data.customerCreate.customer.addressesV2.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: `customer-address-${customerId}-0`,
      endCursor: `customer-address-${customerId}-0`,
    });

    const invalidUpdateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InvalidCustomerInputAddress($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id addressesV2(first: 5) { nodes { address1 } } }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            id: customerId,
            addresses: [{ address1: 'Invalid Province', city: 'Ottawa', countryCode: 'CA', provinceCode: 'ZZ' }],
          },
        },
      });

    expect(invalidUpdateResponse.body.data.customerUpdate).toEqual({
      customer: null,
      userErrors: [{ field: ['input', 'addresses', '0', 'province'], message: 'Province is invalid' }],
    });

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerUpdateAddressList($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer {
              id
              defaultAddress { id address1 city provinceCode countryCodeV2 formattedArea }
              addresses { id address1 city }
              addressesV2(first: 5) {
                nodes { id address1 city provinceCode countryCodeV2 formattedArea }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            id: customerId,
            addresses: [
              {
                address1: '20 Replacement Ave',
                city: 'Montreal',
                countryCode: 'CA',
                provinceCode: 'QC',
                zip: 'H2Y 1C6',
              },
              {
                address1: '20 Replacement Ave',
                city: 'Montreal',
                countryCode: 'CA',
                provinceCode: 'QC',
                zip: 'H2Y 1C6',
              },
            ],
          },
        },
      });

    expect(updateResponse.body.data.customerUpdate.userErrors).toEqual([]);
    const replacementAddress = updateResponse.body.data.customerUpdate.customer.addressesV2.nodes[0];
    expect(updateResponse.body.data.customerUpdate.customer.addressesV2.nodes).toEqual([
      {
        id: replacementAddress.id,
        address1: '20 Replacement Ave',
        city: 'Montreal',
        provinceCode: 'QC',
        countryCodeV2: 'CA',
        formattedArea: 'Montreal QC, Canada',
      },
    ]);
    expect(updateResponse.body.data.customerUpdate.customer.defaultAddress).toEqual(replacementAddress);
    expect(updateResponse.body.data.customerUpdate.customer.addresses).toEqual([
      {
        id: replacementAddress.id,
        address1: '20 Replacement Ave',
        city: 'Montreal',
      },
    ]);

    const downstreamReadResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerInputAddressRead($id: ID!) {
          customer(id: $id) {
            id
            defaultAddress { address1 city }
            addressesV2(first: 5) { nodes { address1 city } }
          }
          customerByIdentifier(identifier: { emailAddress: "customer-input-addresses@example.com" }) {
            id
            defaultAddress { address1 city }
            addressesV2(first: 5) { nodes { address1 city } }
          }
          customers(first: 5, query: "email:customer-input-addresses@example.com") {
            nodes {
              id
              defaultAddress { address1 city }
              addressesV2(first: 5) { nodes { address1 city } }
            }
          }
        }`,
        variables: { id: customerId },
      });

    const expectedAddressRead = {
      defaultAddress: { address1: '20 Replacement Ave', city: 'Montreal' },
      addressesV2: { nodes: [{ address1: '20 Replacement Ave', city: 'Montreal' }] },
    };
    expect(downstreamReadResponse.body.data).toEqual({
      customer: { id: customerId, ...expectedAddressRead },
      customerByIdentifier: { id: customerId, ...expectedAddressRead },
      customers: { nodes: [{ id: customerId, ...expectedAddressRead }] },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'customerCreate',
      'customerUpdate',
      'customerUpdate',
    ]);
    expect(logResponse.body.entries.at(-1).requestBody.variables.input.addresses).toHaveLength(2);
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

  it('mirrors captured long-tail customer address validation and normalization locally', async () => {
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
            email: 'address-long-tail@example.com',
            firstName: 'Address',
            lastName: 'LongTail',
          },
        },
      });
    const customerId = createCustomerResponse.body.data.customerCreate.customer.id;

    const blankAddressResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation BlankAddress($customerId: ID!) {
          customerAddressCreate(customerId: $customerId, address: {}, setAsDefault: true) {
            address {
              id
              firstName
              lastName
              address1
              city
              country
              countryCodeV2
              province
              provinceCode
              zip
              name
              formattedArea
            }
            userErrors { field message }
          }
        }`,
        variables: { customerId },
      });
    expect(blankAddressResponse.body.data.customerAddressCreate.userErrors).toEqual([]);
    expect(blankAddressResponse.body.data.customerAddressCreate.address).toMatchObject({
      firstName: 'Address',
      lastName: 'LongTail',
      address1: null,
      city: null,
      country: null,
      countryCodeV2: null,
      province: null,
      provinceCode: null,
      zip: null,
      name: 'Address LongTail',
      formattedArea: null,
    });

    const blankStringAddressResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation BlankStringAddress($customerId: ID!) {
          customerAddressCreate(
            customerId: $customerId
            address: { firstName: "", lastName: "", address1: "", city: "", countryCode: CA, provinceCode: "", zip: "" }
            setAsDefault: false
          ) {
            address { id firstName lastName address1 city country countryCodeV2 provinceCode zip name formattedArea }
            userErrors { field message }
          }
        }`,
        variables: { customerId },
      });
    expect(blankStringAddressResponse.body.data.customerAddressCreate).toEqual({
      address: {
        id: blankStringAddressResponse.body.data.customerAddressCreate.address.id,
        firstName: 'Address',
        lastName: 'LongTail',
        address1: null,
        city: null,
        country: 'Canada',
        countryCodeV2: 'CA',
        provinceCode: null,
        zip: null,
        name: 'Address LongTail',
        formattedArea: 'Canada',
      },
      userErrors: [],
    });

    const invalidProvinceResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InvalidProvince($customerId: ID!) {
          customerAddressCreate(
            customerId: $customerId
            address: { address1: "5 Invalid Province St", city: "Ottawa", countryCode: CA, provinceCode: "ZZ", zip: "K1A 0B1" }
          ) {
            address { id }
            userErrors { field message }
          }
        }`,
        variables: { customerId },
      });
    expect(invalidProvinceResponse.body.data.customerAddressCreate).toEqual({
      address: null,
      userErrors: [{ field: ['address', 'province'], message: 'Province is invalid' }],
    });

    const invalidCountryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InvalidCountry($customerId: ID!) {
          customerAddressCreate(
            customerId: $customerId
            address: { address1: "6 Invalid Country St", city: "Nowhere", countryCode: ZZ, provinceCode: "ZZ", zip: "00000" }
          ) {
            address { id }
            userErrors { field message }
          }
        }`,
        variables: { customerId },
      });
    expect(invalidCountryResponse.body.data.customerAddressCreate).toEqual({
      address: null,
      userErrors: [{ field: ['address', 'country'], message: 'Country is invalid' }],
    });

    const albertaAddressResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation ValidAlbertaProvince($customerId: ID!) {
          customerAddressCreate(
            customerId: $customerId
            address: { address1: "4 Alberta St", city: "Calgary", countryCode: CA, provinceCode: "AB", zip: "T2P 1J9" }
          ) {
            address { id address1 city country countryCodeV2 province provinceCode zip formattedArea }
            userErrors { field message }
          }
        }`,
        variables: { customerId },
      });
    expect(albertaAddressResponse.body.data.customerAddressCreate).toEqual({
      address: {
        id: albertaAddressResponse.body.data.customerAddressCreate.address.id,
        address1: '4 Alberta St',
        city: 'Calgary',
        country: 'Canada',
        countryCodeV2: 'CA',
        province: 'Alberta',
        provinceCode: 'AB',
        zip: 'T2P 1J9',
        formattedArea: 'Calgary AB, Canada',
      },
      userErrors: [],
    });

    const invalidPostalResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InvalidPostal($customerId: ID!) {
          customerAddressCreate(
            customerId: $customerId
            address: { address1: "7 Postal St", city: "Ottawa", countryCode: CA, provinceCode: "ON", zip: "not-a-postal-code" }
          ) {
            address { id address1 city country countryCodeV2 province provinceCode zip formattedArea }
            userErrors { field message }
          }
        }`,
        variables: { customerId },
      });
    expect(invalidPostalResponse.body.data.customerAddressCreate).toEqual({
      address: {
        id: invalidPostalResponse.body.data.customerAddressCreate.address.id,
        address1: '7 Postal St',
        city: 'Ottawa',
        country: 'Canada',
        countryCodeV2: 'CA',
        province: 'Ontario',
        provinceCode: 'ON',
        zip: 'not-a-postal-code',
        formattedArea: 'Ottawa ON, Canada',
      },
      userErrors: [],
    });

    const createUniqueAddressResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UniqueAddress($customerId: ID!) {
          customerAddressCreate(
            customerId: $customerId
            address: { address1: "8 Duplicate St", city: "Toronto", countryCode: CA, provinceCode: "ON", zip: "M5H 2N2" }
          ) {
            address { id }
            userErrors { field message }
          }
        }`,
        variables: { customerId },
      });
    expect(createUniqueAddressResponse.body.data.customerAddressCreate.userErrors).toEqual([]);

    const duplicateAddressResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DuplicateAddress($customerId: ID!) {
          customerAddressCreate(
            customerId: $customerId
            address: { address1: "8 Duplicate St", city: "Toronto", countryCode: CA, provinceCode: "ON", zip: "M5H 2N2" }
            setAsDefault: false
          ) {
            address { id }
            userErrors { field message }
          }
        }`,
        variables: { customerId },
      });
    expect(duplicateAddressResponse.body.data.customerAddressCreate).toEqual({
      address: null,
      userErrors: [{ field: ['address'], message: 'Address already exists' }],
    });

    const createOtherCustomerResponse = await request(app)
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
            email: 'address-cross-owner@example.com',
            firstName: 'Other',
            lastName: 'Owner',
          },
        },
      });
    const otherCustomerId = createOtherCustomerResponse.body.data.customerCreate.customer.id;
    const createOtherAddressResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation OtherAddress($customerId: ID!) {
          customerAddressCreate(
            customerId: $customerId
            address: { address1: "9 Other St", city: "Ottawa", countryCode: CA, provinceCode: "ON", zip: "K1A 0B1" }
            setAsDefault: true
          ) {
            address { id }
            userErrors { field message }
          }
        }`,
        variables: { customerId: otherCustomerId },
      });
    const otherAddressId = createOtherAddressResponse.body.data.customerAddressCreate.address.id;

    const crossUpdateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CrossUpdate($customerId: ID!, $addressId: ID!) {
          customerAddressUpdate(customerId: $customerId, addressId: $addressId, address: { city: "Cross Customer" }) {
            address { id }
            userErrors { field message }
          }
        }`,
        variables: { customerId, addressId: otherAddressId },
      });
    expect(crossUpdateResponse.body.data.customerAddressUpdate).toEqual({
      address: null,
      userErrors: [{ field: ['addressId'], message: 'Address does not exist' }],
    });

    const crossDefaultResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CrossDefault($customerId: ID!, $addressId: ID!) {
          customerUpdateDefaultAddress(customerId: $customerId, addressId: $addressId) {
            customer { id defaultAddress { id } addresses { id } addressesV2(first: 5) { nodes { id } } }
            userErrors { field message }
          }
        }`,
        variables: { customerId, addressId: otherAddressId },
      });
    expect(crossDefaultResponse.body.data.customerUpdateDefaultAddress).toEqual({
      customer: {
        id: customerId,
        defaultAddress: {
          id: blankAddressResponse.body.data.customerAddressCreate.address.id,
        },
        addresses: expect.arrayContaining([
          { id: blankAddressResponse.body.data.customerAddressCreate.address.id },
          { id: blankStringAddressResponse.body.data.customerAddressCreate.address.id },
          { id: invalidPostalResponse.body.data.customerAddressCreate.address.id },
          { id: createUniqueAddressResponse.body.data.customerAddressCreate.address.id },
        ]),
        addressesV2: {
          nodes: expect.arrayContaining([
            { id: blankAddressResponse.body.data.customerAddressCreate.address.id },
            { id: blankStringAddressResponse.body.data.customerAddressCreate.address.id },
            { id: invalidPostalResponse.body.data.customerAddressCreate.address.id },
            { id: createUniqueAddressResponse.body.data.customerAddressCreate.address.id },
          ]),
        },
      },
      userErrors: [{ field: ['addressId'], message: 'Address does not exist' }],
    });

    const crossDeleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CrossDelete($customerId: ID!, $addressId: ID!) {
          customerAddressDelete(customerId: $customerId, addressId: $addressId) {
            deletedAddressId
            userErrors { field message }
          }
        }`,
        variables: { customerId, addressId: otherAddressId },
      });
    expect(crossDeleteResponse.body.data.customerAddressDelete).toEqual({
      deletedAddressId: null,
      userErrors: [{ field: ['addressId'], message: 'Address does not exist' }],
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

    const updateMergedSourceResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UpdateMergedSource($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: { input: { id: customerOneId, firstName: 'AfterMerge' } },
      });
    expect(updateMergedSourceResponse.body.data.customerUpdate).toEqual({
      customer: null,
      userErrors: [{ field: ['id'], message: 'Customer does not exist' }],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages customerMerge attached resources represented in normalized state', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customerMerge attached resources should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const createMutation = `mutation CustomerCreate($input: CustomerInput!) {
      customerCreate(input: $input) {
        customer { id email defaultPhoneNumber { phoneNumber } metafields(first: 5) { nodes { id namespace key type value } } }
        userErrors { field message }
      }
    }`;

    const createOneResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: createMutation,
        variables: {
          input: {
            email: 'merge-attached-one@example.com',
            phone: '+16475550111',
            firstName: 'Merge',
            lastName: 'One',
            note: 'one note',
            tags: ['har-291-merge', 'merge-one'],
            metafields: [
              { namespace: 'custom', key: 'source_only', type: 'single_line_text_field', value: 'source' },
              { namespace: 'custom', key: 'conflict', type: 'single_line_text_field', value: 'source-conflict' },
            ],
          },
        },
      });
    const createTwoResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: createMutation,
        variables: {
          input: {
            email: 'merge-attached-two@example.com',
            phone: '+16475550222',
            firstName: 'Merge',
            lastName: 'Two',
            note: 'two note',
            tags: ['har-291-merge', 'merge-two'],
            metafields: [
              { namespace: 'custom', key: 'result_only', type: 'single_line_text_field', value: 'result' },
              { namespace: 'custom', key: 'conflict', type: 'single_line_text_field', value: 'result-conflict' },
            ],
          },
        },
      });

    const customerOneId = createOneResponse.body.data.customerCreate.customer.id;
    const customerTwoId = createTwoResponse.body.data.customerCreate.customer.id;

    const addressMutation = `mutation CustomerAddressCreate($customerId: ID!, $address: MailingAddressInput!, $setAsDefault: Boolean) {
      customerAddressCreate(customerId: $customerId, address: $address, setAsDefault: $setAsDefault) {
        address { id address1 city provinceCode countryCodeV2 zip }
        userErrors { field message }
      }
    }`;
    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: addressMutation,
        variables: {
          customerId: customerOneId,
          address: {
            firstName: 'Source',
            lastName: 'Address',
            address1: '1 Source Merge St',
            city: 'Ottawa',
            provinceCode: 'ON',
            countryCode: 'CA',
            zip: 'K1A 0B1',
          },
          setAsDefault: true,
        },
      });
    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: addressMutation,
        variables: {
          customerId: customerTwoId,
          address: {
            firstName: 'Result',
            lastName: 'Address',
            address1: '2 Result Merge Ave',
            city: 'Toronto',
            provinceCode: 'ON',
            countryCode: 'CA',
            zip: 'M5H 2N2',
          },
          setAsDefault: true,
        },
      });

    const orderResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerMergeOrderCreate($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id email customer { id email displayName } }
            userErrors { field message }
          }
        }`,
        variables: {
          order: {
            customerId: customerOneId,
            email: 'merge-attached-one@example.com',
            currency: 'CAD',
            lineItems: [
              {
                title: 'HAR-291 merge source order item',
                quantity: 1,
                priceSet: { shopMoney: { amount: '11.00', currencyCode: 'CAD' } },
              },
            ],
          },
        },
      });
    const orderId = orderResponse.body.data.orderCreate.order.id;
    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerMergeOrderCustomerSet($orderId: ID!, $customerId: ID!) {
          orderCustomerSet(orderId: $orderId, customerId: $customerId) {
            order { id customer { id email displayName } }
            userErrors { field message }
          }
        }`,
        variables: { orderId, customerId: customerOneId },
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
            customerIdOfPhoneNumberToKeep: customerOneId,
            customerIdOfFirstNameToKeep: customerOneId,
            customerIdOfLastNameToKeep: customerTwoId,
            note: 'merged note',
            tags: ['har-291-merge', 'merged'],
          },
        },
      });
    const jobId = mergeResponse.body.data.customerMerge.job.id;

    const downstreamResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerMergeAttachedDownstream($one: ID!, $two: ID!, $jobId: ID!) {
          source: customer(id: $one) { id }
          result: customer(id: $two) {
            id
            email
            note
            tags
            numberOfOrders
            defaultPhoneNumber { phoneNumber }
            defaultAddress { address1 city }
            addressesV2(first: 10) { nodes { address1 city } }
            metafields(first: 10) { nodes { namespace key type value } }
            orders(first: 10) { nodes { id email customer { id email displayName } } }
            lastOrder { id }
          }
          oldEmail: customerByIdentifier(identifier: { emailAddress: "merge-attached-one@example.com" }) { id }
          newEmail: customerByIdentifier(identifier: { emailAddress: "merge-attached-two@example.com" }) {
            id
            addressesV2(first: 10) { nodes { address1 city } }
            metafields(first: 10) { nodes { namespace key type value } }
            orders(first: 10) { nodes { id email } }
          }
          mergeStatus: customerMergeJobStatus(jobId: $jobId) { jobId resultingCustomerId status customerMergeErrors { message } }
        }`,
        variables: { one: customerOneId, two: customerTwoId, jobId },
      });

    expect(downstreamResponse.status).toBe(200);
    expect(downstreamResponse.body.data).toEqual({
      source: null,
      result: {
        id: customerTwoId,
        email: 'merge-attached-two@example.com',
        note: 'merged note',
        tags: ['har-291-merge', 'merged'],
        numberOfOrders: 0,
        defaultPhoneNumber: { phoneNumber: '+16475550111' },
        defaultAddress: { address1: '2 Result Merge Ave', city: 'Toronto' },
        addressesV2: {
          nodes: [
            { address1: '1 Source Merge St', city: 'Ottawa' },
            { address1: '2 Result Merge Ave', city: 'Toronto' },
          ],
        },
        metafields: {
          nodes: [
            { namespace: 'custom', key: 'result_only', type: 'single_line_text_field', value: 'result' },
            { namespace: 'custom', key: 'conflict', type: 'single_line_text_field', value: 'result-conflict' },
            { namespace: 'custom', key: 'source_only', type: 'single_line_text_field', value: 'source' },
          ],
        },
        orders: {
          nodes: [
            {
              id: orderId,
              email: 'merge-attached-two@example.com',
              customer: {
                id: customerTwoId,
                email: 'merge-attached-two@example.com',
                displayName: 'Merge Two',
              },
            },
          ],
        },
        lastOrder: null,
      },
      oldEmail: null,
      newEmail: {
        id: customerTwoId,
        addressesV2: {
          nodes: [
            { address1: '1 Source Merge St', city: 'Ottawa' },
            { address1: '2 Result Merge Ave', city: 'Toronto' },
          ],
        },
        metafields: {
          nodes: [
            { namespace: 'custom', key: 'result_only', type: 'single_line_text_field', value: 'result' },
            { namespace: 'custom', key: 'conflict', type: 'single_line_text_field', value: 'result-conflict' },
            { namespace: 'custom', key: 'source_only', type: 'single_line_text_field', value: 'source' },
          ],
        },
        orders: {
          nodes: [{ id: orderId, email: 'merge-attached-two@example.com' }],
        },
      },
      mergeStatus: {
        jobId,
        resultingCustomerId: customerTwoId,
        status: 'COMPLETED',
        customerMergeErrors: [],
      },
    });
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
              addresses { id address1 city }
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
    expect(updateResponse.body.data.customerSet.customer.addresses).toHaveLength(1);
    expect(updateResponse.body.data.customerSet.customer.addresses[0]).toMatchObject({
      address1: '10 Set St',
      city: 'Ottawa',
    });
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

    const blankAddressReplacementResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetBlankAddress($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer {
              id
              defaultAddress { firstName lastName address1 city country countryCodeV2 provinceCode zip name formattedArea }
              addresses { id address1 city }
              addressesV2(first: 5) {
                nodes { id firstName lastName address1 city country countryCodeV2 provinceCode zip name formattedArea }
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { id: createdCustomerId },
          input: {
            email: 'customer-set-create@example.com',
            addresses: [{}],
          },
        },
      });
    expect(blankAddressReplacementResponse.body.data.customerSet.userErrors).toEqual([]);
    expect(blankAddressReplacementResponse.body.data.customerSet.customer.defaultAddress).toEqual({
      firstName: 'Set',
      lastName: 'Updated',
      address1: null,
      city: null,
      country: null,
      countryCodeV2: null,
      provinceCode: null,
      zip: null,
      name: 'Set Updated',
      formattedArea: null,
    });
    expect(blankAddressReplacementResponse.body.data.customerSet.customer.addresses).toHaveLength(1);
    expect(blankAddressReplacementResponse.body.data.customerSet.customer.addressesV2.nodes).toEqual([
      {
        id: blankAddressReplacementResponse.body.data.customerSet.customer.addressesV2.nodes[0].id,
        firstName: 'Set',
        lastName: 'Updated',
        address1: null,
        city: null,
        country: null,
        countryCodeV2: null,
        provinceCode: null,
        zip: null,
        name: 'Set Updated',
        formattedArea: null,
      },
    ]);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'customerSet',
      'customerSet',
      'customerSet',
      'customerSet',
    ]);
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
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

  it('matches captured broader customerSet identifier, null, and address branches locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('broader customerSet branches should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetSeed($input: CustomerSetInput!) {
          customerSet(input: $input) {
            customer { id email defaultPhoneNumber { phoneNumber } }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'customer-set-broader@example.com',
            firstName: 'Broader',
            lastName: 'Seed',
            phone: '+14155550201',
            tags: ['set'],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.customerSet.userErrors).toEqual([]);
    const seedCustomerId = createResponse.body.data.customerSet.customer.id;

    const duplicateEmailResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetDuplicateEmail($input: CustomerSetInput!) {
          customerSet(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'customer-set-broader@example.com',
            firstName: 'Duplicate',
          },
        },
      });

    expect(duplicateEmailResponse.body.data.customerSet).toEqual({
      customer: null,
      userErrors: [{ field: ['input', 'email'], message: 'Email has already been taken' }],
    });

    const duplicatePhoneResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetDuplicatePhone($input: CustomerSetInput!) {
          customerSet(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            phone: '+14155550201',
            firstName: 'Duplicate',
          },
        },
      });

    expect(duplicatePhoneResponse.body.data.customerSet).toEqual({
      customer: null,
      userErrors: [{ field: ['input', 'phone'], message: 'Phone has already been taken' }],
    });

    const mismatchResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetMismatch($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { email: 'customer-set-broader@example.com' },
          input: { email: 'customer-set-other@example.com', firstName: 'Mismatch' },
        },
      });

    expect(mismatchResponse.body.data.customerSet).toEqual({
      customer: null,
      userErrors: [
        {
          field: ['input'],
          message: 'The identifier value does not match the value of the corresponding field in the input.',
        },
      ],
    });

    const missingIdentifierFieldResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetMissingIdentifierField($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { email: 'customer-set-broader@example.com' },
          input: { firstName: 'Missing' },
        },
      });

    expect(missingIdentifierFieldResponse.body.data.customerSet).toEqual({
      customer: null,
      userErrors: [{ field: ['input'], message: 'The input field corresponding to the identifier is required.' }],
    });

    const multiAddressResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetMultiAddress($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer {
              id
              defaultAddress { address1 city province country zip formattedArea }
              addressesV2(first: 5) {
                nodes { address1 city province country zip formattedArea }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { id: seedCustomerId },
          input: {
            email: 'customer-set-broader@example.com',
            addresses: [
              { address1: '20 Set St', city: 'Ottawa', countryCode: 'CA', provinceCode: 'ON', zip: 'K1A 0B2' },
              { address1: '21 Set St', city: 'Toronto', countryCode: 'CA', provinceCode: 'ON', zip: 'M5H 2N3' },
            ],
          },
        },
      });

    expect(multiAddressResponse.body.data.customerSet.userErrors).toEqual([]);
    expect(multiAddressResponse.body.data.customerSet.customer.defaultAddress).toEqual({
      address1: '20 Set St',
      city: 'Ottawa',
      province: 'Ontario',
      country: 'Canada',
      zip: 'K1A 0B2',
      formattedArea: 'Ottawa ON, Canada',
    });
    expect(multiAddressResponse.body.data.customerSet.customer.addressesV2.nodes).toEqual([
      {
        address1: '20 Set St',
        city: 'Ottawa',
        province: 'Ontario',
        country: 'Canada',
        zip: 'K1A 0B2',
        formattedArea: 'Ottawa ON, Canada',
      },
      {
        address1: '21 Set St',
        city: 'Toronto',
        province: 'Ontario',
        country: 'Canada',
        zip: 'M5H 2N3',
        formattedArea: 'Toronto ON, Canada',
      },
    ]);

    const nullAddressResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetNullAddress($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer {
              id
              defaultAddress { address1 }
              addressesV2(first: 5) { nodes { address1 } }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { id: seedCustomerId },
          input: { email: 'customer-set-broader@example.com', addresses: null },
        },
      });

    expect(nullAddressResponse.body.data.customerSet).toEqual({
      customer: {
        id: seedCustomerId,
        defaultAddress: { address1: '20 Set St' },
        addressesV2: { nodes: [{ address1: '20 Set St' }, { address1: '21 Set St' }] },
      },
      userErrors: [],
    });

    const nullableTaxExemptResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetNullable($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { id: seedCustomerId },
          input: {
            email: null,
            firstName: null,
            lastName: null,
            locale: null,
            note: null,
            phone: null,
            tags: null,
            taxExempt: null,
            taxExemptions: null,
          },
        },
      });

    expect(nullableTaxExemptResponse.body.data.customerSet).toEqual({
      customer: null,
      userErrors: [{ field: ['input', 'taxExempt'], message: 'Tax exempt is of unexpected type NilClass' }],
    });

    const phoneUpsertResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetPhoneUpsert($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer { id displayName defaultPhoneNumber { phoneNumber } tags }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { phone: '+14155550202' },
          input: {
            phone: '+14155550202',
            firstName: 'Phone',
            lastName: 'Upsert',
            tags: ['set', 'phone'],
          },
        },
      });

    expect(phoneUpsertResponse.body.data.customerSet.userErrors).toEqual([]);
    const phoneCustomerId = phoneUpsertResponse.body.data.customerSet.customer.id;

    const phoneUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetPhoneUpdate($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer { id displayName defaultPhoneNumber { phoneNumber } tags }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { phone: '+14155550202' },
          input: {
            phone: '+14155550202',
            firstName: 'Phone',
            lastName: 'Updated',
            tags: ['set', 'phone-updated'],
          },
        },
      });

    expect(phoneUpdateResponse.body.data.customerSet).toEqual({
      customer: {
        id: phoneCustomerId,
        displayName: 'Phone Updated',
        defaultPhoneNumber: { phoneNumber: '+14155550202' },
        tags: ['phone-updated', 'set'],
      },
      userErrors: [],
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeletePhoneCustomer($input: CustomerDeleteInput!) {
          customerDelete(input: $input) {
            deletedCustomerId
            userErrors { field message }
          }
        }`,
        variables: { input: { id: phoneCustomerId } },
      });

    expect(deleteResponse.body.data.customerDelete).toEqual({ deletedCustomerId: phoneCustomerId, userErrors: [] });

    const deletedIdentifierResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CustomerSetDeletedIdentifier($identifier: CustomerSetIdentifiers, $input: CustomerSetInput!) {
          customerSet(identifier: $identifier, input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          identifier: { id: phoneCustomerId },
          input: { firstName: 'Deleted' },
        },
      });

    expect(deletedIdentifierResponse.body.data.customerSet).toEqual({
      customer: null,
      userErrors: [{ field: ['input'], message: 'Resource matching the identifier was not found.' }],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query CustomerSetBroaderRead($seedId: ID!, $deletedId: ID!) {
          seed: customer(id: $seedId) { id email addressesV2(first: 5) { nodes { address1 } } }
          deleted: customer(id: $deletedId) { id }
          byPhone: customerByIdentifier(identifier: { phoneNumber: "+14155550202" }) { id }
          customers(first: 10, query: "tag:set") { nodes { id email tags } }
          customersCount { count precision }
        }`,
        variables: { seedId: seedCustomerId, deletedId: phoneCustomerId },
      });

    expect(readResponse.body.data).toEqual({
      seed: {
        id: seedCustomerId,
        email: 'customer-set-broader@example.com',
        addressesV2: { nodes: [{ address1: '20 Set St' }, { address1: '21 Set St' }] },
      },
      deleted: null,
      byPhone: null,
      customers: {
        nodes: [{ id: seedCustomerId, email: 'customer-set-broader@example.com', tags: ['set'] }],
      },
      customersCount: { count: 1, precision: 'EXACT' },
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

  it('stages dedicated customer tax exemption mutations locally and overlays downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('dedicated customer tax exemption mutations should not hit upstream fetch');
    });

    store.upsertBaseCustomers([
      {
        id: 'gid://shopify/Customer/156',
        firstName: 'Tax',
        lastName: 'Dedicated',
        displayName: 'Tax Dedicated',
        email: 'tax-dedicated@example.com',
        legacyResourceId: '156',
        locale: 'en',
        note: null,
        canDelete: true,
        verifiedEmail: true,
        taxExempt: false,
        taxExemptions: [],
        state: 'DISABLED',
        tags: ['tax'],
        numberOfOrders: 0,
        amountSpent: null,
        defaultEmailAddress: { emailAddress: 'tax-dedicated@example.com' },
        defaultPhoneNumber: null,
        defaultAddress: null,
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-01T00:00:00.000Z',
      },
    ]);

    const app = createApp(snapshotConfig).callback();
    const taxMutationSlice = `
      customer { id taxExempt taxExemptions }
      userErrors { field message }
    `;
    const addResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation AddCustomerTaxExemptions($customerId: ID!, $taxExemptions: [TaxExemption!]!) {
          customerAddTaxExemptions(customerId: $customerId, taxExemptions: $taxExemptions) {
            ${taxMutationSlice}
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/156',
          taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'US_CA_RESELLER_EXEMPTION'],
        },
      });

    expect(addResponse.status).toBe(200);
    expect(addResponse.body.data.customerAddTaxExemptions).toEqual({
      customer: {
        id: 'gid://shopify/Customer/156',
        taxExempt: false,
        taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'US_CA_RESELLER_EXEMPTION'],
      },
      userErrors: [],
    });

    const duplicateAddResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DuplicateAddCustomerTaxExemptions($customerId: ID!, $taxExemptions: [TaxExemption!]!) {
          customerAddTaxExemptions(customerId: $customerId, taxExemptions: $taxExemptions) {
            ${taxMutationSlice}
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/156',
          taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'CA_BC_RESELLER_EXEMPTION'],
        },
      });
    expect(duplicateAddResponse.body.data.customerAddTaxExemptions.customer.taxExemptions).toEqual([
      'CA_BC_RESELLER_EXEMPTION',
      'US_CA_RESELLER_EXEMPTION',
    ]);

    const readAfterAddResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query CustomerTaxExemptionRead($id: ID!) {
          customer(id: $id) { id taxExemptions }
          customerByIdentifier(identifier: { id: $id }) { id taxExemptions }
          customers(first: 5, query: "email:tax-dedicated@example.com") {
            nodes { id taxExemptions }
          }
          customersCount { count precision }
        }`,
        variables: { id: 'gid://shopify/Customer/156' },
      });
    expect(readAfterAddResponse.body.data).toEqual({
      customer: {
        id: 'gid://shopify/Customer/156',
        taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'US_CA_RESELLER_EXEMPTION'],
      },
      customerByIdentifier: {
        id: 'gid://shopify/Customer/156',
        taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'US_CA_RESELLER_EXEMPTION'],
      },
      customers: {
        nodes: [
          {
            id: 'gid://shopify/Customer/156',
            taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'US_CA_RESELLER_EXEMPTION'],
          },
        ],
      },
      customersCount: {
        count: 1,
        precision: 'EXACT',
      },
    });

    const removeResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation RemoveCustomerTaxExemptions($customerId: ID!, $taxExemptions: [TaxExemption!]!) {
          customerRemoveTaxExemptions(customerId: $customerId, taxExemptions: $taxExemptions) {
            ${taxMutationSlice}
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/156',
          taxExemptions: ['US_CA_RESELLER_EXEMPTION'],
        },
      });
    expect(removeResponse.body.data.customerRemoveTaxExemptions.customer.taxExemptions).toEqual([
      'CA_BC_RESELLER_EXEMPTION',
    ]);

    const noOpRemoveResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation NoopRemoveCustomerTaxExemptions($customerId: ID!, $taxExemptions: [TaxExemption!]!) {
          customerRemoveTaxExemptions(customerId: $customerId, taxExemptions: $taxExemptions) {
            ${taxMutationSlice}
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/156',
          taxExemptions: ['US_CA_RESELLER_EXEMPTION'],
        },
      });
    expect(noOpRemoveResponse.body.data.customerRemoveTaxExemptions.customer.taxExemptions).toEqual([
      'CA_BC_RESELLER_EXEMPTION',
    ]);

    const replaceResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation ReplaceCustomerTaxExemptions($customerId: ID!, $taxExemptions: [TaxExemption!]!) {
          customerReplaceTaxExemptions(customerId: $customerId, taxExemptions: $taxExemptions) {
            ${taxMutationSlice}
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/156',
          taxExemptions: ['EU_REVERSE_CHARGE_EXEMPTION_RULE'],
        },
      });
    expect(replaceResponse.body.data.customerReplaceTaxExemptions.customer.taxExemptions).toEqual([
      'EU_REVERSE_CHARGE_EXEMPTION_RULE',
    ]);

    const duplicateReplaceResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DuplicateReplaceCustomerTaxExemptions($customerId: ID!, $taxExemptions: [TaxExemption!]!) {
          customerReplaceTaxExemptions(customerId: $customerId, taxExemptions: $taxExemptions) {
            ${taxMutationSlice}
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/156',
          taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'CA_BC_RESELLER_EXEMPTION'],
        },
      });
    expect(duplicateReplaceResponse.body.data.customerReplaceTaxExemptions.customer.taxExemptions).toEqual([
      'CA_BC_RESELLER_EXEMPTION',
    ]);

    const emptyReplaceResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation EmptyReplaceCustomerTaxExemptions($customerId: ID!, $taxExemptions: [TaxExemption!]!) {
          customerReplaceTaxExemptions(customerId: $customerId, taxExemptions: $taxExemptions) {
            ${taxMutationSlice}
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/156',
          taxExemptions: [],
        },
      });
    expect(emptyReplaceResponse.body.data.customerReplaceTaxExemptions.customer.taxExemptions).toEqual([]);

    for (const root of ['customerAddTaxExemptions', 'customerRemoveTaxExemptions', 'customerReplaceTaxExemptions']) {
      const unknownResponse = await request(app)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query: `mutation UnknownCustomerTaxExemptions($customerId: ID!, $taxExemptions: [TaxExemption!]!) {
            ${root}(customerId: $customerId, taxExemptions: $taxExemptions) {
              ${taxMutationSlice}
            }
          }`,
          variables: {
            customerId: 'gid://shopify/Customer/999999999999999',
            taxExemptions: ['CA_BC_RESELLER_EXEMPTION'],
          },
        });

      expect(unknownResponse.body.data[root]).toEqual({
        customer: null,
        userErrors: [{ field: ['customerId'], message: 'Customer does not exist.' }],
      });
    }

    const invalidEnumResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InvalidCustomerTaxExemptions($customerId: ID!, $taxExemptions: [TaxExemption!]!) {
          customerAddTaxExemptions(customerId: $customerId, taxExemptions: $taxExemptions) {
            ${taxMutationSlice}
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/156',
          taxExemptions: ['NOT_A_TAX_EXEMPTION'],
        },
      });
    expect(invalidEnumResponse.body.data).toBeUndefined();
    expect(invalidEnumResponse.body.errors).toEqual([
      expect.objectContaining({
        message: expect.stringContaining(
          'Variable $taxExemptions of type [TaxExemption!]! was provided invalid value for 0',
        ),
        extensions: expect.objectContaining({
          code: 'INVALID_VARIABLE',
          value: ['NOT_A_TAX_EXEMPTION'],
          problems: [
            expect.objectContaining({
              path: [0],
              explanation: expect.stringContaining('Expected "NOT_A_TAX_EXEMPTION" to be one of:'),
            }),
          ],
        }),
      }),
    ]);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string | null }) => entry.operationName)).toContain(
      'customerAddTaxExemptions',
    );
    expect(logResponse.body.entries.at(0).requestBody.variables).toEqual({
      customerId: 'gid://shopify/Customer/156',
      taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'US_CA_RESELLER_EXEMPTION'],
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

    const updateDeletedResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UpdateDeletedCustomer($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: { input: { id: 'gid://shopify/Customer/402', firstName: 'AfterDelete' } },
      });
    expect(updateDeletedResponse.body.data.customerUpdate).toEqual({
      customer: null,
      userErrors: [{ field: ['id'], message: 'Customer does not exist' }],
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

  it('matches captured CustomerInput inline consent create semantics and rejects inline consent updates', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customer input inline consent should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InlineConsentCreate($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id
              email
              defaultEmailAddress {
                emailAddress
                marketingState
                marketingOptInLevel
                marketingUpdatedAt
              }
              defaultPhoneNumber {
                phoneNumber
                marketingState
                marketingOptInLevel
                marketingUpdatedAt
                marketingCollectedFrom
              }
              emailMarketingConsent {
                marketingState
                marketingOptInLevel
                consentUpdatedAt
              }
              smsMarketingConsent {
                marketingState
                marketingOptInLevel
                consentUpdatedAt
                consentCollectedFrom
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            email: 'inline-consent@example.com',
            phone: '+14155550125',
            firstName: 'Inline',
            lastName: 'Consent',
            emailMarketingConsent: {
              marketingState: 'SUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
              consentUpdatedAt: '2026-04-25T01:00:00Z',
            },
            smsMarketingConsent: {
              marketingState: 'SUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
              consentUpdatedAt: '2026-04-25T01:05:00Z',
            },
          },
        },
      });

    expect(createResponse.status).toBe(200);
    const customerId = createResponse.body.data.customerCreate.customer.id;
    expect(createResponse.body.data.customerCreate).toEqual({
      customer: {
        id: customerId,
        email: 'inline-consent@example.com',
        defaultEmailAddress: {
          emailAddress: 'inline-consent@example.com',
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          marketingUpdatedAt: '2026-04-25T01:00:00Z',
        },
        defaultPhoneNumber: {
          phoneNumber: '+14155550125',
          marketingState: 'SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
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
          marketingOptInLevel: 'SINGLE_OPT_IN',
          consentUpdatedAt: '2026-04-25T01:05:00Z',
          consentCollectedFrom: 'OTHER',
        },
      },
      userErrors: [],
    });

    const updateEmailResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InlineEmailConsentUpdate($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            id: customerId,
            emailMarketingConsent: {
              marketingState: 'UNSUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
              consentUpdatedAt: '2026-04-25T02:00:00Z',
            },
          },
        },
      });

    expect(updateEmailResponse.status).toBe(200);
    expect(updateEmailResponse.body.data.customerUpdate).toEqual({
      customer: null,
      userErrors: [
        {
          field: ['emailMarketingConsent'],
          message:
            'To update emailMarketingConsent, please use the customerEmailMarketingConsentUpdate Mutation instead',
        },
      ],
    });

    const updateSmsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InlineSmsConsentUpdate($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            id: customerId,
            smsMarketingConsent: {
              marketingState: 'UNSUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
              consentUpdatedAt: '2026-04-25T02:05:00Z',
            },
          },
        },
      });

    expect(updateSmsResponse.status).toBe(200);
    expect(updateSmsResponse.body.data.customerUpdate).toEqual({
      customer: null,
      userErrors: [
        {
          field: ['smsMarketingConsent'],
          message: 'To update smsMarketingConsent, please use the customerSmsMarketingConsentUpdate Mutation instead',
        },
      ],
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query InlineConsentRead($id: ID!, $identifier: CustomerIdentifierInput!) {
          customer(id: $id) {
            defaultEmailAddress { marketingState marketingUpdatedAt }
            defaultPhoneNumber { marketingState marketingUpdatedAt marketingCollectedFrom }
          }
          customerByIdentifier(identifier: $identifier) {
            emailMarketingConsent { marketingState consentUpdatedAt }
            smsMarketingConsent { marketingState consentUpdatedAt consentCollectedFrom }
          }
        }`,
        variables: {
          id: customerId,
          identifier: { id: customerId },
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data).toEqual({
      customer: {
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
      customerByIdentifier: {
        emailMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          consentUpdatedAt: '2026-04-25T01:00:00Z',
        },
        smsMarketingConsent: {
          marketingState: 'SUBSCRIBED',
          consentUpdatedAt: '2026-04-25T01:05:00Z',
          consentCollectedFrom: 'OTHER',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors captured consent input variable validation without mutating customer state', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid consent inputs should not hit upstream fetch');
    });

    store.upsertBaseCustomers([makeConsentCustomer()]);
    const app = createApp(snapshotConfig).callback();
    const cases = [
      {
        name: 'missing email consent payload',
        query: `mutation EmailConsent($input: CustomerEmailMarketingConsentUpdateInput!) {
          customerEmailMarketingConsentUpdate(input: $input) {
            customer { id }
            userErrors { field message code }
          }
        }`,
        variables: { input: { customerId: 'gid://shopify/Customer/403' } },
        message:
          'Variable $input of type CustomerEmailMarketingConsentUpdateInput! was provided invalid value for emailMarketingConsent (Expected value to not be null)',
        path: ['emailMarketingConsent'],
        explanation: 'Expected value to not be null',
      },
      {
        name: 'null SMS marketing state',
        query: `mutation SmsConsent($input: CustomerSmsMarketingConsentUpdateInput!) {
          customerSmsMarketingConsentUpdate(input: $input) {
            customer { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/403',
            smsMarketingConsent: {
              marketingState: null,
              marketingOptInLevel: 'SINGLE_OPT_IN',
            },
          },
        },
        message:
          'Variable $input of type CustomerSmsMarketingConsentUpdateInput! was provided invalid value for smsMarketingConsent.marketingState (Expected value to not be null)',
        path: ['smsMarketingConsent', 'marketingState'],
        explanation: 'Expected value to not be null',
      },
      {
        name: 'invalid email opt-in enum',
        query: `mutation EmailConsent($input: CustomerEmailMarketingConsentUpdateInput!) {
          customerEmailMarketingConsentUpdate(input: $input) {
            customer { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/403',
            emailMarketingConsent: {
              marketingState: 'SUBSCRIBED',
              marketingOptInLevel: 'BOGUS',
            },
          },
        },
        message:
          'Variable $input of type CustomerEmailMarketingConsentUpdateInput! was provided invalid value for emailMarketingConsent.marketingOptInLevel (Expected "BOGUS" to be one of: SINGLE_OPT_IN, CONFIRMED_OPT_IN, UNKNOWN)',
        path: ['emailMarketingConsent', 'marketingOptInLevel'],
        explanation: 'Expected "BOGUS" to be one of: SINGLE_OPT_IN, CONFIRMED_OPT_IN, UNKNOWN',
      },
      {
        name: 'invalid SMS timestamp',
        query: `mutation SmsConsent($input: CustomerSmsMarketingConsentUpdateInput!) {
          customerSmsMarketingConsentUpdate(input: $input) {
            customer { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/403',
            smsMarketingConsent: {
              marketingState: 'SUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
              consentUpdatedAt: 'not-a-date',
            },
          },
        },
        message:
          "Variable $input of type CustomerSmsMarketingConsentUpdateInput! was provided invalid value for smsMarketingConsent.consentUpdatedAt (invalid DateTime 'not-a-date')",
        path: ['smsMarketingConsent', 'consentUpdatedAt'],
        explanation: "invalid DateTime 'not-a-date'",
      },
      {
        name: 'unsupported consentCollectedFrom input field',
        query: `mutation SmsConsent($input: CustomerSmsMarketingConsentUpdateInput!) {
          customerSmsMarketingConsentUpdate(input: $input) {
            customer { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/403',
            smsMarketingConsent: {
              marketingState: 'SUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
              consentCollectedFrom: 'SHOPIFY',
            },
          },
        },
        message:
          'Variable $input of type CustomerSmsMarketingConsentUpdateInput! was provided invalid value for smsMarketingConsent.consentCollectedFrom (Field is not defined on CustomerSmsMarketingConsentInput)',
        path: ['smsMarketingConsent', 'consentCollectedFrom'],
        explanation: 'Field is not defined on CustomerSmsMarketingConsentInput',
      },
    ];

    for (const testCase of cases) {
      const response = await request(app)
        .post('/admin/api/2026-04/graphql.json')
        .send({ query: testCase.query, variables: testCase.variables });

      expect(response.status, testCase.name).toBe(200);
      expect(response.body.data, testCase.name).toBeUndefined();
      expect(response.body.errors, testCase.name).toMatchObject([
        {
          message: testCase.message,
          extensions: {
            code: 'INVALID_VARIABLE',
            problems: [
              {
                path: testCase.path,
                explanation: testCase.explanation,
              },
            ],
          },
        },
      ]);
    }

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ConsentReadback($id: ID!) {
          customer(id: $id) {
            id
            defaultEmailAddress { marketingState marketingOptInLevel marketingUpdatedAt }
            defaultPhoneNumber { marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom }
          }
        }`,
        variables: { id: 'gid://shopify/Customer/403' },
      });

    expect(readResponse.body.data.customer).toEqual({
      id: 'gid://shopify/Customer/403',
      defaultEmailAddress: {
        marketingState: 'NOT_SUBSCRIBED',
        marketingOptInLevel: 'SINGLE_OPT_IN',
        marketingUpdatedAt: null,
      },
      defaultPhoneNumber: {
        marketingState: 'NOT_SUBSCRIBED',
        marketingOptInLevel: 'SINGLE_OPT_IN',
        marketingUpdatedAt: null,
        marketingCollectedFrom: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors captured consent validation guardrails and pending transitions', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customer consent validation should stay local');
    });

    store.upsertBaseCustomers([
      makeConsentCustomer(),
      makeConsentCustomer({
        id: 'gid://shopify/Customer/404',
        email: 'no-phone@example.com',
        defaultEmailAddress: {
          emailAddress: 'no-phone@example.com',
          marketingState: 'NOT_SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          marketingUpdatedAt: null,
        },
        defaultPhoneNumber: null,
        smsMarketingConsent: null,
      }),
    ]);

    const app = createApp(snapshotConfig).callback();
    const pendingSingleResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation EmailPendingSingle($input: CustomerEmailMarketingConsentUpdateInput!) {
          customerEmailMarketingConsentUpdate(input: $input) {
            customer {
              id
              defaultEmailAddress { marketingState marketingOptInLevel marketingUpdatedAt }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/403',
            emailMarketingConsent: {
              marketingState: 'PENDING',
              marketingOptInLevel: 'SINGLE_OPT_IN',
              consentUpdatedAt: '2026-04-25T04:00:00Z',
            },
          },
        },
      });

    expect(pendingSingleResponse.status).toBe(200);
    expect(pendingSingleResponse.body.data.customerEmailMarketingConsentUpdate).toEqual({
      customer: {
        id: 'gid://shopify/Customer/403',
        defaultEmailAddress: {
          marketingState: 'NOT_SUBSCRIBED',
          marketingOptInLevel: 'SINGLE_OPT_IN',
          marketingUpdatedAt: null,
        },
      },
      userErrors: [
        {
          field: ['input', 'emailMarketingConsent', 'marketingOptInLevel'],
          message: 'Marketing opt in level must be confirmed opt-in for pending consent state',
          code: 'INVALID',
        },
      ],
    });

    const emailPendingResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation EmailPendingConfirmed($input: CustomerEmailMarketingConsentUpdateInput!) {
          customerEmailMarketingConsentUpdate(input: $input) {
            customer {
              id
              defaultEmailAddress { marketingState marketingOptInLevel marketingUpdatedAt }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/403',
            emailMarketingConsent: {
              marketingState: 'PENDING',
              marketingOptInLevel: 'CONFIRMED_OPT_IN',
              consentUpdatedAt: '2026-04-25T04:01:00Z',
            },
          },
        },
      });

    expect(emailPendingResponse.body.data.customerEmailMarketingConsentUpdate).toEqual({
      customer: {
        id: 'gid://shopify/Customer/403',
        defaultEmailAddress: {
          marketingState: 'PENDING',
          marketingOptInLevel: 'CONFIRMED_OPT_IN',
          marketingUpdatedAt: '2026-04-25T04:01:00Z',
        },
      },
      userErrors: [],
    });

    const smsFutureResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation SmsFuture($input: CustomerSmsMarketingConsentUpdateInput!) {
          customerSmsMarketingConsentUpdate(input: $input) {
            customer { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/403',
            smsMarketingConsent: {
              marketingState: 'SUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
              consentUpdatedAt: '2999-01-01T00:00:00Z',
            },
          },
        },
      });

    expect(smsFutureResponse.body.data.customerSmsMarketingConsentUpdate).toEqual({
      customer: null,
      userErrors: [
        {
          field: ['input', 'smsMarketingConsent', 'consentUpdatedAt'],
          message: 'Consent updated at must not be in the future',
          code: 'INVALID',
        },
      ],
    });

    const smsNoPhoneResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation SmsNoPhone($input: CustomerSmsMarketingConsentUpdateInput!) {
          customerSmsMarketingConsentUpdate(input: $input) {
            customer { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/404',
            smsMarketingConsent: {
              marketingState: 'SUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
              consentUpdatedAt: '2026-04-25T04:02:00Z',
            },
          },
        },
      });

    expect(smsNoPhoneResponse.body.data.customerSmsMarketingConsentUpdate).toEqual({
      customer: null,
      userErrors: [
        {
          field: ['input', 'smsMarketingConsent'],
          message: 'A phone number is required to set the SMS consent state.',
          code: 'INVALID',
        },
      ],
    });

    const smsPendingResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation SmsPendingConfirmed($input: CustomerSmsMarketingConsentUpdateInput!) {
          customerSmsMarketingConsentUpdate(input: $input) {
            customer {
              id
              defaultPhoneNumber { marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/403',
            smsMarketingConsent: {
              marketingState: 'PENDING',
              marketingOptInLevel: 'CONFIRMED_OPT_IN',
              consentUpdatedAt: '2026-04-25T04:03:00Z',
            },
          },
        },
      });

    expect(smsPendingResponse.body.data.customerSmsMarketingConsentUpdate).toEqual({
      customer: {
        id: 'gid://shopify/Customer/403',
        defaultPhoneNumber: {
          marketingState: 'PENDING',
          marketingOptInLevel: 'CONFIRMED_OPT_IN',
          marketingUpdatedAt: '2026-04-25T04:03:00Z',
          marketingCollectedFrom: 'OTHER',
        },
      },
      userErrors: [],
    });

    const notSubscribedResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation EmailNotSubscribed($input: CustomerEmailMarketingConsentUpdateInput!) {
          customerEmailMarketingConsentUpdate(input: $input) {
            customer { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            customerId: 'gid://shopify/Customer/403',
            emailMarketingConsent: {
              marketingState: 'NOT_SUBSCRIBED',
              marketingOptInLevel: 'SINGLE_OPT_IN',
              consentUpdatedAt: '2026-04-25T04:04:00Z',
            },
          },
        },
      });

    expect(notSubscribedResponse.body.data).toEqual({ customerEmailMarketingConsentUpdate: null });
    expect(notSubscribedResponse.body.errors).toMatchObject([
      {
        message: 'Cannot specify NOT_SUBSCRIBED as a marketing state input',
        extensions: { code: 'INVALID' },
        path: ['customerEmailMarketingConsentUpdate'],
      },
    ]);

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ConsentReadback($id: ID!) {
          customer(id: $id) {
            id
            defaultEmailAddress { marketingState marketingOptInLevel marketingUpdatedAt }
            defaultPhoneNumber { marketingState marketingOptInLevel marketingUpdatedAt marketingCollectedFrom }
          }
        }`,
        variables: { id: 'gid://shopify/Customer/403' },
      });

    expect(readResponse.body.data.customer).toEqual({
      id: 'gid://shopify/Customer/403',
      defaultEmailAddress: {
        marketingState: 'PENDING',
        marketingOptInLevel: 'CONFIRMED_OPT_IN',
        marketingUpdatedAt: '2026-04-25T04:01:00Z',
      },
      defaultPhoneNumber: {
        marketingState: 'PENDING',
        marketingOptInLevel: 'CONFIRMED_OPT_IN',
        marketingUpdatedAt: '2026-04-25T04:03:00Z',
        marketingCollectedFrom: 'OTHER',
      },
    });
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
        instrument: null,
        revokedAt: null,
        subscriptionContracts: [],
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

  it('stages customer data erasure request and cancel intents locally with audit state', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customer data erasure mutations should not hit upstream fetch');
    });

    store.upsertBaseCustomers([makeConsentCustomer({ id: 'gid://shopify/Customer/7007' })]);

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CustomerDataErasure($customerId: ID!) {
          request: customerRequestDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
          cancel: customerCancelDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }`,
        variables: { customerId: 'gid://shopify/Customer/7007' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        request: {
          customerId: 'gid://shopify/Customer/7007',
          userErrors: [],
        },
        cancel: {
          customerId: 'gid://shopify/Customer/7007',
          userErrors: [],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();

    const log = store.getLog();
    expect(log).toHaveLength(1);
    expect(log[0]).toMatchObject({
      operationName: 'customerRequestDataErasure',
      path: '/admin/api/2025-01/graphql.json',
      status: 'staged',
      interpreted: {
        operationType: 'mutation',
        operationName: 'CustomerDataErasure',
        rootFields: ['customerRequestDataErasure', 'customerCancelDataErasure'],
        primaryRootField: 'customerRequestDataErasure',
        capability: {
          domain: 'customers',
          execution: 'stage-locally',
        },
      },
      requestBody: {
        variables: { customerId: 'gid://shopify/Customer/7007' },
      },
    });

    const erasureRequest = store.getState().stagedState.customerDataErasureRequests['gid://shopify/Customer/7007'];
    expect(erasureRequest).toMatchObject({
      customerId: 'gid://shopify/Customer/7007',
    });
    expect(erasureRequest?.requestedAt).toBeTruthy();
    expect(erasureRequest?.canceledAt).toBeTruthy();

    const repeatCancelResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation RepeatCustomerDataErasureCancel($customerId: ID!) {
          customerCancelDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }`,
        variables: { customerId: 'gid://shopify/Customer/7007' },
      });

    expect(repeatCancelResponse.status).toBe(200);
    expect(repeatCancelResponse.body).toEqual({
      data: {
        customerCancelDataErasure: {
          customerId: null,
          userErrors: [
            {
              field: ['customerId'],
              message: "Customer's data is not scheduled for erasure",
              code: 'NOT_BEING_ERASED',
            },
          ],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();

    const missingResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation MissingCustomerDataErasure($customerId: ID!) {
          customerRequestDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }`,
        variables: { customerId: 'gid://shopify/Customer/999999999999999' },
      });

    expect(missingResponse.status).toBe(200);
    expect(missingResponse.body).toEqual({
      data: {
        customerRequestDataErasure: {
          customerId: null,
          userErrors: [
            {
              field: ['customerId'],
              message: 'Customer does not exist',
              code: 'DOES_NOT_EXIST',
            },
          ],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
