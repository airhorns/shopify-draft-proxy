import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { ProductRecord, ProductVariantRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function seedTransferProduct({ tracked = true } = {}): void {
  const product: ProductRecord = {
    id: 'gid://shopify/Product/9701',
    legacyResourceId: '9701',
    title: 'Transfer Hoodie',
    handle: 'transfer-hoodie',
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '2026-04-01T00:00:00.000Z',
    updatedAt: '2026-04-02T00:00:00.000Z',
    vendor: 'ACME',
    productType: 'APPAREL',
    tags: ['inventory-transfer'],
    totalInventory: 5,
    tracksInventory: tracked,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: { title: null, description: null },
    category: null,
  };
  const variant: ProductVariantRecord = {
    id: 'gid://shopify/ProductVariant/97011',
    productId: product.id,
    title: 'Default Title',
    sku: 'TRANSFER-HOODIE',
    barcode: null,
    price: '25.00',
    compareAtPrice: null,
    taxable: true,
    inventoryPolicy: 'DENY',
    inventoryQuantity: 5,
    selectedOptions: [],
    inventoryItem: {
      id: 'gid://shopify/InventoryItem/97011',
      tracked,
      requiresShipping: true,
      measurement: null,
      countryCodeOfOrigin: null,
      provinceCodeOfOrigin: null,
      harmonizedSystemCode: null,
      inventoryLevels: [
        {
          id: 'gid://shopify/InventoryLevel/97011-1?inventory_item_id=97011',
          cursor: 'opaque-transfer-origin',
          location: { id: 'gid://shopify/Location/1', name: 'Shop location' },
          quantities: [
            { name: 'available', quantity: 5, updatedAt: '2026-04-01T00:00:00.000Z' },
            { name: 'reserved', quantity: 0, updatedAt: null },
            { name: 'on_hand', quantity: 5, updatedAt: null },
          ],
        },
        {
          id: 'gid://shopify/InventoryLevel/97011-2?inventory_item_id=97011',
          cursor: 'opaque-transfer-destination',
          location: { id: 'gid://shopify/Location/2', name: 'Overflow location' },
          quantities: [
            { name: 'available', quantity: 0, updatedAt: '2026-04-01T00:00:00.000Z' },
            { name: 'reserved', quantity: 0, updatedAt: null },
            { name: 'on_hand', quantity: 0, updatedAt: null },
          ],
        },
      ],
    },
  };
  store.upsertBaseProducts([product]);
  store.replaceBaseVariantsForProduct(product.id, [variant]);
}

describe('inventory transfer lifecycle roots', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages draft transfer lifecycle roots and preserves local reads/log order', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventory transfer roots should not hit upstream fetch in snapshot mode');
    });
    seedTransferProduct();
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CreateTransfer($input: InventoryTransferCreateInput!) {
          inventoryTransferCreate(input: $input) {
            inventoryTransfer {
              id
              name
              referenceName
              status
              totalQuantity
              receivedQuantity
              origin { name location { id name } }
              destination { name location { id name } }
              lineItems(first: 5) {
                nodes {
                  id
                  title
                  totalQuantity
                  shippableQuantity
                  processableQuantity
                  inventoryItem { id sku tracked }
                }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            originLocationId: 'gid://shopify/Location/1',
            destinationLocationId: 'gid://shopify/Location/2',
            referenceName: 'HAR-307-LOCAL',
            note: 'local transfer',
            tags: ['har-307'],
            lineItems: [{ inventoryItemId: 'gid://shopify/InventoryItem/97011', quantity: 2 }],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.inventoryTransferCreate).toMatchObject({
      inventoryTransfer: {
        name: '#T0001',
        referenceName: 'HAR-307-LOCAL',
        status: 'DRAFT',
        totalQuantity: 2,
        receivedQuantity: 0,
        origin: { name: 'Shop location', location: { id: 'gid://shopify/Location/1', name: 'Shop location' } },
        destination: {
          name: 'Overflow location',
          location: { id: 'gid://shopify/Location/2', name: 'Overflow location' },
        },
        lineItems: {
          nodes: [
            {
              title: 'Transfer Hoodie',
              totalQuantity: 2,
              shippableQuantity: 0,
              processableQuantity: 2,
              inventoryItem: { id: 'gid://shopify/InventoryItem/97011', sku: 'TRANSFER-HOODIE', tracked: true },
            },
          ],
        },
      },
      userErrors: [],
    });
    const transferId = createResponse.body.data.inventoryTransferCreate.inventoryTransfer.id;
    const lineItemId = createResponse.body.data.inventoryTransferCreate.inventoryTransfer.lineItems.nodes[0].id;

    const editResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation EditTransfer($id: ID!, $input: InventoryTransferEditInput!) {
          inventoryTransferEdit(id: $id, input: $input) {
            inventoryTransfer { id note tags referenceName }
            userErrors { field message }
          }
        }`,
        variables: { id: transferId, input: { note: 'edited transfer', tags: ['har-307', 'edited'] } },
      });

    expect(editResponse.body.data.inventoryTransferEdit).toEqual({
      inventoryTransfer: {
        id: transferId,
        note: 'edited transfer',
        tags: ['har-307', 'edited'],
        referenceName: 'HAR-307-LOCAL',
      },
      userErrors: [],
    });

    const setItemsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation SetTransferItems($input: InventoryTransferSetItemsInput!) {
          inventoryTransferSetItems(input: $input) {
            inventoryTransfer { id totalQuantity }
            updatedLineItems { inventoryItemId newQuantity deltaQuantity }
            userErrors { field message }
          }
        }`,
        variables: {
          input: { id: transferId, lineItems: [{ inventoryItemId: 'gid://shopify/InventoryItem/97011', quantity: 3 }] },
        },
      });

    expect(setItemsResponse.body.data.inventoryTransferSetItems).toEqual({
      inventoryTransfer: { id: transferId, totalQuantity: 3 },
      updatedLineItems: [{ inventoryItemId: 'gid://shopify/InventoryItem/97011', newQuantity: 3, deltaQuantity: 1 }],
      userErrors: [],
    });

    const removeResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation RemoveTransferItems($input: InventoryTransferRemoveItemsInput!) {
          inventoryTransferRemoveItems(input: $input) {
            inventoryTransfer { id totalQuantity lineItems(first: 5) { nodes { id } } }
            removedQuantities { inventoryItemId newQuantity deltaQuantity }
            userErrors { field message }
          }
        }`,
        variables: { input: { id: transferId, transferLineItemIds: [lineItemId] } },
      });

    expect(removeResponse.body.data.inventoryTransferRemoveItems).toEqual({
      inventoryTransfer: { id: transferId, totalQuantity: 0, lineItems: { nodes: [] } },
      removedQuantities: [{ inventoryItemId: 'gid://shopify/InventoryItem/97011', newQuantity: 0, deltaQuantity: -3 }],
      userErrors: [],
    });

    expect(store.getLog().map((entry) => entry.operationName)).toEqual([
      'inventoryTransferCreate',
      'inventoryTransferEdit',
      'inventoryTransferSetItems',
      'inventoryTransferRemoveItems',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('reserves origin inventory for ready transfers and releases it on cancel', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventory transfer ready/cancel should not hit upstream fetch');
    });
    seedTransferProduct();
    const app = createApp(config).callback();

    const readyCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation ReadyTransfer($input: InventoryTransferCreateAsReadyToShipInput!) {
          inventoryTransferCreateAsReadyToShip(input: $input) {
            inventoryTransfer {
              id
              status
              lineItems(first: 5) { nodes { totalQuantity shippableQuantity processableQuantity } }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            originLocationId: 'gid://shopify/Location/1',
            destinationLocationId: 'gid://shopify/Location/2',
            lineItems: [{ inventoryItemId: 'gid://shopify/InventoryItem/97011', quantity: 2 }],
          },
        },
      });

    expect(readyCreateResponse.body.data.inventoryTransferCreateAsReadyToShip).toMatchObject({
      inventoryTransfer: {
        status: 'READY_TO_SHIP',
        lineItems: { nodes: [{ totalQuantity: 2, shippableQuantity: 2, processableQuantity: 2 }] },
      },
      userErrors: [],
    });
    const transferId = readyCreateResponse.body.data.inventoryTransferCreateAsReadyToShip.inventoryTransfer.id;

    const reservedRead = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query($id: ID!) {
          inventoryItem(id: $id) {
            variant { inventoryQuantity product { totalInventory } }
            inventoryLevels(first: 5) {
              nodes { location { id } quantities(names: ["available", "reserved", "on_hand"]) { name quantity } }
            }
          }
        }`,
        variables: { id: 'gid://shopify/InventoryItem/97011' },
      });

    expect(reservedRead.body.data.inventoryItem.variant).toEqual({
      inventoryQuantity: 3,
      product: { totalInventory: 5 },
    });
    expect(reservedRead.body.data.inventoryItem.inventoryLevels.nodes[0]).toEqual({
      location: { id: 'gid://shopify/Location/1' },
      quantities: [
        { name: 'available', quantity: 3 },
        { name: 'reserved', quantity: 2 },
        { name: 'on_hand', quantity: 5 },
      ],
    });

    const cancelResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation($id: ID!) {
          inventoryTransferCancel(id: $id) {
            inventoryTransfer { id status }
            userErrors { field message }
          }
        }`,
        variables: { id: transferId },
      });

    expect(cancelResponse.body.data.inventoryTransferCancel).toEqual({
      inventoryTransfer: { id: transferId, status: 'CANCELED' },
      userErrors: [],
    });

    const releasedRead = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query($id: ID!) {
          inventoryItem(id: $id) {
            variant { inventoryQuantity }
            inventoryLevels(first: 5) {
              nodes { location { id } quantities(names: ["available", "reserved"]) { name quantity } }
            }
          }
        }`,
        variables: { id: 'gid://shopify/InventoryItem/97011' },
      });

    expect(releasedRead.body.data.inventoryItem.variant.inventoryQuantity).toBe(5);
    expect(releasedRead.body.data.inventoryItem.inventoryLevels.nodes[0]).toEqual({
      location: { id: 'gid://shopify/Location/1' },
      quantities: [
        { name: 'available', quantity: 5 },
        { name: 'reserved', quantity: 0 },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('overlays staged transfers onto live-hybrid reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            inventoryTransfers: {
              nodes: [],
              pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
            },
          },
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    );
    seedTransferProduct();
    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation($input: InventoryTransferCreateInput!) {
          inventoryTransferCreate(input: $input) {
            inventoryTransfer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            originLocationId: 'gid://shopify/Location/1',
            destinationLocationId: 'gid://shopify/Location/2',
            lineItems: [{ inventoryItemId: 'gid://shopify/InventoryItem/97011', quantity: 2 }],
          },
        },
      });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query {
          inventoryTransfers(first: 5) {
            nodes { status totalQuantity }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }`,
      });

    expect(readResponse.body.data.inventoryTransfers).toEqual({
      nodes: [{ status: 'DRAFT', totalQuantity: 2 }],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/InventoryTransfer/2',
        endCursor: 'cursor:gid://shopify/InventoryTransfer/2',
      },
    });
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });

  it('mirrors tracked-item and missing-transfer validation branches', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventory transfer validation should not hit upstream fetch');
    });
    seedTransferProduct({ tracked: false });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation($input: InventoryTransferCreateInput!) {
          inventoryTransferCreate(input: $input) {
            inventoryTransfer { id }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            originLocationId: 'gid://shopify/Location/1',
            destinationLocationId: 'gid://shopify/Location/2',
            lineItems: [{ inventoryItemId: 'gid://shopify/InventoryItem/97011', quantity: 2 }],
          },
        },
      });

    expect(createResponse.body.data.inventoryTransferCreate).toEqual({
      inventoryTransfer: null,
      userErrors: [
        {
          field: ['input', 'lineItems', '0', 'inventoryItemId'],
          message: 'The inventory item does not track inventory.',
        },
      ],
    });

    const cancelResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation($id: ID!) {
          inventoryTransferCancel(id: $id) {
            inventoryTransfer { id status }
            userErrors { field message }
          }
        }`,
        variables: { id: 'gid://shopify/InventoryTransfer/0' },
      });

    expect(cancelResponse.body.data.inventoryTransferCancel).toEqual({
      inventoryTransfer: null,
      userErrors: [{ field: ['id'], message: "The inventory transfer can't be found." }],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
