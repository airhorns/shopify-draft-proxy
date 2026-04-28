import { readFileSync } from 'node:fs';
import path from 'node:path';

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

const repoRoot = path.resolve(import.meta.dirname, '../..');
const inventoryQuantityContracts202604 = JSON.parse(
  readFileSync(
    path.join(
      repoRoot,
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/inventory-quantity-contracts-2026-04.json',
    ),
    'utf8',
  ),
) as {
  setup: {
    location: { id: string; name: string | null };
    product: { productId: string; variantId: string; inventoryItemId: string };
  };
  inventorySetQuantities: { variables: Record<string, unknown>; response: Record<string, unknown> };
  missingSetIdempotency: { variables: Record<string, unknown>; response: Record<string, unknown> };
  missingSetChangeFromQuantity: { variables: Record<string, unknown>; response: Record<string, unknown> };
  legacySetShape: { variables: Record<string, unknown>; response: Record<string, unknown> };
  inventoryAdjustQuantities: { variables: Record<string, unknown>; response: Record<string, unknown> };
  missingAdjustIdempotency: { variables: Record<string, unknown>; response: Record<string, unknown> };
  missingAdjustChangeFromQuantity: { variables: Record<string, unknown>; response: Record<string, unknown> };
};

function makeProduct(id: string, totalInventory: number): ProductRecord {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? null,
    title: 'Inventory Quantity Hoodie',
    handle: 'inventory-quantity-hoodie',
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '2026-04-01T00:00:00.000Z',
    updatedAt: '2026-04-02T00:00:00.000Z',
    vendor: 'ACME',
    productType: 'APPAREL',
    tags: ['inventory'],
    totalInventory,
    tracksInventory: true,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: { title: null, description: null },
    category: null,
  };
}

function makeVariant(productId: string): ProductVariantRecord {
  return {
    id: 'gid://shopify/ProductVariant/93051',
    productId,
    title: 'Default Title',
    sku: 'INV-ROOT',
    barcode: null,
    price: '25.00',
    compareAtPrice: null,
    taxable: true,
    inventoryPolicy: 'DENY',
    inventoryQuantity: 3,
    selectedOptions: [],
    inventoryItem: {
      id: 'gid://shopify/InventoryItem/93051',
      tracked: true,
      requiresShipping: true,
      measurement: null,
      countryCodeOfOrigin: null,
      provinceCodeOfOrigin: null,
      harmonizedSystemCode: null,
      inventoryLevels: [
        {
          id: 'gid://shopify/InventoryLevel/93051-1?inventory_item_id=93051',
          cursor: 'opaque-inventory-level-1',
          location: { id: 'gid://shopify/Location/1', name: 'Shop location' },
          quantities: [
            { name: 'available', quantity: 1, updatedAt: '2026-04-01T00:00:00.000Z' },
            { name: 'on_hand', quantity: 1, updatedAt: null },
            { name: 'damaged', quantity: 0, updatedAt: null },
            { name: 'incoming', quantity: 0, updatedAt: null },
          ],
        },
        {
          id: 'gid://shopify/InventoryLevel/93051-2?inventory_item_id=93051',
          cursor: 'opaque-inventory-level-2',
          location: { id: 'gid://shopify/Location/2', name: 'Overflow location' },
          quantities: [
            { name: 'available', quantity: 2, updatedAt: '2026-04-01T00:00:00.000Z' },
            { name: 'on_hand', quantity: 2, updatedAt: null },
            { name: 'damaged', quantity: 0, updatedAt: null },
            { name: 'incoming', quantity: 0, updatedAt: null },
          ],
        },
      ],
    },
  };
}

function seedInventoryProduct(totalInventory = 3): void {
  const product = makeProduct('gid://shopify/Product/9305', totalInventory);
  store.upsertBaseProducts([product]);
  store.replaceBaseVariantsForProduct(product.id, [makeVariant(product.id)]);
}

