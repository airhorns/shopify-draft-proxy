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

type GraphqlVariables = Record<string, unknown>;

type CapturedCase = {
  name: string;
  query: string;
  variables: GraphqlVariables;
  response: ConformanceGraphqlResult;
};

const scenarioId = 'location-address-code-derivation';
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

function createClient(apiVersion: string): AdminGraphqlClient {
  return createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: adminHeaders,
  });
}

const client = createClient(apiVersion);

const locationAddMutation = `#graphql
  mutation LocationAddressCodeDerivationAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        address {
          country
          countryCode
          province
          provinceCode
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const locationEditMutation = `#graphql
  mutation LocationAddressCodeDerivationEdit($id: ID!, $input: LocationEditInput!) {
    locationEdit(id: $id, input: $input) {
      location {
        id
        name
        address {
          country
          countryCode
          province
          provinceCode
        }
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
  query LocationAddressCodeDerivationRead($id: ID!) {
    location(id: $id) {
      id
      name
      address {
        country
        countryCode
        province
        provinceCode
      }
    }
  }
`;

const locationDeactivateWithDirectiveMutation = `#graphql
  mutation LocationAddressCodeDerivationCleanupDeactivate($locationId: ID!, $idempotencyKey: String!) {
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
  mutation LocationAddressCodeDerivationCleanupDelete($locationId: ID!) {
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

function readAddedLocationId(createCase: CapturedCase): string {
  const data = asRecord(createCase.response.payload.data);
  const locationAdd = asRecord(data?.['locationAdd']);
  const location = asRecord(locationAdd?.['location']);
  const id = location?.['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`locationAdd did not return a location id: ${JSON.stringify(createCase.response.payload)}`);
  }

  const userErrors = asArray(locationAdd?.['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`locationAdd returned userErrors: ${JSON.stringify(userErrors)}`);
  }

  return id;
}

function assertEditSucceeded(editCase: CapturedCase): void {
  const data = asRecord(editCase.response.payload.data);
  const locationEdit = asRecord(data?.['locationEdit']);
  const location = asRecord(locationEdit?.['location']);
  const userErrors = asArray(locationEdit?.['userErrors']);
  if (location === undefined || userErrors.length > 0) {
    throw new Error(`locationEdit did not succeed: ${JSON.stringify(editCase.response.payload)}`);
  }
}

async function cleanupLocation(locationId: string, cleanup: CapturedCase[], uniqueSuffix: string): Promise<void> {
  const locationToken = locationId.split('/').at(-1) ?? 'unknown';
  cleanup.push(
    await runCase('cleanupDeactivate', locationDeactivateWithDirectiveMutation, {
      locationId,
      idempotencyKey: `${scenarioId}-cleanup-${uniqueSuffix}-${locationToken}`,
    }),
  );
  cleanup.push(await runCase('cleanupDelete', locationDeleteMutation, { locationId }));
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const uniqueSuffix = capturedAt.replace(/\D/gu, '').slice(0, 14);
const cleanup: CapturedCase[] = [];
const createdLocationIds: string[] = [];

try {
  const gbAdd = await runCase('gbAdd', locationAddMutation, {
    input: {
      name: `Proxy GB ${uniqueSuffix}`,
      fulfillsOnlineOrders: false,
      address: {
        countryCode: 'GB',
      },
    },
  });
  const gbLocationId = readAddedLocationId(gbAdd);
  createdLocationIds.push(gbLocationId);
  const gbRead = await runCase('gbRead', locationReadQuery, { id: gbLocationId });

  const auAdd = await runCase('auAdd', locationAddMutation, {
    input: {
      name: `Proxy AU ${uniqueSuffix}`,
      fulfillsOnlineOrders: false,
      address: {
        countryCode: 'AU',
        provinceCode: 'NSW',
      },
    },
  });
  const auLocationId = readAddedLocationId(auAdd);
  createdLocationIds.push(auLocationId);
  const auRead = await runCase('auRead', locationReadQuery, { id: auLocationId });

  const caSetupAdd = await runCase('caSetupAdd', locationAddMutation, {
    input: {
      name: `Proxy CA ${uniqueSuffix}`,
      fulfillsOnlineOrders: false,
      address: {
        countryCode: 'CA',
        provinceCode: 'QC',
      },
    },
  });
  const caLocationId = readAddedLocationId(caSetupAdd);
  createdLocationIds.push(caLocationId);

  const caProvinceOnlyEdit = await runCase('caProvinceOnlyEdit', locationEditMutation, {
    id: caLocationId,
    input: {
      address: {
        provinceCode: 'ON',
      },
    },
  });
  assertEditSucceeded(caProvinceOnlyEdit);
  const caProvinceOnlyRead = await runCase('caProvinceOnlyRead', locationReadQuery, { id: caLocationId });

  for (const locationId of [...createdLocationIds].reverse()) {
    await cleanupLocation(locationId, cleanup, uniqueSuffix);
  }

  const capture = {
    capturedAt,
    storeDomain,
    apiVersion,
    cases: {
      gbAdd,
      gbRead,
      auAdd,
      auRead,
      caSetupAdd,
      caProvinceOnlyEdit,
      caProvinceOnlyRead,
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
      await cleanupLocation(locationId, cleanup, `${uniqueSuffix}-error-${index}`);
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
