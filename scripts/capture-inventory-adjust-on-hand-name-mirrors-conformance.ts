/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'inventory-adjust-on-hand-name-mirrors.json');

const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const quantityNames = ['available', 'incoming', 'damaged', 'quality_control', 'reserved', 'safety_stock', 'on_hand'];

const locationsQuery = `#graphql
  query InventoryAdjustOnHandMirrorsLocations {
    locations(first: 1) {
      nodes { id name }
    }
  }
`;

const createMutation = `#graphql
  mutation InventoryAdjustOnHandMirrorsCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 1) {
          nodes {
            id
            inventoryItem { id tracked requiresShipping }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const trackInventoryMutation = `#graphql
  mutation InventoryAdjustOnHandMirrorsTrack($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product { id tracksInventory totalInventory }
      productVariants {
        id
        inventoryQuantity
        inventoryItem { id tracked requiresShipping }
      }
      userErrors { field message }
    }
  }
`;

const inventoryItemLevelQuery = `#graphql
  query InventoryAdjustOnHandMirrorsInventoryItemLevel($inventoryItemId: ID!) {
    inventoryItem(id: $inventoryItemId) {
      id
      inventoryLevels(first: 5) {
        nodes {
          location { id name }
          quantities(names: ["available", "incoming", "damaged", "quality_control", "reserved", "safety_stock", "on_hand"]) {
            name
            quantity
            updatedAt
          }
        }
      }
    }
  }
`;

const inventoryAdjustMutation = `#graphql
  mutation InventoryAdjustOnHandNameMirrors($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
    inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        reason
        changes {
          name
          delta
          quantityAfterChange
          ledgerDocumentUri
          item { id }
          location { id }
        }
      }
      userErrors { field message code }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query InventoryAdjustOnHandNameMirrorsRead($productId: ID!, $variantId: ID!, $inventoryItemId: ID!) {
    product(id: $productId) {
      totalInventory
      tracksInventory
    }
    productVariant(id: $variantId) {
      inventoryQuantity
      inventoryItem {
        inventoryLevels(first: 5) {
          nodes {
            quantities(names: ["available", "incoming", "damaged", "quality_control", "reserved", "safety_stock", "on_hand"]) {
              name
              quantity
              updatedAt
            }
          }
        }
      }
    }
    inventoryItem(id: $inventoryItemId) {
      variant {
        inventoryQuantity
        product {
          totalInventory
          tracksInventory
        }
      }
      inventoryLevels(first: 5) {
        nodes {
          quantities(names: ["available", "incoming", "damaged", "quality_control", "reserved", "safety_stock", "on_hand"]) {
            name
            quantity
            updatedAt
          }
        }
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation InventoryAdjustOnHandMirrorsDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

type LocationRef = {
  id: string;
  name?: string | null;
};

type TrackedProduct = {
  productId: string;
  variantId: string;
  inventoryItemId: string;
  location: LocationRef;
  create: unknown;
  track: unknown;
  inventoryItemLevel: unknown;
};

type AdjustCase = {
  key: string;
  name: string;
  reason: string;
  delta: number;
  mirrorsOnHand: boolean;
};

type CapturedCase = {
  variables: Record<string, unknown>;
  response: unknown;
  downstreamRead: unknown;
};

function asRecord(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${label} was not an object.`);
  }

  return value as Record<string, unknown>;
}

function asOptionalRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function requireArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} was not an array.`);
  }

  return value;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} was not a non-empty string.`);
  }

  return value;
}

function extractFirstLocation(payload: unknown): LocationRef {
  const root = asRecord(payload, 'locations response');
  const data = asRecord(root['data'], 'locations data');
  const locations = asRecord(data['locations'], 'locations connection');
  const nodes = requireArray(locations['nodes'], 'locations nodes');
  const first = asRecord(nodes[0], 'first location');

  return {
    id: requireString(first['id'], 'first location id'),
    name: typeof first['name'] === 'string' ? first['name'] : null,
  };
}

function extractCreatedProduct(
  payload: unknown,
): Omit<TrackedProduct, 'location' | 'create' | 'track' | 'inventoryItemLevel'> {
  const root = asRecord(payload, 'productCreate response');
  const data = asRecord(root['data'], 'productCreate data');
  const productCreate = asRecord(data['productCreate'], 'productCreate payload');
  const userErrors = requireArray(productCreate['userErrors'], 'productCreate userErrors');
  if (userErrors.length !== 0) {
    throw new Error(`productCreate returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }

  const product = asRecord(productCreate['product'], 'created product');
  const variants = asRecord(product['variants'], 'created product variants');
  const nodes = requireArray(variants['nodes'], 'created product variant nodes');
  const variant = asRecord(nodes[0], 'created product variant');
  const inventoryItem = asRecord(variant['inventoryItem'], 'created variant inventory item');

  return {
    productId: requireString(product['id'], 'created product id'),
    variantId: requireString(variant['id'], 'created variant id'),
    inventoryItemId: requireString(inventoryItem['id'], 'created inventory item id'),
  };
}

