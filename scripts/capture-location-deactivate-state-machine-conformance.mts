/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { createHash } from 'node:crypto';
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

type CatalogLocation = {
  id?: string;
  isActive?: boolean;
  fulfillsOnlineOrders?: boolean;
  isPrimary?: boolean;
};

const scenarioId = 'location-deactivate-state-machine';
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

const locationHydrateQuery =
  'query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }';

const locationCatalogQuery = `#graphql
  query LocationDeactivateStateMachineCatalog {
    locations(first: 100, includeInactive: true) {
      nodes {
        id
        isActive
        isPrimary
        fulfillsOnlineOrders
      }
    }
  }
`;

const locationAddMutation = `#graphql
  mutation LocationDeactivateStateMachineAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        isActive
        fulfillsOnlineOrders
        hasActiveInventory
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const locationEditMutation = `#graphql
  mutation LocationDeactivateStateMachineEdit($id: ID!, $input: LocationEditInput!) {
    locationEdit(id: $id, input: $input) {
      location {
        id
        fulfillsOnlineOrders
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const locationDeactivateFields = `#graphql
  fragment LocationDeactivateStateMachineFields on Location {
    id
    name
    isActive
    activatable
    deactivatable
    fulfillsOnlineOrders
    hasActiveInventory
    hasUnfulfilledOrders
    deletable
    shipsInventory
  }
`;

const locationDeactivateMutation = `#graphql
  ${locationDeactivateFields}

  mutation LocationDeactivateStateMachine($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        ...LocationDeactivateStateMachineFields
      }
      locationDeactivateUserErrors {
        field
        message
        code
      }
    }
  }
`;

const locationDeactivateWithDestinationMutation = `#graphql
  ${locationDeactivateFields}

  mutation LocationDeactivateStateMachineWithDestination(
    $locationId: ID!
    $destinationLocationId: ID!
    $idempotencyKey: String!
  ) {
    locationDeactivate(locationId: $locationId, destinationLocationId: $destinationLocationId)
      @idempotent(key: $idempotencyKey) {
      location {
        ...LocationDeactivateStateMachineFields
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
  mutation LocationDeactivateStateMachineDelete($locationId: ID!) {
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

const productCreateMutation = `#graphql
  mutation LocationDeactivateInventoryProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        variants(first: 1) {
          nodes {
            id
            inventoryItem {
              id
              tracked
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
  mutation LocationDeactivateInventoryProductTrack($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      productVariants {
        id
        inventoryItem {
          id
          tracked
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const inventoryActivateMutation = `#graphql
  mutation LocationDeactivateInventoryActivate(
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
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation LocationDeactivateInventoryProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

function digestDocument(document: string): string {
  return `sha256:${createHash('sha256').update(document).digest('hex')}`;
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
  return typeof value === 'object' && value !== null ? (value as JsonRecord) : null;
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

function assertLocationDeactivateCode(capture: CaptureCase, code: string): void {
  assertNoTopLevelErrors(capture);
  const found = userErrors(capture, 'locationDeactivate').some((error) => {
    const record = readObject(error);
    return record?.['code'] === code;
  });
  if (!found) {
    throw new Error(`${capture.name} did not return ${code}: ${JSON.stringify(capture.response.payload)}`);
  }
}

function readLocationAddId(capture: CaptureCase): string {
  const location = readObject(mutationPayload(capture, 'locationAdd')['location']);
  const id = location?.['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${capture.name} did not create a location: ${JSON.stringify(capture.response.payload)}`);
  }
  return id;
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
    throw new Error(`${capture.name} did not create inventory item ids: ${JSON.stringify(capture.response.payload)}`);
  }
  return { productId, variantId, inventoryItemId };
}

function catalogLocations(capture: CaptureCase): CatalogLocation[] {
  const locations = readObject(readData(capture)['locations']);
  const nodes = locations?.['nodes'];
  return Array.isArray(nodes) ? (nodes.map(readObject).filter(Boolean) as CatalogLocation[]) : [];
}

function primaryLocationId(catalog: CaptureCase): string {
  const primary = catalogLocations(catalog).find((location) => location.isPrimary === true);
  if (typeof primary?.id !== 'string') {
    throw new Error(`No primary location found: ${JSON.stringify(catalog.response.payload)}`);
  }
  return primary.id;
}

function locationInput(name: string, fulfillsOnlineOrders: boolean): JsonRecord {
  return {
    name,
    fulfillsOnlineOrders,
    address: {
      address1: '873 State St',
      city: 'Boston',
      provinceCode: 'MA',
      countryCode: 'US',
      zip: '02110',
    },
  };
}

async function createLocation(
  name: string,
  fulfillsOnlineOrders = false,
): Promise<{ create: CaptureCase; id: string }> {
  const create = await runCase(`${name}LocationAdd`, locationAddMutation, {
    input: locationInput(name, fulfillsOnlineOrders),
  });
  assertNoUserErrors(create, 'locationAdd');
  return { create, id: readLocationAddId(create) };
}

async function hydrateLocation(name: string, id: string): Promise<CaptureCase> {
  const hydrate = await runCase(name, locationHydrateQuery, { id });
  assertNoTopLevelErrors(hydrate);
  return hydrate;
}

async function deactivateLocation(name: string, id: string, runId: string): Promise<CaptureCase> {
  const result = await runCase(name, locationDeactivateMutation, {
    locationId: id,
    idempotencyKey: `${scenarioId}-${name}-${runId}`,
  });
  assertNoUserErrors(result, 'locationDeactivate');
  return result;
}

async function setupInventory(
  name: string,
  locationId: string,
  runId: string,
): Promise<{ create: CaptureCase; track: CaptureCase; activate: CaptureCase; productId: string }> {
  const create = await runCase(`${name}ProductCreate`, productCreateMutation, {
    product: { title: `${scenarioId} ${name} product ${runId}` },
  });
  assertNoUserErrors(create, 'productCreate');
  const product = readProduct(create);

  const track = await runCase(`${name}ProductTrackInventory`, productTrackInventoryMutation, {
    productId: product.productId,
    variants: [{ id: product.variantId, inventoryItem: { tracked: true } }],
  });
  assertNoUserErrors(track, 'productVariantsBulkUpdate');

  const activate = await runCase(`${name}InventoryActivate`, inventoryActivateMutation, {
    inventoryItemId: product.inventoryItemId,
    locationId,
    available: 7,
    idempotencyKey: `${scenarioId}-${name}-inventory-${runId}`,
  });
  assertNoUserErrors(activate, 'inventoryActivate');

  return { create, track, activate, productId: product.productId };
}

async function editFulfillsOnlineOrders(location: CatalogLocation, value: boolean, name: string): Promise<CaptureCase> {
  if (typeof location.id !== 'string') {
    throw new Error(`Cannot edit a catalog location without id: ${JSON.stringify(location)}`);
  }
  const result = await runCase(name, locationEditMutation, {
    id: location.id,
    input: { fulfillsOnlineOrders: value },
  });
  assertNoUserErrors(result, 'locationEdit');
  return result;
}

async function cleanupProduct(productId: string | null, cleanup: CaptureCase[]): Promise<void> {
  if (!productId) return;
  cleanup.push(await runCase('activeInventoryProductCleanup', productDeleteMutation, { input: { id: productId } }));
}

async function cleanupLocation(
  locationId: string | null,
  destinationLocationId: string | null,
  cleanup: CaptureCase[],
  runId: string,
  name: string,
): Promise<void> {
  if (!locationId) return;
  const hydrate = await hydrateLocation(`${name}CleanupHydrate`, locationId);
  const location = readObject(readData(hydrate)['location']);
  if (location?.['isActive'] === true) {
    const query = destinationLocationId ? locationDeactivateWithDestinationMutation : locationDeactivateMutation;
    const variables = destinationLocationId
      ? {
          locationId,
          destinationLocationId,
          idempotencyKey: `${scenarioId}-${name}-cleanup-${runId}`,
        }
      : { locationId, idempotencyKey: `${scenarioId}-${name}-cleanup-${runId}` };
    cleanup.push(await runCase(`${name}CleanupDeactivate`, query, variables));
  }
  cleanup.push(await runCase(`${name}CleanupDelete`, locationDeleteMutation, { locationId }));
}

function upstreamCall(hydrate: CaptureCase, id: string): JsonRecord {
  return {
    operationName: 'StorePropertiesLocationHydrate',
    variables: { id },
    query: digestDocument(locationHydrateQuery),
    response: {
      status: hydrate.response.status,
      body: hydrate.response.payload,
    },
  };
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const runId = capturedAt.replace(/\D/gu, '').slice(0, 14);
const setup: JsonRecord = {};
const workflow: JsonRecord = {};
const cleanup: CaptureCase[] = [];
const restoreOnline: CaptureCase[] = [];
const upstreamCalls: JsonRecord[] = [];

let sourceLocationId: string | null = null;
let inactiveDestinationId: string | null = null;
let activeInventoryLocationId: string | null = null;
let activeInventoryProductId: string | null = null;
let onlyOnlineLocationId: string | null = null;
let cleanupDestinationId: string | null = null;
let onlineLocationsToRestore: CatalogLocation[] = [];

try {
  setup['initialCatalog'] = await runCase('initialCatalog', locationCatalogQuery);
  assertNoTopLevelErrors(setup['initialCatalog'] as CaptureCase);
  cleanupDestinationId = primaryLocationId(setup['initialCatalog'] as CaptureCase);

  const source = await createLocation(`${scenarioId} source ${runId}`, false);
  sourceLocationId = source.id;
  setup['sourceCreate'] = source.create;
  const sourceHydrate = await hydrateLocation('sourceHydrate', source.id);
  upstreamCalls.push(upstreamCall(sourceHydrate, source.id));

  const inactiveDestination = await createLocation(`${scenarioId} inactive destination ${runId}`, false);
  inactiveDestinationId = inactiveDestination.id;
  setup['inactiveDestinationCreate'] = inactiveDestination.create;
  setup['inactiveDestinationDeactivate'] = await deactivateLocation(
    'inactiveDestinationDeactivate',
    inactiveDestination.id,
    runId,
  );
  const inactiveDestinationHydrate = await hydrateLocation('inactiveDestinationHydrate', inactiveDestination.id);
  upstreamCalls.push(upstreamCall(inactiveDestinationHydrate, inactiveDestination.id));

  workflow['destinationSameId'] = await runCase('destinationSameId', locationDeactivateWithDestinationMutation, {
    locationId: source.id,
    destinationLocationId: source.id,
    idempotencyKey: `${scenarioId}-destination-same-${runId}`,
  });
  assertLocationDeactivateCode(
    workflow['destinationSameId'] as CaptureCase,
    'DESTINATION_LOCATION_IS_THE_SAME_LOCATION',
  );

  workflow['destinationInactive'] = await runCase('destinationInactive', locationDeactivateWithDestinationMutation, {
    locationId: source.id,
    destinationLocationId: inactiveDestination.id,
    idempotencyKey: `${scenarioId}-destination-inactive-${runId}`,
  });
  assertLocationDeactivateCode(
    workflow['destinationInactive'] as CaptureCase,
    'DESTINATION_LOCATION_NOT_FOUND_OR_INACTIVE',
  );

  const activeInventory = await createLocation(`${scenarioId} active inventory ${runId}`, false);
  activeInventoryLocationId = activeInventory.id;
  setup['activeInventoryCreate'] = activeInventory.create;
  const activeInventorySetup = await setupInventory('activeInventory', activeInventory.id, runId);
  activeInventoryProductId = activeInventorySetup.productId;
  setup['activeInventoryProductCreate'] = activeInventorySetup.create;
  setup['activeInventoryProductTrack'] = activeInventorySetup.track;
  setup['activeInventoryActivate'] = activeInventorySetup.activate;
  const activeInventoryHydrate = await hydrateLocation('activeInventoryHydrate', activeInventory.id);
  upstreamCalls.push(upstreamCall(activeInventoryHydrate, activeInventory.id));
  workflow['sourceHasActiveInventory'] = await runCase('sourceHasActiveInventory', locationDeactivateMutation, {
    locationId: activeInventory.id,
    idempotencyKey: `${scenarioId}-source-active-inventory-${runId}`,
  });
  assertLocationDeactivateCode(workflow['sourceHasActiveInventory'] as CaptureCase, 'HAS_ACTIVE_INVENTORY_ERROR');

  const onlyOnline = await createLocation(`${scenarioId} only online ${runId}`, true);
  onlyOnlineLocationId = onlyOnline.id;
  setup['onlyOnlineCreate'] = onlyOnline.create;
  const onlyOnlineHydrate = await hydrateLocation('onlyOnlineHydrate', onlyOnline.id);
  upstreamCalls.push(upstreamCall(onlyOnlineHydrate, onlyOnline.id));
  onlineLocationsToRestore = catalogLocations(setup['initialCatalog'] as CaptureCase).filter(
    (location) => location.isActive === true && location.fulfillsOnlineOrders === true && location.id !== onlyOnline.id,
  );
  const disableOnline: CaptureCase[] = [];
  for (const location of onlineLocationsToRestore) {
    disableOnline.push(await editFulfillsOnlineOrders(location, false, `disableExternalOnline:${location.id}`));
  }
  setup['onlyOnlineSetupDisables'] = disableOnline;
  workflow['sourceOnlyOnlineFulfiller'] = await runCase('sourceOnlyOnlineFulfiller', locationDeactivateMutation, {
    locationId: onlyOnline.id,
    idempotencyKey: `${scenarioId}-source-only-online-${runId}`,
  });
  assertLocationDeactivateCode(
    workflow['sourceOnlyOnlineFulfiller'] as CaptureCase,
    'CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT',
  );
  for (const location of onlineLocationsToRestore) {
    restoreOnline.push(await editFulfillsOnlineOrders(location, true, `restoreExternalOnline:${location.id}`));
  }
  setup['onlyOnlineRestores'] = restoreOnline;

  const primaryId = cleanupDestinationId;
  if (!primaryId) throw new Error('No primary location available for permanent-blocked capture.');
  const primaryHydrate = await hydrateLocation('primaryLocationHydrate', primaryId);
  upstreamCalls.push(upstreamCall(primaryHydrate, primaryId));
  workflow['sourcePermanentlyBlocked'] = await runCase('sourcePermanentlyBlocked', locationDeactivateMutation, {
    locationId: primaryId,
    idempotencyKey: `${scenarioId}-source-permanent-${runId}`,
  });
  assertLocationDeactivateCode(
    workflow['sourcePermanentlyBlocked'] as CaptureCase,
    'PERMANENTLY_BLOCKED_FROM_DEACTIVATION_ERROR',
  );
} finally {
  for (const location of onlineLocationsToRestore) {
    if (!restoreOnline.some((restore) => restore.variables['id'] === location.id)) {
      try {
        cleanup.push(await editFulfillsOnlineOrders(location, true, `finallyRestoreExternalOnline:${location.id}`));
      } catch (error) {
        console.error(
          JSON.stringify(
            {
              ok: false,
              restoreFailed: location.id,
              error: error instanceof Error ? error.message : String(error),
            },
            null,
            2,
          ),
        );
      }
    }
  }
  await cleanupProduct(activeInventoryProductId, cleanup);
  await cleanupLocation(activeInventoryLocationId, cleanupDestinationId, cleanup, runId, 'activeInventoryLocation');
  await cleanupLocation(onlyOnlineLocationId, cleanupDestinationId, cleanup, runId, 'onlyOnlineLocation');
  await cleanupLocation(sourceLocationId, cleanupDestinationId, cleanup, runId, 'sourceLocation');
  await cleanupLocation(inactiveDestinationId, cleanupDestinationId, cleanup, runId, 'inactiveDestination');
}

const capture = {
  capturedAt,
  storeDomain,
  apiVersion,
  notes: [
    'Captures public Admin API 2026-04 locationDeactivate state-machine guard behavior for destination same-id, inactive destination, active inventory without destination, primary permanent block, and only-online-fulfiller rejection.',
    'The recorder creates disposable merchant-managed locations and a disposable stocked product, temporarily disables pre-existing online-fulfilling locations for the only-online branch, restores them before cleanup, then deactivates/deletes disposable locations.',
  ],
  setup,
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
      cases: Object.keys(workflow),
      cleanup: cleanup.map((entry) => entry.name),
    },
    null,
    2,
  ),
);
