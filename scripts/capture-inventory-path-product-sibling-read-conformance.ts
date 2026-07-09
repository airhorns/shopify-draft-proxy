/* oxlint-disable no-console -- CLI capture scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type GraphqlPayload = JsonRecord;
type GraphqlVariables = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
if (apiVersion !== '2025-01') {
  throw new Error(
    `inventory path product sibling capture requires SHOPIFY_CONFORMANCE_API_VERSION=2025-01, got ${apiVersion}`,
  );
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'inventory-path-product-sibling-read.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlAllowGraphqlErrors(query: string, variables: GraphqlVariables = {}): Promise<GraphqlPayload> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(JSON.stringify(result, null, 2));
  }
  return result.payload as GraphqlPayload;
}

const locationAddMutation = `#graphql
  mutation InventoryPathProductSiblingLocation($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        isActive
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const locationDeactivateMutation = `#graphql
  mutation InventoryPathProductSiblingLocationDeactivate($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        id
        isActive
      }
      locationDeactivateUserErrors { field message code }
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation InventoryPathProductSiblingLocationDelete($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors { field message code }
    }
  }
`;

const productSetMutation = `#graphql
  mutation InventoryPathProductSiblingProductSet($input: ProductSetInput!, $synchronous: Boolean!) {
    productSet(input: $input, synchronous: $synchronous) {
      product {
        id
        title
        status
        totalInventory
        tracksInventory
        variants(first: 1) {
          nodes {
            id
            inventoryQuantity
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
        code
      }
    }
  }
`;

const productSiblingReadQuery = `#graphql
  query InventoryPathProductSiblingRead($inventoryItemId: ID!, $productId: ID!) {
    inventoryItem(id: $inventoryItemId) {
      id
      tracked
    }
    product(id: $productId) {
      id
      totalInventory
      tracksInventory
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation InventoryPathProductSiblingProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

function readRecord(value: unknown, label: string): JsonRecord {
  if (typeof value === 'object' && value !== null && !Array.isArray(value)) return value as JsonRecord;
  throw new Error(`${label} was not an object: ${JSON.stringify(value)}`);
}

function readPath(value: unknown, pathSegments: string[], label: string): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      const index = Number.parseInt(segment, 10);
      if (Number.isInteger(index) && index >= 0 && index < current.length) {
        current = current[index];
        continue;
      }
      throw new Error(`${label} was missing array index ${segment}: ${JSON.stringify(value)}`);
    }
    current = readRecord(current, label)[segment];
  }
  return current;
}

function readStringPath(value: unknown, pathSegments: string[], label: string): string {
  const candidate = readPath(value, pathSegments, label);
  if (typeof candidate === 'string') return candidate;
  throw new Error(`${label} was missing string path ${pathSegments.join('.')}: ${JSON.stringify(value)}`);
}

function readUserErrors(payload: unknown, pathSegments: string[]): unknown[] {
  const candidate = readPath(payload, pathSegments, 'GraphQL payload');
  return Array.isArray(candidate) ? candidate : [];
}

function expectNoUserErrors(payload: unknown, pathSegments: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathSegments);
  if (userErrors.length > 0) throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
}

async function deleteProduct(productId: string | null): Promise<GraphqlPayload | null> {
  if (!productId) return null;
  try {
    return await runGraphqlAllowGraphqlErrors(productDeleteMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(`Product cleanup failed for ${productId}: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
}

async function cleanupLocation(locationId: string | null, runId: string): Promise<JsonRecord | null> {
  if (!locationId) return null;
  const cleanup: JsonRecord = {};
  try {
    cleanup['deactivate'] = await runGraphqlAllowGraphqlErrors(locationDeactivateMutation, {
      locationId,
      idempotencyKey: `inventory-path-product-sibling-location-${runId}-${locationId.split('/').at(-1) ?? 'location'}`,
    });
  } catch (error) {
    cleanup['deactivateError'] = error instanceof Error ? error.message : String(error);
  }
  try {
    cleanup['delete'] = await runGraphqlAllowGraphqlErrors(locationDeleteMutation, { locationId });
  } catch (error) {
    cleanup['deleteError'] = error instanceof Error ? error.message : String(error);
  }
  return cleanup;
}

const runId = `${Date.now()}`;
let locationIdForCleanup: string | null = null;
let untrackedProductIdForCleanup: string | null = null;
let stockedProductIdForCleanup: string | null = null;

await mkdir(outputDir, { recursive: true });

try {
  const locationVariables = {
    input: {
      name: `Inventory path product sibling ${runId}`,
      fulfillsOnlineOrders: true,
      address: {
        address1: '30 Product Sibling St',
        city: 'Boston',
        provinceCode: 'MA',
        countryCode: 'US',
        zip: '02112',
      },
    },
  };
  const locationCreate = await runGraphqlAllowGraphqlErrors(locationAddMutation, locationVariables);
  expectNoUserErrors(locationCreate, ['data', 'locationAdd', 'userErrors'], 'locationAdd');
  locationIdForCleanup = readStringPath(locationCreate, ['data', 'locationAdd', 'location', 'id'], 'locationAdd');

  const untrackedVariables = {
    synchronous: true,
    input: {
      title: `Inventory path untracked product ${runId}`,
      status: 'DRAFT',
      productOptions: [
        {
          name: 'Title',
          position: 1,
          values: [{ name: 'Default Title' }],
        },
      ],
      variants: [
        {
          optionValues: [{ optionName: 'Title', name: 'Default Title' }],
          inventoryItem: { tracked: false, requiresShipping: true },
        },
      ],
    },
  };
  const untrackedProductSet = await runGraphqlAllowGraphqlErrors(productSetMutation, untrackedVariables);
  expectNoUserErrors(untrackedProductSet, ['data', 'productSet', 'userErrors'], 'untracked productSet');
  untrackedProductIdForCleanup = readStringPath(
    untrackedProductSet,
    ['data', 'productSet', 'product', 'id'],
    'untracked productSet',
  );
  const untrackedInventoryItemId = readStringPath(
    untrackedProductSet,
    ['data', 'productSet', 'product', 'variants', 'nodes', '0', 'inventoryItem', 'id'],
    'untracked productSet',
  );

  const stockedVariables = {
    synchronous: true,
    input: {
      title: `Inventory path stocked other product ${runId}`,
      status: 'DRAFT',
      productOptions: [
        {
          name: 'Title',
          position: 1,
          values: [{ name: 'Default Title' }],
        },
      ],
      variants: [
        {
          optionValues: [{ optionName: 'Title', name: 'Default Title' }],
          inventoryItem: { tracked: true, requiresShipping: true },
          inventoryQuantities: [
            {
              locationId: locationIdForCleanup,
              name: 'available',
              quantity: 11,
            },
          ],
        },
      ],
    },
  };
  const stockedProductSet = await runGraphqlAllowGraphqlErrors(productSetMutation, stockedVariables);
  expectNoUserErrors(stockedProductSet, ['data', 'productSet', 'userErrors'], 'stocked productSet');
  stockedProductIdForCleanup = readStringPath(
    stockedProductSet,
    ['data', 'productSet', 'product', 'id'],
    'stocked productSet',
  );

  const readVariables = {
    inventoryItemId: untrackedInventoryItemId,
    productId: untrackedProductIdForCleanup,
  };
  const productSiblingRead = await runGraphqlAllowGraphqlErrors(productSiblingReadQuery, readVariables);

  const cleanup = {
    untrackedProductDelete: await deleteProduct(untrackedProductIdForCleanup),
    stockedProductDelete: await deleteProduct(stockedProductIdForCleanup),
    locationCleanup: await cleanupLocation(locationIdForCleanup, runId),
  };
  untrackedProductIdForCleanup = null;
  stockedProductIdForCleanup = null;
  locationIdForCleanup = null;

  const fixture = {
    scenario: 'inventory-path-product-sibling-read',
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    locationCreate: {
      variables: locationVariables,
      response: locationCreate,
    },
    untrackedProductSet: {
      variables: untrackedVariables,
      response: untrackedProductSet,
    },
    stockedProductSet: {
      variables: stockedVariables,
      response: stockedProductSet,
    },
    productSiblingRead: {
      variables: readVariables,
      response: productSiblingRead,
    },
    cleanup,
    upstreamCalls: [],
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, output: outputPath }, null, 2));
} finally {
  await deleteProduct(untrackedProductIdForCleanup);
  await deleteProduct(stockedProductIdForCleanup);
  await cleanupLocation(locationIdForCleanup, runId);
}
