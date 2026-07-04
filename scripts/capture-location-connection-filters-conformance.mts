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

const scenarioId = 'location-connection-filters';
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

const locationAddMutation = `#graphql
  mutation LocationConnectionLocationAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        isActive
        fulfillsOnlineOrders
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentServiceCreateMutation = `#graphql
  mutation LocationConnectionFulfillmentServiceCreate($name: String!) {
    fulfillmentServiceCreate(
      name: $name
      trackingSupport: true
      inventoryManagement: true
      requiresShippingMethod: true
    ) {
      fulfillmentService {
        id
        serviceName
        location {
          id
          name
          isActive
          isFulfillmentService
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const locationDeactivateMutation = `#graphql
  mutation LocationConnectionDeactivate($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        id
        name
        isActive
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
  mutation LocationConnectionDelete($locationId: ID!) {
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

const fulfillmentServiceDeleteMutation = `#graphql
  mutation LocationConnectionFulfillmentServiceDelete($id: ID!) {
    fulfillmentServiceDelete(id: $id, inventoryAction: DELETE) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const locationConnectionReadQuery = `#graphql
  query LocationConnectionFilteredRead(
    $query: String!
    $alphaQuery: String!
  ) {
    filteredDefault: locations(first: 10, query: $query, sortKey: NAME) {
      edges {
        cursor
      }
      nodes {
        id
        name
        isActive
        isFulfillmentService
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    filteredIncludeInactive: locations(first: 10, query: $query, sortKey: NAME, includeInactive: true) {
      nodes {
        id
        name
        isActive
        isFulfillmentService
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    filteredIncludeLegacy: locations(first: 10, query: $query, sortKey: NAME, includeLegacy: true) {
      nodes {
        id
        name
        isActive
        isFulfillmentService
      }
    }
    queryAlpha: locations(first: 10, query: $alphaQuery) {
      nodes {
        id
        name
        isActive
      }
    }
    nameFirst: locations(first: 1, query: $query, sortKey: NAME) {
      edges {
        cursor
      }
      nodes {
        id
        name
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    reversed: locations(first: 10, query: $query, sortKey: NAME, reverse: true) {
      nodes {
        id
        name
      }
    }
    filteredCount: locationsCount(query: $query) {
      count
      precision
    }
    limitedCount: locationsCount(query: $query, limit: 1) {
      count
      precision
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
  if (!data) throw new Error(`${capture.name} returned no data: ${JSON.stringify(capture.response.payload)}`);
  return data;
}

function mutationPayload(capture: CaptureCase, key: string): JsonRecord {
  const payload = readObject(readData(capture)[key]);
  if (!payload) throw new Error(`${capture.name} returned no ${key}: ${JSON.stringify(capture.response.payload)}`);
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
  if (errors.length > 0) throw new Error(`${capture.name} returned userErrors: ${JSON.stringify(errors)}`);
}

function readLocationAddId(capture: CaptureCase): string {
  const location = readObject(mutationPayload(capture, 'locationAdd')['location']);
  const id = location?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`${capture.name} did not return a location id: ${JSON.stringify(capture.response.payload)}`);
  }
  return id;
}

function readFulfillmentServiceIds(capture: CaptureCase): { serviceId: string; locationId: string } {
  const service = readObject(mutationPayload(capture, 'fulfillmentServiceCreate')['fulfillmentService']);
  const location = readObject(service?.['location']);
  const serviceId = service?.['id'];
  const locationId = location?.['id'];
  if (typeof serviceId !== 'string' || typeof locationId !== 'string') {
    throw new Error(
      `${capture.name} did not return fulfillment-service ids: ${JSON.stringify(capture.response.payload)}`,
    );
  }
  return { serviceId, locationId };
}

function connectionNodes(data: JsonRecord, key: string): JsonRecord[] {
  const connection = readObject(data[key]);
  const nodes = connection?.['nodes'];
  if (!Array.isArray(nodes)) throw new Error(`${key} is not a connection: ${JSON.stringify(connection)}`);
  return nodes.map((node) => {
    const object = readObject(node);
    if (!object) throw new Error(`${key} had a non-object node: ${JSON.stringify(node)}`);
    return object;
  });
}

function assertNames(label: string, nodes: JsonRecord[], expected: string[]): void {
  const actual = nodes.map((node) => node['name']);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${label} names mismatch: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

function assertCount(data: JsonRecord, key: string, count: number, precision: string): void {
  const value = readObject(data[key]);
  if (value?.['count'] !== count || value?.['precision'] !== precision) {
    throw new Error(`${key} mismatch: ${JSON.stringify(value)}`);
  }
}

function assertPageInfo(data: JsonRecord, key: string, expected: JsonRecord): void {
  const pageInfo = readObject(readObject(data[key])?.['pageInfo']);
  for (const [field, value] of Object.entries(expected)) {
    if (pageInfo?.[field] !== value) throw new Error(`${key}.pageInfo.${field} mismatch: ${JSON.stringify(pageInfo)}`);
  }
}

function edgeCursor(data: JsonRecord, key: string, index: number): string {
  const connection = readObject(data[key]);
  const edges = connection?.['edges'];
  const edge = Array.isArray(edges) ? readObject(edges[index]) : null;
  const cursor = edge?.['cursor'];
  if (typeof cursor !== 'string' || cursor.length === 0) {
    throw new Error(`${key}.edges[${index}].cursor missing: ${JSON.stringify(connection)}`);
  }
  return cursor;
}

function assertConnectionRead(
  capture: CaptureCase,
  names: { alpha: string; beta: string; carrier: string; zulu: string },
): void {
  assertNoTopLevelErrors(capture);
  const data = readData(capture);
  assertNames('filteredDefault', connectionNodes(data, 'filteredDefault'), [names.alpha, names.zulu]);
  assertNames('filteredIncludeInactive', connectionNodes(data, 'filteredIncludeInactive'), [
    names.alpha,
    names.beta,
    names.zulu,
  ]);
  const inactive = connectionNodes(data, 'filteredIncludeInactive')[1];
  if (inactive?.['isActive'] !== false) throw new Error(`inactive node was not inactive: ${JSON.stringify(inactive)}`);
  assertNames('filteredIncludeLegacy', connectionNodes(data, 'filteredIncludeLegacy'), [
    names.alpha,
    names.carrier,
    names.zulu,
  ]);
  const legacy = connectionNodes(data, 'filteredIncludeLegacy')[1];
  if (legacy?.['isFulfillmentService'] !== true) {
    throw new Error(`legacy node was not a fulfillment-service location: ${JSON.stringify(legacy)}`);
  }
  assertNames('queryAlpha', connectionNodes(data, 'queryAlpha'), [names.alpha]);
  assertNames('nameFirst', connectionNodes(data, 'nameFirst'), [names.alpha]);
  assertPageInfo(data, 'nameFirst', { hasNextPage: true, hasPreviousPage: false });
  assertNames('reversed', connectionNodes(data, 'reversed'), [names.zulu, names.alpha]);
  assertCount(data, 'filteredCount', 4, 'EXACT');
  assertCount(data, 'limitedCount', 1, 'AT_LEAST');
  edgeCursor(data, 'filteredDefault', 0);
  edgeCursor(data, 'filteredDefault', 1);
}

async function captureMatchingConnectionRead(names: {
  alpha: string;
  beta: string;
  carrier: string;
  zulu: string;
}): Promise<CaptureCase> {
  const exactNameOrQuery = [names.alpha, names.beta, names.carrier, names.zulu]
    .map((name) => `name:'${name}'`)
    .join(' OR ');
  const unquotedNameOrQuery = [names.alpha, names.beta, names.carrier, names.zulu]
    .map((name) => `name:${name}`)
    .join(' OR ');
  const queryCandidates = [exactNameOrQuery, unquotedNameOrQuery, token, `${token}*`];
  const alphaQueryCandidates = [`name:'${names.alpha}'`, `name:${names.alpha}`, names.alpha];
  const failures: string[] = [];
  for (const query of queryCandidates) {
    for (const alphaQuery of alphaQueryCandidates) {
      const capture = await runCase('connectionRead', locationConnectionReadQuery, { query, alphaQuery });
      try {
        assertConnectionRead(capture, names);
        return capture;
      } catch (error) {
        failures.push(`${JSON.stringify({ query, alphaQuery })}: ${(error as Error).message}`);
      }
    }
  }
  throw new Error(`No live locations query candidate matched expected disposable locations: ${failures.join('; ')}`);
}

async function cleanupLocation(
  cleanup: Record<string, CaptureCase>,
  name: string,
  locationId: string | undefined,
  deactivateFirst: boolean,
): Promise<void> {
  if (!locationId) return;
  if (deactivateFirst) {
    cleanup[`${name}Deactivate`] = await runCase(`${name}Deactivate`, locationDeactivateMutation, {
      locationId,
      idempotencyKey: `${scenarioId}-${name}-cleanup-${token}`,
    });
  }
  cleanup[`${name}Delete`] = await runCase(`${name}Delete`, locationDeleteMutation, { locationId });
}

const token = `Conn${Date.now()}`;
const names = {
  alpha: `Alpha-${token}`,
  beta: `Beta-${token}`,
  carrier: `Carrier-${token}`,
  zulu: `Zulu-${token}`,
};
const workflow: Record<string, CaptureCase> = {};
const cleanup: Record<string, CaptureCase> = {};
let alphaLocationId: string | undefined;
let betaLocationId: string | undefined;
let zuluLocationId: string | undefined;
let fulfillmentServiceId: string | undefined;

try {
  workflow.alphaLocationAdd = await runCase('alphaLocationAdd', locationAddMutation, {
    input: { name: names.alpha, fulfillsOnlineOrders: false, address: { countryCode: 'US' } },
  });
  assertNoUserErrors(workflow.alphaLocationAdd, 'locationAdd');
  alphaLocationId = readLocationAddId(workflow.alphaLocationAdd);

  workflow.zuluLocationAdd = await runCase('zuluLocationAdd', locationAddMutation, {
    input: { name: names.zulu, fulfillsOnlineOrders: false, address: { countryCode: 'US' } },
  });
  assertNoUserErrors(workflow.zuluLocationAdd, 'locationAdd');
  zuluLocationId = readLocationAddId(workflow.zuluLocationAdd);

  workflow.betaLocationAdd = await runCase('betaLocationAdd', locationAddMutation, {
    input: { name: names.beta, fulfillsOnlineOrders: false, address: { countryCode: 'US' } },
  });
  assertNoUserErrors(workflow.betaLocationAdd, 'locationAdd');
  betaLocationId = readLocationAddId(workflow.betaLocationAdd);

  workflow.fulfillmentServiceCreate = await runCase('fulfillmentServiceCreate', fulfillmentServiceCreateMutation, {
    name: names.carrier,
  });
  assertNoUserErrors(workflow.fulfillmentServiceCreate, 'fulfillmentServiceCreate');
  fulfillmentServiceId = readFulfillmentServiceIds(workflow.fulfillmentServiceCreate).serviceId;

  workflow.betaDeactivate = await runCase('betaDeactivate', locationDeactivateMutation, {
    locationId: betaLocationId,
    idempotencyKey: `${scenarioId}-deactivate-${token}`,
  });
  assertNoUserErrors(workflow.betaDeactivate, 'locationDeactivate');

  workflow.connectionRead = await captureMatchingConnectionRead(names);
} finally {
  if (fulfillmentServiceId) {
    cleanup.fulfillmentServiceDelete = await runCase('fulfillmentServiceDelete', fulfillmentServiceDeleteMutation, {
      id: fulfillmentServiceId,
    });
  }
  await cleanupLocation(cleanup, 'betaLocation', betaLocationId, false);
  await cleanupLocation(cleanup, 'alphaLocation', alphaLocationId, true);
  await cleanupLocation(cleanup, 'zuluLocation', zuluLocationId, true);
}

const output = {
  scenarioId,
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  workflow,
  cleanup,
  notes: [
    'Captures top-level locations connection filtering, default active/non-legacy behavior, includeInactive/includeLegacy, sortKey NAME, reverse, first-page pageInfo booleans/emitted cursors, and locationsCount(query:, limit:) against disposable locations.',
    'The read query is isolated with a unique exact-name OR search so existing shop locations do not affect ordering or counts.',
    'Current live locationsCount(query:) counts every matching location, including inactive and fulfillment-service legacy locations, even though the connection defaults exclude them.',
  ],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`);
console.log(JSON.stringify({ ok: true, outputPath, token }, null, 2));
