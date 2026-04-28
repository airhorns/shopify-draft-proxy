import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { CustomerRecord } from '../../src/state/types.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makePaymentCustomer(id: string, email: string): CustomerRecord {
  const legacyResourceId = id.split('/').at(-1) ?? id;
  return {
    id,
    firstName: 'Payment',
    lastName: 'Customer',
    displayName: 'Payment Customer',
    email,
    legacyResourceId,
    locale: 'en',
    note: null,
    canDelete: true,
    verifiedEmail: true,
    taxExempt: false,
    state: 'ENABLED',
    tags: [],
    numberOfOrders: 0,
    amountSpent: { amount: '0.0', currencyCode: 'USD' },
    defaultEmailAddress: { emailAddress: email },
    defaultPhoneNumber: null,
    defaultAddress: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
  };
}

describe('customer payment method and reminder staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages payment method lifecycle roots and reminders locally without storing sensitive inputs', async () => {
    store.upsertBaseCustomers([
      makePaymentCustomer('gid://shopify/Customer/8801', 'payment-one@example.com'),
      makePaymentCustomer('gid://shopify/Customer/8802', 'payment-two@example.com'),
    ]);
    store.upsertBaseCustomerPaymentMethods([
      {
        id: 'gid://shopify/CustomerPaymentMethod/base-card',
        customerId: 'gid://shopify/Customer/8801',
        revokedAt: null,
        revokedReason: null,
        instrument: {
          typeName: 'CustomerCreditCard',
          data: {
            __typename: 'CustomerCreditCard',
            brand: 'VISA',
            lastDigits: '4242',
            maskedNumber: '**** **** **** 4242',
          },
        },
        subscriptionContracts: [],
      },
      {
        id: 'gid://shopify/CustomerPaymentMethod/base-paypal',
        customerId: 'gid://shopify/Customer/8801',
        revokedAt: null,
        revokedReason: null,
        instrument: {
          typeName: 'CustomerPaypalBillingAgreement',
          data: {
            __typename: 'CustomerPaypalBillingAgreement',
            paypalAccountEmail: 'billing@example.com',
            inactive: false,
          },
        },
        subscriptionContracts: [],
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customer payment method staging should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const mutationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation PaymentMethodLifecycle(
          $customerId: ID!
          $targetCustomerId: ID!
          $billingAddress: MailingAddressInput!
          $sessionId: String!
          $remoteReference: CustomerPaymentMethodRemoteInput!
          $paymentScheduleId: ID!
        ) {
          cardCreate: customerPaymentMethodCreditCardCreate(
            customerId: $customerId
            billingAddress: $billingAddress
            sessionId: $sessionId
          ) {
            customerPaymentMethod {
              id
              instrument {
                __typename
                ... on CustomerCreditCard { lastDigits maskedNumber }
              }
              customer { id email }
            }
            processing
            userErrors { field message }
          }
          cardUpdate: customerPaymentMethodCreditCardUpdate(
            id: "gid://shopify/CustomerPaymentMethod/base-card"
            billingAddress: $billingAddress
            sessionId: $sessionId
          ) {
            customerPaymentMethod {
              id
              instrument {
                __typename
                ... on CustomerCreditCard { lastDigits maskedNumber }
              }
            }
            processing
            userErrors { field message }
          }
          remoteCreate: customerPaymentMethodRemoteCreate(
            customerId: $customerId
            remoteReference: $remoteReference
          ) {
            customerPaymentMethod { id instrument { __typename } }
            userErrors { field message }
          }
          paypalCreate: customerPaymentMethodPaypalBillingAgreementCreate(
            customerId: $customerId
            billingAddress: $billingAddress
            billingAgreementId: "B-sensitive-agreement"
            inactive: true
          ) {
            customerPaymentMethod {
              id
              instrument {
                __typename
                ... on CustomerPaypalBillingAgreement { paypalAccountEmail inactive }
              }
            }
            userErrors { field message }
          }
          paypalUpdate: customerPaymentMethodPaypalBillingAgreementUpdate(
            id: "gid://shopify/CustomerPaymentMethod/base-paypal"
            billingAddress: $billingAddress
          ) {
            customerPaymentMethod {
              id
              instrument {
                __typename
                ... on CustomerPaypalBillingAgreement { paypalAccountEmail inactive }
              }
            }
            userErrors { field message }
          }
          duplication: customerPaymentMethodGetDuplicationData(
            customerPaymentMethodId: "gid://shopify/CustomerPaymentMethod/base-card"
            targetShopId: "gid://shopify/Shop/target"
            targetCustomerId: $targetCustomerId
          ) {
            encryptedDuplicationData
            userErrors { field message }
          }
          updateUrl: customerPaymentMethodGetUpdateUrl(
            customerPaymentMethodId: "gid://shopify/CustomerPaymentMethod/base-card"
          ) {
            updatePaymentMethodUrl
            userErrors { field message }
          }
          revoke: customerPaymentMethodRevoke(
            customerPaymentMethodId: "gid://shopify/CustomerPaymentMethod/base-card"
          ) {
            revokedCustomerPaymentMethodId
            userErrors { field message }
          }
          reminder: paymentReminderSend(paymentScheduleId: $paymentScheduleId) {
            success
            userErrors { field message code }
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/8801',
          targetCustomerId: 'gid://shopify/Customer/8802',
          billingAddress: {
            firstName: 'Sensitive',
            lastName: 'Billing',
            address1: '1 Secret St',
            countryCode: 'US',
          },
          sessionId: 'csn_sensitive_session',
          remoteReference: {
            stripePaymentMethod: {
              customerId: 'cus_sensitive',
              paymentMethodId: 'pm_sensitive',
            },
          },
          paymentScheduleId: 'gid://shopify/PaymentSchedule/123',
        },
      });

    expect(mutationResponse.status).toBe(200);
    expect(mutationResponse.body.data).toMatchObject({
      cardCreate: {
        customerPaymentMethod: {
          id: expect.stringMatching(/^gid:\/\/shopify\/CustomerPaymentMethod\/\d+$/),
          instrument: {
            __typename: 'CustomerCreditCard',
            lastDigits: null,
            maskedNumber: null,
          },
          customer: {
            id: 'gid://shopify/Customer/8801',
            email: 'payment-one@example.com',
          },
        },
        processing: false,
        userErrors: [],
      },
      cardUpdate: {
        customerPaymentMethod: {
          id: 'gid://shopify/CustomerPaymentMethod/base-card',
          instrument: {
            __typename: 'CustomerCreditCard',
            lastDigits: null,
            maskedNumber: null,
          },
        },
        processing: false,
        userErrors: [],
      },
      remoteCreate: {
        customerPaymentMethod: {
          id: expect.stringMatching(/^gid:\/\/shopify\/CustomerPaymentMethod\/\d+$/),
          instrument: null,
        },
        userErrors: [],
      },
      paypalCreate: {
        customerPaymentMethod: {
          id: expect.stringMatching(/^gid:\/\/shopify\/CustomerPaymentMethod\/\d+$/),
          instrument: {
            __typename: 'CustomerPaypalBillingAgreement',
            paypalAccountEmail: null,
            inactive: true,
          },
        },
        userErrors: [],
      },
      paypalUpdate: {
        customerPaymentMethod: {
          id: 'gid://shopify/CustomerPaymentMethod/base-paypal',
          instrument: {
            __typename: 'CustomerPaypalBillingAgreement',
            paypalAccountEmail: null,
            inactive: false,
          },
        },
        userErrors: [],
      },
      updateUrl: {
        updatePaymentMethodUrl:
          'https://shopify-draft-proxy.local/customer-payment-methods/base-card/update?token=local-only',
        userErrors: [],
      },
      revoke: {
        revokedCustomerPaymentMethodId: 'gid://shopify/CustomerPaymentMethod/base-card',
        userErrors: [],
      },
      reminder: {
        success: true,
        userErrors: [],
      },
    });
    expect(mutationResponse.body.data.duplication.encryptedDuplicationData).toMatch(
      /^shopify-draft-proxy:customer-payment-method-duplication:/u,
    );

    const duplicateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DuplicatePaymentMethod($customerId: ID!, $billingAddress: MailingAddressInput!, $data: String!) {
          customerPaymentMethodCreateFromDuplicationData(
            customerId: $customerId
            billingAddress: $billingAddress
            encryptedDuplicationData: $data
          ) {
            customerPaymentMethod {
              id
              customer { id }
              instrument {
                __typename
                ... on CustomerCreditCard { lastDigits maskedNumber }
              }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/8802',
          billingAddress: { countryCode: 'US' },
          data: mutationResponse.body.data.duplication.encryptedDuplicationData,
        },
      });

    expect(duplicateResponse.status).toBe(200);
    expect(duplicateResponse.body.data.customerPaymentMethodCreateFromDuplicationData).toEqual({
      customerPaymentMethod: {
        id: expect.stringMatching(/^gid:\/\/shopify\/CustomerPaymentMethod\/\d+$/),
        customer: { id: 'gid://shopify/Customer/8802' },
        instrument: {
          __typename: 'CustomerCreditCard',
          lastDigits: null,
          maskedNumber: null,
        },
      },
      userErrors: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query PaymentMethodReadback($customerId: ID!, $targetCustomerId: ID!) {
          hiddenRevoked: customerPaymentMethod(id: "gid://shopify/CustomerPaymentMethod/base-card") { id }
          shownRevoked: customerPaymentMethod(id: "gid://shopify/CustomerPaymentMethod/base-card", showRevoked: true) {
            id
            revokedAt
            revokedReason
          }
          source: customer(id: $customerId) {
            paymentMethods(first: 10, showRevoked: true) { nodes { id revokedAt } }
          }
          target: customer(id: $targetCustomerId) {
            paymentMethods(first: 10) {
              nodes {
                id
                instrument {
                  __typename
                  ... on CustomerCreditCard { lastDigits maskedNumber }
                }
              }
            }
          }
        }`,
        variables: {
          customerId: 'gid://shopify/Customer/8801',
          targetCustomerId: 'gid://shopify/Customer/8802',
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.hiddenRevoked).toBeNull();
    expect(readResponse.body.data.shownRevoked).toEqual({
      id: 'gid://shopify/CustomerPaymentMethod/base-card',
      revokedAt: expect.any(String),
      revokedReason: 'CUSTOMER_REVOKED',
    });
    expect(readResponse.body.data.source.paymentMethods.nodes).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: 'gid://shopify/CustomerPaymentMethod/base-card', revokedAt: expect.any(String) }),
        expect.objectContaining({ id: 'gid://shopify/CustomerPaymentMethod/base-paypal', revokedAt: null }),
      ]),
    );
    expect(readResponse.body.data.target.paymentMethods.nodes).toEqual([
      {
        id: duplicateResponse.body.data.customerPaymentMethodCreateFromDuplicationData.customerPaymentMethod.id,
        instrument: {
          __typename: 'CustomerCreditCard',
          lastDigits: null,
          maskedNumber: null,
        },
      },
    ]);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.status).toBe(200);
    expect(logResponse.body.entries).toHaveLength(2);
    expect(logResponse.body.entries.every((entry: { status: string }) => entry.status === 'staged')).toBe(true);

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.status).toBe(200);
    expect(Object.values(stateResponse.body.stagedState.customerPaymentMethodUpdateUrls)).toEqual([
      expect.objectContaining({
        customerPaymentMethodId: 'gid://shopify/CustomerPaymentMethod/base-card',
        updatePaymentMethodUrl:
          'https://shopify-draft-proxy.local/customer-payment-methods/base-card/update?token=local-only',
      }),
    ]);
    expect(Object.values(stateResponse.body.stagedState.paymentReminderSends)).toEqual([
      expect.objectContaining({
        paymentScheduleId: 'gid://shopify/PaymentSchedule/123',
      }),
    ]);
    expect(JSON.stringify(stateResponse.body)).not.toContain('csn_sensitive_session');
    expect(JSON.stringify(stateResponse.body)).not.toContain('B-sensitive-agreement');
    expect(JSON.stringify(stateResponse.body)).not.toContain('pm_sensitive');
    expect(JSON.stringify(stateResponse.body)).not.toContain('1 Secret St');
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns local userErrors for unknown customers, invalid remote references, and invalid duplication data', async () => {
    store.upsertBaseCustomers([makePaymentCustomer('gid://shopify/Customer/8801', 'payment-one@example.com')]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('customer payment method validation should not hit upstream fetch');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation PaymentMethodValidation($billingAddress: MailingAddressInput!) {
          missingCustomer: customerPaymentMethodCreditCardCreate(
            customerId: "gid://shopify/Customer/missing"
            billingAddress: $billingAddress
            sessionId: "csn_sensitive"
          ) {
            customerPaymentMethod { id }
            userErrors { field message }
          }
          invalidRemote: customerPaymentMethodRemoteCreate(
            customerId: "gid://shopify/Customer/8801"
            remoteReference: {
              stripePaymentMethod: { customerId: "cus", paymentMethodId: "pm" }
              braintreePaymentMethod: { customerId: "cus", paymentMethodToken: "tok" }
            }
          ) {
            customerPaymentMethod { id }
            userErrors { field message code }
          }
          invalidDuplication: customerPaymentMethodCreateFromDuplicationData(
            customerId: "gid://shopify/Customer/8801"
            billingAddress: $billingAddress
            encryptedDuplicationData: "not-real-duplication-data"
          ) {
            customerPaymentMethod { id }
            userErrors { field message code }
          }
          missingUpdateUrl: customerPaymentMethodGetUpdateUrl(
            customerPaymentMethodId: "gid://shopify/CustomerPaymentMethod/missing"
          ) {
            updatePaymentMethodUrl
            userErrors { field message code }
          }
          invalidReminder: paymentReminderSend(paymentScheduleId: "not-a-gid") {
            success
            userErrors { field message code }
          }
        }`,
        variables: {
          billingAddress: { countryCode: 'US' },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      missingCustomer: {
        customerPaymentMethod: null,
        userErrors: [{ field: ['customerId'], message: 'Customer does not exist' }],
      },
      invalidRemote: {
        customerPaymentMethod: null,
        userErrors: [
          {
            field: ['remoteReference'],
            message: 'Exactly one remote reference is required',
            code: 'EXACTLY_ONE_REMOTE_REFERENCE_REQUIRED',
          },
        ],
      },
      invalidDuplication: {
        customerPaymentMethod: null,
        userErrors: [
          {
            field: ['encryptedDuplicationData'],
            message: 'Encrypted duplication data is invalid',
            code: 'INVALID_ENCRYPTED_DUPLICATION_DATA',
          },
        ],
      },
      missingUpdateUrl: {
        updatePaymentMethodUrl: null,
        userErrors: [
          {
            field: ['customerPaymentMethodId'],
            message: 'Customer payment method does not exist',
            code: 'PAYMENT_METHOD_DOES_NOT_EXIST',
          },
        ],
      },
      invalidReminder: {
        success: false,
        userErrors: [
          {
            field: ['paymentScheduleId'],
            message: 'Payment reminder could not be sent',
            code: 'PAYMENT_REMINDER_SEND_UNSUCCESSFUL',
          },
        ],
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
