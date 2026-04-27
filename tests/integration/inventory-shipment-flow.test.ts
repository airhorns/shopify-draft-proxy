import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { LocationRecord, ProductRecord, ProductVariantRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};
type TestApp = ReturnType<ReturnType<typeof createApp>['callback']>;

function makeProduct(id: string): ProductRecord {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? null,
    title: 'Shipment Hoodie',
    handle: 'shipment-hoodie',
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '2026-04-01T00:00:00.000Z',
    updatedAt: '2026-04-02T00:00:00.000Z',
    vendor: 'ACME',
    productType: 'APPAREL',
    tags: ['inventory'],
    totalInventory: 1,
    tracksInventory: true,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: { title: null, description: null },
    category: null,
  };
}

function makeLocation(): LocationRecord {
  return {
    id: 'gid://shopify/Location/1',
    name: 'Shop location',
    legacyResourceId: '1',
    isActive: true,
  };
}

function makeVariant(productId: string): ProductVariantRecord {
  return {
    id: 'gid://shopify/ProductVariant/94051',
    productId,
    title: 'Default Title',
    sku: 'SHIP-ROOT',
    barcode: null,
    price: '25.00',
    compareAtPrice: null,
    taxable: true,
    inventoryPolicy: 'DENY',
    inventoryQuantity: 1,
    selectedOptions: [],
    inventoryItem: {
      id: 'gid://shopify/InventoryItem/94051',
      tracked: true,
      requiresShipping: true,
      measurement: null,
      countryCodeOfOrigin: null,
      provinceCodeOfOrigin: null,
      harmonizedSystemCode: null,
      inventoryLevels: [
        {
          id: 'gid://shopify/InventoryLevel/94051-1?inventory_item_id=94051',
          cursor: 'opaque-inventory-level-1',
          location: { id: 'gid://shopify/Location/1', name: 'Shop location' },
          quantities: [
            { name: 'available', quantity: 1, updatedAt: '2026-04-01T00:00:00.000Z' },
            { name: 'on_hand', quantity: 1, updatedAt: null },
            { name: 'incoming', quantity: 0, updatedAt: null },
          ],
        },
      ],
    },
  };
}

function seedInventoryProduct(): void {
  const product = makeProduct('gid://shopify/Product/9405');
  store.upsertBaseLocations([makeLocation()]);
  store.upsertBaseProducts([product]);
  store.replaceBaseVariantsForProduct(product.id, [makeVariant(product.id)]);
}

async function readInventory(app: TestApp): Promise<Record<string, unknown>> {
  const response = await request(app)
    .post('/admin/api/2025-01/graphql.json')
    .send({
      query: `query InspectInventory($id: ID!) {
        inventoryItem(id: $id) {
          id
          inventoryLevels(first: 5) {
            nodes {
              location { id name }
              quantities(names: ["available", "on_hand", "incoming"]) { name quantity }
            }
          }
          variant { id inventoryQuantity }
        }
      }`,
      variables: { id: 'gid://shopify/InventoryItem/94051' },
    });
  expect(response.status).toBe(200);
  return response.body.data.inventoryItem as Record<string, unknown>;
}

