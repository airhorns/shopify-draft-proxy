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
};

const scenarioId = 'location-catalog-overlay-lifecycle';
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

const locationLimitStatusQuery =
  'query StorePropertiesLocationLimitStatus($first: Int!) { shop { resourceLimits { locationLimit } } locations(first: $first, includeInactive: true, includeLegacy: true) { nodes { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } } pageInfo { hasNextPage } } }';

const deliveryProfileLocationsHydrateQuery =
  'query ShippingDeliveryProfileLocationsHydrate {\n    locationsAvailableForDeliveryProfilesConnection(first: 250) {\n      nodes {\n        id\n        name\n        isActive\n        isFulfillmentService\n      }\n    }\n  }';

const locationAddMutation = `#graphql
  mutation LocationCatalogOverlayAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        isActive
        isFulfillmentService
        fulfillsOnlineOrders
        address {
          address1
          city
          countryCode
          zip
        }
      }
      userErrors {
        field
        code
        message
      }
    }
  }
`;

const locationEditMutation = `#graphql
  mutation LocationCatalogOverlayEdit($id: ID!, $input: LocationEditInput!) {
    locationEdit(id: $id, input: $input) {
      location {
        id
        name
        isActive
        isFulfillmentService
        fulfillsOnlineOrders
        address {
          address1
          city
          countryCode
          zip
        }
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
  mutation LocationCatalogOverlayDeactivate($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        id
        name
        isActive
        isFulfillmentService
        fulfillsOnlineOrders
      }
      locationDeactivateUserErrors {
        field
        code
        message
      }
    }
  }
`;

