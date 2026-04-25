import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import type { BusinessEntityRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeBusinessEntity(id: string, overrides: Partial<BusinessEntityRecord> = {}): BusinessEntityRecord {
  return {
    id,
    displayName: 'Hermes Canada',
    companyName: 'Hermes Canada Ltd.',
    primary: false,
    archived: false,
    address: {
      address1: '150 Elgin St.',
      address2: null,
      city: 'Ottawa',
      countryCode: 'CA',
      province: 'ON',
      zip: 'K2P 1L4',
    },
    shopifyPaymentsAccount: null,
    ...overrides,
  };
}

describe('business entity query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves businessEntities from snapshot state in captured order without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('businessEntities should resolve locally in snapshot mode');
    });

    store.upsertBaseBusinessEntities([
      makeBusinessEntity('gid://shopify/BusinessEntity/200', {
        displayName: 'Hermes United States',
        companyName: 'Hermes US LLC',
        address: {
          address1: '33 King St.',
          address2: 'Suite 400',
          city: 'Wilmington',
          countryCode: 'US',
          province: 'DE',
          zip: '19801',
        },
        shopifyPaymentsAccount: {
          id: 'gid://shopify/ShopifyPaymentsAccount/200',
          activated: true,
          country: 'US',
          defaultCurrency: 'USD',
          onboardable: false,
        },
      }),
      makeBusinessEntity('gid://shopify/BusinessEntity/100', {
        displayName: 'Hermes Canada',
        primary: true,
        archived: false,
      }),
      makeBusinessEntity('gid://shopify/BusinessEntity/300', {
        displayName: 'Hermes Germany',
        companyName: 'Hermes DE GmbH',
        primary: false,
        archived: true,
        address: {
          address1: 'Friedrichstrasse 1',
          address2: null,
          city: 'Berlin',
          countryCode: 'DE',
          province: null,
          zip: '10117',
        },
      }),
    ]);

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query BusinessEntitiesRead {
          businessEntities {
            id
            displayName
            companyName
            primary
            archived
            address {
              address1
              address2
              city
              countryCode
              province
              zip
            }
            shopifyPaymentsAccount {
              id
              activated
              country
              defaultCurrency
              onboardable
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        businessEntities: [
          {
            id: 'gid://shopify/BusinessEntity/200',
            displayName: 'Hermes United States',
            companyName: 'Hermes US LLC',
            primary: false,
            archived: false,
            address: {
              address1: '33 King St.',
              address2: 'Suite 400',
              city: 'Wilmington',
              countryCode: 'US',
              province: 'DE',
              zip: '19801',
            },
            shopifyPaymentsAccount: {
              id: 'gid://shopify/ShopifyPaymentsAccount/200',
              activated: true,
              country: 'US',
              defaultCurrency: 'USD',
              onboardable: false,
            },
          },
          {
            id: 'gid://shopify/BusinessEntity/100',
            displayName: 'Hermes Canada',
            companyName: 'Hermes Canada Ltd.',
            primary: true,
            archived: false,
            address: {
              address1: '150 Elgin St.',
              address2: null,
              city: 'Ottawa',
              countryCode: 'CA',
              province: 'ON',
              zip: 'K2P 1L4',
            },
            shopifyPaymentsAccount: null,
          },
          {
            id: 'gid://shopify/BusinessEntity/300',
            displayName: 'Hermes Germany',
            companyName: 'Hermes DE GmbH',
            primary: false,
            archived: true,
            address: {
              address1: 'Friedrichstrasse 1',
              address2: null,
              city: 'Berlin',
              countryCode: 'DE',
              province: null,
              zip: '10117',
            },
            shopifyPaymentsAccount: null,
          },
        ],
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('serves businessEntity primary fallback, known-id lookup, and unknown-id null in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('businessEntity should resolve locally in snapshot mode');
    });

    store.upsertBaseBusinessEntities([
      makeBusinessEntity('gid://shopify/BusinessEntity/200', { displayName: 'Secondary Entity' }),
      makeBusinessEntity('gid://shopify/BusinessEntity/100', {
        displayName: 'Primary Entity',
        primary: true,
      }),
    ]);

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query BusinessEntityFallbacks($knownId: ID!, $unknownId: ID!) {
          primary: businessEntity {
            id
            displayName
            primary
            archived
          }
          known: businessEntity(id: $knownId) {
            id
            displayName
            primary
          }
          unknown: businessEntity(id: $unknownId) {
            id
          }
        }`,
        variables: {
          knownId: 'gid://shopify/BusinessEntity/200',
          unknownId: 'gid://shopify/BusinessEntity/999999',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        primary: {
          id: 'gid://shopify/BusinessEntity/100',
          displayName: 'Primary Entity',
          primary: true,
          archived: false,
        },
        known: {
          id: 'gid://shopify/BusinessEntity/200',
          displayName: 'Secondary Entity',
          primary: false,
        },
        unknown: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns Shopify-like empty business entity reads when snapshot state has no entities', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('empty business entity reads should resolve locally in snapshot mode');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query EmptyBusinessEntities($id: ID!) {
          businessEntities {
            id
          }
          businessEntity {
            id
          }
          unknown: businessEntity(id: $id) {
            id
          }
        }`,
        variables: { id: 'gid://shopify/BusinessEntity/404' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        businessEntities: [],
        businessEntity: null,
        unknown: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('exposes only safe Shopify Payments account fixture fields and reports sensitive fields', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('businessEntity payment account reads should resolve locally in snapshot mode');
    });

    store.upsertBaseBusinessEntities([
      makeBusinessEntity('gid://shopify/BusinessEntity/100', {
        primary: true,
        shopifyPaymentsAccount: {
          id: 'gid://shopify/ShopifyPaymentsAccount/100',
          activated: true,
          country: 'CA',
          defaultCurrency: 'CAD',
          onboardable: false,
        },
      }),
    ]);

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query BusinessEntityPaymentSafety {
          businessEntity {
            id
            shopifyPaymentsAccount {
              id
              activated
              country
              defaultCurrency
              onboardable
              balance {
                amount
                currencyCode
              }
              bankAccounts(first: 1) {
                nodes {
                  id
                }
              }
              payoutStatementDescriptor
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      businessEntity: {
        id: 'gid://shopify/BusinessEntity/100',
        shopifyPaymentsAccount: {
          id: 'gid://shopify/ShopifyPaymentsAccount/100',
          activated: true,
          country: 'CA',
          defaultCurrency: 'CAD',
          onboardable: false,
          balance: null,
          bankAccounts: null,
          payoutStatementDescriptor: null,
        },
      },
    });
    expect(response.body.errors).toEqual([
      expect.objectContaining({
        message: expect.stringContaining('ShopifyPaymentsAccount.balance'),
        extensions: {
          code: 'UNSUPPORTED_FIELD',
          reason: 'shopify-payments-account-sensitive-field',
        },
      }),
      expect.objectContaining({
        message: expect.stringContaining('ShopifyPaymentsAccount.bankAccounts'),
        extensions: {
          code: 'UNSUPPORTED_FIELD',
          reason: 'shopify-payments-account-sensitive-field',
        },
      }),
      expect.objectContaining({
        message: expect.stringContaining('ShopifyPaymentsAccount.payoutStatementDescriptor'),
        extensions: {
          code: 'UNSUPPORTED_FIELD',
          reason: 'shopify-payments-account-sensitive-field',
        },
      }),
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
