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
const outputPath = path.join(outputDir, 'inventory-adjust-zero-delta-noop.json');
const nonzeroDelta = 3;

const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlAllowGraphqlErrors(query: string, variables: Record<string, unknown> = {}): Promise<unknown> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(JSON.stringify(result, null, 2));
  }

  return result.payload;
}

const locationsQuery = `#graphql
  query InventoryAdjustZeroDeltaLocations {
    locations(first: 1) {
      nodes { id name }
    }
  }
`;

const createMutation = `#graphql
  mutation InventoryAdjustZeroDeltaCreate($product: ProductCreateInput!) {
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
  mutation InventoryAdjustZeroDeltaTrack($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
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
  query InventoryAdjustZeroDeltaInventoryItemLevel($inventoryItemId: ID!) {
    inventoryItem(id: $inventoryItemId) {
      id
      inventoryLevels(first: 5) {
        nodes {
          location { id name }
          quantities(names: ["available", "on_hand"]) { name quantity }
        }
      }
    }
  }
`;

const inventoryAdjustMutation = `#graphql
  mutation InventoryAdjustZeroDeltaNoop($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
    inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        reason
        referenceDocumentUri
        changes {
          name
          delta
          quantityAfterChange
          item { id }
          location { id }
        }
      }
      userErrors { field message code }
    }
  }
`;

const deleteMutation = `#graphql
  mutation InventoryAdjustZeroDeltaDelete($input: ProductDeleteInput!) {
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

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} was not a non-empty string.`);
  }

  return value;
}

function requireArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} was not an array.`);
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
  const product = asRecord(productCreate['product'], 'created product');
  const variants = asRecord(product['variants'], 'created product variants');
  const nodes = requireArray(variants['nodes'], 'created product variant nodes');
  const variant = asRecord(nodes[0], 'created product default variant');
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

  if (!location) {
    return fallback;
  }

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

function assertNoUserErrors(payload: Record<string, unknown>, label: string): void {
  const userErrors = requireArray(payload['userErrors'], `${label} userErrors`);
  if (userErrors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertCapturedBehavior(allZero: unknown, mixed: unknown, nonzeroItemId: string): void {
  const allZeroRoot = inventoryAdjustPayload(allZero);
  assertNoUserErrors(allZeroRoot, 'all-zero adjust');
  if (allZeroRoot['inventoryAdjustmentGroup'] !== null) {
    throw new Error(`Expected all-zero adjust group to be null: ${JSON.stringify(allZeroRoot, null, 2)}`);
  }

  const mixedRoot = inventoryAdjustPayload(mixed);
  assertNoUserErrors(mixedRoot, 'mixed adjust');
  const mixedGroup = asRecord(mixedRoot['inventoryAdjustmentGroup'], 'mixed inventory adjustment group');
  const changes = requireArray(mixedGroup['changes'], 'mixed inventory adjustment group changes');
  if (changes.length === 0) {
    throw new Error('Expected mixed adjust to return at least one change row.');
  }
  if (
    changes.some((change) => {
      const row = asRecord(change, 'mixed inventory adjustment change');
      return row['delta'] === 0;
    })
  ) {
    throw new Error(`Expected mixed adjust to omit zero-delta change rows: ${JSON.stringify(changes, null, 2)}`);
  }
  if (
    !changes.some((change) => {
      const row = asRecord(change, 'mixed available change');
      const item = asRecord(row['item'], 'mixed available change item');
      return row['name'] === 'available' && row['delta'] === nonzeroDelta && item['id'] === nonzeroItemId;
    })
  ) {
    throw new Error(`Expected mixed adjust to include the non-zero available row: ${JSON.stringify(changes, null, 2)}`);
  }
}

async function deleteProduct(productId: string): Promise<unknown> {
  try {
    return await runGraphqlAllowGraphqlErrors(deleteMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(`Cleanup failed for ${productId}: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
}

async function cleanupProducts(productIds: Set<string>): Promise<unknown[]> {
  const responses: unknown[] = [];
  for (const productId of productIds) {
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

try {
  const fallbackLocation = extractFirstLocation(await runGraphql(locationsQuery));
  const zeroProduct = await createTrackedProduct(
    `Hermes Inventory Adjust Zero Delta Noop ${runId}`,
    fallbackLocation,
    productIds,
  );
  const nonzeroProduct = await createTrackedProduct(
    `Hermes Inventory Adjust Mixed Delta ${runId}`,
    fallbackLocation,
    productIds,
  );

  const allZeroVariables = {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: `logistics://inventory-adjust-zero-delta/noop/${runId}`,
      changes: [
        {
          inventoryItemId: zeroProduct.inventoryItemId,
          locationId: zeroProduct.location.id,
          delta: 0,
          changeFromQuantity: 0,
        },
      ],
    },
    idempotencyKey: `inventory-adjust-zero-delta-noop-${runId}`,
  };
  const allZero = await runGraphqlAllowGraphqlErrors(inventoryAdjustMutation, allZeroVariables);

  const mixedVariables = {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: `logistics://inventory-adjust-zero-delta/mixed/${runId}`,
      changes: [
        {
          inventoryItemId: zeroProduct.inventoryItemId,
          locationId: zeroProduct.location.id,
          delta: 0,
          changeFromQuantity: 0,
        },
        {
          inventoryItemId: nonzeroProduct.inventoryItemId,
          locationId: nonzeroProduct.location.id,
          delta: nonzeroDelta,
          changeFromQuantity: 0,
        },
      ],
    },
    idempotencyKey: `inventory-adjust-zero-delta-mixed-${runId}`,
  };
  const mixedZeroAndNonzero = await runGraphqlAllowGraphqlErrors(inventoryAdjustMutation, mixedVariables);

  assertCapturedBehavior(allZero, mixedZeroAndNonzero, nonzeroProduct.inventoryItemId);
  const cleanup = await cleanupProducts(productIds);

  const capturePayload = {
    storeDomain,
    apiVersion,
    setup: {
      fallbackLocation,
      zeroProduct,
      nonzeroProduct,
    },
    allZero: {
      variables: allZeroVariables,
      response: allZero,
    },
    mixedZeroAndNonzero: {
      variables: mixedVariables,
      response: mixedZeroAndNonzero,
    },
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
        zeroInventoryItemId: zeroProduct.inventoryItemId,
        nonzeroInventoryItemId: nonzeroProduct.inventoryItemId,
      },
      null,
      2,
    ),
  );
} finally {
  await cleanupProducts(productIds);
}
