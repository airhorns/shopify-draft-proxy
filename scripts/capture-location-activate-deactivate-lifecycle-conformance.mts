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

type CaptureCase = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

const scenarioId = 'location-activate-deactivate-lifecycle';
const outputApiVersion = '2026-04';
const optionalDirectiveApiVersion = '2025-10';
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

const client202604 = createClient(outputApiVersion);
const clientOptional = createClient(optionalDirectiveApiVersion);

const locationLifecycleFields = `#graphql
  fragment LocationLifecycleFields on Location {
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

const locationHydrateQuery = `#graphql
  query StorePropertiesLocationHydrate($id: ID!) {
    location(id: $id) {
      id
      legacyResourceId
      name
      activatable
      addressVerified
      createdAt
      deactivatable
      deactivatedAt
      deletable
      fulfillsOnlineOrders
      hasActiveInventory
      hasUnfulfilledOrders
      isActive
      isFulfillmentService
      shipsInventory
      updatedAt
      fulfillmentService {
        id
        handle
        serviceName
      }
      address {
        address1
        address2
        city
        country
        countryCode
        formatted
        latitude
        longitude
        phone
        province
        provinceCode
        zip
      }
      suggestedAddresses {
        address1
        countryCode
        formatted
      }
      metafield(namespace: "custom", key: "hours") {
        id
        namespace
        key
        value
        type
      }
      metafields(first: 3) {
        nodes {
          id
          namespace
          key
          value
          type
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      inventoryLevels(first: 3) {
        nodes {
          id
          item {
            id
          }
          location {
            id
            name
          }
          quantities(names: ["available", "committed", "on_hand"]) {
            name
            quantity
            updatedAt
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
    }
  }
`;

const locationAddMutation = `#graphql
  ${locationLifecycleFields}

  mutation LocationLifecycleFixtureAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        ...LocationLifecycleFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const locationDeactivateWithDirectiveMutation = `#graphql
  ${locationLifecycleFields}

  mutation LocationLifecycleDeactivateWithDirective($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        ...LocationLifecycleFields
      }
      locationDeactivateUserErrors {
        field
        code
        message
      }
    }
  }
`;

const locationActivateWithDirectiveMutation = `#graphql
  ${locationLifecycleFields}

  mutation LocationLifecycleActivateWithDirective($locationId: ID!, $idempotencyKey: String!) {
    locationActivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        ...LocationLifecycleFields
      }
      locationActivateUserErrors {
        field
        code
        message
      }
    }
  }
`;

const locationDeactivateWithBareDirectiveMutation = `#graphql
  ${locationLifecycleFields}

  mutation LocationLifecycleDeactivateWithBareDirective($locationId: ID!) {
    locationDeactivate(locationId: $locationId) @idempotent {
      location {
        ...LocationLifecycleFields
      }
      locationDeactivateUserErrors {
        field
        code
        message
      }
    }
  }
`;

const locationActivateWithBareDirectiveMutation = `#graphql
  ${locationLifecycleFields}

  mutation LocationLifecycleActivateWithBareDirective($locationId: ID!) {
    locationActivate(locationId: $locationId) @idempotent {
      location {
        ...LocationLifecycleFields
      }
      locationActivateUserErrors {
        field
        code
        message
      }
    }
  }
`;

const locationDeactivateWithoutDirectiveMutation = `#graphql
mutation LocationDeactivateMissingIdempotencyKey($locationId: ID!) {
  locationDeactivate(locationId: $locationId) {
    location {
      id
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

const locationActivateWithoutDirectiveMutation = `#graphql
mutation LocationActivateMissingIdempotencyKey($locationId: ID!) {
  locationActivate(locationId: $locationId) {
    location {
      id
      isActive
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
  ${locationLifecycleFields}

  query LocationLifecycleRead($id: ID!) {
    location(id: $id) {
      ...LocationLifecycleFields
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation LocationLifecycleFixtureDelete($locationId: ID!) {
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

function digestDocument(document: string): string {
  return `sha256:${createHash('sha256').update(document).digest('hex')}`;
}

async function runCase(
  client: AdminGraphqlClient,
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CaptureCase> {
  const response = await client.runGraphqlRequest(query, variables);
  return {
    name,
    query,
    variables,
    response,
  };
}

function readAddedLocationId(createCase: CaptureCase): string {
  const payload = createCase.response.payload as {
    data?: { locationAdd?: { location?: { id?: unknown }; userErrors?: unknown[] } };
  };
  const id = payload.data?.locationAdd?.location?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(
      `locationAdd did not return a disposable location id: ${JSON.stringify(createCase.response.payload)}`,
    );
  }
  const userErrors = payload.data?.locationAdd?.userErrors ?? [];
  if (userErrors.length > 0) {
    throw new Error(`locationAdd returned userErrors: ${JSON.stringify(userErrors)}`);
  }
  return id;
}

async function cleanupLocation(locationId: string, cleanup: CaptureCase[]): Promise<void> {
  cleanup.push(
    await runCase(client202604, 'cleanupDeactivate', locationDeactivateWithDirectiveMutation, {
      locationId,
      idempotencyKey: `${scenarioId}-cleanup-deactivate`,
    }),
  );
  cleanup.push(await runCase(client202604, 'cleanupDelete', locationDeleteMutation, { locationId }));
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const uniqueSuffix = capturedAt.replace(/\D/gu, '').slice(0, 14);
const cleanup: CaptureCase[] = [];
let locationId: string | null = null;

try {
  const createLocation = await runCase(client202604, 'setupLocationAdd', locationAddMutation, {
    input: {
      name: `HAR-658 lifecycle ${uniqueSuffix}`,
      fulfillsOnlineOrders: false,
      address: {
        address1: '1 Test St',
        city: 'Boston',
        provinceCode: 'MA',
        countryCode: 'US',
        zip: '02110',
      },
    },
  });
  locationId = readAddedLocationId(createLocation);

  const hydrateLocation = await runCase(client202604, 'locationHydrateForProxyCassette', locationHydrateQuery, {
    id: locationId,
  });

  const deactivateWithoutDirective202510 = await runCase(
    clientOptional,
    'locationDeactivateWithoutDirective202510',
    locationDeactivateWithoutDirectiveMutation,
    { locationId },
  );
  const activateWithoutDirective202510 = await runCase(
    clientOptional,
    'locationActivateWithoutDirective202510',
    locationActivateWithoutDirectiveMutation,
    { locationId },
  );
  const deactivateWithDirective202510 = await runCase(
    clientOptional,
    'locationDeactivateWithDirective202510',
    locationDeactivateWithDirectiveMutation,
    {
      locationId,
      idempotencyKey: `${scenarioId}-deactivate-202510-${uniqueSuffix}`,
    },
  );
  const activateWithDirective202510 = await runCase(
    clientOptional,
    'locationActivateWithDirective202510',
    locationActivateWithDirectiveMutation,
    {
      locationId,
      idempotencyKey: `${scenarioId}-activate-202510-${uniqueSuffix}`,
    },
  );

  const deactivateWithoutDirective202604 = await runCase(
    client202604,
    'locationDeactivateWithoutDirective202604',
    locationDeactivateWithoutDirectiveMutation,
    { locationId },
  );
  const activateWithoutDirective202604 = await runCase(
    client202604,
    'locationActivateWithoutDirective202604',
    locationActivateWithoutDirectiveMutation,
    { locationId },
  );
  const deactivateWithDirective202604 = await runCase(
    client202604,
    'locationDeactivateWithDirective202604',
    locationDeactivateWithDirectiveMutation,
    {
      locationId,
      idempotencyKey: `${scenarioId}-deactivate-202604-${uniqueSuffix}`,
    },
  );
  const activateWithDirective202604 = await runCase(
    client202604,
    'locationActivateWithDirective202604',
    locationActivateWithDirectiveMutation,
    {
      locationId,
      idempotencyKey: `${scenarioId}-activate-202604-${uniqueSuffix}`,
    },
  );
  const deactivateWithBareDirective202604 = await runCase(
    client202604,
    'locationDeactivateWithBareDirective202604',
    locationDeactivateWithBareDirectiveMutation,
    { locationId },
  );
  const activateWithBareDirective202604 = await runCase(
    client202604,
    'locationActivateWithBareDirective202604',
    locationActivateWithBareDirectiveMutation,
    { locationId },
  );
  const readAfterKeyedActivate = await runCase(client202604, 'readAfterKeyedActivate', locationReadQuery, {
    id: locationId,
  });

  await cleanupLocation(locationId, cleanup);

  const capture = {
    capturedAt,
    storeDomain,
    apiVersion: outputApiVersion,
    comparedApiVersions: [optionalDirectiveApiVersion, outputApiVersion],
    setup: {
      createLocation,
      hydrateLocation,
    },
    workflow: {
      deactivateWithoutDirective202510,
      activateWithoutDirective202510,
      deactivateWithDirective202510,
      activateWithDirective202510,
      deactivateWithoutDirective202604,
      activateWithoutDirective202604,
      deactivateWithDirective202604,
      activateWithDirective202604,
      deactivateWithBareDirective202604,
      activateWithBareDirective202604,
      readAfterKeyedActivate,
    },
    cleanup,
    upstreamCalls: [
      {
        operationName: 'StorePropertiesLocationHydrate',
        variables: { id: locationId },
        query: digestDocument(locationHydrateQuery),
        response: {
          status: hydrateLocation.response.status,
          body: hydrateLocation.response.payload,
        },
      },
    ],
  };

  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        locationId,
        comparedApiVersions: [optionalDirectiveApiVersion, outputApiVersion],
        cleanup: cleanup.map((entry) => entry.name),
      },
      null,
      2,
    ),
  );
} catch (error) {
  if (typeof locationId === 'string' && locationId.length > 0) {
    try {
      await cleanupLocation(locationId, cleanup);
    } catch (cleanupError) {
      console.error(
        JSON.stringify(
          {
            ok: false,
            cleanupFailed: true,
            locationId,
            error: cleanupError instanceof Error ? cleanupError.message : String(cleanupError),
          },
          null,
          2,
        ),
      );
    }
  }
  throw error;
}
