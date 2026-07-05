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

const scenarioId = 'location-activate-not-found';
const outputApiVersion = '2026-04';
const missingLocationId = 'gid://shopify/Location/999999999999';
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

const locationHydrateQuery =
  'query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }';

const locationActivateMutation = `#graphql
  mutation LocationActivateNotFound($locationId: ID!, $idempotencyKey: String!) {
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

const locationReadQuery = `#graphql
  query LocationActivateNotFoundRead($id: ID!) {
    location(id: $id) {
      id
      isActive
    }
  }
`;

function trimGraphql(query: string): string {
  return query
    .replace(/^#graphql\s*/u, '')
    .replace(/\s+/gu, ' ')
    .trim();
}

async function runCase(name: string, query: string, variables: JsonRecord): Promise<CaptureCase> {
  const response = await client.runGraphqlRequest(query, variables);
  return {
    name,
    query: trimGraphql(query),
    variables,
    response,
  };
}

function readObject(value: unknown, label: string): JsonRecord {
  if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
    return value as JsonRecord;
  }

  throw new Error(`${label} was not an object`);
}

function readArray(value: unknown, label: string): unknown[] {
  if (Array.isArray(value)) {
    return value;
  }

  throw new Error(`${label} was not an array`);
}

function field(record: JsonRecord, name: string): unknown {
  return record[name];
}

function data(capture: CaptureCase): JsonRecord {
  return readObject(
    field(readObject(capture.response.payload, `${capture.name}.payload`), 'data'),
    `${capture.name}.data`,
  );
}

function mutationPayload(capture: CaptureCase, root: string): JsonRecord {
  return readObject(field(data(capture), root), `${capture.name}.${root}`);
}

function assertLocationReadNull(capture: CaptureCase): void {
  const location = field(data(capture), 'location');
  if (location !== null) {
    throw new Error(`${capture.name} expected data.location null, got ${JSON.stringify(location)}`);
  }
}

function assertLocationActivateNotFound(capture: CaptureCase): void {
  const payload = mutationPayload(capture, 'locationActivate');
  const location = field(payload, 'location');
  if (location !== null) {
    throw new Error(`${capture.name} expected locationActivate.location null, got ${JSON.stringify(location)}`);
  }
  const errors = readArray(
    field(payload, 'locationActivateUserErrors'),
    `${capture.name}.locationActivate.locationActivateUserErrors`,
  );
  const hasNotFound = errors.some((error) => {
    const record = readObject(error, `${capture.name}.locationActivateUserErrors.item`);
    return field(record, 'code') === 'LOCATION_NOT_FOUND';
  });
  if (!hasNotFound) {
    throw new Error(`${capture.name} did not include LOCATION_NOT_FOUND: ${JSON.stringify(errors)}`);
  }
}

async function main(): Promise<void> {
  const runId = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const workflow: Record<string, CaptureCase> = {};
  const upstreamCalls: JsonRecord[] = [];

  workflow.locationHydrateMiss = await runCase('locationHydrateMiss', locationHydrateQuery, {
    id: missingLocationId,
  });
  assertLocationReadNull(workflow.locationHydrateMiss);
  upstreamCalls.push({
    operationName: 'StorePropertiesLocationHydrate',
    variables: { id: missingLocationId },
    query: trimGraphql(locationHydrateQuery),
    response: {
      status: workflow.locationHydrateMiss.response.status,
      body: workflow.locationHydrateMiss.response.payload,
    },
  });

  workflow.activateUnknownLocation = await runCase('activateUnknownLocation', locationActivateMutation, {
    locationId: missingLocationId,
    idempotencyKey: `location-activate-not-found-${runId}`,
  });
  assertLocationActivateNotFound(workflow.activateUnknownLocation);

  workflow.readAfterUnknownActivate = await runCase('readAfterUnknownActivate', locationReadQuery, {
    id: missingLocationId,
  });
  assertLocationReadNull(workflow.readAfterUnknownActivate);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId,
        storeDomain,
        apiVersion: outputApiVersion,
        recordedAt: new Date().toISOString(),
        notes: [
          'Captures locationActivate against an absent Location GID returning LOCATION_NOT_FOUND with a null location payload.',
          'The upstreamCalls cassette records the exact StorePropertiesLocationHydrate miss used by the local runtime before returning the not-found userError.',
        ],
        workflow,
        upstreamCalls,
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  console.log(JSON.stringify({ ok: true, outputPath, upstreamCalls: upstreamCalls.length }, null, 2));
}

await main();