const locationActivateMutation = `#graphql
  mutation LocationCatalogOverlayActivate($locationId: ID!, $idempotencyKey: String!) {
    locationActivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        id
        name
        isActive
        isFulfillmentService
        fulfillsOnlineOrders
      }
      locationActivateUserErrors {
        field
        code
        message
      }
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation LocationCatalogOverlayDelete($locationId: ID!) {
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

const lifecycleReadQuery = `#graphql
  fragment LocationCatalogOverlayFields on Location {
    id
    name
    isActive
    isFulfillmentService
    fulfillsOnlineOrders
    address {
      address1
      city
      countryCode
      zip
    }
  }

  query LocationCatalogOverlayRead($query: String!, $baselineId: ID!, $targetId: ID!, $first: Int!) {
    baseline: location(id: $baselineId) {
      ...LocationCatalogOverlayFields
    }
    target: location(id: $targetId) {
      ...LocationCatalogOverlayFields
    }
    byIdentifier: locationByIdentifier(identifier: { id: $baselineId }) {
      ...LocationCatalogOverlayFields
    }
    defaultActive: locations(first: $first, query: $query, sortKey: NAME) {
      edges {
        cursor
        node {
          ...LocationCatalogOverlayFields
        }
      }
      nodes {
        ...LocationCatalogOverlayFields
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    includeInactive: locations(first: $first, query: $query, sortKey: NAME, includeInactive: true) {
      edges {
        cursor
        node {
          ...LocationCatalogOverlayFields
        }
      }
      nodes {
        ...LocationCatalogOverlayFields
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    reverseInactive: locations(first: $first, query: $query, sortKey: NAME, includeInactive: true, reverse: true) {
      nodes {
        ...LocationCatalogOverlayFields
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    matchingCount: locationsCount(query: $query) {
      count
      precision
    }
    allCount: locationsCount {
      count
      precision
    }
    availableForDeliveryProfiles: locationsAvailableForDeliveryProfilesConnection(first: $first) {
      nodes {
        id
        name
        isActive
        isFulfillmentService
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const afterWindowQuery = `#graphql
  fragment LocationCatalogOverlayWindowFields on Location {
    id
    name
    isActive
    isFulfillmentService
    fulfillsOnlineOrders
    address {
      address1
      city
      countryCode
      zip
    }
  }

  query LocationCatalogOverlayAfterWindow($query: String!, $after: String!) {
    afterWindow: locations(first: 1, query: $query, sortKey: NAME, includeInactive: true, after: $after) {
      edges {
        cursor
        node {
          ...LocationCatalogOverlayWindowFields
        }
      }
      nodes {
        ...LocationCatalogOverlayWindowFields
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

const beforeWindowQuery = `#graphql
  fragment LocationCatalogOverlayWindowFields on Location {
    id
    name
    isActive
    isFulfillmentService
    fulfillsOnlineOrders
    address {
      address1
      city
      countryCode
      zip
    }
  }

  query LocationCatalogOverlayBeforeWindow($query: String!, $before: String!) {
    beforeWindow: locations(last: 1, query: $query, sortKey: NAME, includeInactive: true, before: $before) {
      edges {
        cursor
        node {
          ...LocationCatalogOverlayWindowFields
        }
      }
      nodes {
        ...LocationCatalogOverlayWindowFields
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

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function runCase(name: string, query: string, variables: JsonRecord = {}): Promise<CaptureCase> {
  const response = await client.runGraphqlRequest(query, variables);
  return {
    name,
    query: trimGraphql(query),
    variables,
    response,
  };
}

function asRecord(value: unknown): JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : {};
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readData(capture: CaptureCase): JsonRecord {
  const data = asRecord(capture.response.payload.data);
  if (Object.keys(data).length === 0) {
    throw new Error(`${capture.name} returned no data: ${JSON.stringify(capture.response.payload)}`);
  }
  return data;
}

function mutationPayload(capture: CaptureCase, key: string): JsonRecord {
  const payload = asRecord(readData(capture)[key]);
  if (Object.keys(payload).length === 0) {
    throw new Error(`${capture.name} returned no ${key}: ${JSON.stringify(capture.response.payload)}`);
  }
  return payload;
}

function userErrors(capture: CaptureCase, key: string): unknown[] {
  const payload = mutationPayload(capture, key);
  const errors =
    payload['userErrors'] ??
    payload['locationDeactivateUserErrors'] ??
    payload['locationActivateUserErrors'] ??
    payload['locationDeleteUserErrors'];
  return asArray(errors);
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

function readLocationIdFromPayload(capture: CaptureCase, key: 'locationAdd' | 'locationEdit'): string {
  const location = asRecord(mutationPayload(capture, key)['location']);
  const id = location['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${capture.name} did not return a location id: ${JSON.stringify(capture.response.payload)}`);
  }
  assertNoUserErrors(capture, key);
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
  return asArray(locations['nodes']).map((node) => asRecord(node) as LocationNode);
}

function activeMerchantManagedLocationCount(nodes: LocationNode[]): number {
  return nodes.filter((node) => node.isActive === true && node.isFulfillmentService !== true).length;
}

function assertUserErrorCode(capture: CaptureCase, key: 'locationAdd', code: string): void {
  const found = userErrors(capture, key).some((error) => asRecord(error)['code'] === code);
  if (!found) {
    throw new Error(`${capture.name} did not return ${code}: ${JSON.stringify(capture.response.payload)}`);
  }
}

function assertLocationLimitError(capture: CaptureCase, locationLimit: number): void {
  const expectedMessage = `You have reached the maximum number of locations (${locationLimit})`;
  const found = userErrors(capture, 'locationAdd').some((error) => {
    const record = asRecord(error);
    return (
      record['code'] === 'INVALID' &&
      JSON.stringify(record['field']) === JSON.stringify(['input']) &&
      record['message'] === expectedMessage
    );
  });
  if (!found) {
    throw new Error(
      `${capture.name} did not return location-limit INVALID: ${JSON.stringify(capture.response.payload)}`,
    );
  }
}

function locationAddInput(name: string, address1: string): JsonRecord {
  return {
    input: {
      name,
      fulfillsOnlineOrders: false,
      address: {
        countryCode: 'US',
        address1,
        city: 'New York',
        zip: '10001',
      },
    },
  };
}

function exactNameOrQuery(names: string[]): string {
  return names.map((name) => `name:'${name.replace(/'/gu, "\\'")}'`).join(' OR ');
}

function connectionNodes(data: JsonRecord, key: string): JsonRecord[] {
  const connection = asRecord(data[key]);
  return asArray(connection['nodes']).map((node) => asRecord(node));
}

function nodeNames(data: JsonRecord, key: string): string[] {
  return connectionNodes(data, key).map((node) => String(node['name']));
}

function assertNames(label: string, actual: string[], expected: string[]): void {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${label} names mismatch: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

function assertCount(data: JsonRecord, key: string, expected: number): void {
  const count = asRecord(data[key]);
  if (count['count'] !== expected || count['precision'] !== 'EXACT') {
    throw new Error(`${key} mismatch: ${JSON.stringify(count)}, expected count=${expected} precision=EXACT`);
  }
}

function edgeCursor(capture: CaptureCase, key: string, index: number): string {
  const data = readData(capture);
  const connection = asRecord(data[key]);
  const edges = asArray(connection['edges']);
  const edge = asRecord(edges[index]);
  const cursor = edge['cursor'];
  if (typeof cursor !== 'string' || cursor.length === 0) {
    throw new Error(`${capture.name}.${key}.edges[${index}].cursor missing: ${JSON.stringify(connection)}`);
  }
  return cursor;
}

function assertLifecycleRead(
  capture: CaptureCase,
  options: {
    activeNames: string[];
    inactiveIncludedNames: string[];
    targetShouldBeNull: boolean;
  },
): void {
  assertNoTopLevelErrors(capture);
  const data = readData(capture);
  if (asRecord(data['baseline'])['name'] !== options.inactiveIncludedNames[0]) {
    throw new Error(`${capture.name} baseline lookup mismatch: ${JSON.stringify(data['baseline'])}`);
  }
  if (asRecord(data['byIdentifier'])['name'] !== options.inactiveIncludedNames[0]) {
    throw new Error(`${capture.name} locationByIdentifier mismatch: ${JSON.stringify(data['byIdentifier'])}`);
  }
  if (options.targetShouldBeNull) {
    if (data['target'] !== null)
      throw new Error(`${capture.name} expected null target: ${JSON.stringify(data['target'])}`);
  } else if (asRecord(data['target'])['name'] !== options.inactiveIncludedNames.at(-1)) {
    throw new Error(`${capture.name} target lookup mismatch: ${JSON.stringify(data['target'])}`);
  }
  assertNames(`${capture.name}.defaultActive`, nodeNames(data, 'defaultActive'), options.activeNames);
  assertNames(`${capture.name}.includeInactive`, nodeNames(data, 'includeInactive'), options.inactiveIncludedNames);
  assertNames(
    `${capture.name}.reverseInactive`,
    nodeNames(data, 'reverseInactive'),
    [...options.inactiveIncludedNames].reverse(),
  );
  assertCount(data, 'matchingCount', options.inactiveIncludedNames.length);
}

async function captureRead(
  name: string,
  locationNames: string[],
  baselineId: string,
  targetId: string,
  expected: {
    activeNames: string[];
    inactiveIncludedNames: string[];
    targetShouldBeNull: boolean;
  },
): Promise<CaptureCase> {
  const queryCandidates = [
    exactNameOrQuery(locationNames),
    locationNames.map((locationName) => `name:${locationName}`).join(' OR '),
  ];
  const failures: string[] = [];
  for (const query of queryCandidates) {
    const capture = await runCase(name, lifecycleReadQuery, {
      query,
      baselineId,
      targetId,
      first: 250,
    });
    try {
      assertLifecycleRead(capture, expected);
      return capture;
    } catch (error) {
      failures.push(`${JSON.stringify(query)}: ${(error as Error).message}`);
    }
  }
  throw new Error(`${name} could not find a matching location search query: ${failures.join('; ')}`);
}

async function cleanupLocation(
  cleanup: Record<string, CaptureCase>,
  name: string,
  locationId: string | undefined,
  uniqueSuffix: string,
): Promise<void> {
  if (!locationId) return;
  const locationToken = locationId.split('/').at(-1) ?? name;
  cleanup[`${name}Deactivate`] = await runCase(`${name}Deactivate`, locationDeactivateMutation, {
    locationId,
    idempotencyKey: `${scenarioId}-cleanup-${name}-${uniqueSuffix}-${locationToken}`,
  });
  cleanup[`${name}Delete`] = await runCase(`${name}Delete`, locationDeleteMutation, { locationId });
}

function upstreamCall(capture: CaptureCase): JsonRecord {
  return {
    operationName: 'StorePropertiesLocationLimitStatus',
    variables: { first: 250 },
    query: locationLimitStatusQuery,
    response: {
      status: capture.response.status,
      body: capture.response.payload,
    },
  };
}

function deliveryProfileLocationsUpstreamCall(capture: CaptureCase): JsonRecord {
  return {
    operationName: 'ShippingDeliveryProfileLocationsHydrate',
    variables: {},
    query: deliveryProfileLocationsHydrateQuery,
    response: {
      status: capture.response.status,
      body: capture.response.payload,
    },
  };
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const uniqueSuffix = capturedAt.replace(/\D/gu, '').slice(0, 14);
const token = `Overlay${uniqueSuffix}`;
const names = {
  baselineA: `Overlay A ${uniqueSuffix}`,
  baselineB: `Overlay B ${uniqueSuffix}`,
  firstAdd: `Overlay C ${uniqueSuffix}`,
  edited: `Overlay D ${uniqueSuffix}`,
  secondAdd: `Overlay E ${uniqueSuffix}`,
};
const workflow: Record<string, CaptureCase> = {};
const cleanup: Record<string, CaptureCase> = {};
const fillerLocationIds: string[] = [];
let baselineAId: string | undefined;
let baselineBId: string | undefined;
let targetId: string | undefined;
let targetDeleted = false;
let baselineCatalog: CaptureCase | undefined;
let finalCatalog: CaptureCase | undefined;

try {
  workflow.baselineAAdd = await runCase(
    'baselineAAdd',
    locationAddMutation,
    locationAddInput(names.baselineA, '1 Overlay Baseline A'),
  );
  baselineAId = readLocationIdFromPayload(workflow.baselineAAdd, 'locationAdd');

  workflow.baselineBAdd = await runCase(
    'baselineBAdd',
    locationAddMutation,
    locationAddInput(names.baselineB, '2 Overlay Baseline B'),
  );
  baselineBId = readLocationIdFromPayload(workflow.baselineBAdd, 'locationAdd');

  workflow.preFillCatalog = await runCase('preFillCatalog', locationLimitStatusQuery, { first: 250 });
  const locationLimit = readLocationLimit(workflow.preFillCatalog);
  const activeBeforeFill = activeMerchantManagedLocationCount(readLocationNodes(workflow.preFillCatalog));
  const requiredFillers = locationLimit - activeBeforeFill - 1;
  if (requiredFillers < 0) {
    throw new Error(
      `Active merchant-managed location count ${activeBeforeFill} is too close to limit ${locationLimit} for the near-limit capture.`,
    );
  }

  for (let index = 0; index < requiredFillers; index += 1) {
    const create = await runCase(
      `fillerLocationAdd-${String(index + 1).padStart(3, '0')}`,
      locationAddMutation,
      locationAddInput(`Overlay Fill ${uniqueSuffix} ${index + 1}`, `${index + 10} Overlay Fill`),
    );
    fillerLocationIds.push(readLocationIdFromPayload(create, 'locationAdd'));
    workflow[create.name] = create;
    if ((index + 1) % 25 === 0 || index + 1 === requiredFillers) {
      console.log(JSON.stringify({ progress: 'created-fillers', count: index + 1, requiredFillers }));
    }
  }

  baselineCatalog = await runCase('baselineCatalog', locationLimitStatusQuery, { first: 250 });
  const activeAtBaseline = activeMerchantManagedLocationCount(readLocationNodes(baselineCatalog));
  if (activeAtBaseline !== locationLimit - 1) {
    throw new Error(
      `Expected active baseline count ${locationLimit - 1}, got ${activeAtBaseline}: ${JSON.stringify(
        baselineCatalog.response.payload,
      )}`,
    );
  }

  workflow.duplicateAdd = await runCase(
    'duplicateAdd',
    locationAddMutation,
    locationAddInput(names.baselineA, '3 Overlay Duplicate'),
  );
  assertUserErrorCode(workflow.duplicateAdd, 'locationAdd', 'TAKEN');

  workflow.firstAdd = await runCase(
    'firstAdd',
    locationAddMutation,
    locationAddInput(names.firstAdd, '4 Overlay First'),
  );
  targetId = readLocationIdFromPayload(workflow.firstAdd, 'locationAdd');

  workflow.deliveryProfileLocationsBaseline = await runCase(
    'deliveryProfileLocationsBaseline',
    deliveryProfileLocationsHydrateQuery,
  );
  assertNoTopLevelErrors(workflow.deliveryProfileLocationsBaseline);

  workflow.readAfterAdd = await captureRead(
    'readAfterAdd',
    [names.baselineA, names.baselineB, names.firstAdd],
    baselineAId,
    targetId,
    {
      activeNames: [names.baselineA, names.baselineB, names.firstAdd],
      inactiveIncludedNames: [names.baselineA, names.baselineB, names.firstAdd],
      targetShouldBeNull: false,
    },
  );
  workflow.afterWindowAfterAdd = await runCase('afterWindowAfterAdd', afterWindowQuery, {
    query: workflow.readAfterAdd.variables.query,
    after: edgeCursor(workflow.readAfterAdd, 'includeInactive', 0),
  });
  workflow.beforeWindowAfterAdd = await runCase('beforeWindowAfterAdd', beforeWindowQuery, {
    query: workflow.readAfterAdd.variables.query,
    before: edgeCursor(workflow.readAfterAdd, 'includeInactive', 2),
  });

  workflow.secondAdd = await runCase(
    'secondAdd',
    locationAddMutation,
    locationAddInput(names.secondAdd, '5 Overlay Second'),
  );
  assertLocationLimitError(workflow.secondAdd, locationLimit);

  workflow.edit = await runCase('edit', locationEditMutation, {
    id: targetId,
    input: {
      name: names.edited,
      address: {
        address1: '6 Overlay Edited',
        city: 'New York',
        countryCode: 'US',
        zip: '10002',
      },
    },
  });
  readLocationIdFromPayload(workflow.edit, 'locationEdit');

  workflow.readAfterEdit = await captureRead(
    'readAfterEdit',
    [names.baselineA, names.baselineB, names.edited],
    baselineAId,
    targetId,
    {
      activeNames: [names.baselineA, names.baselineB, names.edited],
      inactiveIncludedNames: [names.baselineA, names.baselineB, names.edited],
      targetShouldBeNull: false,
    },
  );

  workflow.deactivate = await runCase('deactivate', locationDeactivateMutation, {
    locationId: targetId,
    idempotencyKey: `${scenarioId}-deactivate-${token}`,
  });
  assertNoUserErrors(workflow.deactivate, 'locationDeactivate');

  workflow.readAfterDeactivate = await captureRead(
    'readAfterDeactivate',
    [names.baselineA, names.baselineB, names.edited],
    baselineAId,
    targetId,
    {
      activeNames: [names.baselineA, names.baselineB],
      inactiveIncludedNames: [names.baselineA, names.baselineB, names.edited],
      targetShouldBeNull: false,
    },
  );

  workflow.activate = await runCase('activate', locationActivateMutation, {
    locationId: targetId,
    idempotencyKey: `${scenarioId}-activate-${token}`,
  });
  assertNoUserErrors(workflow.activate, 'locationActivate');

  workflow.readAfterReactivate = await captureRead(
    'readAfterReactivate',
    [names.baselineA, names.baselineB, names.edited],
    baselineAId,
    targetId,
    {
      activeNames: [names.baselineA, names.baselineB, names.edited],
      inactiveIncludedNames: [names.baselineA, names.baselineB, names.edited],
      targetShouldBeNull: false,
    },
  );

  workflow.deactivateBeforeDelete = await runCase('deactivateBeforeDelete', locationDeactivateMutation, {
    locationId: targetId,
    idempotencyKey: `${scenarioId}-deactivate-delete-${token}`,
  });
  assertNoUserErrors(workflow.deactivateBeforeDelete, 'locationDeactivate');

  workflow.delete = await runCase('delete', locationDeleteMutation, { locationId: targetId });
  assertNoUserErrors(workflow.delete, 'locationDelete');
  targetDeleted = true;

  workflow.readAfterDelete = await captureRead(
    'readAfterDelete',
    [names.baselineA, names.baselineB],
    baselineAId,
    targetId,
    {
      activeNames: [names.baselineA, names.baselineB],
      inactiveIncludedNames: [names.baselineA, names.baselineB],
      targetShouldBeNull: true,
    },
  );
} finally {
  if (!targetDeleted) {
    try {
      await cleanupLocation(cleanup, 'target', targetId, uniqueSuffix);
    } catch (error) {
      console.error(JSON.stringify({ cleanupFailed: 'target', error: (error as Error).message }));
    }
  }
  for (const [index, locationId] of [...fillerLocationIds].reverse().entries()) {
    try {
      await cleanupLocation(cleanup, `filler-${index + 1}`, locationId, uniqueSuffix);
    } catch (error) {
      console.error(JSON.stringify({ cleanupFailed: `filler-${index + 1}`, error: (error as Error).message }));
    }
  }
  try {
    await cleanupLocation(cleanup, 'baselineB', baselineBId, uniqueSuffix);
  } catch (error) {
    console.error(JSON.stringify({ cleanupFailed: 'baselineB', error: (error as Error).message }));
  }
  try {
    await cleanupLocation(cleanup, 'baselineA', baselineAId, uniqueSuffix);
  } catch (error) {
    console.error(JSON.stringify({ cleanupFailed: 'baselineA', error: (error as Error).message }));
  }
  finalCatalog = await runCase('finalCatalog', locationLimitStatusQuery, { first: 250 });
}

if (!baselineAId || !baselineBId || !targetId || !baselineCatalog || !finalCatalog) {
  throw new Error('Capture did not complete required baseline, target, and final catalog cases.');
}

const capture = {
  scenarioId,
  capturedAt,
  storeDomain,
  apiVersion,
  setup: {
    names,
    baselineLocationIds: [baselineAId, baselineBId],
    targetId,
    fillerLocationIds,
    locationLimit: readLocationLimit(baselineCatalog),
    activeMerchantManagedLocationCountAtBaseline: activeMerchantManagedLocationCount(
      readLocationNodes(baselineCatalog),
    ),
    finalActiveMerchantManagedLocationCount: activeMerchantManagedLocationCount(readLocationNodes(finalCatalog)),
  },
  workflow,
  cleanup,
  finalCatalog,
  upstreamCalls: [
    upstreamCall(baselineCatalog),
    upstreamCall(baselineCatalog),
    deliveryProfileLocationsUpstreamCall(workflow.deliveryProfileLocationsBaseline),
    upstreamCall(baselineCatalog),
  ],
  notes: [
    'Live Admin GraphQL 2026-04 capture for preserving an upstream location catalog while staged local lifecycle mutations overlay add/edit/deactivate/reactivate/delete behavior.',
    'The recorder creates two named baseline locations, fills active merchant-managed locations to one below shop.resourceLimits.locationLimit, then captures duplicate-name, sequential add near the limit, over-limit rejection, filtered connection/count reads, cursor windows, and lifecycle readbacks.',
    'The upstreamCalls cassette repeats the same real baseline StorePropertiesLocationLimitStatus capture for the three local locationAdd preflights; the proxy must derive base catalog and limit state from that Shopify response without sending supported mutations upstream.',
    'A separate ShippingDeliveryProfileLocationsHydrate capture retains the delivery-profile eligibility catalog; staged location overlays update only rows already present in that catalog.',
  ],
};

await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      locationLimit: capture.setup.locationLimit,
      activeAtBaseline: capture.setup.activeMerchantManagedLocationCountAtBaseline,
      fillerCount: fillerLocationIds.length,
      finalActiveCount: capture.setup.finalActiveMerchantManagedLocationCount,
    },
    null,
    2,
  ),
);
