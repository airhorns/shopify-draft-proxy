import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { MetafieldDefinitionRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeDefinition(overrides: Partial<MetafieldDefinitionRecord> = {}): MetafieldDefinitionRecord {
  return {
    id: overrides.id ?? 'gid://shopify/MetafieldDefinition/100',
    name: overrides.name ?? 'Material',
    namespace: overrides.namespace ?? 'custom',
    key: overrides.key ?? 'material',
    ownerType: overrides.ownerType ?? 'PRODUCT',
    type: overrides.type ?? {
      name: 'single_line_text_field',
      category: 'TEXT',
    },
    description: overrides.description ?? 'Product material',
    validations: overrides.validations ?? [],
    access: overrides.access ?? {
      admin: 'PUBLIC_READ_WRITE',
      storefront: 'NONE',
    },
    capabilities: overrides.capabilities ?? {
      adminFilterable: { enabled: false, eligible: true, status: 'NOT_FILTERABLE' },
      smartCollectionCondition: { enabled: false, eligible: true },
      uniqueValues: { enabled: false, eligible: true },
    },
    constraints: overrides.constraints ?? {
      key: null,
      values: [],
    },
    pinnedPosition: 'pinnedPosition' in overrides ? (overrides.pinnedPosition ?? null) : 1,
    validationStatus: overrides.validationStatus ?? 'ALL_VALID',
  };
}

describe('metafield definition query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns null for missing singular definitions and empty catalog connections without live access', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('metafield definition snapshot reads must stay local'));
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query {
          missing: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: "custom", key: "missing" }) {
            id
            name
          }
          empty: metafieldDefinitions(ownerType: PRODUCT, first: 5, namespace: "missing") {
            nodes { id }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        missing: null,
        empty: {
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
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('filters and serializes product-owned definition catalog fields from normalized state', async () => {
    store.upsertBaseMetafieldDefinitions([
      makeDefinition({
        id: 'gid://shopify/MetafieldDefinition/200',
        name: 'Origin',
        namespace: 'details',
        key: 'origin',
        pinnedPosition: null,
        description: null,
      }),
      makeDefinition(),
    ]);
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query {
          byNamespace: metafieldDefinitions(ownerType: PRODUCT, first: 5, namespace: "custom") {
            nodes {
              id
              name
              namespace
              key
              ownerType
              type { name category }
              description
              validations { name value }
              access { admin storefront }
              capabilities {
                adminFilterable { enabled eligible status }
                smartCollectionCondition { enabled eligible }
                uniqueValues { enabled eligible }
              }
              constraints { key values(first: 2) { nodes { value } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }
              pinnedPosition
              validationStatus
              metafieldsCount
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          byQuery: metafieldDefinitions(ownerType: PRODUCT, first: 5, namespace: "details", query: "key:origin", pinnedStatus: UNPINNED) {
            nodes { id key pinnedPosition }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body.data.byNamespace.nodes).toEqual([
      {
        id: 'gid://shopify/MetafieldDefinition/100',
        name: 'Material',
        namespace: 'custom',
        key: 'material',
        ownerType: 'PRODUCT',
        type: {
          name: 'single_line_text_field',
          category: 'TEXT',
        },
        description: 'Product material',
        validations: [],
        access: {
          admin: 'PUBLIC_READ_WRITE',
          storefront: 'NONE',
        },
        capabilities: {
          adminFilterable: { enabled: false, eligible: true, status: 'NOT_FILTERABLE' },
          smartCollectionCondition: { enabled: false, eligible: true },
          uniqueValues: { enabled: false, eligible: true },
        },
        constraints: {
          key: null,
          values: {
            nodes: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: null,
              endCursor: null,
            },
          },
        },
        pinnedPosition: 1,
        validationStatus: 'ALL_VALID',
        metafieldsCount: 0,
      },
    ]);
    expect(response.body.data.byNamespace.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/MetafieldDefinition/100',
      endCursor: 'cursor:gid://shopify/MetafieldDefinition/100',
    });
    expect(response.body.data.byQuery.nodes).toEqual([
      {
        id: 'gid://shopify/MetafieldDefinition/200',
        key: 'origin',
        pinnedPosition: null,
      },
    ]);
  });

  it('resolves definition detail metafields and counts from effective product-owned metafields', async () => {
    store.upsertBaseMetafieldDefinitions([makeDefinition()]);
    const app = createApp(config).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Metafield host" }) { product { id } userErrors { field message } } }',
    });
    const productId = createResponse.body.data.productCreate.product.id as string;

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation SetMetafields($metafields: [MetafieldsSetInput!]!) {
          metafieldsSet(metafields: $metafields) {
            metafields { id namespace key type value ownerType }
            userErrors { field message }
          }
        }`,
        variables: {
          metafields: [
            {
              ownerId: productId,
              namespace: 'custom',
              key: 'material',
              type: 'single_line_text_field',
              value: 'Canvas',
            },
          ],
        },
      });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query {
          metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: "custom", key: "material" }) {
            id
            metafieldsCount
            metafields(first: 5) {
              nodes { id namespace key type value ownerType }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body.data.metafieldDefinition.metafieldsCount).toBe(1);
    expect(response.body.data.metafieldDefinition.metafields.nodes).toEqual([
      expect.objectContaining({
        namespace: 'custom',
        key: 'material',
        type: 'single_line_text_field',
        value: 'Canvas',
        ownerType: 'PRODUCT',
      }),
    ]);
    expect(response.body.data.metafieldDefinition.metafields.pageInfo).toMatchObject({
      hasNextPage: false,
      hasPreviousPage: false,
    });
  });

  it('stages standard metafield definition enablement and exposes downstream definition reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('standardMetafieldDefinitionEnable should stage locally');
    });
    const app = createApp(config).callback();

    const mutationResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation EnableStandardDefinition($pin: Boolean!) {
            standardMetafieldDefinitionEnable(
              ownerType: PRODUCT
              id: "gid://shopify/StandardMetafieldDefinitionTemplate/1"
              pin: $pin
              access: { storefront: PUBLIC_READ }
              capabilities: { adminFilterable: { enabled: true } }
            ) {
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
                constraints { key values(first: 2) { nodes { value } } }
                pinnedPosition
                validationStatus
              }
              userErrors { field message code }
            }
          }
        `,
        variables: { pin: true },
      });

    expect(mutationResponse.status).toBe(200);
    expect(mutationResponse.body.data.standardMetafieldDefinitionEnable).toEqual({
      createdDefinition: {
        id: 'gid://shopify/MetafieldDefinition/1',
        name: 'Product subtitle',
        namespace: 'descriptors',
        key: 'subtitle',
        ownerType: 'PRODUCT',
        type: {
          name: 'single_line_text_field',
          category: 'TEXT',
        },
        description: 'Used as a shorthand for a product name',
        validations: [{ name: 'max', value: '70' }],
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
        constraints: {
          key: null,
          values: {
            nodes: [],
          },
        },
        pinnedPosition: 1,
        validationStatus: 'ALL_VALID',
      },
      userErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'standardMetafieldDefinitionEnable',
      status: 'staged',
      interpreted: {
        capability: {
          operationName: 'standardMetafieldDefinitionEnable',
          domain: 'metafields',
          execution: 'stage-locally',
        },
      },
      notes: 'Staged locally in the in-memory metafield definition draft store.',
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          query EnabledStandardDefinition {
            byIdentifier: metafieldDefinition(
              identifier: { ownerType: PRODUCT, namespace: "descriptors", key: "subtitle" }
            ) {
              id
              name
              namespace
              key
              ownerType
              pinnedPosition
            }
            catalog: metafieldDefinitions(ownerType: PRODUCT, first: 5, namespace: "descriptors") {
              nodes { id namespace key name }
            }
          }
        `,
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.byIdentifier).toEqual({
      id: 'gid://shopify/MetafieldDefinition/1',
      name: 'Product subtitle',
      namespace: 'descriptors',
      key: 'subtitle',
      ownerType: 'PRODUCT',
      pinnedPosition: 1,
    });
    expect(readResponse.body.data.catalog.nodes).toEqual([
      {
        id: 'gid://shopify/MetafieldDefinition/1',
        namespace: 'descriptors',
        key: 'subtitle',
        name: 'Product subtitle',
      },
    ]);
  });

  it('mirrors captured standard definition enablement validation branches locally', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('validation branches should stay local'));
    const app = createApp(config).callback();

    const validationCases = [
      {
        name: 'missing selector',
        variables: {
          ownerType: 'PRODUCT',
        },
        userErrors: [
          {
            field: null,
            message: 'A namespace and key or standard metafield definition template id must be provided.',
            code: 'TEMPLATE_NOT_FOUND',
          },
        ],
      },
      {
        name: 'unknown id',
        variables: {
          ownerType: 'PRODUCT',
          id: 'gid://shopify/StandardMetafieldDefinitionTemplate/999999999',
        },
        userErrors: [
          {
            field: ['id'],
            message: 'Id is not a valid standard metafield definition template id',
            code: 'TEMPLATE_NOT_FOUND',
          },
        ],
      },
      {
        name: 'unknown namespace/key',
        variables: {
          ownerType: 'PRODUCT',
          namespace: 'codex_missing_standard',
          key: 'codex_missing_key',
        },
        userErrors: [
          {
            field: null,
            message: "A standard definition wasn't found for the specified owner type, namespace, and key.",
            code: 'TEMPLATE_NOT_FOUND',
          },
        ],
      },
      {
        name: 'incompatible owner type',
        variables: {
          ownerType: 'CUSTOMER',
          id: 'gid://shopify/StandardMetafieldDefinitionTemplate/1',
        },
        userErrors: [
          {
            field: ['id'],
            message: 'Id is not a valid standard metafield definition template id',
            code: 'TEMPLATE_NOT_FOUND',
          },
        ],
      },
    ];

    for (const validationCase of validationCases) {
      const response = await request(app)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query: `#graphql
            mutation StandardDefinitionValidation(
              $ownerType: MetafieldOwnerType!
              $id: ID
              $namespace: String
              $key: String
            ) {
              standardMetafieldDefinitionEnable(
                ownerType: $ownerType
                id: $id
                namespace: $namespace
                key: $key
              ) {
                createdDefinition { id }
                userErrors { field message code }
              }
            }
          `,
          variables: validationCase.variables,
        });

      expect(response.status, validationCase.name).toBe(200);
      expect(response.body.data.standardMetafieldDefinitionEnable, validationCase.name).toEqual({
        createdDefinition: null,
        userErrors: validationCase.userErrors,
      });
    }

    expect(globalThis.fetch).not.toHaveBeenCalled();
    expect(store.listEffectiveMetafieldDefinitions()).toEqual([]);
  });

  it('stages metafield definition pinning locally and updates downstream pinned reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('definition pinning must stay local'));
    store.upsertBaseMetafieldDefinitions([
      makeDefinition({
        id: 'gid://shopify/MetafieldDefinition/10',
        namespace: 'existing',
        key: 'first',
        pinnedPosition: 1,
      }),
      makeDefinition({
        id: 'gid://shopify/MetafieldDefinition/11',
        namespace: 'existing',
        key: 'second',
        pinnedPosition: 2,
      }),
      makeDefinition({
        id: 'gid://shopify/MetafieldDefinition/300',
        name: 'Fit',
        namespace: 'custom',
        key: 'fit',
        pinnedPosition: null,
      }),
      makeDefinition({
        id: 'gid://shopify/MetafieldDefinition/301',
        name: 'Care',
        namespace: 'custom',
        key: 'care',
        pinnedPosition: null,
      }),
    ]);
    const app = createApp(config).callback();

    const pinByIdentifierResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation PinByIdentifier($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionPin(identifier: $identifier) {
            pinnedDefinition { id key pinnedPosition }
            userErrors { field message code }
          }
        }`,
        variables: {
          identifier: {
            ownerType: 'PRODUCT',
            namespace: 'custom',
            key: 'fit',
          },
        },
      });

    expect(pinByIdentifierResponse.status).toBe(200);
    expect(pinByIdentifierResponse.body.data.metafieldDefinitionPin).toEqual({
      pinnedDefinition: {
        id: 'gid://shopify/MetafieldDefinition/300',
        key: 'fit',
        pinnedPosition: 3,
      },
      userErrors: [],
    });

    const pinByIdResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation PinById($definitionId: ID!) {
          metafieldDefinitionPin(definitionId: $definitionId) {
            pinnedDefinition { id key pinnedPosition }
            userErrors { field message code }
          }
        }`,
        variables: {
          definitionId: 'gid://shopify/MetafieldDefinition/301',
        },
      });

    expect(pinByIdResponse.status).toBe(200);
    expect(pinByIdResponse.body.data.metafieldDefinitionPin.pinnedDefinition).toEqual({
      id: 'gid://shopify/MetafieldDefinition/301',
      key: 'care',
      pinnedPosition: 4,
    });

    const afterPinsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query {
          pinned: metafieldDefinitions(ownerType: PRODUCT, first: 5, namespace: "custom", sortKey: PINNED_POSITION, pinnedStatus: PINNED) {
            nodes { id key pinnedPosition }
          }
        }`,
      });

    expect(afterPinsResponse.body.data.pinned.nodes).toEqual([
      {
        id: 'gid://shopify/MetafieldDefinition/301',
        key: 'care',
        pinnedPosition: 4,
      },
      {
        id: 'gid://shopify/MetafieldDefinition/300',
        key: 'fit',
        pinnedPosition: 3,
      },
    ]);

    const unpinResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UnpinByIdentifier($identifier: MetafieldDefinitionIdentifierInput!) {
          metafieldDefinitionUnpin(identifier: $identifier) {
            unpinnedDefinition { id key pinnedPosition }
            userErrors { field message code }
          }
        }`,
        variables: {
          identifier: {
            ownerType: 'PRODUCT',
            namespace: 'custom',
            key: 'fit',
          },
        },
      });

    expect(unpinResponse.status).toBe(200);
    expect(unpinResponse.body.data.metafieldDefinitionUnpin).toEqual({
      unpinnedDefinition: {
        id: 'gid://shopify/MetafieldDefinition/300',
        key: 'fit',
        pinnedPosition: null,
      },
      userErrors: [],
    });

    const afterUnpinResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query {
          pinned: metafieldDefinitions(ownerType: PRODUCT, first: 5, namespace: "custom", sortKey: PINNED_POSITION, pinnedStatus: PINNED) {
            nodes { id key pinnedPosition }
          }
          unpinned: metafieldDefinitions(ownerType: PRODUCT, first: 5, namespace: "custom", sortKey: PINNED_POSITION, pinnedStatus: UNPINNED) {
            nodes { id key pinnedPosition }
          }
        }`,
      });

    expect(afterUnpinResponse.body.data.pinned.nodes).toEqual([
      {
        id: 'gid://shopify/MetafieldDefinition/301',
        key: 'care',
        pinnedPosition: 3,
      },
    ]);
    expect(afterUnpinResponse.body.data.unpinned.nodes).toEqual([
      {
        id: 'gid://shopify/MetafieldDefinition/300',
        key: 'fit',
        pinnedPosition: null,
      },
    ]);
    expect(store.getLog()).toHaveLength(3);
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });
});
