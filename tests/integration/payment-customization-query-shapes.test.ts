import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import type { PaymentCustomizationRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function buildPaymentCustomization(overrides: Partial<PaymentCustomizationRecord> = {}): PaymentCustomizationRecord {
  const id = overrides.id ?? 'gid://shopify/PaymentCustomization/100';
  return {
    id,
    title: 'Primary payment customization',
    enabled: true,
    functionId: 'gid://shopify/ShopifyFunction/function-a',
    shopifyFunction: {
      id: 'gid://shopify/ShopifyFunction/function-a',
      title: 'Payment function A',
      apiType: 'payment_customization',
      app: {
        id: 'gid://shopify/App/1',
        title: 'Checkout tools',
      },
    },
    errorHistory: {
      errors: [
        {
          message: 'Captured function timeout',
          code: 'TIMEOUT',
        },
      ],
    },
    metafields: [
      {
        id: `gid://shopify/Metafield/${id.split('/').at(-1) ?? '100'}1`,
        paymentCustomizationId: id,
        namespace: 'settings',
        key: 'message',
        type: 'json',
        value: '{"label":"Pay now"}',
        jsonValue: { label: 'Pay now' },
        ownerType: 'PAYMENT_CUSTOMIZATION',
      },
    ],
    ...overrides,
  };
}

describe('payment customization query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns Shopify-like empty catalog and null detail in snapshot mode without upstream access', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('payment customization snapshot read should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query PaymentCustomizationEmpty($id: ID!, $query: String!) {
          paymentCustomizations(first: 2, query: $query) {
            nodes { id }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          paymentCustomization(id: $id) {
            id
            title
          }
        }`,
        variables: {
          id: 'gid://shopify/PaymentCustomization/999',
          query: 'enabled:true',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        paymentCustomizations: {
          nodes: [],
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        paymentCustomization: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('serializes normalized catalog/detail records with filters, reverse pagination, function links, and metafields', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('payment customization snapshot read should not hit upstream fetch');
    });
    const first = buildPaymentCustomization();
    const second = buildPaymentCustomization({
      id: 'gid://shopify/PaymentCustomization/200',
      title: 'Secondary payment customization',
      functionId: 'gid://shopify/ShopifyFunction/function-b',
      shopifyFunction: {
        id: 'gid://shopify/ShopifyFunction/function-b',
        title: 'Payment function B',
        apiType: 'payment_customization',
        app: {
          id: 'gid://shopify/App/2',
          title: 'Checkout filters',
        },
      },
      errorHistory: { errors: [] },
      metafields: [],
    });
    store.upsertBasePaymentCustomizations([first, second]);

    const app = createApp(config).callback();
    const firstPage = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query PaymentCustomizationCatalog($after: String) {
          page: paymentCustomizations(first: 1, after: $after, query: "enabled:true", reverse: true) {
            edges {
              cursor
              node {
                id
                title
                enabled
                functionId
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          functionFiltered: paymentCustomizations(first: 10, query: "function_id:function-a") {
            nodes { id }
          }
          idFiltered: paymentCustomizations(first: 10, query: "id:200") {
            nodes { id }
          }
          detail: paymentCustomization(id: "gid://shopify/PaymentCustomization/100") {
            __typename
            id
            legacyResourceId
            title
            enabled
            functionId
            shopifyFunction {
              id
              title
              apiType
              app { id title }
            }
            metafield(namespace: "settings", key: "message") {
              id
              namespace
              key
              type
              value
              jsonValue
              ownerType
            }
            metafields(first: 1) {
              nodes { id key value ownerType }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            missingMetafield: metafield(namespace: "settings", key: "missing") { id }
            errorHistory { errors { message code } }
          }
        }`,
        variables: { after: null },
      });

    expect(firstPage.status).toBe(200);
    expect(firstPage.body.data.page).toEqual({
      edges: [
        {
          cursor: 'cursor:gid://shopify/PaymentCustomization/200',
          node: {
            id: 'gid://shopify/PaymentCustomization/200',
            title: 'Secondary payment customization',
            enabled: true,
            functionId: 'gid://shopify/ShopifyFunction/function-b',
          },
        },
      ],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/PaymentCustomization/200',
        endCursor: 'cursor:gid://shopify/PaymentCustomization/200',
      },
    });
    expect(firstPage.body.data.functionFiltered.nodes).toEqual([{ id: 'gid://shopify/PaymentCustomization/100' }]);
    expect(firstPage.body.data.idFiltered.nodes).toEqual([{ id: 'gid://shopify/PaymentCustomization/200' }]);
    expect(firstPage.body.data.detail).toEqual({
      __typename: 'PaymentCustomization',
      id: 'gid://shopify/PaymentCustomization/100',
      legacyResourceId: '100',
      title: 'Primary payment customization',
      enabled: true,
      functionId: 'gid://shopify/ShopifyFunction/function-a',
      shopifyFunction: {
        id: 'gid://shopify/ShopifyFunction/function-a',
        title: 'Payment function A',
        apiType: 'payment_customization',
        app: {
          id: 'gid://shopify/App/1',
          title: 'Checkout tools',
        },
      },
      metafield: {
        id: 'gid://shopify/Metafield/1001',
        namespace: 'settings',
        key: 'message',
        type: 'json',
        value: '{"label":"Pay now"}',
        jsonValue: { label: 'Pay now' },
        ownerType: 'PAYMENT_CUSTOMIZATION',
      },
      metafields: {
        nodes: [
          {
            id: 'gid://shopify/Metafield/1001',
            key: 'message',
            value: '{"label":"Pay now"}',
            ownerType: 'PAYMENT_CUSTOMIZATION',
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: 'cursor:gid://shopify/Metafield/1001',
          endCursor: 'cursor:gid://shopify/Metafield/1001',
        },
      },
      missingMetafield: null,
      errorHistory: {
        errors: [
          {
            message: 'Captured function timeout',
            code: 'TIMEOUT',
          },
        ],
      },
    });

    const nextPage = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query PaymentCustomizationCatalog($after: String) {
          paymentCustomizations(first: 1, after: $after, query: "enabled:true", reverse: true) {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: { after: firstPage.body.data.page.pageInfo.endCursor },
      });

    expect(nextPage.status).toBe(200);
    expect(nextPage.body.data.paymentCustomizations).toEqual({
      nodes: [{ id: 'gid://shopify/PaymentCustomization/100' }],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: true,
        startCursor: 'cursor:gid://shopify/PaymentCustomization/100',
        endCursor: 'cursor:gid://shopify/PaymentCustomization/100',
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
