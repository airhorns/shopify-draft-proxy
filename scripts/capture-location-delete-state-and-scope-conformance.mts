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

const scenarioId = 'location-delete-state-and-scope';
const outputApiVersion = '2026-04';
const requestedConfig = readConformanceScriptConfig({
  defaultApiVersion: outputApiVersion,
  exitOnMissing: true,
});
const { storeDomain, adminOrigin } = requestedConfig;
const adminAccessToken = await getValidConformanceAccessToken({
  adminOrigin,
  apiVersion: outputApiVersion,
});
const adminHeaders = buildAdminAuthHeaders(adminAccessToken);
const outputDir = path.join('fixtures', 'conformance', storeDomain, outputApiVersion, 'store-properties');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

function createClient(apiVersion: string): AdminGraphqlClient {
  return createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: adminHeaders,
  });
}

const client = createClient(outputApiVersion);

const locationHydrateQuery = `#graphql
  query StorePropertiesLocationHydrate($id: ID!) {
    location(id: $id) {
      id
      legacyResourceId
      name
      activatable
      addressVerified
      createdAt
      deactivatable
      deactivatedAt
      deletable
      fulfillsOnlineOrders
      hasActiveInventory
      hasUnfulfilledOrders
      isActive
      isFulfillmentService
      isPrimary
      shipsInventory
      updatedAt
      fulfillmentService {
        id
        handle
        serviceName
      }
      address {
        address1
        address2
        city
        country
        countryCode
        formatted
        latitude
        longitude
        phone
        province
        provinceCode
        zip
      }
      suggestedAddresses {
        address1
        countryCode
        formatted
      }
      metafield(namespace: "custom", key: "hours") {
        id
        namespace
        key
        value
        type
      }
      metafields(first: 3) {
        nodes {
          id
          namespace
          key
          value
          type
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      inventoryLevels(first: 3) {
        nodes {
          id
          item {
            id
          }
          location {
            id
            name
          }
          quantities(names: ["available", "committed", "on_hand"]) {
            name
            quantity
            updatedAt
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
    }
  }
`;

const primaryLocationQuery = `#graphql
  query LocationDeletePrimaryLocationSeed {
    locations(first: 20) {
      nodes {
        id
        name
        isActive
        isFulfillmentService
        hasActiveInventory
        hasUnfulfilledOrders
        isPrimary
      }
    }
  }
`;

const locationsReadQuery = `#graphql
  query LocationDeleteReadBack($first: Int!) {
    locations(first: $first) {
      edges {
        node {
          id
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const locationAddMutation = `#graphql
  mutation LocationDeleteFixtureAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        isActive
        hasActiveInventory
        isFulfillmentService
      }
      userErrors {
        field
        message
      }
    }
  }
