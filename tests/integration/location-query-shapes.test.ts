import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import type { ProductRecord, ProductVariantRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeProduct(id: string, title: string): ProductRecord {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? null,
    title,
    handle: title.toLowerCase().replace(/\s+/g, '-'),
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '2025-01-01T00:00:00.000Z',
    updatedAt: '2025-01-01T00:00:00.000Z',
    vendor: null,
    productType: null,
    tags: [],
    totalInventory: 0,
    tracksInventory: true,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: {
      title: null,
      description: null,
    },
    category: null,
  };
}

function makeVariant(productId: string, variantId: string, inventoryItemId: string): ProductVariantRecord {
  return {
    id: variantId,
    productId,
    title: 'Default Title',
    sku: null,
    barcode: null,
    price: null,
    compareAtPrice: null,
    taxable: null,
    inventoryPolicy: null,
    inventoryQuantity: 0,
    selectedOptions: [],
    inventoryItem: {
      id: inventoryItemId,
      tracked: true,
      requiresShipping: true,
      measurement: null,
      countryCodeOfOrigin: null,
      provinceCodeOfOrigin: null,
      harmonizedSystemCode: null,
      inventoryLevels: [
        {
          id: 'gid://shopify/InventoryLevel/1',
          cursor: 'opaque-location-1',
          location: {
            id: 'gid://shopify/Location/1',
            name: 'Alpha Warehouse',
          },
          quantities: [
            {
              name: 'available',
              quantity: 4,
              updatedAt: '2025-01-02T00:00:00.000Z',
            },
          ],
        },
        {
          id: 'gid://shopify/InventoryLevel/2',
          cursor: 'opaque-location-2',
          location: {
            id: 'gid://shopify/Location/2',
            name: 'Beta Warehouse',
          },
          quantities: [
            {
              name: 'available',
              quantity: 6,
              updatedAt: '2025-01-02T00:00:00.000Z',
            },
          ],
        },
      ],
    },
  };
}

describe('location query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves top-level locations from effective inventory levels without hitting upstream in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('locations should resolve locally in snapshot mode');
    });

    const firstProduct = makeProduct('gid://shopify/Product/1', 'Alpha Product');
    const secondProduct = makeProduct('gid://shopify/Product/2', 'Beta Product');
    store.upsertBaseProducts([firstProduct, secondProduct]);

    store.replaceBaseVariantsForProduct(
      firstProduct.id,
      [makeVariant(firstProduct.id, 'gid://shopify/ProductVariant/1', 'gid://shopify/InventoryItem/1')],
    );
    store.replaceBaseVariantsForProduct(secondProduct.id, [
      {
        ...makeVariant(secondProduct.id, 'gid://shopify/ProductVariant/2', 'gid://shopify/InventoryItem/2'),
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/2',
          tracked: true,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: [
            {
              id: 'gid://shopify/InventoryLevel/3',
              cursor: 'opaque-location-1-repeat',
              location: {
                id: 'gid://shopify/Location/1',
                name: 'Alpha Warehouse',
              },
              quantities: [
                {
                  name: 'available',
                  quantity: 3,
                  updatedAt: '2025-01-02T00:00:00.000Z',
                },
              ],
            },
          ],
        },
      },
    ]);

    const app = createApp(config).callback();

    const firstPage = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query LocationCatalog($first: Int!, $after: String) { locations(first: $first, after: $after) { edges { cursor node { id name } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: { first: 1 },
      });

    expect(firstPage.status).toBe(200);
    expect(firstPage.body).toEqual({
      data: {
        locations: {
          edges: [
            {
              cursor: 'cursor:gid://shopify/Location/1',
              node: {
                id: 'gid://shopify/Location/1',
                name: 'Alpha Warehouse',
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: 'cursor:gid://shopify/Location/1',
            endCursor: 'cursor:gid://shopify/Location/1',
          },
        },
      },
    });

    const secondPage = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query LocationCatalog($first: Int!, $after: String) { locations(first: $first, after: $after) { nodes { id name } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: { first: 10, after: 'cursor:gid://shopify/Location/1' },
      });

    expect(secondPage.status).toBe(200);
    expect(secondPage.body).toEqual({
      data: {
        locations: {
          nodes: [
            {
              id: 'gid://shopify/Location/2',
              name: 'Beta Warehouse',
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: true,
            startCursor: 'cursor:gid://shopify/Location/2',
            endCursor: 'cursor:gid://shopify/Location/2',
          },
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
