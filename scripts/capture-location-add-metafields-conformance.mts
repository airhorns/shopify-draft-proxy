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

const scenarioId = 'location-add-metafields';
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
const locationAddDocumentPath = 'config/parity-requests/store-properties/location-add-metafields.graphql';
const locationReadDocumentPath = 'config/parity-requests/store-properties/location-add-metafields-read.graphql';

const locationDeactivateWithDirectiveMutation = `#graphql
  mutation LocationAddMetafieldsCleanupDeactivate($locationId: ID!, $idempotencyKey: String!) {
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
  mutation LocationAddMetafieldsCleanupDelete($locationId: ID!) {
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

function createClient(version: string): AdminGraphqlClient {
  return createAdminGraphqlClient({
    adminOrigin,
    apiVersion: version,
    headers: adminHeaders,
  });
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

async function readText(filePath: string): Promise<string> {
  return await readFile(filePath, 'utf8');
}

async function runCase(
  client: AdminGraphqlClient,
  name: string,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<CaptureCase> {
  const response = await client.runGraphqlRequest(query, variables);
  return {
    name,
    query,
    variables,
    response,
  };
}

async function cleanupLocation(
  client: AdminGraphqlClient,
  locationId: string,
  cleanup: CaptureCase[],
  suffix: string,
  uniqueSuffix: string,
): Promise<void> {
  const locationToken = locationId.split('/').at(-1) ?? suffix;
  cleanup.push(
    await runCase(client, `cleanupDeactivate-${suffix}`, locationDeactivateWithDirectiveMutation, {
      locationId,
      idempotencyKey: `${scenarioId}-cleanup-${suffix}-${uniqueSuffix}-${locationToken}`,
    }),
  );
  cleanup.push(await runCase(client, `cleanupDelete-${suffix}`, locationDeleteMutation, { locationId }));
}

const client = createClient(apiVersion);
const locationAddMutation = await readText(locationAddDocumentPath);
const locationReadQuery = await readText(locationReadDocumentPath);

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const uniqueSuffix = capturedAt.replace(/\D/gu, '').slice(0, 14);
const cleanup: CaptureCase[] = [];
const createdLocationIds: string[] = [];

try {
  const metafieldAdd = await runCase(client, 'metafieldAdd', locationAddMutation, {
    input: {
      name: `Proxy Metafields ${uniqueSuffix}`,
      address: {
        address1: '3 Spadina',
        city: 'Toronto',
        countryCode: 'CA',
        zip: 'M5T 2C4',
      },
      metafields: [
        {
          namespace: 'custom',
          key: 'har1041',
          type: 'single_line_text_field',
          value: '9-5',
        },
      ],
    },
  });
  const locationId = readAddedLocationId(metafieldAdd);
  createdLocationIds.push(locationId);
  const metafieldRead = await runCase(client, 'metafieldRead', locationReadQuery, {
    id: locationId,
  });

  const invalidMetafieldType = await runCase(client, 'invalidMetafieldType', locationAddMutation, {
    input: {
      name: `Proxy Invalid Metafield ${uniqueSuffix}`,
      address: {
        countryCode: 'CA',
      },
      metafields: [
        {
          namespace: 'custom',
          key: 'har1041',
          type: 'bogus_type',
          value: 'x',
        },
      ],
    },
  });
  const blankMetafieldValue = await runCase(client, 'blankMetafieldValue', locationAddMutation, {
    input: {
      name: `Proxy Blank Metafield Value ${uniqueSuffix}`,
      address: {
        countryCode: 'CA',
      },
      metafields: [
        {
          namespace: 'custom',
          key: 'har1041_blank',
          type: 'single_line_text_field',
          value: '',
        },
      ],
    },
  });
  const blankValueLocationId = readAddedLocationId(blankMetafieldValue);
  createdLocationIds.push(blankValueLocationId);

  const blankMetafieldKey = await runCase(client, 'blankMetafieldKey', locationAddMutation, {
    input: {
      name: `Proxy Blank Metafield ${uniqueSuffix}`,
      address: {
        countryCode: 'CA',
      },
      metafields: [
        {
          namespace: 'custom',
          key: '',
          type: 'single_line_text_field',
          value: 'x',
        },
      ],
    },
  });

  await cleanupLocation(client, locationId, cleanup, 'metafieldAdd', uniqueSuffix);
  await cleanupLocation(client, blankValueLocationId, cleanup, 'blankMetafieldValue', uniqueSuffix);

  const capture = {
    capturedAt,
    storeDomain,
    apiVersion,
    cases: {
      metafieldAdd,
      metafieldRead,
      invalidMetafieldType,
      blankMetafieldValue,
      blankMetafieldKey,
    },
    cleanup,
    upstreamCalls: [],
  };

  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        createdLocationIds,
        cleanup: cleanup.map((entry) => entry.name),
      },
      null,
      2,
    ),
  );
} catch (error) {
  for (const [index, locationId] of createdLocationIds.entries()) {
    try {
      await cleanupLocation(client, locationId, cleanup, `error-${index}`, uniqueSuffix);
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