function extractInventoryItemLocation(payload: unknown, fallback: LocationRef): LocationRef {
  const root = asRecord(payload, 'inventory item level response');
  const data = asRecord(root['data'], 'inventory item level data');
  const inventoryItem = asOptionalRecord(data['inventoryItem']);
  const inventoryLevels = asOptionalRecord(inventoryItem?.['inventoryLevels']);
  const nodes = Array.isArray(inventoryLevels?.['nodes']) ? inventoryLevels['nodes'] : [];
  const firstNode = asOptionalRecord(nodes[0]);
  const location = asOptionalRecord(firstNode?.['location']);

  if (!location) return fallback;

  return {
    id: requireString(location['id'], 'inventory item level location id'),
    name: typeof location['name'] === 'string' ? location['name'] : null,
  };
}

function inventoryAdjustPayload(payload: unknown): Record<string, unknown> {
  const root = asRecord(payload, 'inventoryAdjustQuantities response');
  const data = asRecord(root['data'], 'inventoryAdjustQuantities data');

  return asRecord(data['inventoryAdjustQuantities'], 'inventoryAdjustQuantities payload');
}

function assertAdjustmentPayload(
  payload: unknown,
  adjustCase: AdjustCase,
  expectedInventoryItemId: string,
  expectedLocationId: string,
  expectedLedgerDocumentUri: string,
): void {
  const root = inventoryAdjustPayload(payload);
  const userErrors = requireArray(root['userErrors'], `${adjustCase.key} userErrors`);
  if (userErrors.length !== 0) {
    throw new Error(`${adjustCase.key} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }

  const group = asRecord(root['inventoryAdjustmentGroup'], `${adjustCase.key} adjustment group`);
  const changes = requireArray(group['changes'], `${adjustCase.key} changes`).map((change, index) =>
    asRecord(change, `${adjustCase.key} change ${index}`),
  );
  const expectedLength = adjustCase.mirrorsOnHand ? 2 : 1;
  if (changes.length !== expectedLength) {
    throw new Error(`${adjustCase.key} expected ${expectedLength} change rows: ${JSON.stringify(changes, null, 2)}`);
  }

  const submitted = changes.find((change) => change['name'] === adjustCase.name);
  if (!submitted) {
    throw new Error(`${adjustCase.key} missing submitted quantity row: ${JSON.stringify(changes, null, 2)}`);
  }
  const submittedItem = asRecord(submitted['item'], `${adjustCase.key} submitted item`);
  const submittedLocation = asRecord(submitted['location'], `${adjustCase.key} submitted location`);
  if (
    group['reason'] !== adjustCase.reason ||
    submitted['delta'] !== adjustCase.delta ||
    submitted['quantityAfterChange'] !== null ||
    submitted['ledgerDocumentUri'] !== expectedLedgerDocumentUri ||
    submittedItem['id'] !== expectedInventoryItemId ||
    submittedLocation['id'] !== expectedLocationId
  ) {
    throw new Error(`${adjustCase.key} submitted row mismatch: ${JSON.stringify(root, null, 2)}`);
  }

  const onHand = changes.find((change) => change['name'] === 'on_hand');
  if (adjustCase.mirrorsOnHand && !onHand) {
    throw new Error(`${adjustCase.key} missing on_hand companion row: ${JSON.stringify(changes, null, 2)}`);
  }
  if (!adjustCase.mirrorsOnHand && onHand) {
    throw new Error(
      `${adjustCase.key} unexpectedly returned on_hand companion row: ${JSON.stringify(changes, null, 2)}`,
    );
  }
  if (onHand) {
    const onHandItem = asRecord(onHand['item'], `${adjustCase.key} on_hand item`);
    const onHandLocation = asRecord(onHand['location'], `${adjustCase.key} on_hand location`);
    if (
      onHand['delta'] !== adjustCase.delta ||
      onHand['quantityAfterChange'] !== null ||
      onHandItem['id'] !== expectedInventoryItemId ||
      onHandLocation['id'] !== expectedLocationId
    ) {
      throw new Error(`${adjustCase.key} on_hand row mismatch: ${JSON.stringify(root, null, 2)}`);
    }
  }
}

async function deleteProduct(productId: string): Promise<unknown> {
  try {
    return await runGraphql(deleteMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(`Cleanup failed for ${productId}: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
}

async function cleanupProducts(productIds: Set<string>): Promise<unknown[]> {
  const responses: unknown[] = [];
  for (const productId of Array.from(productIds)) {
    responses.push(await deleteProduct(productId));
    productIds.delete(productId);
  }

  return responses;
}

async function createTrackedProduct(
  label: string,
  fallbackLocation: LocationRef,
  productIds: Set<string>,
): Promise<TrackedProduct> {
  const create = await runGraphql(createMutation, {
    product: {
      title: label,
      status: 'DRAFT',
    },
  });
  const product = extractCreatedProduct(create);
  productIds.add(product.productId);

  const track = await runGraphql(trackInventoryMutation, {
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
  });
  const inventoryItemLevel = await runGraphql(inventoryItemLevelQuery, {
    inventoryItemId: product.inventoryItemId,
  });

  return {
    ...product,
    location: extractInventoryItemLocation(inventoryItemLevel, fallbackLocation),
    create,
    track,
    inventoryItemLevel,
  };
}

await mkdir(outputDir, { recursive: true });

const productIds = new Set<string>();
const runId = `${Date.now()}`;
const adjustCases: AdjustCase[] = [
  { key: 'damaged', name: 'damaged', reason: 'damaged', delta: 2, mirrorsOnHand: true },
  { key: 'reserved', name: 'reserved', reason: 'reservation_created', delta: 3, mirrorsOnHand: true },
  { key: 'qualityControl', name: 'quality_control', reason: 'quality_control', delta: 4, mirrorsOnHand: true },
  { key: 'safetyStock', name: 'safety_stock', reason: 'safety_stock', delta: 5, mirrorsOnHand: true },
  { key: 'incoming', name: 'incoming', reason: 'received', delta: 6, mirrorsOnHand: false },
];

try {
  const fallbackLocation = extractFirstLocation(await runGraphql(locationsQuery));
  const product = await createTrackedProduct(
    `Hermes Inventory Adjust On Hand Mirrors ${runId}`,
    fallbackLocation,
    productIds,
  );
  const cases: Record<string, CapturedCase> = {};

  for (const adjustCase of adjustCases) {
    const ledgerDocumentUri = `https://example.com/inventory-adjust-on-hand-mirrors/${adjustCase.name}/${runId}`;
    const variables = {
      input: {
        name: adjustCase.name,
        reason: adjustCase.reason,
        changes: [
          {
            inventoryItemId: product.inventoryItemId,
            locationId: product.location.id,
            delta: adjustCase.delta,
            changeFromQuantity: 0,
            ledgerDocumentUri,
          },
        ],
      },
      idempotencyKey: `inventory-adjust-on-hand-mirrors-${adjustCase.name}-${runId}`,
    };
    const response = await runGraphql(inventoryAdjustMutation, variables);
    assertAdjustmentPayload(response, adjustCase, product.inventoryItemId, product.location.id, ledgerDocumentUri);
    const downstreamRead = await runGraphql(downstreamReadQuery, {
      productId: product.productId,
      variantId: product.variantId,
      inventoryItemId: product.inventoryItemId,
    });

    cases[adjustCase.key] = {
      variables,
      response,
      downstreamRead,
    };
  }

  const cleanup = await cleanupProducts(productIds);
  const capturePayload = {
    storeDomain,
    apiVersion,
    quantityNames,
    setup: {
      fallbackLocation,
      product,
    },
    cases,
    cleanup,
  };

  await writeFile(outputPath, `${JSON.stringify(capturePayload, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        apiVersion,
        storeDomain,
        inventoryItemId: product.inventoryItemId,
        capturedCases: Object.keys(cases),
      },
      null,
      2,
    ),
  );
} finally {
  await cleanupProducts(productIds);
}
