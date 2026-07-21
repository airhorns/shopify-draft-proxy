/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
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

const scenarioId = 'location-deactivate-cold-targets';
const apiVersion = '2026-04';
const { storeDomain, adminOrigin } = readConformanceScriptConfig({
  defaultApiVersion: apiVersion,
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminHeaders = buildAdminAuthHeaders(adminAccessToken);
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'store-properties');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

function createClient(): AdminGraphqlClient {
  return createAdminGraphqlClient({ adminOrigin, apiVersion, headers: adminHeaders });
}

const client = createClient();
const locationHydrateQuery =
  'query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }';

const locationFields = `#graphql
  fragment LocationDeactivateColdTargetFields on Location {
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

const locationAddMutation = `#graphql
  mutation LocationDeactivateColdTargetAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location { id name isActive }
      userErrors { field message code }
    }
  }
`;

const locationDeactivateMutation = `#graphql
  ${locationFields}

  mutation LocationDeactivateColdTargets(
    $locationId: ID!
    $destinationLocationId: ID!
    $idempotencyKey: String!
  ) {
    locationDeactivate(locationId: $locationId, destinationLocationId: $destinationLocationId)
      @idempotent(key: $idempotencyKey) {
      location { ...LocationDeactivateColdTargetFields }
      locationDeactivateUserErrors { field message code }
    }
  }
`;

const locationDeactivateCleanupMutation = `#graphql
  mutation LocationDeactivateColdTargetCleanup($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location { id isActive }
      locationDeactivateUserErrors { field message code }
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation LocationDeactivateColdTargetDelete($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors { field message code }
    }
  }