describe('inventory shipment lifecycle roots', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages shipment create, tracking, transit, receive, reads, downstream inventory, and raw log order locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventory shipment roots should not hit upstream fetch in snapshot mode');
    });
    seedInventoryProduct();
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CreateShipment($input: InventoryShipmentCreateInput!) {
          inventoryShipmentCreate(input: $input) {
            inventoryShipment {
              id
              name
              status
              lineItemTotalQuantity
              totalReceivedQuantity
              tracking { trackingNumber company trackingUrl arrivesAt }
              lineItems(first: 10) {
                nodes {
                  id
                  quantity
                  acceptedQuantity
                  rejectedQuantity
                  unreceivedQuantity
                  inventoryItem { id sku tracked }
                }
              }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            movementId: 'gid://shopify/InventoryTransfer/7001',
            trackingInput: {
              trackingNumber: '1Z999',
              company: 'UPS',
              trackingUrl: 'https://example.test/track/1Z999',
              arrivesAt: '2026-04-30T00:00:00.000Z',
            },
            lineItems: [{ inventoryItemId: 'gid://shopify/InventoryItem/94051', quantity: 5 }],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.inventoryShipmentCreate.userErrors).toEqual([]);
    const shipment = createResponse.body.data.inventoryShipmentCreate.inventoryShipment;
    const shipmentId = shipment.id as string;
    const lineItemId = shipment.lineItems.nodes[0].id as string;
    expect(shipment).toMatchObject({
      name: '#S1',
      status: 'DRAFT',
      lineItemTotalQuantity: 5,
      totalReceivedQuantity: 0,
      tracking: {
        trackingNumber: '1Z999',
        company: 'UPS',
        trackingUrl: 'https://example.test/track/1Z999',
        arrivesAt: '2026-04-30T00:00:00.000Z',
      },
      lineItems: {
        nodes: [
          {
            quantity: 5,
            acceptedQuantity: 0,
            rejectedQuantity: 0,
            unreceivedQuantity: 5,
            inventoryItem: { id: 'gid://shopify/InventoryItem/94051', sku: 'SHIP-ROOT', tracked: true },
          },
        ],
      },
    });
    expect(await readInventory(app)).toMatchObject({
      variant: { inventoryQuantity: 1 },
      inventoryLevels: {
        nodes: [
          {
            quantities: [
              { name: 'available', quantity: 1 },
              { name: 'on_hand', quantity: 1 },
              { name: 'incoming', quantity: 0 },
            ],
          },
        ],
      },
    });

    const transitResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation Transit($id: ID!) {
          inventoryShipmentMarkInTransit(id: $id) {
            inventoryShipment { id status totalReceivedQuantity totalRejectedQuantity }
            userErrors { field message code }
          }
        }`,
        variables: { id: shipmentId },
      });
    expect(transitResponse.body.data.inventoryShipmentMarkInTransit).toEqual({
      inventoryShipment: {
        id: shipmentId,
        status: 'IN_TRANSIT',
        totalReceivedQuantity: 0,
        totalRejectedQuantity: 0,
      },
      userErrors: [],
    });
    expect(await readInventory(app)).toMatchObject({
      variant: { inventoryQuantity: 1 },
      inventoryLevels: { nodes: [{ quantities: expect.arrayContaining([{ name: 'incoming', quantity: 5 }]) }] },
    });

    const receiveResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation Receive($id: ID!, $lineItems: [InventoryShipmentReceiveItemInput!]) {
          inventoryShipmentReceive(id: $id, lineItems: $lineItems) {
            inventoryShipment {
              id
              status
              totalAcceptedQuantity
              totalReceivedQuantity
              totalRejectedQuantity
              lineItems(first: 10) { nodes { id acceptedQuantity rejectedQuantity unreceivedQuantity } }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: shipmentId,
          lineItems: [{ shipmentLineItemId: lineItemId, quantity: 3, reason: 'ACCEPTED' }],
        },
      });

    expect(receiveResponse.body.data.inventoryShipmentReceive).toEqual({
      inventoryShipment: {
        id: shipmentId,
        status: 'PARTIALLY_RECEIVED',
        totalAcceptedQuantity: 3,
        totalReceivedQuantity: 3,
        totalRejectedQuantity: 0,
        lineItems: { nodes: [{ id: lineItemId, acceptedQuantity: 3, rejectedQuantity: 0, unreceivedQuantity: 2 }] },
      },
      userErrors: [],
    });
    expect(await readInventory(app)).toMatchObject({
      variant: { inventoryQuantity: 4 },
      inventoryLevels: {
        nodes: [
          {
            quantities: [
              { name: 'available', quantity: 4 },
              { name: 'on_hand', quantity: 4 },
              { name: 'incoming', quantity: 2 },
            ],
          },
        ],
      },
    });

    const detailResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query Shipment($id: ID!) {
          inventoryShipment(id: $id) {
            id
            status
            lineItemsCount { count precision }
            lineItems(first: 1) { nodes { id quantity unreceivedQuantity } pageInfo { hasNextPage hasPreviousPage } }
          }
          missing: inventoryShipment(id: "gid://shopify/InventoryShipment/does-not-exist") { id }
        }`,
        variables: { id: shipmentId },
      });

    expect(detailResponse.body.data.inventoryShipment).toEqual({
      id: shipmentId,
      status: 'PARTIALLY_RECEIVED',
      lineItemsCount: { count: 1, precision: 'EXACT' },
      lineItems: {
        nodes: [{ id: lineItemId, quantity: 5, unreceivedQuantity: 2 }],
        pageInfo: { hasNextPage: false, hasPreviousPage: false },
      },
    });
    expect(detailResponse.body.data.missing).toBeNull();

    expect(store.getLog().map((entry) => entry.operationName)).toEqual([
      'inventoryShipmentCreate',
      'inventoryShipmentMarkInTransit',
      'inventoryShipmentReceive',
    ]);
    expect(store.getLog().map((entry) => entry.requestBody?.['query'])).toEqual([
      expect.stringContaining('inventoryShipmentCreate'),
      expect.stringContaining('inventoryShipmentMarkInTransit'),
      expect.stringContaining('inventoryShipmentReceive'),
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns local userErrors for invalid inventory item ids and shipment status transitions', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventory shipment validation should not hit upstream fetch');
    });
    seedInventoryProduct();
    const app = createApp(config).callback();

    const invalidCreate = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InvalidCreate($input: InventoryShipmentCreateInput!) {
          inventoryShipmentCreate(input: $input) {
            inventoryShipment { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            movementId: 'gid://shopify/InventoryTransfer/7001',
            lineItems: [{ inventoryItemId: 'gid://shopify/InventoryItem/missing', quantity: 1 }],
          },
        },
      });

    expect(invalidCreate.body.data.inventoryShipmentCreate).toEqual({
      inventoryShipment: null,
      userErrors: [
        {
          field: ['input', 'lineItems', '0', 'inventoryItemId'],
          message: 'The specified inventory item could not be found.',
          code: 'NOT_FOUND',
        },
      ],
    });

    const createInTransit = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CreateInTransit($input: InventoryShipmentCreateInput!) {
          inventoryShipmentCreateInTransit(input: $input) {
            inventoryShipment { id status lineItems(first: 1) { nodes { id } } }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            movementId: 'gid://shopify/InventoryTransfer/7001',
            lineItems: [{ inventoryItemId: 'gid://shopify/InventoryItem/94051', quantity: 1 }],
          },
        },
      });
    const shipmentId = createInTransit.body.data.inventoryShipmentCreateInTransit.inventoryShipment.id as string;

    const invalidTransition = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation TransitAgain($id: ID!) {
          inventoryShipmentMarkInTransit(id: $id) {
            inventoryShipment { id }
            userErrors { field message code }
          }
        }`,
        variables: { id: shipmentId },
      });

    expect(invalidTransition.body.data.inventoryShipmentMarkInTransit).toEqual({
      inventoryShipment: null,
      userErrors: [
        {
          field: ['id'],
          message: 'Only draft shipments can be marked in transit.',
          code: 'INVALID_STATUS',
        },
      ],
    });
  });

  it('stages add, quantity update, tracking, removal, and delete effects without upstream writes', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventory shipment lifecycle mutations should not hit upstream fetch');
    });
    seedInventoryProduct();
    const app = createApp(config).callback();

    const createInTransit = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CreateInTransit($input: InventoryShipmentCreateInput!) {
          inventoryShipmentCreateInTransit(input: $input) {
            inventoryShipment {
              id
              status
              lineItems(first: 10) { nodes { id quantity unreceivedQuantity } }
            }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            movementId: 'gid://shopify/InventoryTransfer/7002',
            lineItems: [{ inventoryItemId: 'gid://shopify/InventoryItem/94051', quantity: 2 }],
          },
        },
      });

    expect(createInTransit.status).toBe(200);
    expect(createInTransit.body.data.inventoryShipmentCreateInTransit.userErrors).toEqual([]);
    const shipmentId = createInTransit.body.data.inventoryShipmentCreateInTransit.inventoryShipment.id as string;
    const originalLineItemId = createInTransit.body.data.inventoryShipmentCreateInTransit.inventoryShipment.lineItems
      .nodes[0].id as string;
    expect(await readInventory(app)).toMatchObject({
      inventoryLevels: { nodes: [{ quantities: expect.arrayContaining([{ name: 'incoming', quantity: 2 }]) }] },
    });

    const setTracking = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation SetTracking($id: ID!, $tracking: InventoryShipmentTrackingInput!) {
          inventoryShipmentSetTracking(id: $id, tracking: $tracking) {
            inventoryShipment { id tracking { trackingNumber company trackingUrl } }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: shipmentId,
          tracking: {
            trackingNumber: 'TRACK-2',
            company: 'USPS',
            trackingUrl: 'https://example.test/track/TRACK-2',
          },
        },
      });

    expect(setTracking.body.data.inventoryShipmentSetTracking).toEqual({
      inventoryShipment: {
        id: shipmentId,
        tracking: {
          trackingNumber: 'TRACK-2',
          company: 'USPS',
          trackingUrl: 'https://example.test/track/TRACK-2',
        },
      },
      userErrors: [],
    });

    const addItems = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation AddItems($id: ID!, $lineItems: [InventoryShipmentLineItemInput!]!) {
          inventoryShipmentAddItems(id: $id, lineItems: $lineItems) {
            addedItems { id quantity unreceivedQuantity }
            inventoryShipment { id lineItemTotalQuantity }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: shipmentId,
          lineItems: [{ inventoryItemId: 'gid://shopify/InventoryItem/94051', quantity: 3 }],
        },
      });

    const addedLineItemId = addItems.body.data.inventoryShipmentAddItems.addedItems[0].id as string;
    expect(addItems.body.data.inventoryShipmentAddItems).toMatchObject({
      addedItems: [{ id: addedLineItemId, quantity: 3, unreceivedQuantity: 3 }],
      inventoryShipment: { id: shipmentId, lineItemTotalQuantity: 5 },
      userErrors: [],
    });
    expect(await readInventory(app)).toMatchObject({
      inventoryLevels: { nodes: [{ quantities: expect.arrayContaining([{ name: 'incoming', quantity: 5 }]) }] },
    });

    const updateQuantities = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UpdateQuantities($id: ID!, $items: [InventoryShipmentUpdateItemQuantitiesInput!]) {
          inventoryShipmentUpdateItemQuantities(id: $id, items: $items) {
            shipment { id lineItemTotalQuantity }
            updatedLineItems { id quantity unreceivedQuantity }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: shipmentId,
          items: [{ shipmentLineItemId: addedLineItemId, quantity: 4 }],
        },
      });

    expect(updateQuantities.body.data.inventoryShipmentUpdateItemQuantities).toMatchObject({
      shipment: { id: shipmentId, lineItemTotalQuantity: 6 },
      updatedLineItems: [{ id: addedLineItemId, quantity: 4, unreceivedQuantity: 4 }],
      userErrors: [],
    });
    expect(await readInventory(app)).toMatchObject({
      inventoryLevels: { nodes: [{ quantities: expect.arrayContaining([{ name: 'incoming', quantity: 6 }]) }] },
    });

    const invalidUpdate = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation InvalidUpdate($id: ID!, $items: [InventoryShipmentUpdateItemQuantitiesInput!]) {
          inventoryShipmentUpdateItemQuantities(id: $id, items: $items) {
            shipment { id }
            updatedLineItems { id quantity }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: shipmentId,
          items: [
            { shipmentLineItemId: addedLineItemId, quantity: 5 },
            { shipmentLineItemId: 'gid://shopify/InventoryShipmentLineItem/missing', quantity: 1 },
          ],
        },
      });

    expect(invalidUpdate.body.data.inventoryShipmentUpdateItemQuantities).toMatchObject({
      shipment: null,
      userErrors: [
        {
          field: ['items', '1', 'shipmentLineItemId'],
          message: 'Shipment line item could not be found.',
          code: 'NOT_FOUND',
        },
      ],
    });
    expect(await readInventory(app)).toMatchObject({
      inventoryLevels: { nodes: [{ quantities: expect.arrayContaining([{ name: 'incoming', quantity: 6 }]) }] },
    });

    const removeItems = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation RemoveItems($id: ID!, $lineItems: [ID!]!) {
          inventoryShipmentRemoveItems(id: $id, lineItems: $lineItems) {
            inventoryShipment {
              id
              lineItemTotalQuantity
              lineItems(first: 10) { nodes { id quantity } }
            }
            userErrors { field message code }
          }
        }`,
        variables: { id: shipmentId, lineItems: [originalLineItemId] },
      });

    expect(removeItems.body.data.inventoryShipmentRemoveItems).toEqual({
      inventoryShipment: {
        id: shipmentId,
        lineItemTotalQuantity: 4,
        lineItems: { nodes: [{ id: addedLineItemId, quantity: 4 }] },
      },
      userErrors: [],
    });
    expect(await readInventory(app)).toMatchObject({
      inventoryLevels: { nodes: [{ quantities: expect.arrayContaining([{ name: 'incoming', quantity: 4 }]) }] },
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DeleteShipment($id: ID!) {
          inventoryShipmentDelete(id: $id) {
            id
            userErrors { field message code }
          }
        }`,
        variables: { id: shipmentId },
      });

    expect(deleteResponse.body.data.inventoryShipmentDelete).toEqual({
      id: shipmentId,
      userErrors: [],
    });
    expect(await readInventory(app)).toMatchObject({
      inventoryLevels: { nodes: [{ quantities: expect.arrayContaining([{ name: 'incoming', quantity: 0 }]) }] },
    });

    const readDeleted = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query Shipment($id: ID!) { inventoryShipment(id: $id) { id } }`,
        variables: { id: shipmentId },
      });
    expect(readDeleted.body.data.inventoryShipment).toBeNull();
    expect(store.getLog().map((entry) => entry.operationName)).toEqual([
      'inventoryShipmentCreateInTransit',
      'inventoryShipmentSetTracking',
      'inventoryShipmentAddItems',
      'inventoryShipmentUpdateItemQuantities',
      'inventoryShipmentRemoveItems',
      'inventoryShipmentDelete',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
