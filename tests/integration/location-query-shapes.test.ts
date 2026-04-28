import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../support/runtime.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
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

  it('returns an empty top-level locations connection when no local locations exist', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('empty locations should resolve locally in snapshot mode');
    });
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query EmptyLocations {
          locations(first: 5) {
            edges {
              cursor
              node { id }
            }
            nodes { id }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        locations: {
          edges: [],
          nodes: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
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
          unknownCustomIdentifier: locationByIdentifier(
            identifier: { customId: { namespace: "custom", key: "location_code", value: "missing" } }
          ) {
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
        unknownCustomIdentifier: null,
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

  it('stages locationAdd locally and exposes the new location through reads and meta state', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('locationAdd should stage locally in snapshot mode');
    });
    const app = createApp(config).callback();

    const mutationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation AddLocation($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location {
              id
              name
              fulfillsOnlineOrders
              address { countryCode zip }
              metafield(namespace: "my_field", key: "delivery_type") { namespace key value type ownerType }
              metafields(first: 3) { edges { cursor node { namespace key value type ownerType } } }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            name: 'Staged Warehouse',
            address: { countryCode: 'US', zip: '10006' },
            fulfillsOnlineOrders: false,
            metafields: [
              {
                namespace: 'my_field',
                key: 'delivery_type',
                type: 'single_line_text_field',
                value: 'local',
              },
            ],
          },
        },
      });

    const location = mutationResponse.body.data.locationAdd.location;
    expect(mutationResponse.status).toBe(200);
    expect(mutationResponse.body.data.locationAdd.userErrors).toEqual([]);
    expect(location).toEqual({
      id: 'gid://shopify/Location/1?shopify-draft-proxy=synthetic',
      name: 'Staged Warehouse',
      fulfillsOnlineOrders: false,
      address: { countryCode: 'US', zip: '10006' },
      metafield: {
        namespace: 'my_field',
        key: 'delivery_type',
        value: 'local',
        type: 'single_line_text_field',
        ownerType: 'LOCATION',
      },
      metafields: {
        edges: [
          {
            cursor: 'cursor:gid://shopify/Metafield/2',
            node: {
              namespace: 'my_field',
              key: 'delivery_type',
              value: 'local',
              type: 'single_line_text_field',
              ownerType: 'LOCATION',
            },
          },
        ],
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadStagedLocation($id: ID!) {
          location(id: $id) { id name fulfillsOnlineOrders address { countryCode zip } }
          locationByIdentifier(identifier: { id: $id }) { id name }
        }`,
        variables: { id: location.id },
      });

    expect(readResponse.body.data).toEqual({
      location: {
        id: location.id,
        name: 'Staged Warehouse',
        fulfillsOnlineOrders: false,
        address: { countryCode: 'US', zip: '10006' },
      },
      locationByIdentifier: {
        id: location.id,
        name: 'Staged Warehouse',
      },
    });

    const catalogResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: 'query { locations(first: 5) { nodes { id name } } }',
    });

    expect(catalogResponse.body.data.locations.nodes).toEqual([{ id: location.id, name: 'Staged Warehouse' }]);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries).toMatchObject([
      {
        operationName: 'locationAdd',
        status: 'staged',
        stagedResourceIds: [location.id],
        interpreted: {
          primaryRootField: 'locationAdd',
          capability: { domain: 'store-properties', execution: 'stage-locally' },
        },
      },
    ]);

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.locations[location.id]).toMatchObject({
      id: location.id,
      name: 'Staged Warehouse',
      fulfillsOnlineOrders: false,
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages locationEdit locally and downstream inventory location reads observe edits', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('locationEdit should stage locally in snapshot mode');
    });
    store.upsertBaseLocations([
      {
        id: 'gid://shopify/Location/1',
        name: 'Alpha Warehouse',
        fulfillsOnlineOrders: true,
        address: {
          address1: 'Old street',
          address2: null,
          city: null,
          country: 'US',
          countryCode: 'US',
          formatted: ['Old street', 'US'],
          latitude: null,
          longitude: null,
          phone: null,
          province: null,
          provinceCode: null,
          zip: '10001',
        },
      },
    ]);
    const product = makeProduct('gid://shopify/Product/1', 'Alpha Product');
    store.upsertBaseProducts([product]);
    store.replaceBaseVariantsForProduct(product.id, [
      makeVariant(product.id, 'gid://shopify/ProductVariant/1', 'gid://shopify/InventoryItem/1'),
    ]);
    const app = createApp(config).callback();

    const mutationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation EditLocation($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location { id name fulfillsOnlineOrders address { address1 countryCode zip } }
            userErrors { field message }
          }
        }`,
        variables: {
          id: 'gid://shopify/Location/1',
          input: {
            name: 'Edited Warehouse',
            address: { address1: 'New street', zip: '10002' },
            fulfillsOnlineOrders: false,
          },
        },
      });

    expect(mutationResponse.body.data.locationEdit).toEqual({
      location: {
        id: 'gid://shopify/Location/1',
        name: 'Edited Warehouse',
        fulfillsOnlineOrders: false,
        address: { address1: 'New street', countryCode: 'US', zip: '10002' },
      },
      userErrors: [],
    });

    const inventoryResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query($inventoryItemId: ID!) {
          inventoryItem(id: $inventoryItemId) {
            inventoryLevels(first: 5) { nodes { location { id name } } }
          }
        }`,
        variables: { inventoryItemId: 'gid://shopify/InventoryItem/1' },
      });

    expect(inventoryResponse.body.data.inventoryItem.inventoryLevels.nodes[0].location).toEqual({
      id: 'gid://shopify/Location/1',
      name: 'Edited Warehouse',
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns captured locationAdd/locationEdit validation userErrors without staging changes', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid location mutations should resolve locally in snapshot mode');
    });
    store.upsertBaseLocations([{ id: 'gid://shopify/Location/1', name: 'Alpha Warehouse' }]);
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation {
          blankAdd: locationAdd(input: { name: "", address: { countryCode: US } }) {
            location { id }
            userErrors { field message }
          }
          blankEdit: locationEdit(id: "gid://shopify/Location/1", input: { name: "" }) {
            location { id }
            userErrors { field message }
          }
          missingEdit: locationEdit(id: "gid://shopify/Location/999999999999", input: { name: "Nope" }) {
            location { id }
            userErrors { field message }
          }
        }`,
      });

    expect(response.body.data).toEqual({
      blankAdd: {
        location: null,
        userErrors: [{ field: ['input', 'name'], message: 'Add a location name' }],
      },
      blankEdit: {
        location: null,
        userErrors: [{ field: ['input', 'name'], message: 'Add a location name' }],
      },
      missingEdit: {
        location: null,
        userErrors: [{ field: ['id'], message: 'Location not found.' }],
      },
    });
    expect(store.listEffectiveLocations()).toEqual([{ id: 'gid://shopify/Location/1', name: 'Alpha Warehouse' }]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages locationDeactivate and locationActivate locally with inventory transfer read-after-write', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('location lifecycle mutations should stage locally in snapshot mode');
    });
    store.upsertBaseLocations([
      {
        id: 'gid://shopify/Location/1',
        name: 'Alpha Warehouse',
        isActive: true,
        deactivatable: true,
        hasActiveInventory: true,
      },
      {
        id: 'gid://shopify/Location/2',
        name: 'Beta Warehouse',
        isActive: true,
        deactivatable: true,
      },
    ]);
    const product = makeProduct('gid://shopify/Product/1', 'Alpha Product');
    store.upsertBaseProducts([product]);
    store.replaceBaseVariantsForProduct(product.id, [
      makeVariant(product.id, 'gid://shopify/ProductVariant/1', 'gid://shopify/InventoryItem/1'),
    ]);
    const app = createApp(config).callback();

    const deactivateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeactivateLocation($sourceId: ID!, $destinationId: ID!) {
          locationDeactivate(locationId: $sourceId, destinationLocationId: $destinationId) @idempotent(key: "deactivate-location-1") {
            location { id name isActive activatable deactivatable deactivatedAt deletable shipsInventory hasActiveInventory }
            locationDeactivateUserErrors { field message code }
          }
        }`,
        variables: {
          sourceId: 'gid://shopify/Location/1',
          destinationId: 'gid://shopify/Location/2',
        },
      });

    expect(deactivateResponse.status).toBe(200);
    expect(deactivateResponse.body.data.locationDeactivate).toEqual({
      location: {
        id: 'gid://shopify/Location/1',
        name: 'Alpha Warehouse',
        isActive: false,
        activatable: true,
        deactivatable: false,
        deactivatedAt: '2024-01-01T00:00:00.000Z',
        deletable: true,
        shipsInventory: false,
        hasActiveInventory: false,
      },
      locationDeactivateUserErrors: [],
    });

    const inventoryResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query($inventoryItemId: ID!) {
          inventoryItem(id: $inventoryItemId) {
            inventoryLevels(first: 5) {
              nodes {
                location { id name }
                quantities(names: ["available"]) { name quantity }
              }
            }
          }
        }`,
        variables: { inventoryItemId: 'gid://shopify/InventoryItem/1' },
      });

    expect(inventoryResponse.body.data.inventoryItem.inventoryLevels.nodes).toEqual([
      {
        location: { id: 'gid://shopify/Location/2', name: 'Beta Warehouse' },
        quantities: [{ name: 'available', quantity: 10 }],
      },
    ]);

    const catalogAfterDeactivateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query {
          locations(first: 5) {
            nodes {
              id
              name
              isActive
              activatable
              deactivatable
              deactivatedAt
              deletable
              fulfillsOnlineOrders
              hasActiveInventory
              shipsInventory
            }
          }
        }`,
      });

    expect(catalogAfterDeactivateResponse.body.data.locations.nodes).toEqual([
      {
        id: 'gid://shopify/Location/1',
        name: 'Alpha Warehouse',
        isActive: false,
        activatable: true,
        deactivatable: false,
        deactivatedAt: '2024-01-01T00:00:00.000Z',
        deletable: true,
        fulfillsOnlineOrders: false,
        hasActiveInventory: false,
        shipsInventory: false,
      },
      {
        id: 'gid://shopify/Location/2',
        name: 'Beta Warehouse',
        isActive: true,
        activatable: true,
        deactivatable: true,
        deactivatedAt: null,
        deletable: false,
        fulfillsOnlineOrders: true,
        hasActiveInventory: true,
        shipsInventory: true,
      },
    ]);

    const activateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ActivateLocation($id: ID!) {
          locationActivate(locationId: $id) @idempotent(key: "activate-location-1") {
            location { id isActive activatable deactivatable deactivatedAt deletable shipsInventory }
            locationActivateUserErrors { field message code }
          }
        }`,
        variables: { id: 'gid://shopify/Location/1' },
      });

    expect(activateResponse.body.data.locationActivate).toEqual({
      location: {
        id: 'gid://shopify/Location/1',
        isActive: true,
        activatable: false,
        deactivatable: true,
        deactivatedAt: null,
        deletable: false,
        shipsInventory: true,
      },
      locationActivateUserErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('validates location lifecycle constraints and removes deleted locations from reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('locationDelete should stage locally in snapshot mode');
    });
    store.upsertBaseLocations([
      {
        id: 'gid://shopify/Location/3',
        name: 'Gamma Warehouse',
        isActive: true,
        deactivatable: true,
      },
    ]);
    const app = createApp(config).callback();

    const activeDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteActiveLocation($id: ID!) {
          locationDelete(locationId: $id) {
            deletedLocationId
            locationDeleteUserErrors { field message code }
          }
        }`,
        variables: { id: 'gid://shopify/Location/3' },
      });

    expect(activeDeleteResponse.body.data.locationDelete).toEqual({
      deletedLocationId: null,
      locationDeleteUserErrors: [
        {
          field: ['locationId'],
          message: 'The location cannot be deleted while it is active.',
          code: 'LOCATION_IS_ACTIVE',
        },
      ],
    });

    await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeactivateLocation($id: ID!) {
          locationDeactivate(locationId: $id) @idempotent(key: "deactivate-gamma") {
            location { id }
            locationDeactivateUserErrors { field message code }
          }
        }`,
        variables: { id: 'gid://shopify/Location/3' },
      });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteInactiveLocation($id: ID!) {
          locationDelete(locationId: $id) {
            deletedLocationId
            locationDeleteUserErrors { field message code }
          }
        }`,
        variables: { id: 'gid://shopify/Location/3' },
      });

    expect(deleteResponse.body.data.locationDelete).toEqual({
      deletedLocationId: 'gid://shopify/Location/3',
      locationDeleteUserErrors: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query($id: ID!) {
          location(id: $id) { id }
          locationByIdentifier(identifier: { id: $id }) { id }
        }`,
        variables: { id: 'gid://shopify/Location/3' },
      });

    expect(readResponse.body.data).toEqual({
      location: null,
      locationByIdentifier: null,
    });

    const catalogResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: 'query { locations(first: 5) { nodes { id name } } }',
    });

    expect(catalogResponse.body.data.locations).toEqual({ nodes: [] });

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.locations['gid://shopify/Location/3']).toMatchObject({
      id: 'gid://shopify/Location/3',
      deleted: true,
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('requires the 2026-04 idempotent directive for locationActivate and locationDeactivate', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('idempotency validation should resolve locally before upstream');
    });
    store.upsertBaseLocations([{ id: 'gid://shopify/Location/4', name: 'Delta Warehouse', isActive: false }]);
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation ActivateWithoutKey($id: ID!) {
          locationActivate(locationId: $id) {
            location { id }
            locationActivateUserErrors { field message code }
          }
        }`,
        variables: { id: 'gid://shopify/Location/4' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toMatchObject({
      errors: [
        {
          message: 'The @idempotent directive is required for this mutation but was not provided.',
          path: ['locationActivate'],
          extensions: {
            code: 'BAD_REQUEST',
          },
        },
      ],
      data: {
        locationActivate: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