`;

const locationReadQuery = `#graphql
  ${locationFields}

  query LocationDeactivateColdTargetRead($locationId: ID!) {
    location(id: $locationId) { ...LocationDeactivateColdTargetFields }
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

function record(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function mutationPayload(capture: CaptureCase, root: string): JsonRecord {
  const payload = record(record(capture.response.payload.data)?.[root]);
  if (!payload) throw new Error(`${capture.name} returned no ${root}: ${JSON.stringify(capture.response.payload)}`);
  return payload;
}

function userErrors(capture: CaptureCase, root: string): JsonRecord[] {
  const payload = mutationPayload(capture, root);
  const errors =
    payload['userErrors'] ?? payload['locationDeactivateUserErrors'] ?? payload['locationDeleteUserErrors'];
  return Array.isArray(errors) ? (errors.map(record).filter(Boolean) as JsonRecord[]) : [];
}

function assertNoErrors(capture: CaptureCase, root?: string): void {
  if (capture.response.status !== 200 || capture.response.payload.errors) {
    throw new Error(`${capture.name} failed: ${JSON.stringify(capture.response.payload)}`);
  }
  if (root && userErrors(capture, root).length > 0) {
    throw new Error(`${capture.name} returned userErrors: ${JSON.stringify(userErrors(capture, root))}`);
  }
}

function assertCode(capture: CaptureCase, code: string): void {
  assertNoErrors(capture);
  if (!userErrors(capture, 'locationDeactivate').some((error) => error['code'] === code)) {
    throw new Error(`${capture.name} did not return ${code}: ${JSON.stringify(capture.response.payload)}`);
  }
}

function locationId(capture: CaptureCase): string {
  const id = record(mutationPayload(capture, 'locationAdd')['location'])?.['id'];
  if (typeof id !== 'string') throw new Error(`${capture.name} did not return a location id`);
  return id;
}

async function createLocation(name: string): Promise<{ capture: CaptureCase; id: string }> {
  const capture = await runCase(`${name}Add`, locationAddMutation, {
    input: {
      name,
      fulfillsOnlineOrders: false,
      address: {
        address1: '873 State St',
        city: 'Boston',
        provinceCode: 'MA',
        countryCode: 'US',
        zip: '02110',
      },
    },
  });
  assertNoErrors(capture, 'locationAdd');
  return { capture, id: locationId(capture) };
}

async function hydrateLocation(name: string, id: string): Promise<CaptureCase> {
  const capture = await runCase(name, locationHydrateQuery, { id });
  assertNoErrors(capture);
  return capture;
}

function upstreamCall(capture: CaptureCase): JsonRecord {
  return {
    operationName: 'StorePropertiesLocationHydrate',
    query: capture.query,
    variables: capture.variables,
    response: { status: capture.response.status, body: capture.response.payload },
  };
}

async function cleanupLocation(id: string | null, name: string, runId: string, cleanup: CaptureCase[]): Promise<void> {
  if (!id) return;
  const hydrate = await hydrateLocation(`${name}CleanupHydrate`, id);
  if (
    record(hydrate.response.payload.data)?.['location'] &&
    record(record(hydrate.response.payload.data)?.['location'])?.['isActive'] === true
  ) {
    cleanup.push(
      await runCase(`${name}CleanupDeactivate`, locationDeactivateCleanupMutation, {
        locationId: id,
        idempotencyKey: `${scenarioId}-${name}-cleanup-${runId}`,
      }),
    );
  }
  cleanup.push(await runCase(`${name}CleanupDelete`, locationDeleteMutation, { locationId: id }));
}

await mkdir(outputDir, { recursive: true });
const capturedAt = new Date().toISOString();
const runId = capturedAt.replace(/\D/gu, '').slice(0, 14);
const setup: JsonRecord = {};
const workflow: JsonRecord = {};
const cleanup: CaptureCase[] = [];
const upstreamCalls: JsonRecord[] = [];
const missingDestinationId = 'gid://shopify/Location/999999999999';
let sourceId: string | null = null;
let activeDestinationId: string | null = null;
let inactiveDestinationId: string | null = null;

try {
  const source = await createLocation(`${scenarioId} source ${runId}`);
  sourceId = source.id;
  setup['sourceAdd'] = source.capture;
  const activeDestination = await createLocation(`${scenarioId} active destination ${runId}`);
  activeDestinationId = activeDestination.id;
  setup['activeDestinationAdd'] = activeDestination.capture;
  const inactiveDestination = await createLocation(`${scenarioId} inactive destination ${runId}`);
  inactiveDestinationId = inactiveDestination.id;
  setup['inactiveDestinationAdd'] = inactiveDestination.capture;
  setup['inactiveDestinationDeactivate'] = await runCase(
    'inactiveDestinationDeactivate',
    locationDeactivateCleanupMutation,
    {
      locationId: inactiveDestination.id,
      idempotencyKey: `${scenarioId}-inactive-setup-${runId}`,
    },
  );
  assertNoErrors(setup['inactiveDestinationDeactivate'] as CaptureCase, 'locationDeactivate');

  for (const [name, id] of [
    ['sourceHydrate', source.id],
    ['activeDestinationHydrate', activeDestination.id],
    ['inactiveDestinationHydrate', inactiveDestination.id],
    ['missingDestinationHydrate', missingDestinationId],
  ] as const) {
    const hydrate = await hydrateLocation(name, id);
    setup[name] = hydrate;
    upstreamCalls.push(upstreamCall(hydrate));
  }

  workflow['inactiveDestination'] = await runCase('inactiveDestination', locationDeactivateMutation, {
    locationId: source.id,
    destinationLocationId: inactiveDestination.id,
    idempotencyKey: `${scenarioId}-inactive-${runId}`,
  });
  assertCode(workflow['inactiveDestination'] as CaptureCase, 'DESTINATION_LOCATION_NOT_FOUND_OR_INACTIVE');

  workflow['missingDestination'] = await runCase('missingDestination', locationDeactivateMutation, {
    locationId: source.id,
    destinationLocationId: missingDestinationId,
    idempotencyKey: `${scenarioId}-missing-${runId}`,
  });
  assertCode(workflow['missingDestination'] as CaptureCase, 'DESTINATION_LOCATION_NOT_SHOPIFY_MANAGED');

  workflow['activeDestination'] = await runCase('activeDestination', locationDeactivateMutation, {
    locationId: source.id,
    destinationLocationId: activeDestination.id,
    idempotencyKey: `${scenarioId}-active-${runId}`,
  });
  assertNoErrors(workflow['activeDestination'] as CaptureCase, 'locationDeactivate');
  workflow['sourceRead'] = await runCase('sourceRead', locationReadQuery, { locationId: source.id });
  workflow['destinationRead'] = await runCase('destinationRead', locationReadQuery, {
    locationId: activeDestination.id,
  });
  assertNoErrors(workflow['sourceRead'] as CaptureCase);
  assertNoErrors(workflow['destinationRead'] as CaptureCase);
} finally {
  await cleanupLocation(sourceId, 'source', runId, cleanup);
  await cleanupLocation(activeDestinationId, 'activeDestination', runId, cleanup);
  await cleanupLocation(inactiveDestinationId, 'inactiveDestination', runId, cleanup);
}

const capture = {
  capturedAt,
  storeDomain,
  apiVersion,
  notes: [
    'Captures mutation-first locationDeactivate behavior for independently cold source and active/inactive/missing destination targets.',
    'Every proxy prerequisite is a real query-only Shopify interaction recorded verbatim in upstreamCalls; no proxy state is seeded.',
  ],
  setup,
  workflow,
  cleanup,
  upstreamCalls,
};

await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ ok: true, outputPath, cases: Object.keys(workflow) }, null, 2));
