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

  it('stages payment customization lifecycle mutations locally with downstream read visibility', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('payment customization mutations should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation CreatePaymentCustomization($input: PaymentCustomizationInput!) {
          paymentCustomizationCreate(paymentCustomization: $input) {
            paymentCustomization {
              id
              title
              enabled
              functionId
              metafields(first: 5) {
                nodes { id namespace key type value jsonValue ownerType }
              }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            title: 'Local payment rule',
            enabled: false,
            functionId: 'gid://shopify/ShopifyFunction/function-local',
            metafields: [
              {
                namespace: 'settings',
                key: 'label',
                type: 'json',
                value: '{"label":"Local"}',
              },
            ],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.paymentCustomizationCreate.userErrors).toEqual([]);
    const created = createResponse.body.data.paymentCustomizationCreate.paymentCustomization;
    expect(created).toEqual({
      id: 'gid://shopify/PaymentCustomization/1',
      title: 'Local payment rule',
      enabled: false,
      functionId: 'gid://shopify/ShopifyFunction/function-local',
      metafields: {
        nodes: [
          {
            id: 'gid://shopify/Metafield/2',
            namespace: 'settings',
            key: 'label',
            type: 'json',
            value: '{"label":"Local"}',
            jsonValue: { label: 'Local' },
            ownerType: 'PAYMENT_CUSTOMIZATION',
          },
        ],
      },
    });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdatePaymentCustomization($id: ID!, $input: PaymentCustomizationInput!) {
          paymentCustomizationUpdate(id: $id, paymentCustomization: $input) {
            paymentCustomization {
              id
              title
              enabled
              functionId
              metafield(namespace: "settings", key: "label") {
                value
                jsonValue
              }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: created.id,
          input: {
            title: 'Updated payment rule',
            enabled: true,
            metafields: [
              {
                namespace: 'settings',
                key: 'label',
                type: 'json',
                value: '{"label":"Updated"}',
              },
            ],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.paymentCustomizationUpdate).toEqual({
      paymentCustomization: {
        id: created.id,
        title: 'Updated payment rule',
        enabled: true,
        functionId: 'gid://shopify/ShopifyFunction/function-local',
        metafield: {
          value: '{"label":"Updated"}',
          jsonValue: { label: 'Updated' },
        },
      },
      userErrors: [],
    });

    const activationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ActivatePaymentCustomization($ids: [ID!]!, $enabled: Boolean!) {
          paymentCustomizationActivation(ids: $ids, enabled: $enabled) {
            ids
            userErrors { field message code }
          }
        }`,
        variables: {
          ids: [created.id],
          enabled: false,
        },
      });

    expect(activationResponse.status).toBe(200);
    expect(activationResponse.body.data.paymentCustomizationActivation).toEqual({
      ids: [created.id],
      userErrors: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadPaymentCustomization($id: ID!) {
          paymentCustomization(id: $id) {
            id
            title
            enabled
            functionId
          }
          paymentCustomizations(first: 5, query: "enabled:false") {
            nodes { id title enabled }
          }
        }`,
        variables: { id: created.id },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data).toEqual({
      paymentCustomization: {
        id: created.id,
        title: 'Updated payment rule',
        enabled: false,
        functionId: 'gid://shopify/ShopifyFunction/function-local',
      },
      paymentCustomizations: {
        nodes: [{ id: created.id, title: 'Updated payment rule', enabled: false }],
      },
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeletePaymentCustomization($id: ID!) {
          paymentCustomizationDelete(id: $id) {
            deletedId
            userErrors { field message code }
          }
        }`,
        variables: { id: created.id },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.paymentCustomizationDelete).toEqual({
      deletedId: created.id,
      userErrors: [],
    });

    const deletedReadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query DeletedPaymentCustomization($id: ID!) {
          paymentCustomization(id: $id) { id }
          paymentCustomizations(first: 5) {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
        variables: { id: created.id },
      });

    expect(deletedReadResponse.status).toBe(200);
    expect(deletedReadResponse.body.data).toEqual({
      paymentCustomization: null,
      paymentCustomizations: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
    });
    expect(store.getLog().map((entry) => entry.status)).toEqual(['staged', 'staged', 'staged', 'staged']);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('accepts function handles, de-duplicates activation ids, and keeps raw mutation log bodies', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('payment customization function-handle flow should not hit upstream fetch');
    });
    const app = createApp(config).callback();
    const createRequestBody = {
      query: `mutation HandlePaymentCustomization($input: PaymentCustomizationInput!) {
        paymentCustomizationCreate(paymentCustomization: $input) {
          paymentCustomization {
            id
            title
            enabled
            functionId
          }
          userErrors { field message code }
        }
      }`,
      operationName: 'HandlePaymentCustomization',
      variables: {
        input: {
          title: 'Handle-backed payment rule',
          enabled: true,
          functionHandle: 'conformance-payment-customization',
        },
      },
    };

    const createResponse = await request(app).post('/admin/api/2026-04/graphql.json').send(createRequestBody);

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.paymentCustomizationCreate.userErrors).toEqual([]);
    const created = createResponse.body.data.paymentCustomizationCreate.paymentCustomization;
    expect(created).toEqual({
      id: 'gid://shopify/PaymentCustomization/1',
      title: 'Handle-backed payment rule',
      enabled: true,
      functionId: null,
    });
    expect(store.getState().stagedState.paymentCustomizations[created.id]?.functionHandle).toBe(
      'conformance-payment-customization',
    );

    const activationRequestBody = {
      query: `mutation ActivateDuplicatePaymentCustomizationIds($ids: [ID!]!, $enabled: Boolean!) {
        paymentCustomizationActivation(ids: $ids, enabled: $enabled) {
          ids
          userErrors { field message code }
        }
      }`,
      variables: {
        ids: [created.id, created.id],
        enabled: false,
      },
      extensions: {
        persistedQuery: {
          version: 1,
          sha256Hash: 'payment-customization-activation-test',
        },
      },
    };

    const activationResponse = await request(app).post('/admin/api/2026-04/graphql.json').send(activationRequestBody);

    expect(activationResponse.status).toBe(200);
    expect(activationResponse.body.data.paymentCustomizationActivation).toEqual({
      ids: [created.id],
      userErrors: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadHandlePaymentCustomization {
          paymentCustomizations(first: 5, query: "enabled:false") {
            nodes { id title enabled functionId }
          }
        }`,
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.paymentCustomizations.nodes).toEqual([
      {
        id: created.id,
        title: 'Handle-backed payment rule',
        enabled: false,
        functionId: null,
      },
    ]);
    expect(store.getLog().map((entry) => entry.requestBody)).toEqual([createRequestBody, activationRequestBody]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors captured payment customization validation branches locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('payment customization validation should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation PaymentCustomizationValidation(
          $badCreate: PaymentCustomizationInput!
          $missingFunction: PaymentCustomizationInput!
          $missingHandle: PaymentCustomizationInput!
          $multipleIdentifiers: PaymentCustomizationInput!
          $missingTitle: PaymentCustomizationInput!
          $missingEnabled: PaymentCustomizationInput!
          $invalidMetafields: PaymentCustomizationInput!
          $unknownId: ID!
          $badUpdate: PaymentCustomizationInput!
          $activationIds: [ID!]!
          $emptyActivationIds: [ID!]!
          $enabled: Boolean!
        ) {
          badCreate: paymentCustomizationCreate(paymentCustomization: $badCreate) {
            paymentCustomization { id title enabled functionId }
            userErrors { field message code }
          }
          missingFunction: paymentCustomizationCreate(paymentCustomization: $missingFunction) {
            paymentCustomization { id }
            userErrors { field message code }
          }
          missingHandle: paymentCustomizationCreate(paymentCustomization: $missingHandle) {
            paymentCustomization { id }
            userErrors { field message code }
          }
          multipleIdentifiers: paymentCustomizationCreate(paymentCustomization: $multipleIdentifiers) {
            paymentCustomization { id }
            userErrors { field message code }
          }
          missingTitle: paymentCustomizationCreate(paymentCustomization: $missingTitle) {
            paymentCustomization { id }
            userErrors { field message code }
          }
          missingEnabled: paymentCustomizationCreate(paymentCustomization: $missingEnabled) {
            paymentCustomization { id }
            userErrors { field message code }
          }
          invalidMetafields: paymentCustomizationCreate(paymentCustomization: $invalidMetafields) {
            paymentCustomization { id }
            userErrors { field message code }
          }
          unknownUpdate: paymentCustomizationUpdate(id: $unknownId, paymentCustomization: $badUpdate) {
            paymentCustomization { id title enabled functionId }
            userErrors { field message code }
          }
          unknownActivation: paymentCustomizationActivation(ids: $activationIds, enabled: $enabled) {
            ids
            userErrors { field message code }
          }
          emptyActivation: paymentCustomizationActivation(ids: $emptyActivationIds, enabled: $enabled) {
            ids
            userErrors { field message code }
          }
          unknownDelete: paymentCustomizationDelete(id: $unknownId) {
            deletedId
            userErrors { field message code }
          }
        }`,
        variables: {
          badCreate: {
            title: 'Hermes invalid payment customization',
            enabled: true,
            functionId: 'gid://shopify/ShopifyFunction/0',
          },
          missingFunction: {
            title: 'Hermes missing function',
            enabled: true,
          },
          missingHandle: {
            title: 'Hermes missing handle',
            enabled: true,
            functionHandle: 'missing-function',
          },
          multipleIdentifiers: {
            title: 'Hermes multiple function identifiers',
            enabled: true,
            functionId: 'gid://shopify/ShopifyFunction/function-local',
            functionHandle: 'conformance-payment-customization',
          },
          missingTitle: {
            enabled: true,
            functionId: 'gid://shopify/ShopifyFunction/0',
          },
          missingEnabled: {
            title: 'Hermes missing enabled',
            functionId: 'gid://shopify/ShopifyFunction/0',
          },
          invalidMetafields: {
            title: 'Hermes invalid metafields',
            enabled: true,
            functionHandle: 'conformance-payment-customization',
            metafields: [
              {
                namespace: 'settings',
                type: 'json',
                value: '{"label":"Missing key"}',
              },
            ],
          },
          unknownId: 'gid://shopify/PaymentCustomization/0',
          badUpdate: {
            title: 'Hermes unknown update',
            enabled: false,
            functionId: 'gid://shopify/ShopifyFunction/0',
          },
          activationIds: ['gid://shopify/PaymentCustomization/0'],
          emptyActivationIds: [],
          enabled: true,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      badCreate: {
        paymentCustomization: null,
        userErrors: [
          {
            field: ['paymentCustomization', 'functionId'],
            message:
              'Function gid://shopify/ShopifyFunction/0 not found. Ensure that it is released in the current app (347082227713), and that the app is installed.',
            code: 'FUNCTION_NOT_FOUND',
          },
        ],
      },
      missingFunction: {
        paymentCustomization: null,
        userErrors: [
          {
            field: ['paymentCustomization', 'functionId'],
            message: 'Required input field must be present.',
            code: 'REQUIRED_INPUT_FIELD',
          },
        ],
      },
      missingHandle: {
        paymentCustomization: null,
        userErrors: [
          {
            field: ['paymentCustomization', 'functionHandle'],
            message:
              'Function missing-function not found. Ensure that it is released in the current app (347082227713), and that the app is installed.',
            code: 'FUNCTION_NOT_FOUND',
          },
        ],
      },
      multipleIdentifiers: {
        paymentCustomization: null,
        userErrors: [
          {
            field: ['paymentCustomization', 'functionHandle'],
            message: 'Only one of function_id or function_handle can be provided, not both.',
            code: 'MULTIPLE_FUNCTION_IDENTIFIERS',
          },
        ],
      },
      missingTitle: {
        paymentCustomization: null,
        userErrors: [
          {
            field: ['paymentCustomization', 'title'],
            message: 'Required input field must be present.',
            code: 'REQUIRED_INPUT_FIELD',
          },
        ],
      },
      missingEnabled: {
        paymentCustomization: null,
        userErrors: [
          {
            field: ['paymentCustomization', 'enabled'],
            message: 'Required input field must be present.',
            code: 'REQUIRED_INPUT_FIELD',
          },
        ],
      },
      invalidMetafields: {
        paymentCustomization: null,
        userErrors: [
          {
            field: ['paymentCustomization', 'metafields'],
            message: 'Could not create or update metafields.',
            code: 'INVALID_METAFIELDS',
          },
        ],
      },
      unknownUpdate: {
        paymentCustomization: null,
        userErrors: [
          {
            field: ['id'],
            message: 'Could not find PaymentCustomization with id: gid://shopify/PaymentCustomization/0',
            code: 'PAYMENT_CUSTOMIZATION_NOT_FOUND',
          },
        ],
      },
      unknownActivation: {
        ids: [],
        userErrors: [
          {
            field: ['ids'],
            message: 'Could not find payment customizations with IDs: gid://shopify/PaymentCustomization/0',
            code: 'PAYMENT_CUSTOMIZATION_NOT_FOUND',
          },
        ],
      },
      emptyActivation: {
        ids: [],
        userErrors: [],
      },
      unknownDelete: {
        deletedId: null,
        userErrors: [
          {
            field: ['id'],
            message: 'Could not find PaymentCustomization with id: gid://shopify/PaymentCustomization/0',
            code: 'PAYMENT_CUSTOMIZATION_NOT_FOUND',
          },
        ],
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
