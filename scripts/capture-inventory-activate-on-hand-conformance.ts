/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturePayload = Record<string, unknown>;
type CaptureCase = {
  query: string;
  variables: Record<string, unknown>;
  response: CapturePayload;
};
type LocationRecord = {
  id: string;
  name?: string | null;
  isActive?: boolean | null;
};
type TrackedProduct = {
  productId: string;
  variantId: string;
  inventoryItemId: string;
  create: CaptureCase;
  track: CaptureCase;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
if (apiVersion !== '2026-04') {
  throw new Error(`inventory-activate-on-hand capture must run against 2026-04, got ${apiVersion}`);
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'inventory-activate-on-hand.json');

const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function requestDocument(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'products', name), 'utf8');
}

const inventoryActivateOnHandDocument = await requestDocument('inventory-activate-on-hand.graphql');
const inventoryActivateConflictDocument = await requestDocument(
  'inventory-activate-available-on-hand-conflict.graphql',
);
const inventoryActivateReadDocument = await requestDocument('inventory-activate-on-hand-read.graphql');

const locationsQuery = `#graphql
  query InventoryActivateOnHandLocations {
    locations(first: 100) {
      nodes {
        id
        name
        isActive
      }
    }
  }
`;

const productCreateMutation = `#graphql
  mutation InventoryActivateOnHandProductCreate($product: ProductCreateInput!) {
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
  mutation InventoryActivateOnHandTrack($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
        tracksInventory
        totalInventory
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

const productDeleteMutation = `#graphql
  mutation InventoryActivateOnHandProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

async function runGraphqlAllowGraphqlErrors(
  query: string,
  variables: Record<string, unknown> = {},
): Promise<CapturePayload> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(JSON.stringify(result, null, 2));
  }

  return result.payload as CapturePayload;
}

async function captureCase(query: string, variables: Record<string, unknown>): Promise<CaptureCase> {
  return {
    query,
    variables,
    response: await runGraphqlAllowGraphqlErrors(query, variables),
  };
}

function userErrorsAt(payload: CapturePayload, pathParts: string[]): unknown[] {
  let cursor: unknown = payload;
  for (const part of pathParts) {
    if (typeof cursor !== 'object' || cursor === null) return [];
    cursor = (cursor as Record<string, unknown>)[part];
  }
  if (typeof cursor !== 'object' || cursor === null) return [];
  const errors = (cursor as { userErrors?: unknown }).userErrors;
  return Array.isArray(errors) ? errors : [];
}

