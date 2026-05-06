/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

type Captures = {
  existingLocations?: GraphqlCapture;
  destinationCreate?: GraphqlCapture;
  serviceCreate?: GraphqlCapture;
  invalidKeep?: GraphqlCapture;
  invalidDelete?: GraphqlCapture;
  validKeep?: GraphqlCapture;
  cleanup?: JsonRecord;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, 'fulfillment-service-delete-inventory-action-validation.json');

const locationsQuery = `#graphql
  query FulfillmentServiceDeleteInventoryActionValidationLocations($first: Int!) {
    locations(first: $first) {
      nodes {
        id
        name
        isActive
        isFulfillmentService
      }
    }
  }
`;

const locationAddMutation = `#graphql
  mutation FulfillmentServiceDeleteInventoryActionValidationLocationAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        isActive
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentServiceCreateMutation = `#graphql
  mutation FulfillmentServiceDeleteInventoryActionValidationCreate($name: String!) {
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

const fulfillmentServiceDeleteWithDestinationMutation = `#graphql
  mutation FulfillmentServiceDeleteInventoryActionValidationWithDestination(
    $id: ID!
    $destinationLocationId: ID!
    $inventoryAction: FulfillmentServiceDeleteInventoryAction!
  ) {
    fulfillmentServiceDelete(
      id: $id
      destinationLocationId: $destinationLocationId
      inventoryAction: $inventoryAction
    ) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentServiceDeleteKeepMutation = `#graphql
  mutation FulfillmentServiceDeleteInventoryActionValidationKeep($id: ID!) {
    fulfillmentServiceDelete(id: $id, inventoryAction: KEEP) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentServiceDeleteCleanupMutation = `#graphql
  mutation FulfillmentServiceDeleteInventoryActionValidationCleanupService($id: ID!) {
    fulfillmentServiceDelete(id: $id, inventoryAction: DELETE) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

function locationDeactivateMutation(idempotencyKey: string): string {
  return `#graphql
  mutation FulfillmentServiceDeleteInventoryActionValidationCleanupDeactivate(
    $locationId: ID!
    $destinationLocationId: ID!
  ) {
    locationDeactivate(locationId: $locationId, destinationLocationId: $destinationLocationId)
      @idempotent(key: "${idempotencyKey}") {
      location {
        id
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
  mutation FulfillmentServiceDeleteInventoryActionValidationCleanupDeleteLocation($locationId: ID!) {
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

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  return {
    query: trimGraphql(query),
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null ? (value as JsonRecord) : null;
}

function readNodes(value: unknown): JsonRecord[] {
  const record = readObject(value);
  const nodes = record?.['nodes'];
  return Array.isArray(nodes) ? nodes.filter((node): node is JsonRecord => readObject(node) !== null) : [];
}

function data(captureResult: GraphqlCapture): JsonRecord {
  const dataValue = captureResult.response.payload.data;
  const record = readObject(dataValue);
  if (!record) {
    throw new Error(`Capture response did not include data: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return record;
}

function mutationPayload(captureResult: GraphqlCapture, key: string): JsonRecord {
  const payload = readObject(data(captureResult)[key]);
  if (!payload) {
    throw new Error(`Capture response did not include ${key}: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return payload;
}

function userErrors(captureResult: GraphqlCapture, key: string): unknown[] {
  const errors = mutationPayload(captureResult, key)['userErrors'];
  return Array.isArray(errors) ? errors : [];
}

function assertNoTopLevelErrors(captureResult: GraphqlCapture, label: string): void {
  if (captureResult.response.status !== 200 || captureResult.response.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function assertNoUserErrors(captureResult: GraphqlCapture, key: string, label: string): void {
  assertNoTopLevelErrors(captureResult, label);
  const errors = userErrors(captureResult, key);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertInventoryActionUserError(captureResult: GraphqlCapture, label: string): void {
  assertNoTopLevelErrors(captureResult, label);
  const payload = mutationPayload(captureResult, 'fulfillmentServiceDelete');
  if (payload['deletedId'] !== null) {
    throw new Error(`${label} returned deletedId: ${JSON.stringify(payload)}`);
  }
  const errors = userErrors(captureResult, 'fulfillmentServiceDelete');
  const firstError = readObject(errors[0]);
  const field = firstError?.['field'];
  if (!Array.isArray(field) || field.length !== 1 || field[0] !== 'inventoryAction') {
    throw new Error(`${label} did not return inventoryAction userError: ${JSON.stringify(payload)}`);
  }
}

function readLocationId(captureResult: GraphqlCapture): string {
  const locationAdd = mutationPayload(captureResult, 'locationAdd');
  const location = readObject(locationAdd['location']);
  const id = location?.['id'];
  if (typeof id !== 'string') {
    throw new Error(
      `Unable to read created destination location id: ${JSON.stringify(captureResult.response.payload)}`,
    );
  }
  return id;
}

function readFulfillmentService(captureResult: GraphqlCapture): { id: string; locationId: string } {
  const payload = mutationPayload(captureResult, 'fulfillmentServiceCreate');
  const service = readObject(payload['fulfillmentService']);
  const location = readObject(service?.['location']);
  const id = service?.['id'];
  const locationId = location?.['id'];
  if (typeof id !== 'string' || typeof locationId !== 'string') {
    throw new Error(
      `Unable to read created fulfillment service ids: ${JSON.stringify(captureResult.response.payload)}`,
    );
  }
  return { id, locationId };
}

function readCleanupDestinationLocationId(captureResult: GraphqlCapture, createdLocationIds: string[]): string | null {
  const locations = readObject(data(captureResult)['locations']);
  return (
    readNodes(locations)
      .filter((location) => !createdLocationIds.includes(String(location['id'])))
      .filter((location) => location['isActive'] === true)
      .filter((location) => location['isFulfillmentService'] !== true)
      .map((location) => location['id'])
      .find((id): id is string => typeof id === 'string') ?? null
  );
}

async function cleanupLocation(
  cleanup: JsonRecord,
  key: string,
  locationId: string | null,
  destinationLocationId: string | null,
  stamp: string,
): Promise<void> {
  if (!locationId || !destinationLocationId) {
    return;
  }
  cleanup[`${key}Deactivate`] = await capture(locationDeactivateMutation(`fs-delete-action-cleanup-${key}-${stamp}`), {
    locationId,
    destinationLocationId,
  });
  cleanup[`${key}Delete`] = await capture(locationDeleteMutation, { locationId });
}

const startedAt = new Date().toISOString();
const stamp = startedAt.replace(/[-:.TZ]/gu, '').slice(0, 14);
const captures: Captures = {};
let destinationLocationId: string | null = null;
let cleanupDestinationLocationId: string | null = null;
let serviceId: string | null = null;
let serviceLocationId: string | null = null;

try {
  captures.existingLocations = await capture(locationsQuery, { first: 20 });
  assertNoTopLevelErrors(captures.existingLocations, 'existing locations');

  captures.destinationCreate = await capture(locationAddMutation, {
    input: {
      name: `FS delete action destination ${stamp}`,
      address: {
        countryCode: 'US',
        address1: '123 Broadway',
        city: 'New York',
        provinceCode: 'NY',
        zip: '10006',
      },
      fulfillsOnlineOrders: true,
    },
  });
  assertNoUserErrors(captures.destinationCreate, 'locationAdd', 'destination location create');
  destinationLocationId = readLocationId(captures.destinationCreate);

  captures.serviceCreate = await capture(fulfillmentServiceCreateMutation, {
    name: `FS delete action service ${stamp}`,
  });
  assertNoUserErrors(captures.serviceCreate, 'fulfillmentServiceCreate', 'fulfillment service create');
  const service = readFulfillmentService(captures.serviceCreate);
  serviceId = service.id;
  serviceLocationId = service.locationId;
  cleanupDestinationLocationId = readCleanupDestinationLocationId(captures.existingLocations, [
    destinationLocationId,
    serviceLocationId,
  ]);

  captures.invalidKeep = await capture(fulfillmentServiceDeleteWithDestinationMutation, {
    id: service.id,
    destinationLocationId,
    inventoryAction: 'KEEP',
  });
  assertInventoryActionUserError(captures.invalidKeep, 'KEEP with destination delete');

  captures.invalidDelete = await capture(fulfillmentServiceDeleteWithDestinationMutation, {
    id: service.id,
    destinationLocationId,
    inventoryAction: 'DELETE',
  });
  assertInventoryActionUserError(captures.invalidDelete, 'DELETE with destination delete');

  captures.validKeep = await capture(fulfillmentServiceDeleteKeepMutation, {
    id: service.id,
  });
  assertNoUserErrors(captures.validKeep, 'fulfillmentServiceDelete', 'valid KEEP delete');
} finally {
  const cleanup: JsonRecord = {};
  if (serviceId && !captures.validKeep) {
    cleanup['serviceDelete'] = await capture(fulfillmentServiceDeleteCleanupMutation, {
      id: serviceId,
    });
  }
  await cleanupLocation(cleanup, 'serviceLocation', serviceLocationId, cleanupDestinationLocationId, stamp);
  await cleanupLocation(cleanup, 'destination', destinationLocationId, cleanupDestinationLocationId, stamp);
  captures.cleanup = cleanup;
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      startedAt,
      notes: [
        'fulfillmentServiceDelete inventoryAction validation capture for KEEP and DELETE with destinationLocationId.',
        'The active app schema exposes fulfillmentServiceDelete.userErrors as UserError without a selectable code field, so code parity is covered by local runtime tests rather than this live capture.',
      ],
      captures,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);

console.log(`Wrote ${outputPath}`);
