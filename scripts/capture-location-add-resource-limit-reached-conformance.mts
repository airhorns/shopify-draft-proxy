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
  name?: unknown;
  isActive?: unknown;
  isFulfillmentService?: unknown;
  isPrimary?: unknown;
};

type LocationAddPayload = {
  locationAdd?: {
    location?: {
      id?: unknown;
    } | null;
    userErrors?: unknown[];
  } | null;
};

const scenarioId = 'location-add-resource-limit-reached';
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

function createClient(apiVersion: string): AdminGraphqlClient {
  return createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: adminHeaders,
  });
}

const client = createClient(apiVersion);

const locationLimitStatusQuery =
  'query StorePropertiesLocationLimitStatus($first: Int!) { shop { resourceLimits { locationLimit } } locations(first: $first, includeInactive: true) { nodes { id isActive isFulfillmentService } pageInfo { hasNextPage } } }';

const catalogQuery = `#graphql
  ${locationLimitStatusQuery}
`;

const locationAddMutation = `#graphql
  mutation LocationAddResourceLimitReached($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
      }
      userErrors {
        field
        code
        message
      }
    }
  }
`;

const locationDeactivateWithDirectiveMutation = `#graphql
  mutation LocationAddResourceLimitCleanupDeactivate($locationId: ID!, $idempotencyKey: String!) {
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
  mutation LocationAddResourceLimitCleanupDelete($locationId: ID!) {
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

function readLocationLimit(catalog: CaptureCase): number {
  const payload = asRecord(catalog.response.payload);
  const data = asRecord(payload['data']);
  const shop = asRecord(data['shop']);
  const resourceLimits = asRecord(shop['resourceLimits']);
  const limit = resourceLimits['locationLimit'];
  if (typeof limit !== 'number' || !Number.isFinite(limit) || limit < 1) {
    throw new Error(`Could not read shop.resourceLimits.locationLimit: ${JSON.stringify(catalog.response.payload)}`);
  }
  return limit;
}

function readLocationNodes(catalog: CaptureCase): LocationNode[] {
  const payload = asRecord(catalog.response.payload);
  const data = asRecord(payload['data']);
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

function readAddedLocationId(createCase: CaptureCase): string {
  const payload = createCase.response.payload as { data?: LocationAddPayload };
  const id = payload.data?.locationAdd?.location?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(
      `locationAdd did not return a disposable location id: ${JSON.stringify(createCase.response.payload)}`,
    );
  }
  const userErrors = payload.data?.locationAdd?.userErrors ?? [];
  if (userErrors.length > 0) {
    throw new Error(`locationAdd returned userErrors before the shop reached cap: ${JSON.stringify(userErrors)}`);
  }
  return id;
}

function hasLocationLimitError(addCase: CaptureCase, locationLimit: number): boolean {
  const payload = addCase.response.payload as { data?: LocationAddPayload };
  const userErrors = payload.data?.locationAdd?.userErrors ?? [];
  const expectedMessage = `You have reached the maximum number of locations (${locationLimit})`;
  return userErrors.some((error) => {
    const record = asRecord(error);
    return record['code'] === 'INVALID' && record['message'] === expectedMessage;
  });
}

function locationAddInput(name: string): JsonRecord {
  return {
    input: {
      name,
      address: {
        countryCode: 'US',
        address1: '1 Resource Limit St',
        city: 'New York',
        zip: '10001',
      },
    },
  };
}

async function cleanupLocation(
  locationId: string,
  cleanup: CaptureCase[],
  suffix: string,
  uniqueSuffix: string,
): Promise<void> {
  const locationToken = locationId.split('/').at(-1) ?? suffix;
  cleanup.push(
    await runCase(`cleanupDeactivate-${suffix}`, locationDeactivateWithDirectiveMutation, {
      locationId,
      idempotencyKey: `${scenarioId}-cleanup-${suffix}-${uniqueSuffix}-${locationToken}`,
    }),
  );
  cleanup.push(await runCase(`cleanupDelete-${suffix}`, locationDeleteMutation, { locationId }));
}

function locationLimitStatusCall(capture: CaptureCase) {
  return {
    operationName: 'StorePropertiesLocationLimitStatus',
    variables: {
      first: 250,
    },
    query: locationLimitStatusQuery,
    response: {
      status: capture.response.status,
      body: capture.response.payload,
    },
  };
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const uniqueSuffix = capturedAt.replace(/\D/gu, '').slice(0, 14);
const cleanup: CaptureCase[] = [];
const created: CaptureCase[] = [];
const createdLocationIds: string[] = [];
let atCapAdd: CaptureCase | null = null;
let initialCatalog: CaptureCase | null = null;
let atCapCatalog: CaptureCase | null = null;
let finalCatalog: CaptureCase | null = null;

try {
  initialCatalog = await runCase('initialCatalog', catalogQuery, { first: 250 });
  const locationLimit = readLocationLimit(initialCatalog);
  const initialActiveCount = activeMerchantManagedLocationCount(readLocationNodes(initialCatalog));
  const requiredCreates = locationLimit - initialActiveCount;
  if (requiredCreates < 0) {
    throw new Error(`Active location count ${initialActiveCount} already exceeds location limit ${locationLimit}.`);
  }

  for (let index = 0; index < requiredCreates; index += 1) {
    const create = await runCase(
      `setupLocationAdd-${String(index + 1).padStart(3, '0')}`,
      locationAddMutation,
      locationAddInput(`Proxy Cap ${uniqueSuffix} ${index + 1}`),
    );
    created.push(create);
    createdLocationIds.push(readAddedLocationId(create));
    if ((index + 1) % 25 === 0) {
      console.log(JSON.stringify({ progress: 'created-locations', count: index + 1, requiredCreates }));
    }
  }

  atCapCatalog = await runCase('atCapCatalog', catalogQuery, { first: 250 });
  atCapAdd = await runCase('atCapAdd', locationAddMutation, locationAddInput(`Proxy Cap Overflow ${uniqueSuffix}`));
  if (!hasLocationLimitError(atCapAdd, locationLimit)) {
    throw new Error(`Expected captured location-limit userError, got: ${JSON.stringify(atCapAdd.response.payload)}`);
  }
} finally {
  for (const [index, locationId] of [...createdLocationIds].reverse().entries()) {
    try {
      await cleanupLocation(locationId, cleanup, `created-${index + 1}`, uniqueSuffix);
    } catch (error) {
      console.error(
        JSON.stringify(
          {
            ok: false,
            cleanupFailed: true,
            locationId,
            error: error instanceof Error ? error.message : String(error),
          },
          null,
          2,
        ),
      );
    }
  }
  finalCatalog = await runCase('finalCatalog', catalogQuery, { first: 250 });
}

if (initialCatalog === null || atCapCatalog === null || atCapAdd === null || finalCatalog === null) {
  throw new Error('Capture did not complete the required catalog and at-cap add cases.');
}

const capture = {
  scenarioId,
  capturedAt,
  storeDomain,
  apiVersion,
  setup: {
    locationLimit: readLocationLimit(initialCatalog),
    activeMerchantManagedLocationCountBefore: activeMerchantManagedLocationCount(readLocationNodes(initialCatalog)),
    activeMerchantManagedLocationCountAtCap: activeMerchantManagedLocationCount(readLocationNodes(atCapCatalog)),
    createdLocationIds,
  },
  initialCatalog,
  created,
  atCapCatalog,
  atCapAdd,
  cleanup,
  finalCatalog,
  expected: {
    emptyLog: {
      entries: [],
    },
  },
  notes: [
    'The atCapAdd response is recorded from live Shopify after this script creates disposable active locations until the shop reaches shop.resourceLimits.locationLimit.',
    'The upstreamCalls cassette records the live StorePropertiesLocationLimitStatus read that the proxy uses to derive cap state before rejecting a local locationAdd.',
  ],
  upstreamCalls: [locationLimitStatusCall(atCapCatalog)],
};

await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      locationLimit: capture.setup.locationLimit,
      activeBefore: capture.setup.activeMerchantManagedLocationCountBefore,
      createdCount: createdLocationIds.length,
      activeAtCap: capture.setup.activeMerchantManagedLocationCountAtCap,
      cleanupCases: cleanup.length,
      finalActiveCount: activeMerchantManagedLocationCount(readLocationNodes(finalCatalog)),
    },
    null,
    2,
  ),
);
