/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  type AdminGraphqlClient,
  type ConformanceGraphqlResult,
  createAdminGraphqlClient,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlVariables = Record<string, unknown>;

type CapturedCase = {
  name: string;
  query: string;
  variables: GraphqlVariables;
  response: ConformanceGraphqlResult;
};

const scenarioId = 'location-activate-non-unique-name';
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
const observeDocumentPath = path.join(
  'config',
  'parity-requests',
  'store-properties',
  'location-activate-non-unique-observed-active.graphql',
);
const activateDocumentPath = path.join(
  'config',
  'parity-requests',
  'store-properties',
  'location-activate-limit-and-relocation.graphql',
);

function createClient(apiVersion: string): AdminGraphqlClient {
  return createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: adminHeaders,
  });
}

const client = createClient(apiVersion);

const locationHydrateQuery =
  'query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }';

const locationAddMutation = `#graphql
  mutation LocationActivateNonUniqueAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        isActive
        fulfillsOnlineOrders
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
  mutation LocationActivateNonUniqueDeactivate($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        id
        name
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
  mutation LocationActivateNonUniqueDelete($locationId: ID!) {
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

async function runCase(name: string, query: string, variables: GraphqlVariables = {}): Promise<CapturedCase> {
  const response = await client.runGraphqlRequest(query, variables);
  return {
    name,
    query,
    variables,
    response,
  };
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : undefined;
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function data(capturedCase: CapturedCase): Record<string, unknown> | undefined {
  return asRecord(capturedCase.response.payload.data);
}

function payload(capturedCase: CapturedCase, rootName: string): Record<string, unknown> | undefined {
  return asRecord(data(capturedCase)?.[rootName]);
}

function readAddedLocationId(createCase: CapturedCase): string {
  const location = asRecord(payload(createCase, 'locationAdd')?.['location']);
  const id = location?.['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`locationAdd did not return a location id: ${JSON.stringify(createCase.response.payload)}`);
  }
  const userErrors = asArray(payload(createCase, 'locationAdd')?.['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`locationAdd returned userErrors: ${JSON.stringify(userErrors)}`);
  }
  return id;
}

function userErrorCodes(capturedCase: CapturedCase, rootName: string, errorFieldName: string): string[] {
  return asArray(payload(capturedCase, rootName)?.[errorFieldName])
    .map((error) => asRecord(error)?.['code'])
    .filter((code): code is string => typeof code === 'string');
}

function assertNoUserErrors(capturedCase: CapturedCase, rootName: string, errorFieldName: string): void {
  const codes = userErrorCodes(capturedCase, rootName, errorFieldName);
  if (codes.length > 0) {
    throw new Error(`${capturedCase.name} returned userErrors: ${JSON.stringify(capturedCase.response.payload)}`);
  }
}

function assertUserErrorCode(capturedCase: CapturedCase, rootName: string, errorFieldName: string, code: string): void {
  if (!userErrorCodes(capturedCase, rootName, errorFieldName).includes(code)) {
    throw new Error(`${capturedCase.name} did not return ${code}: ${JSON.stringify(capturedCase.response.payload)}`);
  }
}

function completeAddress(address1: string, zip: string): Record<string, string> {
  return {
    address1,
    city: 'Boston',
    provinceCode: 'MA',
    countryCode: 'US',
    zip,
  };
}

async function cleanupLocation(locationId: string, cleanup: CapturedCase[], uniqueSuffix: string): Promise<void> {
  cleanup.push(
    await runCase('cleanupDeactivate', locationDeactivateMutation, {
      locationId,
      idempotencyKey: `${scenarioId}-cleanup-deactivate-${uniqueSuffix}-${locationId.split('/').at(-1)}`,
    }),
  );
  cleanup.push(await runCase('cleanupDelete', locationDeleteMutation, { locationId }));
}

function connectionNodes(capturedCase: CapturedCase): unknown[] {
  return asArray(payload(capturedCase, 'locationsAvailableForDeliveryProfilesConnection')?.['nodes']);
}

await mkdir(outputDir, { recursive: true });

const observeDocument = await readFile(observeDocumentPath, 'utf8');
const activateDocument = await readFile(activateDocumentPath, 'utf8');
const capturedAt = new Date().toISOString();
const uniqueSuffix = capturedAt.replace(/\D/gu, '').slice(0, 14);
const duplicateName = `Non unique activation ${uniqueSuffix}`;
const createdLocationIds: string[] = [];
const cleanup: CapturedCase[] = [];

let targetCreate: CapturedCase;
let targetDeactivate: CapturedCase;
let duplicateCreate: CapturedCase;
let observeActiveDuplicate: CapturedCase;
let targetHydrate: CapturedCase;
let activate: CapturedCase;

try {
  targetCreate = await runCase('targetCreate', locationAddMutation, {
    input: {
      name: duplicateName,
      fulfillsOnlineOrders: false,
      address: completeAddress('1 Non Unique Activation St', '02108'),
    },
  });
  const targetLocationId = readAddedLocationId(targetCreate);
  createdLocationIds.push(targetLocationId);

  targetDeactivate = await runCase('targetDeactivate', locationDeactivateMutation, {
    locationId: targetLocationId,
    idempotencyKey: `${scenarioId}-target-deactivate-${uniqueSuffix}`,
  });
  assertNoUserErrors(targetDeactivate, 'locationDeactivate', 'locationDeactivateUserErrors');

  duplicateCreate = await runCase('duplicateCreate', locationAddMutation, {
    input: {
      name: duplicateName,
      fulfillsOnlineOrders: true,
      address: completeAddress('2 Non Unique Activation St', '02109'),
    },
  });
  const duplicateLocationId = readAddedLocationId(duplicateCreate);
  createdLocationIds.push(duplicateLocationId);

  observeActiveDuplicate = await runCase('observeActiveDuplicate', observeDocument, { first: 200 });
  if (
    !connectionNodes(observeActiveDuplicate).some(
      (node) =>
        asRecord(node)?.['id'] === duplicateLocationId &&
        asRecord(node)?.['name'] === duplicateName &&
        asRecord(node)?.['isActive'] === true,
    )
  ) {
    throw new Error(
      `locationsAvailableForDeliveryProfilesConnection did not expose the active duplicate location: ${JSON.stringify(
        observeActiveDuplicate.response.payload,
      )}`,
    );
  }

  targetHydrate = await runCase('targetHydrate', locationHydrateQuery, { id: targetLocationId });
  const hydratedLocation = asRecord(data(targetHydrate)?.['location']);
  if (hydratedLocation?.['name'] !== duplicateName || hydratedLocation?.['isActive'] !== false) {
    throw new Error(`target hydrate did not return the inactive duplicate: ${JSON.stringify(targetHydrate.response)}`);
  }

  const activateVariables = {
    locationId: targetLocationId,
    idempotencyKey: `${scenarioId}-${uniqueSuffix}`,
  };
  activate = await runCase('activate', activateDocument, activateVariables);
  assertUserErrorCode(activate, 'locationActivate', 'locationActivateUserErrors', 'HAS_NON_UNIQUE_NAME');

  const output = {
    scenarioId,
    storeDomain,
    apiVersion,
    capturedAt,
    setup: {
      duplicateName,
      targetLocationId,
      duplicateLocationId,
      plan: [
        'Create a disposable target location.',
        'Deactivate the target so a second active location can use the same name.',
        'Create a second active disposable location with that name.',
        'Record the active duplicate through locationsAvailableForDeliveryProfilesConnection.',
        'Hydrate the inactive target through StorePropertiesLocationHydrate.',
        'Activate the inactive target and capture HAS_NON_UNIQUE_NAME.',
        'Deactivate and delete both disposable locations during cleanup.',
      ],
    },
    proxyVariables: {
      observeActiveDuplicate: { first: 200 },
      activate: activateVariables,
    },
    workflow: {
      targetCreate,
      targetDeactivate,
      duplicateCreate,
      observeActiveDuplicate,
      targetHydrate,
      activate,
    },
    upstreamCalls: [
      {
        operationName: 'LocationActivateNonUniqueObservedActive',
        variables: { first: 200 },
        query: observeDocument,
        response: {
          status: observeActiveDuplicate.response.status,
          body: observeActiveDuplicate.response.payload,
        },
      },
      {
        operationName: 'StorePropertiesLocationHydrate',
        variables: { id: targetLocationId },
        query: locationHydrateQuery,
        response: {
          status: targetHydrate.response.status,
          body: targetHydrate.response.payload,
        },
      },
    ],
    cleanup,
  };

  for (const locationId of createdLocationIds.toReversed()) {
    await cleanupLocation(locationId, cleanup, uniqueSuffix);
  }

  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
  console.log(
    `Captured locationActivate HAS_NON_UNIQUE_NAME: ${JSON.stringify(
      payload(activate, 'locationActivate')?.['locationActivateUserErrors'],
    )}`,
  );
} catch (error) {
  for (const locationId of createdLocationIds.toReversed()) {
    await cleanupLocation(locationId, cleanup, uniqueSuffix);
  }
  throw error;
}
