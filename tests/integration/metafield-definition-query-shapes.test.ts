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
});