function seedInventoryQuantityContractProduct(): void {
  const { product, location } = inventoryQuantityContracts202604.setup;
  store.upsertBaseProducts([makeProduct(product.productId, 0)]);
  store.replaceBaseVariantsForProduct(product.productId, [
    {
      ...makeVariant(product.productId),
      id: product.variantId,
      productId: product.productId,
      inventoryQuantity: 0,
      inventoryItem: {
        id: product.inventoryItemId,
        tracked: true,
        requiresShipping: true,
        measurement: null,
        countryCodeOfOrigin: null,
        provinceCodeOfOrigin: null,
        harmonizedSystemCode: null,
        inventoryLevels: [
          {
            id: `gid://shopify/InventoryLevel/local?inventory_item_id=${encodeURIComponent(product.inventoryItemId)}`,
            cursor: 'contract-inventory-level',
            location,
            quantities: [
              { name: 'available', quantity: 0, updatedAt: null },
              { name: 'on_hand', quantity: 0, updatedAt: null },
            ],
          },
        ],
      },
    },
  ]);
}

function firstGraphqlError(response: { body: { errors?: Array<Record<string, unknown>> } }): Record<string, unknown> {
  const error = response.body.errors?.[0];
  if (!error) {
    throw new Error(`Expected GraphQL error, received ${JSON.stringify(response.body)}`);
  }
  return error;
}

function capturedFirstError(source: { response: Record<string, unknown> }): Record<string, unknown> {
  return (source.response['errors'] as Array<Record<string, unknown>>)[0]!;
}

function selectedInventoryChanges(payload: Record<string, unknown>, root: string): unknown {
  const data = payload['data'] as Record<string, unknown> | undefined;
  const rootPayload = data?.[root] as { inventoryAdjustmentGroup?: { changes?: unknown[] } } | undefined;
  return rootPayload?.inventoryAdjustmentGroup?.changes;
}

