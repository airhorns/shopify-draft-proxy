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

type LocationNode = {
  id?: unknown;
  isActive?: unknown;
  isFulfillmentService?: unknown;
};

const scenarioId = 'location-activate-limit-and-control';
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
const locationLimitStatusQuery =
  'query StorePropertiesLocationLimitStatus($first: Int!) { shop { resourceLimits { locationLimit } } locations(first: $first, includeInactive: true) { nodes { id isActive isFulfillmentService } pageInfo { hasNextPage } } }';

const locationAddMutation = `#graphql
  mutation LocationActivateLimitFixtureAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        isActive
      }
      userErrors {
        field
        code
        message
      }
    }
  }
`;

const locationDeactivateMutation = `#graphql
  mutation LocationActivateLimitFixtureDeactivate($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        id
        isActive
      }
      locationDeactivateUserErrors {
        field
        code
        message
      }
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation LocationActivateLimitFixtureDelete($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors {
        field
        code
        message
      }
    }
  }
`;

const locationActivateMutation = `#graphql
  mutation LocationActivateLimitAndControl($locationId: ID!, $idempotencyKey: String!) {
    locationActivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        id
        isActive
      }
      locationActivateUserErrors {
        field
        code
        message
      }
    }
  }
`;

async function runCase(name: string, query: string, variables: JsonRecord = {}): Promise<CaptureCase> {
  const response = await client.runGraphqlRequest(query, variables);
  return {
    name,
    query,
    variables,
    response,
  };
}

function asRecord(value: unknown): JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : {};
}

function assertNoTopLevelErrors(capture: CaptureCase): void {
  if (capture.response.status !== 200 || capture.response.payload.errors) {
    throw new Error(`${capture.name} failed: ${JSON.stringify(capture.response.payload)}`);
  }
}

function mutationPayload(capture: CaptureCase, key: string): JsonRecord {
  assertNoTopLevelErrors(capture);
  const data = asRecord(capture.response.payload.data);
  return asRecord(data[key]);
}

function userErrors(capture: CaptureCase, key: string): unknown[] {
  const payload = mutationPayload(capture, key);
  const errors =
    payload['userErrors'] ?? payload['locationDeactivateUserErrors'] ?? payload['locationActivateUserErrors'];
  return Array.isArray(errors) ? errors : [];
}

function assertNoUserErrors(capture: CaptureCase, key: string): void {
  const errors = userErrors(capture, key);
  if (errors.length > 0) {
    throw new Error(`${capture.name} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function readAddedLocationId(capture: CaptureCase): string {
  const location = asRecord(mutationPayload(capture, 'locationAdd')['location']);
  const id = location['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${capture.name} did not return a location id: ${JSON.stringify(capture.response.payload)}`);
  }
  assertNoUserErrors(capture, 'locationAdd');
  return id;
}

function readLocationLimit(catalog: CaptureCase): number {
  assertNoTopLevelErrors(catalog);
  const data = asRecord(catalog.response.payload.data);
  const shop = asRecord(data['shop']);
  const resourceLimits = asRecord(shop['resourceLimits']);
  const limit = resourceLimits['locationLimit'];
  if (typeof limit !== 'number' || !Number.isFinite(limit) || limit < 1) {
    throw new Error(`Could not read shop.resourceLimits.locationLimit: ${JSON.stringify(catalog.response.payload)}`);
  }
  return limit;
}

function readLocationNodes(catalog: CaptureCase): LocationNode[] {
  assertNoTopLevelErrors(catalog);
  const data = asRecord(catalog.response.payload.data);
  const locations = asRecord(data['locations']);
  const pageInfo = asRecord(locations['pageInfo']);
  if (pageInfo['hasNextPage'] === true) {
    throw new Error(
      'Location catalog exceeded first:250; recorder needs pagination before it can safely count locations.',
    );
  }
  const nodes = locations['nodes'];
  return Array.isArray(nodes) ? nodes.map((node) => asRecord(node) as LocationNode) : [];
}

function activeMerchantManagedLocationCount(nodes: LocationNode[]): number {
  return nodes.filter((node) => node.isActive === true && node.isFulfillmentService !== true).length;
}

function locationAddInput(name: string): JsonRecord {
  return {
    input: {
      name,
      address: {
        countryCode: 'US',
        address1: '1 Activation Limit St',
        city: 'New York',
        zip: '10001',
      },
    },
  };
}

