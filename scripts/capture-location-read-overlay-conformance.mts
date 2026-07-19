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

type CaptureCase = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

type LocationAddData = {
  locationAdd?: {
    location?: {
      id?: unknown;
    } | null;
    userErrors?: unknown[];
  } | null;
};

const scenarioId = 'location-read-overlay-hydrates-real-id';
const apiVersion = '2026-04';
const fixtureFallbackIds = new Set([
  'gid://shopify/Location/112831103282',
  'gid://shopify/Location/106318430514',
  'gid://shopify/Location/106318430514-inactive',
]);
const fixtureProbeId = 'gid://shopify/Location/112831103282';

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

function createClient(apiVersion: string): AdminGraphqlClient {
  return createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: adminHeaders,
  });
}

const client = createClient(apiVersion);

const locationsCatalogQuery = `#graphql
  query LocationReadOverlayCatalog($first: Int!) {
    locations(first: $first) {
      nodes {
        id
        name
      }
    }
  }
`;

const locationAddMutation = `#graphql
  mutation LocationReadOverlaySeed($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        fulfillsOnlineOrders
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const locationReadQuery = `#graphql
  query LocationReadOverlayHydratesRealId($realId: ID!, $fixtureId: ID!, $stagedId: ID!) {
    real: location(id: $realId) {
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
      isPrimary
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
    realByIdentifier: locationByIdentifier(identifier: { id: $realId }) {
      id
      name
      isActive
    }
    fixture: location(id: $fixtureId) {
      id
      name
      isActive
      fulfillsOnlineOrders
    }
    staged: location(id: $stagedId) {
      id
      name
      isActive
    }
  }
`;

const locationHydrateQuery = `query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }`;

const locationDeactivateWithDirectiveMutation = `#graphql
  mutation LocationReadOverlayCleanupDeactivate($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        id
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
  mutation LocationReadOverlayCleanupDelete($locationId: ID!) {
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

async function runCase(
  client: AdminGraphqlClient,
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CaptureCase> {
  const response = await client.runGraphqlRequest(query, variables);
  return {
    name,
    query: query.replace(/^#graphql\n/u, '').trim(),
    variables,
    response,
  };
}

function readAddedLocationId(createCase: CaptureCase): string {
  const data = createCase.response.payload.data as LocationAddData | undefined;
  const id = data?.locationAdd?.location?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(
      `locationAdd did not return a disposable location id: ${JSON.stringify(createCase.response.payload)}`,
    );
  }

  const userErrors = data?.locationAdd?.userErrors ?? [];
  if (userErrors.length > 0) {
    throw new Error(`locationAdd returned userErrors: ${JSON.stringify(userErrors)}`);
  }

  return id;
}

function readRealLocationId(catalog: CaptureCase): string {
  const nodes = catalog.response.payload.data?.locations?.nodes;
  if (!Array.isArray(nodes)) {
    throw new Error(`locations catalog did not return nodes: ${JSON.stringify(catalog.response.payload)}`);
  }
  const locationId = nodes
    .map((node) => node?.id)
    .find((id): id is string => typeof id === 'string' && !fixtureFallbackIds.has(id));
  if (!locationId) {
    throw new Error(`locations catalog did not include a non-fixture location id: ${JSON.stringify(nodes)}`);
  }
  return locationId;
}

async function cleanupLocation(
  locationId: string,
  cleanup: CaptureCase[],
  uniqueSuffix: string,
): Promise<void> {
  const locationToken = locationId.split('/').at(-1) ?? uniqueSuffix;
  cleanup.push(
    await runCase(client, 'cleanupDeactivate', locationDeactivateWithDirectiveMutation, {
      locationId,
      idempotencyKey: `${scenarioId}-cleanup-${uniqueSuffix}-${locationToken}`,
    }),
  );
  cleanup.push(await runCase(client, 'cleanupDelete', locationDeleteMutation, { locationId }));
}

function upstreamCallFromHydrate(hydrate: CaptureCase, id: string) {
  return {
    operationName: 'StorePropertiesLocationHydrate',
    variables: { id },
    query: locationHydrateQuery,
    response: {
      status: hydrate.response.status,
      body: hydrate.response.payload,
    },
  };
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const uniqueSuffix = capturedAt.replace(/\D/gu, '').slice(0, 14);
const cleanup: CaptureCase[] = [];
let overlayLocationId: string | null = null;

try {
  const catalog = await runCase(client, 'catalog', locationsCatalogQuery, { first: 10 });
  const realLocationId = readRealLocationId(catalog);
  const createOverlay = await runCase(client, 'createOverlay', locationAddMutation, {
    input: {
      name: `Overlay Probe ${uniqueSuffix}`,
      fulfillsOnlineOrders: false,
      address: {
        address1: '3 Spadina',
        city: 'Toronto',
        countryCode: 'CA',
        zip: 'M5T 2C4',
      },
    },
  });
  overlayLocationId = readAddedLocationId(createOverlay);

  const read = await runCase(client, 'read', locationReadQuery, {
    realId: realLocationId,
    fixtureId: fixtureProbeId,
    stagedId: overlayLocationId,
  });
  const realHydrate = await runCase(client, 'realHydrate', locationHydrateQuery, { id: realLocationId });
  const fixtureHydrate = await runCase(client, 'fixtureHydrate', locationHydrateQuery, { id: fixtureProbeId });

  await cleanupLocation(overlayLocationId, cleanup, uniqueSuffix);

  const capture = {
    metadata: {
      capturedAt,
      storeDomain,
      apiVersion,
      realLocationId,
      fixtureLocationId: fixtureProbeId,
      overlayLocationId,
    },
    setup: {
      catalog,
      createOverlay,
    },
    read,
    hydrates: {
      realHydrate,
      fixtureHydrate,
    },
    cleanup,
    upstreamCalls: [
      upstreamCallFromHydrate(realHydrate, realLocationId),
      upstreamCallFromHydrate(fixtureHydrate, fixtureProbeId),
    ],
  };

  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        realLocationId,
        fixtureLocationId: fixtureProbeId,
        overlayLocationId,
        cleanup: cleanup.map((entry) => entry.name),
      },
      null,
      2,
    ),
  );
} catch (error) {
  if (overlayLocationId) {
    try {
      await cleanupLocation(overlayLocationId, cleanup, uniqueSuffix);
    } catch (cleanupError) {
      console.error(
        JSON.stringify(
          {
            ok: false,
            cleanupFailed: true,
            locationId: overlayLocationId,
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
