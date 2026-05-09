/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  type AdminGraphqlClient,
  type ConformanceGraphqlResult,
  createAdminGraphqlClient,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CaptureCase = {
  name: string;
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

const scenarioId = 'location-delete-inventory-level-cascade';
const apiVersion = '2026-04';
const requestedConfig = readConformanceScriptConfig({
  defaultApiVersion: apiVersion,
  exitOnMissing: true,
});
const { storeDomain, adminOrigin } = requestedConfig;
const adminAccessToken = await getValidConformanceAccessToken({
  adminOrigin,
  apiVersion,
});
const adminHeaders = buildAdminAuthHeaders(adminAccessToken);
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'store-properties');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

function createClient(): AdminGraphqlClient {
  return createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: adminHeaders,
  });
}

const client = createClient();

const productCreateMutation = `#graphql
  mutation LocationDeleteInventoryProductCreate($product: ProductCreateInput!) {
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

const productTrackInventoryMutation = `#graphql
  mutation LocationDeleteInventoryProductTrack($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
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

const locationAddMutation = `#graphql
  mutation LocationDeleteInventoryLocationAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        isActive
        hasActiveInventory
        fulfillsOnlineOrders
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const inventoryActivateMutation = `#graphql
  mutation LocationDeleteInventoryActivate(
    $inventoryItemId: ID!
    $locationId: ID!
    $available: Int
    $idempotencyKey: String!
  ) {
    inventoryActivate(
      inventoryItemId: $inventoryItemId
      locationId: $locationId
      available: $available
    ) @idempotent(key: $idempotencyKey) {
      inventoryLevel {
        id
        location {
          id
          name
        }
        quantities(names: ["available", "on_hand"]) {
          name
          quantity
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const inventoryDeactivateMutation = `#graphql
  mutation LocationDeleteInventoryDefaultDeactivate($inventoryLevelId: ID!, $idempotencyKey: String!) {
    inventoryDeactivate(inventoryLevelId: $inventoryLevelId) @idempotent(key: $idempotencyKey) {
      userErrors {
        field
        message
      }
    }
  }
`;

const locationDeactivateMutation = `#graphql
  mutation LocationDeleteInventoryLocationDeactivate(
    $locationId: ID!
    $destinationLocationId: ID!
    $idempotencyKey: String!
  ) {
    locationDeactivate(locationId: $locationId, destinationLocationId: $destinationLocationId)
      @idempotent(key: $idempotencyKey) {
      location {
        id
        name
        isActive
        hasActiveInventory
        deletable
      }
      locationDeactivateUserErrors {
        field
        message
        code
      }
    }
  }
`;

const locationDeactivateCleanupMutation = `#graphql
  mutation LocationDeleteInventoryCleanupDeactivate($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        id
        isActive
        hasActiveInventory
      }
      locationDeactivateUserErrors {
        field
        message
        code
      }
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation LocationDeleteInventoryLocationDelete($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors {
        field
        message
        code
      }
    }
  }
`;

const inventoryItemReadQuery = `#graphql
  query LocationDeleteInventoryRead($inventoryItemId: ID!, $deletedLocationId: ID!) {
    inventoryItem(id: $inventoryItemId) {
      id
      locationsCount {
        count
        precision
      }
      deletedLevel: inventoryLevel(locationId: $deletedLocationId) {
        id
        location {
          id
          name
        }
      }
      inventoryLevels(first: 10) {
        nodes {
          id
          location {
            id
            name
          }
          quantities(names: ["available", "on_hand"]) {
            name
            quantity
          }
        }
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation LocationDeleteInventoryProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const locationHydrateQuery = `#graphql
  query LocationDeleteInventoryCleanupHydrate($id: ID!) {
    location(id: $id) {
      id
      isActive
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function runCase(name: string, query: string, variables: JsonRecord = {}): Promise<CaptureCase> {
  return {
    name,
    query: trimGraphql(query),
    variables,
    response: await client.runGraphqlRequest(query, variables),
  };
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readData(capture: CaptureCase): JsonRecord {
  const data = readObject(capture.response.payload.data);
  if (!data) {
    throw new Error(`${capture.name} returned no data: ${JSON.stringify(capture.response.payload)}`);
  }
  return data;
}

function mutationPayload(capture: CaptureCase, key: string): JsonRecord {
  const payload = readObject(readData(capture)[key]);
  if (!payload) {
    throw new Error(`${capture.name} returned no ${key} payload: ${JSON.stringify(capture.response.payload)}`);
  }
  return payload;
}

function userErrors(capture: CaptureCase, key: string): unknown[] {
  const payload = mutationPayload(capture, key);
  const errors =
    payload['userErrors'] ?? payload['locationDeactivateUserErrors'] ?? payload['locationDeleteUserErrors'];
  return Array.isArray(errors) ? errors : [];
}

function assertNoTopLevelErrors(capture: CaptureCase): void {
  if (capture.response.status !== 200 || capture.response.payload.errors) {
    throw new Error(`${capture.name} failed: ${JSON.stringify(capture.response.payload)}`);
  }
}

function assertNoUserErrors(capture: CaptureCase, key: string): void {
  assertNoTopLevelErrors(capture);
  const errors = userErrors(capture, key);
  if (errors.length > 0) {
    throw new Error(`${capture.name} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function readProduct(capture: CaptureCase): { productId: string; variantId: string; inventoryItemId: string } {
  const product = readObject(mutationPayload(capture, 'productCreate')['product']);
  const variants = readObject(product?.['variants']);
  const nodes = variants?.['nodes'];
  const variant = Array.isArray(nodes) ? readObject(nodes[0]) : null;
  const inventoryItem = readObject(variant?.['inventoryItem']);
  const productId = product?.['id'];
  const variantId = variant?.['id'];
  const inventoryItemId = inventoryItem?.['id'];
  if (typeof productId !== 'string' || typeof variantId !== 'string' || typeof inventoryItemId !== 'string') {
    throw new Error(
      `${capture.name} did not create product inventory ids: ${JSON.stringify(capture.response.payload)}`,
    );
  }
  return { productId, variantId, inventoryItemId };
}

function readLocationAddId(capture: CaptureCase): string {
  const location = readObject(mutationPayload(capture, 'locationAdd')['location']);
  const id = location?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`${capture.name} did not create a location: ${JSON.stringify(capture.response.payload)}`);
  }
  return id;
}

function locationInput(name: string): JsonRecord {
  return {
    name,
    fulfillsOnlineOrders: false,
    address: {
      address1: '873 State St',
      city: 'Boston',
      provinceCode: 'MA',
      countryCode: 'US',
      zip: '02110',
    },
  };
}

async function createLocation(name: string): Promise<{ create: CaptureCase; id: string }> {
  const create = await runCase(`${name}LocationAdd`, locationAddMutation, {
    input: locationInput(name),
  });
  assertNoUserErrors(create, 'locationAdd');
  return { create, id: readLocationAddId(create) };
}

function inventoryItem(capture: CaptureCase): JsonRecord {
  const item = readObject(readData(capture)['inventoryItem']);
  if (!item) {
    throw new Error(`${capture.name} returned no inventoryItem: ${JSON.stringify(capture.response.payload)}`);
  }
  return item;
}

function inventoryNodes(capture: CaptureCase): JsonRecord[] {
  const levels = readObject(inventoryItem(capture)['inventoryLevels']);
  const nodes = levels?.['nodes'];
  return Array.isArray(nodes) ? (nodes.map(readObject).filter(Boolean) as JsonRecord[]) : [];
}

function inventoryLocationIds(capture: CaptureCase): string[] {
  return inventoryNodes(capture)
    .map((node) => readObject(node['location'])?.['id'])
    .filter((id): id is string => typeof id === 'string');
}

function inventoryLevelIdsOutside(capture: CaptureCase, retainedLocationIds: string[]): string[] {
  return inventoryNodes(capture)
    .filter((node) => {
      const locationId = readObject(node['location'])?.['id'];
      return typeof locationId === 'string' && !retainedLocationIds.includes(locationId);
    })
    .map((node) => node['id'])
    .filter((id): id is string => typeof id === 'string');
}

function locationsCount(capture: CaptureCase): number | null {
  const count = readObject(inventoryItem(capture)['locationsCount'])?.['count'];
  return typeof count === 'number' ? count : null;
}

function assertInventoryLocations(
  capture: CaptureCase,
  expectedCount: number,
  present: string[],
  absent: string[],
): void {
  assertNoTopLevelErrors(capture);
  const count = locationsCount(capture);
  const ids = inventoryLocationIds(capture);
  if (count !== expectedCount || ids.length !== expectedCount) {
    throw new Error(
      `${capture.name} expected ${expectedCount} locations, got count=${count} ids=${JSON.stringify(ids)}`,
    );
  }
  for (const id of present) {
    if (!ids.includes(id)) {
      throw new Error(`${capture.name} missing inventory level at ${id}: ${JSON.stringify(ids)}`);
    }
  }
  for (const id of absent) {
    if (ids.includes(id)) {
      throw new Error(`${capture.name} still has inventory level at ${id}: ${JSON.stringify(ids)}`);
    }
  }
}

async function cleanupProduct(productId: string | null, cleanup: CaptureCase[]): Promise<void> {
  if (!productId) return;
  cleanup.push(await runCase('productCleanup', productDeleteMutation, { input: { id: productId } }));
}

async function cleanupLocation(
  locationId: string | null,
  cleanup: CaptureCase[],
  runId: string,
  name: string,
): Promise<void> {
  if (!locationId) return;
  const hydrate = await runCase(`${name}CleanupHydrate`, locationHydrateQuery, { id: locationId });
  assertNoTopLevelErrors(hydrate);
  const location = readObject(readData(hydrate)['location']);
  if (!location) return;
  if (location['isActive'] === true) {
    cleanup.push(
      await runCase(`${name}CleanupDeactivate`, locationDeactivateCleanupMutation, {
        locationId,
        idempotencyKey: `${scenarioId}-${name}-cleanup-${runId}`,
      }),
    );
  }
  cleanup.push(await runCase(`${name}CleanupDelete`, locationDeleteMutation, { locationId }));
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const runId = capturedAt.replace(/\D/gu, '').slice(0, 14);
const workflow: JsonRecord = {};
const cleanup: CaptureCase[] = [];
const upstreamCalls: JsonRecord[] = [];

let productId: string | null = null;
let targetLocationId: string | null = null;
let destinationLocationId: string | null = null;

try {
  const target = await createLocation(`${scenarioId} target ${runId}`);
  targetLocationId = target.id;
  workflow['targetLocationAdd'] = target.create;

  const destination = await createLocation(`${scenarioId} destination ${runId}`);
  destinationLocationId = destination.id;
  workflow['destinationLocationAdd'] = destination.create;

  workflow['productCreate'] = await runCase('productCreate', productCreateMutation, {
    product: { title: `${scenarioId} product ${runId}` },
  });
  assertNoUserErrors(workflow['productCreate'] as CaptureCase, 'productCreate');
  const product = readProduct(workflow['productCreate'] as CaptureCase);
  productId = product.productId;

  workflow['productTrack'] = await runCase('productTrack', productTrackInventoryMutation, {
    productId: product.productId,
    variants: [{ id: product.variantId, inventoryItem: { tracked: true } }],
  });
  assertNoUserErrors(workflow['productTrack'] as CaptureCase, 'productVariantsBulkUpdate');

  workflow['destinationInventoryActivate'] = await runCase('destinationInventoryActivate', inventoryActivateMutation, {
    inventoryItemId: product.inventoryItemId,
    locationId: destination.id,
    available: 9,
    idempotencyKey: `${scenarioId}-destination-${runId}`,
  });
  assertNoUserErrors(workflow['destinationInventoryActivate'] as CaptureCase, 'inventoryActivate');

  workflow['defaultInventoryRead'] = await runCase('defaultInventoryRead', inventoryItemReadQuery, {
    inventoryItemId: product.inventoryItemId,
    deletedLocationId: target.id,
  });
  assertNoTopLevelErrors(workflow['defaultInventoryRead'] as CaptureCase);
  const defaultLevelIds = inventoryLevelIdsOutside(workflow['defaultInventoryRead'] as CaptureCase, [destination.id]);
  const defaultDeactivations: CaptureCase[] = [];
  for (const inventoryLevelId of defaultLevelIds) {
    const deactivate = await runCase('defaultInventoryDeactivate', inventoryDeactivateMutation, {
      inventoryLevelId,
      idempotencyKey: `${scenarioId}-default-${defaultDeactivations.length}-${runId}`,
    });
    assertNoUserErrors(deactivate, 'inventoryDeactivate');
    defaultDeactivations.push(deactivate);
  }
  workflow['defaultInventoryDeactivations'] = defaultDeactivations;

  workflow['targetInventoryActivate'] = await runCase('targetInventoryActivate', inventoryActivateMutation, {
    inventoryItemId: product.inventoryItemId,
    locationId: target.id,
    available: 5,
    idempotencyKey: `${scenarioId}-target-${runId}`,
  });
  assertNoUserErrors(workflow['targetInventoryActivate'] as CaptureCase, 'inventoryActivate');

  workflow['inventoryBeforeDelete'] = await runCase('inventoryBeforeDelete', inventoryItemReadQuery, {
    inventoryItemId: product.inventoryItemId,
    deletedLocationId: target.id,
  });
  assertInventoryLocations(workflow['inventoryBeforeDelete'] as CaptureCase, 2, [target.id, destination.id], []);

  workflow['targetDeactivate'] = await runCase('targetDeactivate', locationDeactivateMutation, {
    locationId: target.id,
    destinationLocationId: destination.id,
    idempotencyKey: `${scenarioId}-deactivate-${runId}`,
  });
  assertNoUserErrors(workflow['targetDeactivate'] as CaptureCase, 'locationDeactivate');

  workflow['targetDelete'] = await runCase('targetDelete', locationDeleteMutation, {
    locationId: target.id,
  });
  assertNoUserErrors(workflow['targetDelete'] as CaptureCase, 'locationDelete');
  targetLocationId = null;

  workflow['inventoryAfterDelete'] = await runCase('inventoryAfterDelete', inventoryItemReadQuery, {
    inventoryItemId: product.inventoryItemId,
    deletedLocationId: target.id,
  });
  assertInventoryLocations(workflow['inventoryAfterDelete'] as CaptureCase, 1, [destination.id], [target.id]);
} finally {
  await cleanupProduct(productId, cleanup);
  await cleanupLocation(targetLocationId, cleanup, runId, 'targetLocation');
  await cleanupLocation(destinationLocationId, cleanup, runId, 'destinationLocation');
}

const capture = {
  capturedAt,
  storeDomain,
  apiVersion,
  notes: [
    'Captures Admin API 2026-04 inventory-level behavior across a successful locationDelete lifecycle: two disposable merchant-managed locations are stocked for one disposable product, the target location is deactivated with a destination, then deleted.',
    'The before-delete read proves the inventory item had levels at two locations. The after-delete read proves Shopify no longer returns a level for the deleted location and locationsCount drops to one.',
  ],
  workflow,
  cleanup,
  upstreamCalls,
};

await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      workflow: Object.keys(workflow),
      cleanup: cleanup.map((entry) => entry.name),
      upstreamCalls: upstreamCalls.length,
    },
    null,
    2,
  ),
);
