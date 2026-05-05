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
  invalidDestination?: GraphqlCapture;
  validTransfer?: GraphqlCapture;
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
const outputPath = path.join(outputDir, 'fulfillment-service-delete-transfer.json');

const locationAddMutation = `#graphql
  mutation FulfillmentServiceDeleteTransferLocationAdd($input: LocationAddInput!) {
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

const locationsQuery = `#graphql
  query FulfillmentServiceDeleteTransferLocations($first: Int!) {
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

const fulfillmentServiceCreateMutation = `#graphql
  mutation FulfillmentServiceDeleteTransferCreate($name: String!) {
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

const fulfillmentServiceDeleteTransferMutation = `#graphql
  mutation FulfillmentServiceDeleteTransfer(
    $id: ID!
    $destinationLocationId: ID!
  ) {
    fulfillmentServiceDelete(
      id: $id
      destinationLocationId: $destinationLocationId
      inventoryAction: TRANSFER
    ) {
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
  mutation FulfillmentServiceDeleteTransferCleanupDeactivate(
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
  mutation FulfillmentServiceDeleteTransferCleanupDeleteLocation($locationId: ID!) {
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

const fulfillmentServiceDeleteCleanupMutation = `#graphql
  mutation FulfillmentServiceDeleteTransferCleanupService($id: ID!) {
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

function userErrors(payload: JsonRecord, key: string): unknown[] {
  const mutationPayload = readObject(payload[key]);
  const errors = mutationPayload?.['userErrors'];
  return Array.isArray(errors) ? errors : [];
}

function assertNoTopLevelErrors(captureResult: GraphqlCapture, label: string): void {
  if (captureResult.response.status !== 200 || captureResult.response.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function assertNoUserErrors(captureResult: GraphqlCapture, key: string, label: string): void {
  assertNoTopLevelErrors(captureResult, label);
  const errors = userErrors(data(captureResult), key);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertHasUserErrors(captureResult: GraphqlCapture, key: string, label: string): void {
  assertNoTopLevelErrors(captureResult, label);
  const errors = userErrors(data(captureResult), key);
  if (errors.length === 0) {
    throw new Error(`${label} unexpectedly returned no userErrors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function readLocationId(captureResult: GraphqlCapture): string {
  const locationAdd = readObject(data(captureResult)['locationAdd']);
  const location = readObject(locationAdd?.['location']);
  const id = location?.['id'];
  if (typeof id !== 'string') {
    throw new Error(
      `Unable to read created destination location id: ${JSON.stringify(captureResult.response.payload)}`,
    );
  }
  return id;
}

function readFulfillmentService(captureResult: GraphqlCapture): { id: string; locationId: string } {
  const payload = readObject(data(captureResult)['fulfillmentServiceCreate']);
  const service = readObject(payload?.['fulfillmentService']);
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

function readCleanupDestinationLocationId(captureResult: GraphqlCapture, createdLocationId: string): string | null {
  const locations = readObject(data(captureResult)['locations']);
  return (
    readNodes(locations)
      .filter((location) => location['id'] !== createdLocationId)
      .filter((location) => location['isActive'] === true)
      .filter((location) => location['isFulfillmentService'] !== true)
      .map((location) => location['id'])
      .find((id): id is string => typeof id === 'string') ?? null
  );
}

const startedAt = new Date().toISOString();
const stamp = startedAt.replace(/[-:.TZ]/gu, '').slice(0, 14);
const captures: Captures = {};
let destinationLocationId: string | null = null;
let cleanupDestinationLocationId: string | null = null;
let serviceId: string | null = null;

try {
  captures.existingLocations = await capture(locationsQuery, { first: 20 });
  assertNoTopLevelErrors(captures.existingLocations, 'existing locations');

  captures.destinationCreate = await capture(locationAddMutation, {
    input: {
      name: `HAR-571 transfer destination ${stamp}`,
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
  cleanupDestinationLocationId = readCleanupDestinationLocationId(captures.existingLocations, destinationLocationId);

  captures.serviceCreate = await capture(fulfillmentServiceCreateMutation, {
    name: `HAR-571 transfer FS ${stamp}`,
  });
  assertNoUserErrors(captures.serviceCreate, 'fulfillmentServiceCreate', 'fulfillment service create');
  const service = readFulfillmentService(captures.serviceCreate);
  serviceId = service.id;

  captures.invalidDestination = await capture(fulfillmentServiceDeleteTransferMutation, {
    id: service.id,
    destinationLocationId: 'gid://shopify/Location/999999999',
  });
  assertHasUserErrors(captures.invalidDestination, 'fulfillmentServiceDelete', 'invalid destination transfer delete');

  captures.validTransfer = await capture(fulfillmentServiceDeleteTransferMutation, {
    id: service.id,
    destinationLocationId,
  });
  assertNoUserErrors(captures.validTransfer, 'fulfillmentServiceDelete', 'valid transfer delete');
} finally {
  const cleanup: JsonRecord = {};
  if (serviceId && !captures.validTransfer) {
    cleanup['serviceDelete'] = await capture(fulfillmentServiceDeleteCleanupMutation, {
      id: serviceId,
    });
  }
  if (destinationLocationId && cleanupDestinationLocationId) {
    cleanup['destinationDeactivate'] = await capture(
      locationDeactivateMutation(`har-571-cleanup-deactivate-${stamp}`),
      {
        locationId: destinationLocationId,
        destinationLocationId: cleanupDestinationLocationId,
      },
    );
    cleanup['destinationDelete'] = await capture(locationDeleteMutation, {
      locationId: destinationLocationId,
    });
  }
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
        'HAR-571 fulfillmentServiceDelete TRANSFER capture with destination validation and valid delete behavior.',
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
