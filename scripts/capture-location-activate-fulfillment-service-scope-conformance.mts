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

const scenarioId = 'location-activate-fulfillment-service-scope';
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

const locationHydrateQuery =
  'query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }';

const fulfillmentServiceCreateMutation = `#graphql
  mutation LocationActivateFulfillmentServiceScopeCreate($name: String!) {
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
          fulfillsOnlineOrders
          shipsInventory
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
  mutation LocationActivateFulfillmentServiceScopeDeactivateAttempt($locationId: ID!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: "location-activate-fulfillment-service-scope-deactivate") {
      location {
        id
        isActive
        isFulfillmentService
      }
      locationDeactivateUserErrors {
        field
        message
        code
      }
    }
  }
`;

const locationActivateMutation = `#graphql
  mutation LocationActivateFulfillmentServiceScope($locationId: ID!) {
    locationActivate(locationId: $locationId) @idempotent(key: "location-activate-fulfillment-service-scope") {
      location {
        id
        isActive
        isFulfillmentService
      }
      locationActivateUserErrors {
        field
        message
        code
      }
    }
  }
`;

const locationReadQuery = `#graphql
  query LocationActivateFulfillmentServiceScopeRead($id: ID!) {
    location(id: $id) {
      id
      isActive
      isFulfillmentService
    }
  }
`;

const fulfillmentServiceDeleteMutation = `#graphql
  mutation LocationActivateFulfillmentServiceScopeCleanup($id: ID!) {
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
  return query
    .replace(/^#graphql\s*/u, '')
    .replace(/\s+/gu, ' ')
    .trim();
}

function digestDocument(query: string): string {
  return createHash('sha256').update(trimGraphql(query)).digest('hex');
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

function assertNoUserErrors(capture: CaptureCase, root: string): void {
  const payload = mutationPayload(capture, root);
  const errors = readArray(field(payload, 'userErrors'), `${capture.name}.${root}.userErrors`);
  if (errors.length > 0) {
    throw new Error(`${capture.name} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertUserErrorCode(capture: CaptureCase, root: string, errorsField: string, expectedCode: string): void {
  const payload = mutationPayload(capture, root);
  const errors = readArray(field(payload, errorsField), `${capture.name}.${root}.${errorsField}`);
  const hasExpectedCode = errors.some((error) => {
    const record = readObject(error, `${capture.name}.${errorsField}.item`);
    return field(record, 'code') === expectedCode;
  });
  if (!hasExpectedCode) {
    throw new Error(`${capture.name} did not include ${expectedCode}: ${JSON.stringify(errors)}`);
  }
}

function readFulfillmentService(capture: CaptureCase): { serviceId: string; locationId: string } {
  const payload = mutationPayload(capture, 'fulfillmentServiceCreate');
  const service = readObject(field(payload, 'fulfillmentService'), `${capture.name}.fulfillmentService`);
  const location = readObject(field(service, 'location'), `${capture.name}.fulfillmentService.location`);
  const serviceId = field(service, 'id');
  const locationId = field(location, 'id');
  if (typeof serviceId !== 'string' || typeof locationId !== 'string') {
    throw new Error(`${capture.name} did not return service/location ids`);
  }
  return { serviceId, locationId };
}

async function main(): Promise<void> {
  const runId = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const setup: Record<string, CaptureCase> = {};
  const workflow: Record<string, CaptureCase> = {};
  const cleanup: Record<string, CaptureCase> = {};
  const upstreamCalls: JsonRecord[] = [];
  let fulfillmentServiceId: string | null = null;

  try {
    setup.fulfillmentServiceCreate = await runCase('fulfillmentServiceCreate', fulfillmentServiceCreateMutation, {
      name: `Location Activate FS Scope ${runId}`,
    });
    assertNoUserErrors(setup.fulfillmentServiceCreate, 'fulfillmentServiceCreate');
    const fulfillmentService = readFulfillmentService(setup.fulfillmentServiceCreate);
    fulfillmentServiceId = fulfillmentService.serviceId;

    workflow.deactivateAttempt = await runCase('deactivateAttempt', locationDeactivateMutation, {
      locationId: fulfillmentService.locationId,
    });
    assertUserErrorCode(
      workflow.deactivateAttempt,
      'locationDeactivate',
      'locationDeactivateUserErrors',
      'PERMANENTLY_BLOCKED_FROM_DEACTIVATION_ERROR',
    );

    workflow.readAfterDeactivateAttempt = await runCase('readAfterDeactivateAttempt', locationReadQuery, {
      id: fulfillmentService.locationId,
    });

    const hydrate = await runCase('fulfillmentServiceLocationHydrate', locationHydrateQuery, {
      id: fulfillmentService.locationId,
    });
    upstreamCalls.push({
      operationName: 'StorePropertiesLocationHydrate',
      variables: { id: fulfillmentService.locationId },
      query: digestDocument(locationHydrateQuery),
      response: {
        status: hydrate.response.status,
        body: hydrate.response.payload,
      },
    });

    workflow.activateFulfillmentServiceLocation = await runCase(
      'activateFulfillmentServiceLocation',
      locationActivateMutation,
      {
        locationId: fulfillmentService.locationId,
      },
    );
    assertUserErrorCode(
      workflow.activateFulfillmentServiceLocation,
      'locationActivate',
      'locationActivateUserErrors',
      'LOCATION_NOT_FOUND',
    );

    workflow.readAfterActivate = await runCase('readAfterActivate', locationReadQuery, {
      id: fulfillmentService.locationId,
    });
  } finally {
    if (fulfillmentServiceId !== null) {
      cleanup.fulfillmentServiceDelete = await runCase('fulfillmentServiceDelete', fulfillmentServiceDeleteMutation, {
        id: fulfillmentServiceId,
      });
      assertNoUserErrors(cleanup.fulfillmentServiceDelete, 'fulfillmentServiceDelete');
    }
  }

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
          'Captures locationActivate against a fulfillment-service-managed Location returning LOCATION_NOT_FOUND.',
          'The public Admin API creates fulfillment-service locations active; a recorded deactivate attempt proves that this store cannot construct the inactive fulfillment-service-managed branch through public lifecycle mutations.',
        ],
        setup,
        workflow,
        cleanup,
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
