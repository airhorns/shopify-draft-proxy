import { rm } from 'node:fs/promises';

import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

const createMutation = `#graphql
  mutation InventoryMoveAdjustmentGroupShapeCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 1) {
          nodes {
            id
            inventoryItem {
              id
              tracked
              requiresShipping
            }
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const trackInventoryMutation = `#graphql
  mutation InventoryMoveAdjustmentGroupShapeTrack(
    $productId: ID!
    $variants: [ProductVariantsBulkInput!]!
  ) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
        totalInventory
        tracksInventory
      }
      productVariants {
        id
        inventoryItem {
          id
          tracked
          requiresShipping
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation InventoryMoveAdjustmentGroupShapeDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const hydrateInventoryNodesQuery =
  'query ProductsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { ... on InventoryItem { id tracked requiresShipping countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode measurement { weight { value unit } } variant { id title inventoryQuantity selectedOptions { name value } product { id title handle status totalInventory tracksInventory } } inventoryLevels(first: 10, includeInactive: true) { nodes { id isActive location { id name } quantities(names: ["available", "on_hand", "committed", "incoming", "reserved"]) { name quantity updatedAt } } } } ... on InventoryLevel { id isActive location { id name } quantities(names: ["available", "on_hand", "committed", "incoming", "reserved"]) { name quantity updatedAt } item { id tracked requiresShipping variant { id title inventoryQuantity selectedOptions { name value } product { id title handle status totalInventory tracksInventory } } inventoryLevels(first: 10, includeInactive: true) { nodes { id isActive location { id name } quantities(names: ["available", "on_hand", "committed", "incoming", "reserved"]) { name quantity updatedAt } } } } } } }';

function requireRecord(value: unknown, label: string): JsonRecord {
  const record = readRecord(value);
  if (!record) {
    throw new Error(`Missing required capture object: ${label}`);
  }
  return record;
}

function extractCreatedProduct(payload: JsonRecord): {
  productId: string;
  variantId: string;
  inventoryItemId: string;
} {
  const productCreate = requireRecord(
    requireRecord(payload['data'], 'productCreate data')['productCreate'],
    'productCreate',
  );
  const product = requireRecord(productCreate['product'], 'created product');
  const variant = requireRecord(
    readArray(requireRecord(product['variants'], 'created product variants')['nodes'])[0],
    'created default variant',
  );
  const inventoryItem = requireRecord(variant['inventoryItem'], 'created default inventory item');

  return {
    productId: requireString(product['id'], 'created product id'),
    variantId: requireString(variant['id'], 'created variant id'),
    inventoryItemId: requireString(inventoryItem['id'], 'created inventory item id'),
  };
}

function quantityValue(level: JsonRecord, name: string): number {
  const quantities = readArray(level['quantities']);
  const match = quantities.map((quantity) => readRecord(quantity)).find((quantity) => quantity?.['name'] === name);
  const value = match?.['quantity'];
  return typeof value === 'number' ? value : 0;
}

function extractHydratedLevel(hydratePayload: JsonRecord): {
  locationId: string;
  locationName: string;
  available: number;
} {
  const data = requireRecord(hydratePayload['data'], 'hydrate data');
  const node = requireRecord(readArray(data['nodes'])[0], 'hydrated inventory item');
  const levels = readArray(requireRecord(node['inventoryLevels'], 'hydrated inventory levels')['nodes'])
    .map((level) => readRecord(level))
    .filter((level): level is JsonRecord => level !== null && level['isActive'] !== false);
  const level = levels[0];
  if (!level) {
    throw new Error('Hydrated inventory item did not expose an active inventory level.');
  }
  const location = requireRecord(level['location'], 'hydrated inventory level location');
  return {
    locationId: requireString(location['id'], 'hydrated location id'),
    locationName: requireString(location['name'], 'hydrated location name'),
    available: quantityValue(level, 'available'),
  };
}

function assertNoTopLevelErrors(payload: JsonRecord, label: string): void {
  if (payload['errors'] !== undefined) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(payload['errors'], null, 2)}`);
  }
}

const capture = await createConformanceCapture();
const setRequest = await capture.readRequest('products', 'inventory-move-adjustment-group-shape-set.graphql');
const moveRequest = await capture.readRequest('products', 'inventory-move-adjustment-group-shape.graphql');
const outputPath = capture.fixturePath('products', 'inventory-move-adjustment-group-shape.json');

let productId: string | null = null;

try {
  const created = await capture.run(
    createMutation,
    {
      product: {
        title: `Hermes Inventory Move Adjustment Group Shape ${capture.stamp}`,
        status: 'DRAFT',
      },
    },
    'create disposable inventory product',
  );
  const product = extractCreatedProduct(created);
  productId = product.productId;

  const trackInventory = await capture.run(
    trackInventoryMutation,
    {
      productId: product.productId,
      variants: [
        {
          id: product.variantId,
          inventoryItem: {
            tracked: true,
            requiresShipping: true,
          },
        },
      ],
    },
    'mark inventory tracked',
  );
  capture.mutationRoot(trackInventory, 'productVariantsBulkUpdate', 'mark inventory tracked');

  const hydrateVariables = { ids: [product.inventoryItemId] };
  const hydrateResult = await capture.runGraphqlRequest<JsonRecord>(hydrateInventoryNodesQuery, hydrateVariables);
  const hydratePayload = hydrateResult.payload as JsonRecord;
  if (hydrateResult.status < 200 || hydrateResult.status >= 300) {
    throw new Error(`hydrate inventory item failed: ${JSON.stringify(hydrateResult, null, 2)}`);
  }
  assertNoTopLevelErrors(hydratePayload, 'hydrate inventory item');
  const hydratedLevel = extractHydratedLevel(hydratePayload);
  const seedQuantity = hydratedLevel.available + 5;

  const seedSetVariables = {
    idempotencyKey: `inventory-move-adjustment-group-shape-${capture.stamp}-set`,
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: `logistics://inventory-move-adjustment-group-shape/${capture.stamp}/set`,
      quantities: [
        {
          inventoryItemId: product.inventoryItemId,
          locationId: hydratedLevel.locationId,
          quantity: seedQuantity,
          changeFromQuantity: hydratedLevel.available,
        },
      ],
    },
  };
  const seedSet = await capture.run(setRequest, seedSetVariables, 'seed available inventory quantity');
  capture.mutationRoot(seedSet, 'inventorySetQuantities', 'seed available inventory quantity');

  const moveVariables = {
    idempotencyKey: `inventory-move-adjustment-group-shape-${capture.stamp}-move`,
    input: {
      reason: 'correction',
      referenceDocumentUri: `logistics://inventory-move-adjustment-group-shape/${capture.stamp}/move`,
      changes: [
        {
          inventoryItemId: product.inventoryItemId,
          quantity: 2,
          from: {
            locationId: hydratedLevel.locationId,
            name: 'available',
            changeFromQuantity: seedQuantity,
          },
          to: {
            locationId: hydratedLevel.locationId,
            name: 'damaged',
            changeFromQuantity: 0,
            ledgerDocumentUri: `ledger://inventory-move-adjustment-group-shape/${capture.stamp}/move`,
          },
        },
      ],
    },
  };
  const move = await capture.run(moveRequest, moveVariables, 'move available inventory to damaged');
  const moveRoot = capture.mutationRoot(move, 'inventoryMoveQuantities', 'move available inventory to damaged');
  const adjustmentGroup = requireRecord(moveRoot['inventoryAdjustmentGroup'], 'move inventory adjustment group');
  requireString(adjustmentGroup['id'], 'move inventory adjustment group id');
  requireString(adjustmentGroup['createdAt'], 'move inventory adjustment group createdAt');

  await capture.writeJson(outputPath, {
    capturedAt: new Date().toISOString(),
    apiVersion: capture.apiVersion,
    storeDomain: capture.storeDomain,
    operations: ['inventorySetQuantities', 'inventoryMoveQuantities'],
    setup: {
      product,
      location: {
        id: hydratedLevel.locationId,
        name: hydratedLevel.locationName,
      },
      trackedInventory: trackInventory,
      inventoryHydrate: {
        variables: hydrateVariables,
        response: hydratePayload,
      },
      seedSet: {
        variables: seedSetVariables,
        response: seedSet,
      },
    },
    move: {
      variables: moveVariables,
      response: move,
    },
    upstreamCalls: [
      {
        operationName: 'ProductsHydrateNodes',
        variables: hydrateVariables,
        query: hydrateInventoryNodesQuery,
        response: {
          status: hydrateResult.status,
          body: hydratePayload,
        },
      },
    ],
  });

  process.stdout.write(
    `${JSON.stringify(
      {
        ok: true,
        outputPath,
        productId: product.productId,
        inventoryItemId: product.inventoryItemId,
        locationId: hydratedLevel.locationId,
      },
      null,
      2,
    )}\n`,
  );
} finally {
  if (productId) {
    try {
      await capture.runGraphqlRequest(deleteMutation, { input: { id: productId } });
    } finally {
      await rm('pending/inventory-move-adjustment-group-shape-blocker.md', { force: true });
    }
  }
}