`;

function locationDeactivateMutation(idempotencyKey: string): string {
  return `#graphql
  mutation LocationDeleteFixtureDeactivate($locationId: ID!) {
    locationDeactivate(locationId: $locationId)
      @idempotent(key: "${idempotencyKey}") {
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
}

function locationDeactivateCleanupMutation(idempotencyKey: string): string {
  return `#graphql
  mutation LocationDeleteFixtureCleanupDeactivate($locationId: ID!, $destinationLocationId: ID!) {
    locationDeactivate(locationId: $locationId, destinationLocationId: $destinationLocationId)
      @idempotent(key: "${idempotencyKey}") {
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
}

const locationDeleteMutation = `#graphql
  mutation LocationDeleteStateAndScope($locationId: ID!) {
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
        quantities(names: ["available", "committed", "on_hand"]) {
          name
          quantity
          updatedAt
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

const fulfillmentServiceCreateMutation = `#graphql
  mutation LocationDeleteFulfillmentServiceCreate($name: String!) {
    fulfillmentServiceCreate(
      name: $name
      trackingSupport: true
      inventoryManagement: true
      requiresShippingMethod: true
    ) {
      fulfillmentService {
        id
        handle
        serviceName
        location {
          id
          name
          isFulfillmentService
          isActive
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentServiceDeleteMutation = `#graphql
  mutation LocationDeleteFulfillmentServiceCleanup($id: ID!) {
    fulfillmentServiceDelete(id: $id, inventoryAction: DELETE) {
      deletedId
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
    throw new Error(`${capture.name} did not return data: ${JSON.stringify(capture.response.payload)}`);
  }
  return data;
}

function mutationPayload(capture: CaptureCase, key: string): JsonRecord {
  const payload = readObject(readData(capture)[key]);
  if (!payload) {
    throw new Error(`${capture.name} did not return ${key}: ${JSON.stringify(capture.response.payload)}`);
  }
  return payload;
}

function userErrors(capture: CaptureCase, key: string): unknown[] {
  const errors =
    mutationPayload(capture, key)['userErrors'] ??
    mutationPayload(capture, key)['locationDeleteUserErrors'] ??
    mutationPayload(capture, key)['locationDeactivateUserErrors'];
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

function assertHasLocationDeleteErrors(capture: CaptureCase): void {
  assertNoTopLevelErrors(capture);
  const errors = userErrors(capture, 'locationDelete');
  if (errors.length === 0) {
    throw new Error(`${capture.name} returned no locationDeleteUserErrors.`);
  }
}

function readLocationAddId(capture: CaptureCase): string {
  const location = readObject(mutationPayload(capture, 'locationAdd')['location']);
  const id = location?.['id'];
  if (typeof id !== 'string') {
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
    throw new Error(
      `${capture.name} did not create a product inventory item: ${JSON.stringify(capture.response.payload)}`,
    );
  }
  return { productId, variantId, inventoryItemId };
}

function readFulfillmentService(capture: CaptureCase): { serviceId: string; locationId: string } {
  const service = readObject(mutationPayload(capture, 'fulfillmentServiceCreate')['fulfillmentService']);
  const location = readObject(service?.['location']);
  const serviceId = service?.['id'];
  const locationId = location?.['id'];
  if (typeof serviceId !== 'string' || typeof locationId !== 'string') {
    throw new Error(
      `${capture.name} did not create a fulfillment service location: ${JSON.stringify(capture.response.payload)}`,
    );
  }
  return { serviceId, locationId };
}

function readPrimaryLocationId(capture: CaptureCase): string {
  const locations = readObject(readData(capture)['locations']);
  const nodes = locations?.['nodes'];
  const primary = Array.isArray(nodes) ? nodes.map(readObject).find((node) => node?.['isPrimary'] === true) : null;
  const id = primary?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`Could not find a primary location: ${JSON.stringify(capture.response.payload)}`);
  }
  return id;
}

function locationInput(name: string): JsonRecord {
  return {
    name,
    fulfillsOnlineOrders: false,
    address: {
      address1: '663 State St',
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

async function hydrateLocation(name: string, id: string): Promise<CaptureCase> {
  const hydrate = await runCase(name, locationHydrateQuery, { id });
  assertNoTopLevelErrors(hydrate);
  return hydrate;
}

async function deleteLocation(name: string, id: string): Promise<CaptureCase> {
  return runCase(name, locationDeleteMutation, { locationId: id });
}

async function deactivateLocation(name: string, id: string, key: string): Promise<CaptureCase> {
  const result = await runCase(name, locationDeactivateMutation(key), { locationId: id });
  assertNoUserErrors(result, 'locationDeactivate');
  return result;
}

async function setupInventory(
  name: string,
  locationId: string,
  available: number,
  runId: string,
): Promise<{ create: CaptureCase; track: CaptureCase; activate: CaptureCase; productId: string }> {
  const create = await runCase(`${name}ProductCreate`, productCreateMutation, {
    product: { title: `${name} product ${runId}` },
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
    available,
    idempotencyKey: `${scenarioId}-${name}-${runId}`,
  });
  assertNoUserErrors(activate, 'inventoryActivate');

  return { create, track, activate, productId: product.productId };
}

async function cleanupProduct(productId: string | null, cleanup: CaptureCase[], name: string): Promise<void> {
  if (!productId) return;
  cleanup.push(await runCase(name, productDeleteMutation, { input: { id: productId } }));
}

async function cleanupMerchantLocation(
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
    const query = destinationLocationId
      ? locationDeactivateCleanupMutation(`${scenarioId}-${name}-cleanup-${runId}`)
      : locationDeactivateMutation(`${scenarioId}-${name}-cleanup-${runId}`);
    const variables = destinationLocationId ? { locationId, destinationLocationId } : { locationId };
    cleanup.push(await runCase(`${name}CleanupDeactivate`, query, variables));
  }
  cleanup.push(await deleteLocation(`${name}CleanupDelete`, locationId));
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const runId = capturedAt.replace(/\D/gu, '').slice(0, 14);
const setup: JsonRecord = {};
const workflow: JsonRecord = {};
const cleanup: CaptureCase[] = [];
const upstreamCalls: Array<{
  operationName: string;
  variables: JsonRecord;
  query: string;
  response: { status: number; body: unknown };
}> = [];

let activeNoInventoryLocationId: string | null = null;
let activeInventoryLocationId: string | null = null;
let activeInventoryProductId: string | null = null;
let fulfillmentServiceId: string | null = null;
let fulfillmentServiceLocationId: string | null = null;
let cleanupDestinationLocationId: string | null = null;

try {
  setup['primaryLocations'] = await runCase('primaryLocations', primaryLocationQuery);
  assertNoTopLevelErrors(setup['primaryLocations'] as CaptureCase);
  cleanupDestinationLocationId = readPrimaryLocationId(setup['primaryLocations'] as CaptureCase);

  const inactiveNoInventory = await createLocation(`HAR-663 delete success ${runId}`);
  setup['inactiveNoInventoryCreate'] = inactiveNoInventory.create;
  setup['inactiveNoInventoryDeactivate'] = await deactivateLocation(
    'inactiveNoInventoryDeactivate',
    inactiveNoInventory.id,
    `${scenarioId}-inactive-no-inventory-${runId}`,
  );
  const inactiveNoInventoryHydrate = await hydrateLocation('inactiveNoInventoryHydrate', inactiveNoInventory.id);
  workflow['inactiveNoInventoryDelete'] = await deleteLocation('inactiveNoInventoryDelete', inactiveNoInventory.id);
  assertNoUserErrors(workflow['inactiveNoInventoryDelete'] as CaptureCase, 'locationDelete');
  workflow['locationsAfterInactiveNoInventoryDelete'] = await runCase(
    'locationsAfterInactiveNoInventoryDelete',
    locationsReadQuery,
    {
      first: 50,
    },
  );
  assertNoTopLevelErrors(workflow['locationsAfterInactiveNoInventoryDelete'] as CaptureCase);
  upstreamCalls.push({
    operationName: 'LocationDeleteReadBack',
    variables: { first: 50 },
    query: digestDocument(locationsReadQuery),
    response: {
      status: (workflow['locationsAfterInactiveNoInventoryDelete'] as CaptureCase).response.status,
      body: (workflow['locationsAfterInactiveNoInventoryDelete'] as CaptureCase).response.payload,
    },
  });
  upstreamCalls.push({
    operationName: 'StorePropertiesLocationHydrate',
    variables: { id: inactiveNoInventory.id },
    query: digestDocument(locationHydrateQuery),
    response: {
      status: inactiveNoInventoryHydrate.response.status,
      body: inactiveNoInventoryHydrate.response.payload,
    },
  });

  const activeNoInventory = await createLocation(`HAR-663 active no inventory ${runId}`);
  activeNoInventoryLocationId = activeNoInventory.id;
  setup['activeNoInventoryCreate'] = activeNoInventory.create;
  const activeNoInventoryHydrate = await hydrateLocation('activeNoInventoryHydrate', activeNoInventory.id);
  workflow['activeNoInventoryDelete'] = await deleteLocation('activeNoInventoryDelete', activeNoInventory.id);
  assertHasLocationDeleteErrors(workflow['activeNoInventoryDelete'] as CaptureCase);
  upstreamCalls.push({
    operationName: 'StorePropertiesLocationHydrate',
    variables: { id: activeNoInventory.id },
    query: digestDocument(locationHydrateQuery),
    response: {
      status: activeNoInventoryHydrate.response.status,
      body: activeNoInventoryHydrate.response.payload,
    },
  });

  const activeInventory = await createLocation(`HAR-663 active inventory ${runId}`);
  activeInventoryLocationId = activeInventory.id;
  setup['activeInventoryCreate'] = activeInventory.create;
  const activeInventorySetup = await setupInventory('activeInventory', activeInventory.id, 7, runId);
  activeInventoryProductId = activeInventorySetup.productId;
  setup['activeInventoryProductCreate'] = activeInventorySetup.create;
  setup['activeInventoryProductTrack'] = activeInventorySetup.track;
  setup['activeInventoryActivate'] = activeInventorySetup.activate;
  const activeInventoryHydrate = await hydrateLocation('activeInventoryHydrate', activeInventory.id);
  workflow['activeInventoryDelete'] = await deleteLocation('activeInventoryDelete', activeInventory.id);
  assertHasLocationDeleteErrors(workflow['activeInventoryDelete'] as CaptureCase);
  upstreamCalls.push({
    operationName: 'StorePropertiesLocationHydrate',
    variables: { id: activeInventory.id },
    query: digestDocument(locationHydrateQuery),
    response: {
      status: activeInventoryHydrate.response.status,
      body: activeInventoryHydrate.response.payload,
    },
  });

  const primaryLocationId = readPrimaryLocationId(setup['primaryLocations'] as CaptureCase);
  const primaryHydrate = await hydrateLocation('primaryLocationHydrate', primaryLocationId);
  workflow['primaryLocationDelete'] = await deleteLocation('primaryLocationDelete', primaryLocationId);
  assertHasLocationDeleteErrors(workflow['primaryLocationDelete'] as CaptureCase);
  upstreamCalls.push({
    operationName: 'StorePropertiesLocationHydrate',
    variables: { id: primaryLocationId },
    query: digestDocument(locationHydrateQuery),
    response: {
      status: primaryHydrate.response.status,
      body: primaryHydrate.response.payload,
    },
  });

  setup['fulfillmentServiceCreate'] = await runCase('fulfillmentServiceCreate', fulfillmentServiceCreateMutation, {
    name: `HAR-663 FS ${runId}`,
  });
  assertNoUserErrors(setup['fulfillmentServiceCreate'] as CaptureCase, 'fulfillmentServiceCreate');
  const fulfillmentService = readFulfillmentService(setup['fulfillmentServiceCreate'] as CaptureCase);
  fulfillmentServiceId = fulfillmentService.serviceId;
  fulfillmentServiceLocationId = fulfillmentService.locationId;
  const fulfillmentServiceHydrate = await hydrateLocation(
    'fulfillmentServiceLocationHydrate',
    fulfillmentService.locationId,
  );
  workflow['fulfillmentServiceLocationDelete'] = await deleteLocation(
    'fulfillmentServiceLocationDelete',
    fulfillmentService.locationId,
  );
  assertHasLocationDeleteErrors(workflow['fulfillmentServiceLocationDelete'] as CaptureCase);
  upstreamCalls.push({
    operationName: 'StorePropertiesLocationHydrate',
    variables: { id: fulfillmentService.locationId },
    query: digestDocument(locationHydrateQuery),
    response: {
      status: fulfillmentServiceHydrate.response.status,
      body: fulfillmentServiceHydrate.response.payload,
    },
  });
} finally {
  await cleanupProduct(activeInventoryProductId, cleanup, 'activeInventoryProductCleanup');
  await cleanupMerchantLocation(
    activeInventoryLocationId,
    cleanupDestinationLocationId,
    cleanup,
    runId,
    'activeInventoryLocation',
  );
  await cleanupMerchantLocation(
    activeNoInventoryLocationId,
    cleanupDestinationLocationId,
    cleanup,
    runId,
    'activeNoInventoryLocation',
  );
  if (fulfillmentServiceId) {
    cleanup.push(
      await runCase('fulfillmentServiceCleanup', fulfillmentServiceDeleteMutation, { id: fulfillmentServiceId }),
    );
  } else if (fulfillmentServiceLocationId) {
    console.warn(`Fulfillment service location ${fulfillmentServiceLocationId} was created without a service id.`);
  }
}

const capture = {
  capturedAt,
  storeDomain,
  apiVersion: outputApiVersion,
  notes: [
    'Captures public Admin API locationDelete guard behavior for successful inactive/no-inventory deletion, active/no-inventory validation, active stocked validation, primary-location validation, and fulfillment-service-managed scope validation.',
    'The public Admin API did not permit constructing an inactive stocked location: inventoryActivate rejects deactivated locations, and locationDeactivate requires inventory relocation while stock remains. The proxy still models inactive stocked fixture state in runtime tests.',
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
