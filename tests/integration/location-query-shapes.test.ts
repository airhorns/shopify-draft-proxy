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

    store.replaceBaseVariantsForProduct(firstProduct.id, [
      makeVariant(firstProduct.id, 'gid://shopify/ProductVariant/1', 'gid://shopify/InventoryItem/1'),
    ]);
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

  it('serves location detail reads from the effective inventory-level graph in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('location detail should resolve locally in snapshot mode');
    });

    const product = makeProduct('gid://shopify/Product/1', 'Alpha Product');
    store.upsertBaseProducts([product]);
    store.replaceBaseVariantsForProduct(product.id, [
      makeVariant(product.id, 'gid://shopify/ProductVariant/1', 'gid://shopify/InventoryItem/1'),
    ]);

    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query LocationDetail($id: ID!, $inventoryItemId: ID!) {
          location(id: $id) {
            id
            legacyResourceId
            name
            isActive
            activatable
            deactivatable
            deactivatedAt
            deletable
            fulfillsOnlineOrders
            hasActiveInventory
            hasUnfulfilledOrders
            isFulfillmentService
            shipsInventory
            address {
              address1
              address2
              city
              country
              countryCode
              formatted
              latitude
              longitude
              phone
              province
              provinceCode
              zip
            }
            suggestedAddresses {
              address1
              countryCode
              formatted
            }
            fulfillmentService {
              id
              handle
            }
            metafield(namespace: "custom", key: "hours") {
              id
            }
            metafields(first: 5) {
              nodes {
                id
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            inventoryLevel(inventoryItemId: $inventoryItemId) {
              id
              location {
                id
                name
              }
              item {
                id
              }
              quantities(names: ["available", "committed"]) {
                name
                quantity
                updatedAt
              }
            }
            inventoryLevels(first: 5) {
              nodes {
                id
                location {
                  id
                  name
                }
                item {
                  id
                }
                quantities(names: ["available"]) {
                  name
                  quantity
                  updatedAt
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
          }
        }`,
        variables: {
          id: 'gid://shopify/Location/1',
          inventoryItemId: 'gid://shopify/InventoryItem/1',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        location: {
          id: 'gid://shopify/Location/1',
          legacyResourceId: '1',
          name: 'Alpha Warehouse',
          isActive: true,
          activatable: true,
          deactivatable: false,
          deactivatedAt: null,
          deletable: false,
          fulfillsOnlineOrders: true,
          hasActiveInventory: true,
          hasUnfulfilledOrders: false,
          isFulfillmentService: false,
          shipsInventory: true,
          address: {
            address1: null,
            address2: null,
            city: null,
            country: null,
            countryCode: null,
            formatted: [],
            latitude: null,
            longitude: null,
            phone: null,
            province: null,
            provinceCode: null,
            zip: null,
          },
          suggestedAddresses: [],
          fulfillmentService: null,
          metafield: null,
          metafields: {
            nodes: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: null,
              endCursor: null,
            },
          },
          inventoryLevel: {
            id: 'gid://shopify/InventoryLevel/1',
            location: {
              id: 'gid://shopify/Location/1',
              name: 'Alpha Warehouse',
            },
            item: {
              id: 'gid://shopify/InventoryItem/1',
            },
            quantities: [
              {
                name: 'available',
                quantity: 4,
                updatedAt: '2025-01-02T00:00:00.000Z',
              },
              {
                name: 'committed',
                quantity: 0,
                updatedAt: null,
              },
            ],
          },
          inventoryLevels: {
            nodes: [
              {
                id: 'gid://shopify/InventoryLevel/1',
                location: {
                  id: 'gid://shopify/Location/1',
                  name: 'Alpha Warehouse',
                },
                item: {
                  id: 'gid://shopify/InventoryItem/1',
                },
                quantities: [
                  {
                    name: 'available',
                    quantity: 4,
                    updatedAt: '2025-01-02T00:00:00.000Z',
                  },
                ],
              },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: 'opaque-location-1',
              endCursor: 'opaque-location-1',
            },
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('supports primary-location fallback and identifier lookup null behavior', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('location lookup should resolve locally in snapshot mode');
    });

    const product = makeProduct('gid://shopify/Product/1', 'Alpha Product');
    store.upsertBaseProducts([product]);
    store.replaceBaseVariantsForProduct(product.id, [
      makeVariant(product.id, 'gid://shopify/ProductVariant/1', 'gid://shopify/InventoryItem/1'),
    ]);

    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query LocationIdentifierLookup($knownId: ID!, $unknownId: ID!) {
          primary: location {
            id
            name
          }
          known: locationByIdentifier(identifier: { id: $knownId }) {
            id
            name
          }
          unknownLocation: location(id: $unknownId) {
            id
          }
          unknownIdentifier: locationByIdentifier(identifier: { id: $unknownId }) {
            id
          }
        }`,
        variables: {
          knownId: 'gid://shopify/Location/2',
          unknownId: 'gid://shopify/Location/999999999999',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        primary: {
          id: 'gid://shopify/Location/1',
          name: 'Alpha Warehouse',
        },
        known: {
          id: 'gid://shopify/Location/2',
          name: 'Beta Warehouse',
        },
        unknownLocation: null,
        unknownIdentifier: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns Shopify-like validation errors for invalid location identifiers', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid location identifier should resolve locally in snapshot mode');
    });

    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query InvalidLocationIdentifier {
          emptyIdentifier: locationByIdentifier(identifier: {}) {
            id
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message: "OneOf Input Object 'LocationIdentifierInput' must specify exactly one key.",
          path: ['locationByIdentifier', 'identifier'],
          extensions: {
            code: 'invalidOneOfInputObject',
            inputObjectType: 'LocationIdentifierInput',
          },
        },
      ],
    });
  });
});
