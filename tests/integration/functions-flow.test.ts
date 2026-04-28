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
});