async function cleanupLocation(
  locationId: string | null,
  cleanup: CaptureCase[],
  suffix: string,
  uniqueSuffix: string,
): Promise<void> {
  if (!locationId) return;
  const locationToken = locationId.split('/').at(-1) ?? suffix;
  cleanup.push(
    await runCase(`cleanupDeactivate-${suffix}`, locationDeactivateMutation, {
      locationId,
      idempotencyKey: `${scenarioId}-cleanup-${suffix}-${uniqueSuffix}-${locationToken}`,
    }),
  );
  cleanup.push(await runCase(`cleanupDelete-${suffix}`, locationDeleteMutation, { locationId }));
}

function upstreamCall(operationName: string, query: string, variables: JsonRecord, capture: CaptureCase): JsonRecord {
  return {
    operationName,
    variables,
    query,
    response: {
      status: capture.response.status,
      body: capture.response.payload,
    },
  };
}

function assertLocationLimitError(capture: CaptureCase): void {
  const found = userErrors(capture, 'locationActivate').some((error) => {
    const record = asRecord(error);
    return (
      record['code'] === 'LOCATION_LIMIT' &&
      JSON.stringify(record['field']) === JSON.stringify(['locationId']) &&
      record['message'] === 'Shop has reached its location limit.'
    );
  });
  if (!found) {
    throw new Error(`${capture.name} did not return LOCATION_LIMIT: ${JSON.stringify(capture.response.payload)}`);
  }
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const uniqueSuffix = capturedAt.replace(/\D/gu, '').slice(0, 14);
const cleanup: CaptureCase[] = [];
const createdFillLocationIds: string[] = [];

let controlLocationId: string | null = null;
let controlHydrate: CaptureCase | null = null;
let controlLimitStatus: CaptureCase | null = null;
let controlActivate: CaptureCase | null = null;
let limitLocationId: string | null = null;
let limitHydrate: CaptureCase | null = null;
let limitStatus: CaptureCase | null = null;
let limitActivate: CaptureCase | null = null;
let finalCatalog: CaptureCase | null = null;

try {
  const controlCreate = await runCase(
    'controlLocationAdd',
    locationAddMutation,
    locationAddInput(`HAR1853 Activate Control ${uniqueSuffix}`),
  );
  controlLocationId = readAddedLocationId(controlCreate);
  cleanup.push(controlCreate);
  const controlDeactivate = await runCase('controlDeactivate', locationDeactivateMutation, {
    locationId: controlLocationId,
    idempotencyKey: `${scenarioId}-control-deactivate-${uniqueSuffix}`,
  });
  assertNoUserErrors(controlDeactivate, 'locationDeactivate');
  cleanup.push(controlDeactivate);
  controlHydrate = await runCase('controlHydrate', locationHydrateQuery, { id: controlLocationId });
  controlLimitStatus = await runCase('controlLimitStatus', locationLimitStatusQuery, { first: 250 });
  controlActivate = await runCase('controlActivate', locationActivateMutation, {
    locationId: controlLocationId,
    idempotencyKey: `${scenarioId}-control-${uniqueSuffix}`,
  });
  assertNoUserErrors(controlActivate, 'locationActivate');

  const limitCreate = await runCase(
    'limitLocationAdd',
    locationAddMutation,
    locationAddInput(`HAR1853 Activate Limit ${uniqueSuffix}`),
  );
  limitLocationId = readAddedLocationId(limitCreate);
  cleanup.push(limitCreate);
  const limitDeactivate = await runCase('limitDeactivate', locationDeactivateMutation, {
    locationId: limitLocationId,
    idempotencyKey: `${scenarioId}-limit-deactivate-${uniqueSuffix}`,
  });
  assertNoUserErrors(limitDeactivate, 'locationDeactivate');
  cleanup.push(limitDeactivate);
  limitHydrate = await runCase('limitHydrate', locationHydrateQuery, { id: limitLocationId });

  const beforeFillStatus = await runCase('beforeFillStatus', locationLimitStatusQuery, { first: 250 });
  const locationLimit = readLocationLimit(beforeFillStatus);
  const activeBeforeFill = activeMerchantManagedLocationCount(readLocationNodes(beforeFillStatus));
  const requiredCreates = locationLimit - activeBeforeFill;
  if (requiredCreates < 0) {
    throw new Error(`Active location count ${activeBeforeFill} already exceeds location limit ${locationLimit}.`);
  }

  for (let index = 0; index < requiredCreates; index += 1) {
    const create = await runCase(
      `fillLocationAdd-${String(index + 1).padStart(3, '0')}`,
      locationAddMutation,
      locationAddInput(`HAR1853 Activate Fill ${uniqueSuffix} ${index + 1}`),
    );
    const id = readAddedLocationId(create);
    createdFillLocationIds.push(id);
    cleanup.push(create);
    if ((index + 1) % 25 === 0 || index + 1 === requiredCreates) {
      console.log(JSON.stringify({ progress: 'created-locations', count: index + 1, requiredCreates }));
    }
  }

  limitStatus = await runCase('limitStatus', locationLimitStatusQuery, { first: 250 });
  limitActivate = await runCase('limitActivate', locationActivateMutation, {
    locationId: limitLocationId,
    idempotencyKey: `${scenarioId}-limit-${uniqueSuffix}`,
  });
  assertLocationLimitError(limitActivate);
} finally {
  for (const [index, locationId] of [...createdFillLocationIds].reverse().entries()) {
    try {
      await cleanupLocation(locationId, cleanup, `fill-${index + 1}`, uniqueSuffix);
    } catch (error) {
      console.error(
        JSON.stringify({
          ok: false,
          cleanupFailed: true,
          locationId,
          error: error instanceof Error ? error.message : String(error),
        }),
      );
    }
  }
  await cleanupLocation(limitLocationId, cleanup, 'limit', uniqueSuffix);
  await cleanupLocation(controlLocationId, cleanup, 'control', uniqueSuffix);
  finalCatalog = await runCase('finalCatalog', locationLimitStatusQuery, { first: 250 });
}

if (
  !controlLocationId ||
  !controlHydrate ||
  !controlLimitStatus ||
  !controlActivate ||
  !limitLocationId ||
  !limitHydrate ||
  !limitStatus ||
  !limitActivate ||
  !finalCatalog
) {
  throw new Error('Capture did not complete every required locationActivate limit/control case.');
}

const capture = {
  scenarioId,
  capturedAt,
  storeDomain,
  apiVersion,
  notes: [
    'Live Admin GraphQL 2026-04 capture for locationActivate control success and LOCATION_LIMIT rejection.',
    'The recorder creates a disposable inactive control location, captures a successful activation, then creates a second inactive target, fills the active merchant-managed location cap with disposable locations, captures the at-cap locationActivate rejection, and cleans up every disposable location.',
    'HAS_ONGOING_RELOCATION is intentionally not present in this parity fixture: a focused live attempt with stocked source-location deactivation plus destination relocation completed synchronously and immediate reactivation succeeded, so that internal/transient branch remains runtime-test-only evidence.',
  ],
  setup: {
    createdFillLocationIds,
    locationLimit: readLocationLimit(limitStatus),
    activeMerchantManagedLocationCountAtCap: activeMerchantManagedLocationCount(readLocationNodes(limitStatus)),
    finalActiveMerchantManagedLocationCount: activeMerchantManagedLocationCount(readLocationNodes(finalCatalog)),
  },
  proxyVariables: {
    limit: {
      locationId: limitLocationId,
      idempotencyKey: `${scenarioId}-limit-${uniqueSuffix}`,
    },
    control: {
      locationId: controlLocationId,
      idempotencyKey: `${scenarioId}-control-${uniqueSuffix}`,
    },
  },
  expected: {
    limit: limitActivate.response.payload,
    control: controlActivate.response.payload,
  },
  cleanup,
  upstreamCalls: [
    upstreamCall('StorePropertiesLocationHydrate', locationHydrateQuery, { id: controlLocationId }, controlHydrate),
    upstreamCall('StorePropertiesLocationLimitStatus', locationLimitStatusQuery, { first: 250 }, controlLimitStatus),
    upstreamCall('StorePropertiesLocationHydrate', locationHydrateQuery, { id: limitLocationId }, limitHydrate),
    upstreamCall('StorePropertiesLocationLimitStatus', locationLimitStatusQuery, { first: 250 }, limitStatus),
  ],
};

await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      locationLimit: capture.setup.locationLimit,
      activeAtCap: capture.setup.activeMerchantManagedLocationCountAtCap,
      createdFillCount: createdFillLocationIds.length,
      finalActiveCount: capture.setup.finalActiveMerchantManagedLocationCount,
    },
    null,
    2,
  ),
);
