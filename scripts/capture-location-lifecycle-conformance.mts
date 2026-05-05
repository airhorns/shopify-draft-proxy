// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createHash } from 'node:crypto';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const scenarioId = 'location-activate-deactivate-with-idempotency-directive';
const requestedConfig = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const apiVersion = '2026-04';
const { storeDomain, adminOrigin } = requestedConfig;
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminHeaders = buildAdminAuthHeaders(adminAccessToken);
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'store-properties');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const client202604 = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: adminHeaders,
});
const client202601 = createAdminGraphqlClient({
  adminOrigin,
  apiVersion: '2026-01',
  headers: adminHeaders,
});

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

const locationReadQuery = `#graphql
  ${locationLifecycleFields}

  query LocationLifecycleRead($id: ID!) {
    location(id: $id) {
      ...LocationLifecycleFields
    }
  }
`;

const locationDeactivateMissingDirectiveMutation = `#graphql
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

async function runCase(client, name, query, variables) {
  const response = await client.runGraphqlRequest(query, variables);
  return {
    name,
    query,
    variables,
    response,
  };
}

function readAddedLocationId(createCase) {
  const id = createCase.response.payload?.data?.locationAdd?.location?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(
      `locationAdd did not return a disposable location id: ${JSON.stringify(createCase.response.payload)}`,
    );
  }
  const userErrors = createCase.response.payload?.data?.locationAdd?.userErrors ?? [];
  if (userErrors.length > 0) {
    throw new Error(`locationAdd returned userErrors: ${JSON.stringify(userErrors)}`);
  }
  return id;
}

async function cleanupLocation(locationId, cleanup) {
  const deactivateCleanup = await runCase(client202604, 'cleanupDeactivate', locationDeactivateWithDirectiveMutation, {
    locationId,
    idempotencyKey: `${scenarioId}-cleanup-deactivate`,
  });
  cleanup.push(deactivateCleanup);
  const deleteCleanup = await runCase(client202604, 'cleanupDelete', locationDeleteMutation, { locationId });
  cleanup.push(deleteCleanup);
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const uniqueSuffix = capturedAt.replace(/\D/gu, '').slice(0, 14);
const cleanup = [];
let locationId = null;

try {
  const createCase = await runCase(client202604, 'setupLocationAdd', locationAddMutation, {
    input: {
      name: `HAR-649 lifecycle ${uniqueSuffix}`,
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
  locationId = readAddedLocationId(createCase);

  const hydrateCase = await runCase(client202604, 'locationHydrateForProxyCassette', locationHydrateQuery, {
    id: locationId,
  });
  const deactivateWithDirective = await runCase(
    client202604,
    'locationDeactivateWithIdempotencyDirective',
    locationDeactivateWithDirectiveMutation,
    {
      locationId,
      idempotencyKey: `${scenarioId}-deactivate-${uniqueSuffix}`,
    },
  );
  const readAfterDeactivate = await runCase(client202604, 'readAfterDeactivate', locationReadQuery, {
    id: locationId,
  });
  const activateWithDirective = await runCase(
    client202604,
    'locationActivateWithIdempotencyDirective',
    locationActivateWithDirectiveMutation,
    {
      locationId,
      idempotencyKey: `${scenarioId}-activate-${uniqueSuffix}`,
    },
  );
  const missingDirective202604 = await runCase(
    client202604,
    'locationDeactivateMissingDirective202604',
    locationDeactivateMissingDirectiveMutation,
    { locationId },
  );
  const deactivateWithoutDirective202601 = await runCase(
    client202601,
    'locationDeactivateWithoutDirective202601',
    locationDeactivateMissingDirectiveMutation,
    { locationId },
  );

  await cleanupLocation(locationId, cleanup);

  const capture = {
    capturedAt,
    storeDomain,
    apiVersion,
    setup: {
      createLocation: createCase,
      hydrateLocation: hydrateCase,
    },
    workflow: {
      deactivateWithDirective,
      readAfterDeactivate,
      activateWithDirective,
      missingDirective202604,
      deactivateWithoutDirective202601,
    },
    cleanup,
    upstreamCalls: [
      {
        operationName: 'StorePropertiesLocationHydrate',
        variables: { id: locationId },
        query: digestDocument(locationHydrateQuery),
        response: {
          status: hydrateCase.response.status,
          body: hydrateCase.response.payload,
        },
      },
    ],
  };

  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  console.log(
    JSON.stringify({ ok: true, outputPath, locationId, cleanup: cleanup.map((entry) => entry.name) }, null, 2),
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