function assertNoUserErrors(payload: CapturePayload, pathParts: string[], label: string): void {
  const errors = userErrorsAt(payload, pathParts);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function createdProductIds(payload: CapturePayload): {
  productId: string;
  variantId: string;
  inventoryItemId: string;
} {
  const productCreate = (
    payload as {
      data?: {
        productCreate?: {
          product?: {
            id?: unknown;
            variants?: {
              nodes?: Array<{ id?: unknown; inventoryItem?: { id?: unknown } }>;
            };
          };
        };
      };
    }
  ).data?.productCreate;
  const product = productCreate?.product;
  const variant = product?.variants?.nodes?.[0];
  const inventoryItemId = variant?.inventoryItem?.id;
  if (typeof product?.id !== 'string' || typeof variant?.id !== 'string' || typeof inventoryItemId !== 'string') {
    throw new Error(`Unable to read created product ids: ${JSON.stringify(productCreate, null, 2)}`);
  }

  return {
    productId: product.id,
    variantId: variant.id,
    inventoryItemId,
  };
}

function activeLocations(payload: CapturePayload): LocationRecord[] {
  const nodes = (
    payload as {
      data?: {
        locations?: {
          nodes?: Array<{ id?: unknown; name?: unknown; isActive?: unknown }>;
        };
      };
    }
  ).data?.locations?.nodes;
  if (!Array.isArray(nodes)) return [];

  return nodes
    .filter(
      (node): node is { id: string; name?: string | null; isActive?: boolean | null } => typeof node.id === 'string',
    )
    .filter((node) => node.isActive !== false);
}

function inventoryLevelIdFromActivation(payload: CapturePayload): string {
  const id = (
    payload as {
      data?: {
        inventoryActivate?: {
          inventoryLevel?: {
            id?: unknown;
          } | null;
        };
      };
    }
  ).data?.inventoryActivate?.inventoryLevel?.id;
  if (typeof id !== 'string') {
    throw new Error(`inventoryActivate did not return an inventoryLevel id: ${JSON.stringify(payload, null, 2)}`);
  }

  return id;
}

async function createTrackedProduct(label: string, runId: string): Promise<TrackedProduct> {
  const createVariables = {
    product: {
      title: `Inventory activate onHand ${label} ${runId}`,
      status: 'DRAFT',
    },
  };
  const create = await captureCase(productCreateMutation, createVariables);
  assertNoUserErrors(create.response, ['data', 'productCreate'], `${label} productCreate`);
  const ids = createdProductIds(create.response);

  const trackVariables = {
    productId: ids.productId,
    variants: [
      {
        id: ids.variantId,
        inventoryItem: {
          tracked: true,
          requiresShipping: true,
        },
      },
    ],
  };
  const track = await captureCase(trackInventoryMutation, trackVariables);
  assertNoUserErrors(track.response, ['data', 'productVariantsBulkUpdate'], `${label} track inventory`);

  return {
    ...ids,
    create,
    track,
  };
}

async function deleteProduct(productId: string): Promise<CapturePayload | null> {
  try {
    return await runGraphql(productDeleteMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(`Cleanup failed for ${productId}: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const products: TrackedProduct[] = [];

try {
  const locations = (await runGraphql(locationsQuery)) as CapturePayload;
  const activeLocationNodes = activeLocations(locations);
  if (activeLocationNodes.length < 2) {
    throw new Error(
      `inventory-activate-on-hand capture requires at least two active locations: ${JSON.stringify(
        activeLocationNodes,
        null,
        2,
      )}`,
    );
  }
  const primaryLocation =
    activeLocationNodes.find((location) => location.name === 'Shop location') ?? activeLocationNodes[0];
  const secondaryLocation =
    activeLocationNodes.find((location) => location.name === 'My Custom Location') ??
    activeLocationNodes.find((location) => location.id !== primaryLocation?.id);
  if (!primaryLocation || !secondaryLocation) {
    throw new Error('Unable to resolve primary and secondary inventory locations.');
  }

  const activateProduct = await createTrackedProduct('activate', runId);
  products.push(activateProduct);
  const conflictProduct = await createTrackedProduct('conflict', runId);
  products.push(conflictProduct);
  const outOfRangeProduct = await createTrackedProduct('out-of-range', runId);
  products.push(outOfRangeProduct);

  const activateOnHandVariables = {
    inventoryItemId: activateProduct.inventoryItemId,
    locationId: secondaryLocation.id,
    onHand: 50,
    idempotencyKey: `inventory-activate-on-hand-success-${runId}`,
  };
  const activateOnHand = await captureCase(inventoryActivateOnHandDocument, activateOnHandVariables);
  assertNoUserErrors(activateOnHand.response, ['data', 'inventoryActivate'], 'activate onHand');

  const activateOnHandRead = await captureCase(inventoryActivateReadDocument, {
    inventoryLevelId: inventoryLevelIdFromActivation(activateOnHand.response),
  });

  const availableOnHandConflict = await captureCase(inventoryActivateConflictDocument, {
    inventoryItemId: conflictProduct.inventoryItemId,
    locationId: secondaryLocation.id,
    available: 10,
    onHand: 20,
    idempotencyKey: `inventory-activate-available-on-hand-conflict-${runId}`,
  });

  const alreadyActiveOnHand = await captureCase(inventoryActivateOnHandDocument, {
    inventoryItemId: activateProduct.inventoryItemId,
    locationId: secondaryLocation.id,
    onHand: 5,
    idempotencyKey: `inventory-activate-on-hand-already-active-${runId}`,
  });

  const onHandOutOfRange = await captureCase(inventoryActivateOnHandDocument, {
    inventoryItemId: outOfRangeProduct.inventoryItemId,
    locationId: secondaryLocation.id,
    onHand: 1_000_000_001,
    idempotencyKey: `inventory-activate-on-hand-out-of-range-${runId}`,
  });

  const cleanup: Record<string, CapturePayload | null> = {};
  for (const product of [...products].reverse()) {
    cleanup[product.productId] = await deleteProduct(product.productId);
  }

  const fixture = {
    summary:
      'inventoryActivate onHand behavior for fresh activation, read-after-write, available/onHand conflict, already-active rejection, and out-of-range rejection.',
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    runId,
    locations: {
      primary: primaryLocation,
      secondary: secondaryLocation,
    },
    setup: {
      activateProduct,
      conflictProduct,
      outOfRangeProduct,
    },
    cases: {
      activateOnHand,
      activateOnHandRead,
      availableOnHandConflict,
      alreadyActiveOnHand,
      onHandOutOfRange,
    },
    cleanup,
    upstreamCalls: [],
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  for (const product of [...products].reverse()) {
    await deleteProduct(product.productId);
  }
  throw error;
}
