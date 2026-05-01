import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

describe('Shopify Function metadata flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages validation, cart transform, and tax app metadata locally with downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('function metadata staging must not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const createValidationBody = {
      query: `mutation CreateValidation($validation: ValidationCreateInput!) {
        validationCreate(validation: $validation) {
          validation {
            id
            title
            enable
            blockOnFailure
            functionHandle
            shopifyFunction { id title handle apiType }
          }
          userErrors { field message code }
        }
      }`,
      variables: {
        validation: {
          functionHandle: 'validation-local',
          title: 'Local validation',
          enable: true,
          blockOnFailure: true,
        },
      },
    };

    const createValidation = await request(app).post('/admin/api/2026-04/graphql.json').send(createValidationBody);

    expect(createValidation.status).toBe(200);
    expect(createValidation.body.data.validationCreate.userErrors).toEqual([]);
    const validation = createValidation.body.data.validationCreate.validation;
    expect(validation).toMatchObject({
      id: expect.stringMatching(/^gid:\/\/shopify\/Validation\/[0-9]+$/u),
      title: 'Local validation',
      enable: true,
      blockOnFailure: true,
      functionHandle: 'validation-local',
      shopifyFunction: {
        id: 'gid://shopify/ShopifyFunction/validation-local',
        title: 'Validation Local',
        handle: 'validation-local',
        apiType: 'VALIDATION',
      },
    });

    const updateValidation = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateValidation($id: ID!, $validation: ValidationUpdateInput!) {
          validationUpdate(id: $id, validation: $validation) {
            validation { id title enable blockOnFailure }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: validation.id,
          validation: {
            title: 'Updated validation',
            enable: false,
            blockOnFailure: false,
          },
        },
      });

    expect(updateValidation.status).toBe(200);
    expect(updateValidation.body.data.validationUpdate).toEqual({
      validation: {
        id: validation.id,
        title: 'Updated validation',
        enable: false,
        blockOnFailure: false,
      },
      userErrors: [],
    });

    const createCartTransformBody = {
      query: `mutation CreateCartTransform($functionHandle: String!, $blockOnFailure: Boolean!) {
        cartTransformCreate(functionHandle: $functionHandle, blockOnFailure: $blockOnFailure) {
          cartTransform {
            id
            title
            blockOnFailure
            functionId
            functionHandle
          }
          userErrors { field message code }
        }
      }`,
      variables: {
        functionHandle: 'cart-transform-local',
        blockOnFailure: true,
      },
    };

    const createCartTransform = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send(createCartTransformBody);

    expect(createCartTransform.status).toBe(200);
    expect(createCartTransform.body.data.cartTransformCreate.userErrors).toEqual([]);
    const cartTransform = createCartTransform.body.data.cartTransformCreate.cartTransform;
    expect(cartTransform).toMatchObject({
      id: expect.stringMatching(/^gid:\/\/shopify\/CartTransform\/[0-9]+$/u),
      title: 'Cart Transform Local',
      blockOnFailure: true,
      functionId: 'gid://shopify/ShopifyFunction/cart-transform-local',
      functionHandle: 'cart-transform-local',
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadFunctionMetadata($validationId: ID!) {
          validation(id: $validationId) {
            id
            title
            enable
            shopifyFunction { id handle apiType }
          }
          validations(first: 5) {
            nodes { id title enable }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          cartTransforms(first: 5) {
            nodes { id title blockOnFailure functionHandle }
          }
          validationFunctions: shopifyFunctions(first: 5, apiType: VALIDATION) {
            nodes { id handle apiType }
          }
          cartFunctions: shopifyFunctions(first: 5, apiType: CART_TRANSFORM) {
            nodes { id handle apiType }
          }
          cartFunction: shopifyFunction(id: "gid://shopify/ShopifyFunction/cart-transform-local") {
            id
            title
            handle
            apiType
          }
        }`,
        variables: { validationId: validation.id },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data).toMatchObject({
      validation: {
        id: validation.id,
        title: 'Updated validation',
        enable: false,
        shopifyFunction: {
          id: 'gid://shopify/ShopifyFunction/validation-local',
          handle: 'validation-local',
          apiType: 'VALIDATION',
        },
      },
      validations: {
        nodes: [{ id: validation.id, title: 'Updated validation', enable: false }],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: `cursor:${validation.id}`,
          endCursor: `cursor:${validation.id}`,
        },
      },
      cartTransforms: {
        nodes: [
          {
            id: cartTransform.id,
            title: 'Cart Transform Local',
            blockOnFailure: true,
            functionHandle: 'cart-transform-local',
          },
        ],
      },
      validationFunctions: {
        nodes: [
          { id: 'gid://shopify/ShopifyFunction/validation-local', handle: 'validation-local', apiType: 'VALIDATION' },
        ],
      },
      cartFunctions: {
        nodes: [
          {
            id: 'gid://shopify/ShopifyFunction/cart-transform-local',
            handle: 'cart-transform-local',
            apiType: 'CART_TRANSFORM',
          },
        ],
      },
      cartFunction: {
        id: 'gid://shopify/ShopifyFunction/cart-transform-local',
        title: 'Cart Transform Local',
        handle: 'cart-transform-local',
        apiType: 'CART_TRANSFORM',
      },
    });

    const nodeRead = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadValidationNode($id: ID!) {
          node(id: $id) {
            id
            __typename
            ... on Validation {
              title
              enable
              shopifyFunction { id handle apiType }
            }
          }
          nodes(ids: [$id, "gid://shopify/Validation/404"]) {
            id
            __typename
            ... on Validation { title }
          }
        }`,
        variables: { id: validation.id },
      });

    expect(nodeRead.status).toBe(200);
    expect(nodeRead.body.data).toEqual({
      node: {
        id: validation.id,
        __typename: 'Validation',
        title: 'Updated validation',
        enable: false,
        shopifyFunction: {
          id: 'gid://shopify/ShopifyFunction/validation-local',
          handle: 'validation-local',
          apiType: 'VALIDATION',
        },
      },
      nodes: [
        {
          id: validation.id,
          __typename: 'Validation',
          title: 'Updated validation',
        },
        null,
      ],
    });

    const taxAppConfigure = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ConfigureTaxApp($ready: Boolean!) {
          taxAppConfigure(ready: $ready) {
            taxAppConfiguration { id ready state updatedAt }
            userErrors { field message code }
          }
        }`,
        variables: { ready: true },
      });

    expect(taxAppConfigure.status).toBe(200);
    expect(taxAppConfigure.body.data.taxAppConfigure).toEqual({
      taxAppConfiguration: {
        id: 'gid://shopify/TaxAppConfiguration/local',
        ready: true,
        state: 'READY',
        updatedAt: expect.any(String),
      },
      userErrors: [],
    });

    const deleteValidation = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteValidation($id: ID!) {
          validationDelete(id: $id) {
            deletedId
            userErrors { field message code }
          }
        }`,
        variables: { id: validation.id },
      });
    expect(deleteValidation.body.data.validationDelete).toEqual({ deletedId: validation.id, userErrors: [] });

    const deleteCartTransform = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteCartTransform($id: ID!) {
          cartTransformDelete(id: $id) {
            deletedId
            userErrors { field message code }
          }
        }`,
        variables: { id: cartTransform.id },
      });
    expect(deleteCartTransform.body.data.cartTransformDelete).toEqual({
      deletedId: cartTransform.id,
      userErrors: [],
    });

    const emptyRead = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query DeletedFunctionMetadata($validationId: ID!) {
          validation(id: $validationId) { id }
          validations(first: 5) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          cartTransforms(first: 5) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
        }`,
        variables: { validationId: validation.id },
      });

    expect(emptyRead.body.data).toEqual({
      validation: null,
      validations: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
      cartTransforms: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
    });
    expect(store.getLog().map((entry) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
    ]);
    expect(store.getLog()[0]?.requestBody).toEqual(createValidationBody);
    expect(store.getLog()[2]?.requestBody).toEqual(createCartTransformBody);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages Functions inner mutations through bulkOperationRunMutation', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('bulkOperationRunMutation Functions imports should stay local'));
    const app = createApp(config).callback();
    const stagedUploadPath = 'shopify-draft-proxy/gid://shopify/StagedUploadTarget0/functions-validation-create.jsonl';
    const validVariables = {
      validation: {
        functionHandle: 'bulk-validation',
        title: 'Bulk validation',
        enable: true,
      },
    };
    const invalidVariables = {
      validation: {
        title: 'Missing function metadata',
      },
    };
    store.stageUploadContent(
      [stagedUploadPath],
      `${JSON.stringify(validVariables)}\n${JSON.stringify(invalidVariables)}\n`,
    );

    const innerMutation = `mutation BulkValidationCreate($validation: ValidationCreateInput!) {
      validationCreate(validation: $validation) {
        validation { id title enable functionHandle }
        userErrors { field message code }
      }
    }`;
    const bulkResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation BulkImport($mutation: String!, $stagedUploadPath: String!) {
          bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $stagedUploadPath) {
            bulkOperation { id status type objectCount rootObjectCount url query }
            userErrors { field message }
          }
        }`,
        variables: {
          mutation: innerMutation,
          stagedUploadPath,
        },
      });

    expect(bulkResponse.status).toBe(200);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(bulkResponse.body.data.bulkOperationRunMutation.userErrors).toEqual([]);
    expect(bulkResponse.body.data.bulkOperationRunMutation.bulkOperation).toMatchObject({
      status: 'COMPLETED',
      type: 'MUTATION',
      objectCount: '1',
      rootObjectCount: '1',
      query: innerMutation,
    });

    const operationId = bulkResponse.body.data.bulkOperationRunMutation.bulkOperation.id as string;
    const resultResponse = await request(app).get(
      `/__meta/bulk-operations/${encodeURIComponent(operationId)}/result.jsonl`,
    );
    const resultRows = resultResponse.text
      .trim()
      .split('\n')
      .map((line) => JSON.parse(line) as Record<string, unknown>);

    expect(resultResponse.status).toBe(200);
    expect(resultRows).toHaveLength(2);
    expect(resultRows[0]).toMatchObject({
      line: 1,
      response: {
        data: {
          validationCreate: {
            validation: {
              title: 'Bulk validation',
              enable: true,
              functionHandle: 'bulk-validation',
            },
            userErrors: [],
          },
        },
      },
    });
    const firstResultRow = resultRows[0];
    if (!firstResultRow) {
      throw new Error('Expected bulkOperationRunMutation to write a first result row.');
    }
    const firstResultResponse = firstResultRow['response'] as {
      data?: { validationCreate?: { validation?: { id?: unknown } } };
    };
    const bulkValidationId = firstResultResponse.data?.validationCreate?.validation?.id;
    expect(bulkValidationId).toEqual(expect.stringMatching(/^gid:\/\/shopify\/Validation\/[0-9]+$/u));
    expect(resultRows[1]).toEqual({
      line: 2,
      response: {
        data: {
          validationCreate: {
            validation: null,
            userErrors: [
              {
                field: ['validation', 'functionHandle'],
                message: 'Function handle or function ID must be provided',
                code: 'MISSING_FUNCTION',
              },
            ],
          },
        },
      },
    });

    const readValidation = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadBulkValidation($id: ID!) {
          validation(id: $id) { id title enable functionHandle }
          shopifyFunction(id: "gid://shopify/ShopifyFunction/bulk-validation") { id handle apiType }
        }`,
        variables: { id: bulkValidationId },
      });

    expect(readValidation.body.data).toEqual({
      validation: {
        id: bulkValidationId,
        title: 'Bulk validation',
        enable: true,
        functionHandle: 'bulk-validation',
      },
      shopifyFunction: {
        id: 'gid://shopify/ShopifyFunction/bulk-validation',
        handle: 'bulk-validation',
        apiType: 'VALIDATION',
      },
    });
  });

  it('preserves app ownership metadata when function-backed resources reference known Functions', async () => {
    store.upsertStagedShopifyFunction({
      id: 'gid://shopify/ShopifyFunction/validation-owned',
      title: 'Owned validation function',
      handle: 'validation-owned',
      apiType: 'VALIDATION',
      description: 'Function metadata captured from the installed app',
      appKey: 'validation-app-key',
      app: {
        __typename: 'App',
        id: 'gid://shopify/App/validation-app',
        title: 'Validation App',
        handle: 'validation-app',
        apiKey: 'validation-app-key',
      },
    });
    store.upsertStagedShopifyFunction({
      id: 'gid://shopify/ShopifyFunction/validation-owned-v2',
      title: 'Second validation function',
      handle: 'validation-owned-v2',
      apiType: 'VALIDATION',
      description: 'Replacement Function metadata from the installed app',
      appKey: 'validation-app-key-v2',
      app: {
        __typename: 'App',
        id: 'gid://shopify/App/validation-app-v2',
        title: 'Validation App V2',
        handle: 'validation-app-v2',
        apiKey: 'validation-app-key-v2',
      },
    });
    store.upsertStagedShopifyFunction({
      id: 'gid://shopify/ShopifyFunction/cart-owned',
      title: 'Owned cart function',
      handle: 'cart-owned',
      apiType: 'CART_TRANSFORM',
      description: 'Cart transform Function metadata captured from the installed app',
      appKey: 'cart-app-key',
      app: {
        __typename: 'App',
        id: 'gid://shopify/App/cart-app',
        title: 'Cart App',
        handle: 'cart-app',
        apiKey: 'cart-app-key',
      },
    });
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('known Function ownership flow must not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const createValidation = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateOwnedValidation($validation: ValidationCreateInput!) {
          validationCreate(validation: $validation) {
            validation {
              id
              functionId
              functionHandle
              shopifyFunction {
                id
                title
                handle
                apiType
                description
                appKey
                app { __typename id title handle apiKey }
              }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          validation: {
            functionId: 'gid://shopify/ShopifyFunction/validation-owned',
            title: 'Owned validation',
          },
        },
      });

    expect(createValidation.status).toBe(200);
    expect(createValidation.body.data.validationCreate.userErrors).toEqual([]);
    const validation = createValidation.body.data.validationCreate.validation;
    expect(validation).toMatchObject({
      functionId: 'gid://shopify/ShopifyFunction/validation-owned',
      functionHandle: 'validation-owned',
      shopifyFunction: {
        id: 'gid://shopify/ShopifyFunction/validation-owned',
        title: 'Owned validation function',
        handle: 'validation-owned',
        apiType: 'VALIDATION',
        description: 'Function metadata captured from the installed app',
        appKey: 'validation-app-key',
        app: {
          __typename: 'App',
          id: 'gid://shopify/App/validation-app',
          title: 'Validation App',
          handle: 'validation-app',
          apiKey: 'validation-app-key',
        },
      },
    });

    const updateValidation = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateOwnedValidation($id: ID!, $validation: ValidationUpdateInput!) {
          validationUpdate(id: $id, validation: $validation) {
            validation {
              id
              functionId
              functionHandle
              shopifyFunction {
                id
                handle
                appKey
                app { title apiKey }
              }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: validation.id,
          validation: {
            functionHandle: 'validation-owned-v2',
          },
        },
      });

    expect(updateValidation.body.data.validationUpdate).toEqual({
      validation: {
        id: validation.id,
        functionId: 'gid://shopify/ShopifyFunction/validation-owned-v2',
        functionHandle: 'validation-owned-v2',
        shopifyFunction: {
          id: 'gid://shopify/ShopifyFunction/validation-owned-v2',
          handle: 'validation-owned-v2',
          appKey: 'validation-app-key-v2',
          app: {
            title: 'Validation App V2',
            apiKey: 'validation-app-key-v2',
          },
        },
      },
      userErrors: [],
    });

    const createCartTransform = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateOwnedCartTransform($functionHandle: String!) {
          cartTransformCreate(functionHandle: $functionHandle) {
            cartTransform { id functionId functionHandle title }
            userErrors { field message code }
          }
        }`,
        variables: { functionHandle: 'cart-owned' },
      });

    expect(createCartTransform.body.data.cartTransformCreate).toMatchObject({
      cartTransform: {
        functionId: 'gid://shopify/ShopifyFunction/cart-owned',
        functionHandle: 'cart-owned',
        title: 'Owned cart function',
      },
      userErrors: [],
    });

    const readFunctions = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadOwnedFunctions {
          validationFunctions: shopifyFunctions(first: 5, apiType: VALIDATION) {
            nodes { id handle appKey app { title apiKey } }
          }
          cartFunction: shopifyFunction(id: "gid://shopify/ShopifyFunction/cart-owned") {
            id
            handle
            appKey
            app { __typename title apiKey }
          }
        }`,
      });

    expect(readFunctions.body.data).toMatchObject({
      validationFunctions: {
        nodes: expect.arrayContaining([
          {
            id: 'gid://shopify/ShopifyFunction/validation-owned-v2',
            handle: 'validation-owned-v2',
            appKey: 'validation-app-key-v2',
            app: {
              title: 'Validation App V2',
              apiKey: 'validation-app-key-v2',
            },
          },
        ]),
      },
      cartFunction: {
        id: 'gid://shopify/ShopifyFunction/cart-owned',
        handle: 'cart-owned',
        appKey: 'cart-app-key',
        app: {
          __typename: 'App',
          title: 'Cart App',
          apiKey: 'cart-app-key',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns local userErrors for invalid Function metadata operations without upstream writes', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('Function metadata validation must not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const missingValidationFunction = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation MissingValidationFunction($validation: ValidationCreateInput!) {
          validationCreate(validation: $validation) {
            validation { id }
            userErrors { field message code }
          }
        }`,
        variables: { validation: { title: 'Missing function reference' } },
      });

    expect(missingValidationFunction.body.data.validationCreate).toEqual({
      validation: null,
      userErrors: [
        {
          field: ['validation', 'functionHandle'],
          message: 'Function handle or function ID must be provided',
          code: 'MISSING_FUNCTION',
        },
      ],
    });

    const missingCartFunction = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation MissingCartFunction {
          cartTransformCreate {
            cartTransform { id }
            userErrors { field message code }
          }
        }`,
      });

    expect(missingCartFunction.body.data.cartTransformCreate).toEqual({
      cartTransform: null,
      userErrors: [
        {
          field: ['functionHandle'],
          message: 'Function handle or function ID must be provided',
          code: 'MISSING_FUNCTION',
        },
      ],
    });

    const unknownValidationUpdate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateUnknownValidation {
          validationUpdate(id: "gid://shopify/Validation/404", validation: { title: "Nope" }) {
            validation { id }
            userErrors { field message code }
          }
        }`,
      });

    expect(unknownValidationUpdate.body.data.validationUpdate).toEqual({
      validation: null,
      userErrors: [
        {
          field: ['id'],
          message: 'No function-backed resource exists with id gid://shopify/Validation/404',
          code: 'NOT_FOUND',
        },
      ],
    });

    const unknownCartTransformDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteUnknownCartTransform {
          cartTransformDelete(id: "gid://shopify/CartTransform/404") {
            deletedId
            userErrors { field message code }
          }
        }`,
      });

    expect(unknownCartTransformDelete.body.data.cartTransformDelete).toEqual({
      deletedId: null,
      userErrors: [
        {
          field: ['id'],
          message: 'No function-backed resource exists with id gid://shopify/CartTransform/404',
          code: 'NOT_FOUND',
        },
      ],
    });

    expect(store.listEffectiveValidations()).toEqual([]);
    expect(store.listEffectiveCartTransforms()).toEqual([]);
    expect(store.getLog().map((entry) => entry.status)).toEqual(['staged', 'staged', 'staged', 'staged']);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