describe('inventory quantity roots', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves inventoryItems and inventoryProperties locally in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventory reads should resolve locally in snapshot mode');
    });
    seedInventoryProduct();
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query {
          inventoryItems(first: 10, query: "sku:INV-ROOT") {
            nodes {
              id
              legacyResourceId
              sku
              tracked
              requiresShipping
              duplicateSkuCount
              locationsCount { count precision }
              inventoryLevel(locationId: "gid://shopify/Location/1") {
                id
                quantities(names: ["available", "on_hand"]) { name quantity }
              }
              variant { id inventoryQuantity }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          empty: inventoryItems(first: 1, query: "id:0") {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          inventoryProperties {
            quantityNames { name displayName isInUse belongsTo comprises }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body.data.inventoryItems.nodes).toEqual([
      {
        id: 'gid://shopify/InventoryItem/93051',
        legacyResourceId: '93051',
        sku: 'INV-ROOT',
        tracked: true,
        requiresShipping: true,
        duplicateSkuCount: 0,
        locationsCount: { count: 2, precision: 'EXACT' },
        inventoryLevel: {
          id: 'gid://shopify/InventoryLevel/93051-1?inventory_item_id=93051',
          quantities: [
            { name: 'available', quantity: 1 },
            { name: 'on_hand', quantity: 1 },
          ],
        },
        variant: { id: 'gid://shopify/ProductVariant/93051', inventoryQuantity: 3 },
      },
    ]);
    expect(response.body.data.inventoryItems.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/InventoryItem/93051',
      endCursor: 'cursor:gid://shopify/InventoryItem/93051',
    });
    expect(response.body.data.empty).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
    expect(response.body.data.inventoryProperties.quantityNames).toContainEqual({
      name: 'on_hand',
      displayName: 'On hand',
      isInUse: true,
      belongsTo: [],
      comprises: ['available', 'committed', 'damaged', 'quality_control', 'reserved', 'safety_stock'],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages inventorySetQuantities locally and exposes downstream inventory item reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventorySetQuantities should not hit upstream fetch');
    });
    seedInventoryProduct();
    const app = createApp(config).callback();

    const mutationResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation SetInventory($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            inventoryAdjustmentGroup {
              reason
              referenceDocumentUri
              changes { name delta quantityAfterChange item { id } location { id name } ledgerDocumentUri }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            name: 'available',
            reason: 'correction',
            referenceDocumentUri: 'logistics://har-305/set/local',
            ignoreCompareQuantity: true,
            quantities: [
              {
                inventoryItemId: 'gid://shopify/InventoryItem/93051',
                locationId: 'gid://shopify/Location/1',
                quantity: 7,
              },
              {
                inventoryItemId: 'gid://shopify/InventoryItem/93051',
                locationId: 'gid://shopify/Location/2',
                quantity: 3,
              },
            ],
          },
        },
      });

    expect(mutationResponse.status).toBe(200);
    expect(mutationResponse.body.data.inventorySetQuantities).toMatchObject({
      inventoryAdjustmentGroup: {
        reason: 'correction',
        referenceDocumentUri: 'logistics://har-305/set/local',
        changes: [
          {
            name: 'available',
            delta: 6,
            quantityAfterChange: null,
            ledgerDocumentUri: null,
            item: { id: 'gid://shopify/InventoryItem/93051' },
            location: { id: 'gid://shopify/Location/1', name: 'Shop location' },
          },
          {
            name: 'available',
            delta: 1,
            quantityAfterChange: null,
            ledgerDocumentUri: null,
            item: { id: 'gid://shopify/InventoryItem/93051' },
            location: { id: 'gid://shopify/Location/2', name: 'Overflow location' },
          },
          {
            name: 'on_hand',
            delta: 6,
          },
          {
            name: 'on_hand',
            delta: 1,
          },
        ],
      },
      userErrors: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query($inventoryItemId: ID!, $productId: ID!) {
          inventoryItem(id: $inventoryItemId) {
            id
            variant { id inventoryQuantity product { id totalInventory } }
            inventoryLevels(first: 10) {
              nodes { location { id } quantities(names: ["available", "on_hand"]) { name quantity } }
            }
          }
          product(id: $productId) { id totalInventory }
        }`,
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/93051',
          productId: 'gid://shopify/Product/9305',
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.inventoryItem.variant.inventoryQuantity).toBe(10);
    expect(readResponse.body.data.inventoryItem.variant.product.totalInventory).toBe(3);
    expect(readResponse.body.data.product.totalInventory).toBe(3);
    expect(readResponse.body.data.inventoryItem.inventoryLevels.nodes).toEqual([
      {
        location: { id: 'gid://shopify/Location/1' },
        quantities: [
          { name: 'available', quantity: 7 },
          { name: 'on_hand', quantity: 7 },
        ],
      },
      {
        location: { id: 'gid://shopify/Location/2' },
        quantities: [
          { name: 'available', quantity: 3 },
          { name: 'on_hand', quantity: 3 },
        ],
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'inventorySetQuantities',
      status: 'staged',
    });
  });

  it('replays the captured 2026-04 inventory quantity mutation contract locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('2026-04 inventory quantity contracts should resolve locally before upstream');
    });
    seedInventoryQuantityContractProduct();
    const app = createApp(config).callback();

    const setMutation = `mutation InventoryQuantityContractSet($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
      inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
        inventoryAdjustmentGroup {
          reason
          referenceDocumentUri
          changes { name delta quantityAfterChange item { id } location { id name } }
        }
        userErrors { field message }
      }
    }`;
    const setMissingIdempotencyMutation = `mutation InventoryQuantityContractSetMissingIdempotency($input: InventorySetQuantitiesInput!) {
      inventorySetQuantities(input: $input) {
        inventoryAdjustmentGroup { id }
        userErrors { field message }
      }
    }`;
    const adjustMutation = `mutation InventoryQuantityContractAdjust($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
      inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
        inventoryAdjustmentGroup {
          reason
          referenceDocumentUri
          changes { name delta quantityAfterChange item { id } location { id name } }
        }
        userErrors { field message }
      }
    }`;
    const adjustMissingIdempotencyMutation = `mutation InventoryQuantityContractAdjustMissingIdempotency($input: InventoryAdjustQuantitiesInput!) {
      inventoryAdjustQuantities(input: $input) {
        inventoryAdjustmentGroup { id }
        userErrors { field message }
      }
    }`;

    const missingSetChangeFromResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: setMutation,
      variables: inventoryQuantityContracts202604.missingSetChangeFromQuantity.variables,
    });

    expect(firstGraphqlError(missingSetChangeFromResponse)).toMatchObject({
      message: capturedFirstError(inventoryQuantityContracts202604.missingSetChangeFromQuantity)['message'],
      path: ['inventorySetQuantities'],
      extensions: { code: 'INVALID_FIELD_ARGUMENTS' },
    });
    expect(missingSetChangeFromResponse.body.data).toEqual({ inventorySetQuantities: null });

    const missingSetIdempotencyResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: setMissingIdempotencyMutation,
      variables: inventoryQuantityContracts202604.missingSetIdempotency.variables,
    });

    expect(firstGraphqlError(missingSetIdempotencyResponse)).toMatchObject({
      message: capturedFirstError(inventoryQuantityContracts202604.missingSetIdempotency)['message'],
      path: ['inventorySetQuantities'],
      extensions: { code: 'BAD_REQUEST' },
    });
    expect(missingSetIdempotencyResponse.body.data).toEqual({ inventorySetQuantities: null });

    const legacySetShapeResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: setMutation,
      variables: inventoryQuantityContracts202604.legacySetShape.variables,
    });

    expect(firstGraphqlError(legacySetShapeResponse)).toMatchObject({
      message: capturedFirstError(inventoryQuantityContracts202604.legacySetShape)['message'],
      extensions: {
        code: 'INVALID_VARIABLE',
        problems: [
          { path: ['ignoreCompareQuantity'], explanation: 'Field is not defined on InventorySetQuantitiesInput' },
        ],
      },
    });

    const setResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: setMutation,
      variables: inventoryQuantityContracts202604.inventorySetQuantities.variables,
    });

    expect(setResponse.body.data.inventorySetQuantities.userErrors).toEqual([]);
    expect(selectedInventoryChanges(setResponse.body, 'inventorySetQuantities')).toEqual(
      selectedInventoryChanges(
        inventoryQuantityContracts202604.inventorySetQuantities.response,
        'inventorySetQuantities',
      ),
    );

    const missingAdjustChangeFromResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: adjustMissingIdempotencyMutation,
      variables: inventoryQuantityContracts202604.missingAdjustChangeFromQuantity.variables,
    });

    expect(firstGraphqlError(missingAdjustChangeFromResponse)).toMatchObject({
      message: capturedFirstError(inventoryQuantityContracts202604.missingAdjustChangeFromQuantity)['message'],
      path: ['inventoryAdjustQuantities'],
      extensions: { code: 'INVALID_FIELD_ARGUMENTS' },
    });
    expect(missingAdjustChangeFromResponse.body.data).toEqual({ inventoryAdjustQuantities: null });

    const missingAdjustIdempotencyResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: adjustMissingIdempotencyMutation,
      variables: inventoryQuantityContracts202604.missingAdjustIdempotency.variables,
    });

    expect(firstGraphqlError(missingAdjustIdempotencyResponse)).toMatchObject({
      message: capturedFirstError(inventoryQuantityContracts202604.missingAdjustIdempotency)['message'],
      path: ['inventoryAdjustQuantities'],
      extensions: { code: 'BAD_REQUEST' },
    });
    expect(missingAdjustIdempotencyResponse.body.data).toEqual({ inventoryAdjustQuantities: null });

    const adjustResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: adjustMutation,
      variables: inventoryQuantityContracts202604.inventoryAdjustQuantities.variables,
    });

    expect(adjustResponse.body.data.inventoryAdjustQuantities.userErrors).toEqual([]);
    expect(selectedInventoryChanges(adjustResponse.body, 'inventoryAdjustQuantities')).toEqual(
      selectedInventoryChanges(
        inventoryQuantityContracts202604.inventoryAdjustQuantities.response,
        'inventoryAdjustQuantities',
      ),
    );

    const downstreamResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query($inventoryItemId: ID!, $productId: ID!) {
          inventoryItem(id: $inventoryItemId) {
            variant { inventoryQuantity product { totalInventory } }
            inventoryLevels(first: 5) {
              nodes { location { id name } quantities(names: ["available", "on_hand"]) { name quantity } }
            }
          }
          product(id: $productId) { totalInventory }
        }`,
        variables: {
          inventoryItemId: inventoryQuantityContracts202604.setup.product.inventoryItemId,
          productId: inventoryQuantityContracts202604.setup.product.productId,
        },
      });

    expect(downstreamResponse.body.data.inventoryItem.variant.inventoryQuantity).toBe(7);
    expect(downstreamResponse.body.data.inventoryItem.variant.product.totalInventory).toBe(0);
    expect(downstreamResponse.body.data.product.totalInventory).toBe(0);
    expect(downstreamResponse.body.data.inventoryItem.inventoryLevels.nodes).toEqual([
      {
        location: inventoryQuantityContracts202604.setup.location,
        quantities: [
          { name: 'available', quantity: 7 },
          { name: 'on_hand', quantity: 7 },
        ],
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages inventoryMoveQuantities locally and rejects unsupported move branches visibly', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventoryMoveQuantities should not hit upstream fetch');
    });
    seedInventoryProduct(10);
    const app = createApp(config).callback();

    const moveResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation MoveInventory($input: InventoryMoveQuantitiesInput!) {
          inventoryMoveQuantities(input: $input) {
            inventoryAdjustmentGroup {
              reason
              referenceDocumentUri
              changes { name delta quantityAfterChange ledgerDocumentUri item { id } location { id name } }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            reason: 'correction',
            referenceDocumentUri: 'logistics://har-305/move/local',
            changes: [
              {
                inventoryItemId: 'gid://shopify/InventoryItem/93051',
                quantity: 1,
                from: { locationId: 'gid://shopify/Location/2', name: 'available' },
                to: {
                  locationId: 'gid://shopify/Location/2',
                  name: 'damaged',
                  ledgerDocumentUri: 'ledger://har-305/damaged/local',
                },
              },
            ],
          },
        },
      });

    expect(moveResponse.status).toBe(200);
    expect(moveResponse.body.data.inventoryMoveQuantities).toEqual({
      inventoryAdjustmentGroup: {
        reason: 'correction',
        referenceDocumentUri: 'logistics://har-305/move/local',
        changes: [
          {
            name: 'available',
            delta: -1,
            quantityAfterChange: null,
            ledgerDocumentUri: null,
            item: { id: 'gid://shopify/InventoryItem/93051' },
            location: { id: 'gid://shopify/Location/2', name: 'Overflow location' },
          },
          {
            name: 'damaged',
            delta: 1,
            quantityAfterChange: null,
            ledgerDocumentUri: 'ledger://har-305/damaged/local',
            item: { id: 'gid://shopify/InventoryItem/93051' },
            location: { id: 'gid://shopify/Location/2', name: 'Overflow location' },
          },
        ],
      },
      userErrors: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query($inventoryItemId: ID!) {
          inventoryItem(id: $inventoryItemId) {
            variant { inventoryQuantity product { totalInventory } }
            inventoryLevels(first: 10) {
              nodes { location { id } quantities(names: ["available", "on_hand", "damaged"]) { name quantity } }
            }
          }
        }`,
        variables: { inventoryItemId: 'gid://shopify/InventoryItem/93051' },
      });

    expect(readResponse.body.data.inventoryItem.variant).toEqual({
      inventoryQuantity: 2,
      product: { totalInventory: 10 },
    });
    expect(readResponse.body.data.inventoryItem.inventoryLevels.nodes[1]).toEqual({
      location: { id: 'gid://shopify/Location/2' },
      quantities: [
        { name: 'available', quantity: 1 },
        { name: 'on_hand', quantity: 2 },
        { name: 'damaged', quantity: 1 },
      ],
    });

    const unsupportedResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation MoveInventory($input: InventoryMoveQuantitiesInput!) {
          inventoryMoveQuantities(input: $input) {
            inventoryAdjustmentGroup { id }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            reason: 'correction',
            referenceDocumentUri: 'logistics://har-305/move/unsupported',
            changes: [
              {
                inventoryItemId: 'gid://shopify/InventoryItem/93051',
                quantity: 1,
                from: { locationId: 'gid://shopify/Location/1', name: 'available' },
                to: { locationId: 'gid://shopify/Location/2', name: 'available' },
              },
            ],
          },
        },
      });

    expect(unsupportedResponse.status).toBe(200);
    expect(unsupportedResponse.body.data.inventoryMoveQuantities).toEqual({
      inventoryAdjustmentGroup: null,
      userErrors: [
        {
          field: ['input', 'changes', '0'],
          message: "The quantities can't be moved between different locations.",
        },
        {
          field: ['input', 'changes', '0'],
          message: "The quantity names for each change can't be the same.",
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
