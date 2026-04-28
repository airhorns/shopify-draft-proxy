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

async function createProduct(app: ReturnType<typeof createApp>['callback'] extends () => infer T ? T : never) {
  const response = await request(app)
    .post('/admin/api/2025-01/graphql.json')
    .send({
      query: `#graphql
      mutation {
        productCreate(product: { title: "Definition host" }) {
          product { id }
          userErrors { field message }
        }
      }
    `,
    });

  expect(response.status).toBe(200);
  expect(response.body.data.productCreate.userErrors).toEqual([]);
  return response.body.data.productCreate.product.id as string;
}

describe('metafield definition draft flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages product-owner definition create and update locally with downstream reads and meta inspection', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('definition lifecycle must stay local'));
    const server = createApp(config).callback();

    const createResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation CreateDefinition($definition: MetafieldDefinitionInput!) {
            metafieldDefinitionCreate(definition: $definition) {
              createdDefinition {
                id
                name
                namespace
                key
                ownerType
                type { name category }
                description
                validations { name value }
                access { admin storefront customerAccount }
                capabilities {
                  adminFilterable { enabled eligible status }
                  smartCollectionCondition { enabled eligible }
                  uniqueValues { enabled eligible }
                }
                pinnedPosition
                validationStatus
              }
              userErrors { field message code }
            }
          }
        `,
        variables: {
          definition: {
            name: 'Care guide',
            namespace: 'custom',
            key: 'care',
            ownerType: 'PRODUCT',
            type: 'single_line_text_field',
            description: 'Short care instructions',
            validations: [{ name: 'max', value: '20' }],
            access: { storefront: 'PUBLIC_READ' },
            capabilities: { adminFilterable: { enabled: true } },
            pin: true,
          },
        },
      });

    expect(createResponse.status).toBe(200);
    const createdDefinition = createResponse.body.data.metafieldDefinitionCreate.createdDefinition;
    expect(createdDefinition).toEqual({
      id: 'gid://shopify/MetafieldDefinition/1',
      name: 'Care guide',
      namespace: 'custom',
      key: 'care',
      ownerType: 'PRODUCT',
      type: { name: 'single_line_text_field', category: 'TEXT' },
      description: 'Short care instructions',
      validations: [{ name: 'max', value: '20' }],
      access: {
        admin: 'PUBLIC_READ_WRITE',
        storefront: 'PUBLIC_READ',
        customerAccount: 'NONE',
      },
      capabilities: {
        adminFilterable: { enabled: true, eligible: true, status: 'FILTERABLE' },
        smartCollectionCondition: { enabled: false, eligible: true },
        uniqueValues: { enabled: false, eligible: true },
      },
      pinnedPosition: 1,
      validationStatus: 'ALL_VALID',
    });
    expect(createResponse.body.data.metafieldDefinitionCreate.userErrors).toEqual([]);

    const updateResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation UpdateDefinition($definition: MetafieldDefinitionUpdateInput!) {
            metafieldDefinitionUpdate(definition: $definition) {
              updatedDefinition {
                id
                name
                namespace
                key
                ownerType
                type { name }
                description
                validations { name value }
              }
              userErrors { field message code }
              validationJob { id }
            }
          }
        `,
        variables: {
          definition: {
            name: 'Care code',
            namespace: 'custom',
            key: 'care',
            ownerType: 'PRODUCT',
            description: 'Updated instructions',
            validations: [{ name: 'regex', value: '^[A-Z]+$' }],
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.metafieldDefinitionUpdate).toEqual({
      updatedDefinition: {
        id: createdDefinition.id,
        name: 'Care code',
        namespace: 'custom',
        key: 'care',
        ownerType: 'PRODUCT',
        type: { name: 'single_line_text_field' },
        description: 'Updated instructions',
        validations: [{ name: 'regex', value: '^[A-Z]+$' }],
      },
      userErrors: [],
      validationJob: null,
    });

    const immutableTypeResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation ImmutableType($definition: MetafieldDefinitionUpdateInput!) {
            metafieldDefinitionUpdate(definition: $definition) {
              updatedDefinition { id type { name } }
              userErrors { field message code }
            }
          }
        `,
        variables: {
          definition: {
            namespace: 'custom',
            key: 'care',
            ownerType: 'PRODUCT',
            type: 'number_integer',
          },
        },
      });

    expect(immutableTypeResponse.status).toBe(200);
    expect(immutableTypeResponse.body.data.metafieldDefinitionUpdate).toEqual({
      updatedDefinition: null,
      userErrors: [{ field: ['definition', 'type'], message: "Type can't be changed.", code: 'IMMUTABLE' }],
    });

    const readResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          query {
            detail: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: "custom", key: "care" }) {
              id
              name
              namespace
              key
              type { name }
              validations { name value }
              pinnedPosition
            }
            catalog: metafieldDefinitions(ownerType: PRODUCT, namespace: "custom", first: 5) {
              nodes { id key name pinnedPosition }
            }
          }
        `,
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.detail).toEqual({
      id: createdDefinition.id,
      name: 'Care code',
      namespace: 'custom',
      key: 'care',
      type: { name: 'single_line_text_field' },
      validations: [{ name: 'regex', value: '^[A-Z]+$' }],
      pinnedPosition: 1,
    });
    expect(readResponse.body.data.catalog.nodes).toEqual([
      { id: createdDefinition.id, key: 'care', name: 'Care code', pinnedPosition: 1 },
    ]);

    const stateResponse = await request(server).get('/__meta/state');
    const logResponse = await request(server).get('/__meta/log');
    expect(stateResponse.body.stagedState.metafieldDefinitions[createdDefinition.id]).toMatchObject({
      id: createdDefinition.id,
      name: 'Care code',
      namespace: 'custom',
      key: 'care',
      ownerType: 'PRODUCT',
    });
    expect(logResponse.body.entries).toHaveLength(3);
    expect(logResponse.body.entries[0]).toMatchObject({
      operationName: 'metafieldDefinitionCreate',
      status: 'staged',
      requestBody: {
        variables: {
          definition: expect.objectContaining({ namespace: 'custom', key: 'care' }),
        },
      },
      interpreted: {
        capability: {
          operationName: 'metafieldDefinitionCreate',
          domain: 'metafields',
          execution: 'stage-locally',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('uses staged definitions when validating and materializing product metafieldsSet writes', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('definition-backed metafieldsSet must stay local'));
    const server = createApp(config).callback();

    await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation {
            metafieldDefinitionCreate(
              definition: {
                name: "Warehouse code"
                namespace: "custom"
                key: "warehouse"
                ownerType: PRODUCT
                type: "single_line_text_field"
                validations: [{ name: "max", value: "3" }, { name: "regex", value: "^[A-Z]+$" }]
              }
            ) {
              createdDefinition { id }
              userErrors { field message code }
            }
          }
        `,
      });

    const productId = await createProduct(server);
    const setMutation = `#graphql
      mutation SetMetafields($metafields: [MetafieldsSetInput!]!) {
        metafieldsSet(metafields: $metafields) {
          metafields { id namespace key type value }
          userErrors { field message code elementIndex }
        }
      }
    `;

    const validResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: setMutation,
        variables: {
          metafields: [
            {
              ownerId: productId,
              namespace: 'custom',
              key: 'warehouse',
              value: 'NYC',
            },
          ],
        },
      });

    expect(validResponse.status).toBe(200);
    expect(validResponse.body.data.metafieldsSet.userErrors).toEqual([]);
    expect(validResponse.body.data.metafieldsSet.metafields[0]).toMatchObject({
      namespace: 'custom',
      key: 'warehouse',
      type: 'single_line_text_field',
      value: 'NYC',
    });

    const invalidTypeResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: setMutation,
        variables: {
          metafields: [
            {
              ownerId: productId,
              namespace: 'custom',
              key: 'warehouse',
              type: 'number_integer',
              value: 'ABC',
            },
          ],
        },
      });

    expect(invalidTypeResponse.body.data.metafieldsSet.metafields).toEqual([]);
    expect(invalidTypeResponse.body.data.metafieldsSet.userErrors).toEqual([
      {
        field: ['metafields', '0', 'type'],
        message: 'Type must be single_line_text_field for this metafield definition.',
        code: 'INVALID_TYPE',
        elementIndex: 0,
      },
    ]);

    const invalidValueResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: setMutation,
        variables: {
          metafields: [
            {
              ownerId: productId,
              namespace: 'custom',
              key: 'warehouse',
              value: 'ny',
            },
          ],
        },
      });

    expect(invalidValueResponse.body.data.metafieldsSet.metafields).toEqual([]);
    expect(invalidValueResponse.body.data.metafieldsSet.userErrors).toEqual([
      {
        field: ['metafields', '0', 'value'],
        message: 'Value does not match the validation pattern for this metafield definition.',
        code: 'INVALID',
        elementIndex: 0,
      },
    ]);
  });

  it('stages definition delete locally and removes matching product-owned metafields when requested', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('definition delete must stay local'));
    const server = createApp(config).callback();

    const createDefinitionResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation {
            metafieldDefinitionCreate(
              definition: {
                name: "Material"
                namespace: "custom"
                key: "material"
                ownerType: PRODUCT
                type: "single_line_text_field"
              }
            ) {
              createdDefinition { id namespace key ownerType }
              userErrors { field message code }
            }
          }
        `,
      });
    const definition = createDefinitionResponse.body.data.metafieldDefinitionCreate.createdDefinition;
    const productId = await createProduct(server);

    await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation SetMetafields($metafields: [MetafieldsSetInput!]!) {
            metafieldsSet(metafields: $metafields) {
              metafields { id }
              userErrors { field message }
            }
          }
        `,
        variables: {
          metafields: [
            {
              ownerId: productId,
              namespace: 'custom',
              key: 'material',
              value: 'Canvas',
            },
          ],
        },
      });

    const deleteResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation DeleteDefinition($id: ID!, $deleteAllAssociatedMetafields: Boolean!) {
            metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: $deleteAllAssociatedMetafields) {
              deletedDefinitionId
              deletedDefinition { ownerType namespace key }
              userErrors { field message code }
            }
          }
        `,
        variables: {
          id: definition.id,
          deleteAllAssociatedMetafields: true,
        },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.metafieldDefinitionDelete).toEqual({
      deletedDefinitionId: definition.id,
      deletedDefinition: {
        ownerType: 'PRODUCT',
        namespace: 'custom',
        key: 'material',
      },
      userErrors: [],
    });

    const definitionReadResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          query {
            definition: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: "custom", key: "material" }) {
              id
            }
            definitions: metafieldDefinitions(ownerType: PRODUCT, namespace: "custom", first: 5) {
              nodes { id }
            }
          }
        `,
      });
    const productReadResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          query($productId: ID!) {
            product(id: $productId) {
              material: metafield(namespace: "custom", key: "material") { id }
              metafields(first: 5) { nodes { namespace key value } }
            }
          }
        `,
        variables: { productId },
      });

    expect(definitionReadResponse.status).toBe(200);
    expect(definitionReadResponse.body.data.definition).toBeNull();
    expect(definitionReadResponse.body.data.definitions.nodes).toEqual([]);
    expect(productReadResponse.status).toBe(200);
    expect(productReadResponse.body.data.product.material).toBeNull();
    expect(productReadResponse.body.data.product.metafields.nodes).toEqual([]);

    const stateResponse = await request(server).get('/__meta/state');
    const logResponse = await request(server).get('/__meta/log');
    expect(stateResponse.body.stagedState.deletedMetafieldDefinitionIds).toEqual({
      [definition.id]: true,
    });
    expect(logResponse.body.entries.at(-1)).toMatchObject({
      operationName: 'metafieldDefinitionDelete',
      requestBody: {
        variables: {
          id: definition.id,
          deleteAllAssociatedMetafields: true,
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('assigns standard enabled definitions the next owner-type pinned position', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('standard definition enablement must stay local'));
    const server = createApp(config).callback();

    const pinnedCustomDefinitionResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation {
            metafieldDefinitionCreate(
              definition: {
                name: "Pinned custom definition"
                namespace: "custom"
                key: "pinned_custom"
                ownerType: PRODUCT
                type: "single_line_text_field"
                pin: true
              }
            ) {
              createdDefinition { id pinnedPosition }
              userErrors { field message code }
            }
          }
        `,
      });

    expect(pinnedCustomDefinitionResponse.status).toBe(200);
    expect(pinnedCustomDefinitionResponse.body.data.metafieldDefinitionCreate).toEqual({
      createdDefinition: {
        id: 'gid://shopify/MetafieldDefinition/1',
        pinnedPosition: 1,
      },
      userErrors: [],
    });

    const enabledStandardDefinitionResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation {
            standardMetafieldDefinitionEnable(
              ownerType: PRODUCT
              id: "gid://shopify/StandardMetafieldDefinitionTemplate/2"
              pin: true
            ) {
              createdDefinition { id namespace key pinnedPosition }
              userErrors { field message code }
            }
          }
        `,
      });

    expect(enabledStandardDefinitionResponse.status).toBe(200);
    expect(enabledStandardDefinitionResponse.body.data.standardMetafieldDefinitionEnable).toEqual({
      createdDefinition: {
        id: expect.stringMatching(/^gid:\/\/shopify\/MetafieldDefinition\//),
        namespace: 'descriptors',
        key: 'care_guide',
        pinnedPosition: 2,
      },
      userErrors: [],
    });

    const pinnedCatalogResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          query {
            metafieldDefinitions(ownerType: PRODUCT, sortKey: PINNED_POSITION, pinnedStatus: PINNED, first: 5) {
              nodes { namespace key pinnedPosition }
            }
          }
        `,
      });

    expect(pinnedCatalogResponse.status).toBe(200);
    expect(pinnedCatalogResponse.body.data.metafieldDefinitions.nodes).toEqual([
      { namespace: 'descriptors', key: 'care_guide', pinnedPosition: 2 },
      { namespace: 'custom', key: 'pinned_custom', pinnedPosition: 1 },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
