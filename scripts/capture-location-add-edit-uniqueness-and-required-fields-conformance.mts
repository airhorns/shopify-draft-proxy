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

const scenarioId = 'location-add-edit-uniqueness-and-required-fields';
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
  mutation LocationAddEditValidationAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        isActive
        fulfillsOnlineOrders
        address {
          address1
          city
          countryCode
          provinceCode
          zip
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
  mutation LocationAddEditValidationEdit($id: ID!, $input: LocationEditInput!) {
    locationEdit(id: $id, input: $input) {
      location {
        id
        name
        isActive
        fulfillsOnlineOrders
        address {
          address1
          city
          countryCode
          provinceCode
          zip
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

const locationDeactivateMutation = `#graphql
  mutation LocationAddEditValidationCleanupDeactivate($locationId: ID!, $idempotencyKey: String!) {
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
  mutation LocationAddEditValidationCleanupDelete($locationId: ID!) {
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

function caseData(capturedCase: CapturedCase): Record<string, unknown> | undefined {
  return asRecord(capturedCase.response.payload.data);
}

function readAddedLocationId(createCase: CapturedCase): string {
  const data = caseData(createCase);
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

function maybeReadAddedLocationId(createCase: CapturedCase): string | null {
  const data = caseData(createCase);
  const locationAdd = asRecord(data?.['locationAdd']);
  const location = asRecord(locationAdd?.['location']);
  const id = location?.['id'];
  return typeof id === 'string' && id.length > 0 ? id : null;
}

function userErrors(capturedCase: CapturedCase, payloadName: 'locationAdd' | 'locationEdit'): unknown[] {
  const data = caseData(capturedCase);
  const payload = asRecord(data?.[payloadName]);
  return asArray(payload?.['userErrors']);
}

function assertUserErrorCode(
  label: string,
  capturedCase: CapturedCase,
  payloadName: 'locationAdd' | 'locationEdit',
  code: string,
): void {
  if (!userErrors(capturedCase, payloadName).some((error) => asRecord(error)?.['code'] === code)) {
    throw new Error(`${label} did not return ${code}: ${JSON.stringify(capturedCase.response.payload)}`);
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
      idempotencyKey: `${scenarioId}-cleanup-${uniqueSuffix}-${locationId.split('/').at(-1)}`,
    }),
  );
  cleanup.push(await runCase('cleanupDelete', locationDeleteMutation, { locationId }));
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const uniqueSuffix = capturedAt.replace(/\D/gu, '').slice(0, 14);
const cleanup: CapturedCase[] = [];
const createdLocationIds: string[] = [];
const longName = 'N'.repeat(101);
const longAddress = 'A'.repeat(256);

try {
  const primaryCreate = await runCase('primaryCreate', locationAddMutation, {
    input: {
      name: `Proxy validation primary ${uniqueSuffix}`,
      fulfillsOnlineOrders: false,
      address: completeAddress('1 Test St', '02110'),
    },
  });
  const primaryLocationId = readAddedLocationId(primaryCreate);
  createdLocationIds.push(primaryLocationId);

  const backupCreate = await runCase('backupCreate', locationAddMutation, {
    input: {
      name: `Proxy validation backup ${uniqueSuffix}`,
      fulfillsOnlineOrders: false,
      address: completeAddress('2 Test St', '02111'),
    },
  });
  const backupLocationId = readAddedLocationId(backupCreate);
  createdLocationIds.push(backupLocationId);

  const duplicateAdd = await runCase('duplicateAdd', locationAddMutation, {
    input: {
      name: `Proxy validation primary ${uniqueSuffix}`,
      fulfillsOnlineOrders: false,
      address: completeAddress('3 Test St', '02112'),
    },
  });
  assertUserErrorCode('duplicateAdd', duplicateAdd, 'locationAdd', 'TAKEN');

  const duplicateEdit = await runCase('duplicateEdit', locationEditMutation, {
    id: backupLocationId,
    input: {
      name: `Proxy validation primary ${uniqueSuffix}`,
    },
  });
  assertUserErrorCode('duplicateEdit', duplicateEdit, 'locationEdit', 'TAKEN');

  const missingRequiredAdd = await runCase('missingRequiredAdd', locationAddMutation, {
    input: {
      name: `Proxy validation missing ${uniqueSuffix}`,
      address: {
        countryCode: 'US',
      },
    },
  });
  const missingRequiredLocationId = maybeReadAddedLocationId(missingRequiredAdd);
  if (missingRequiredLocationId !== null) {
    createdLocationIds.push(missingRequiredLocationId);
  }

  const blankZipAdd = await runCase('blankZipAdd', locationAddMutation, {
    input: {
      name: `Proxy validation blank zip ${uniqueSuffix}`,
      address: {
        ...completeAddress('4 Test St', ''),
      },
    },
  });
  const blankZipLocationId = maybeReadAddedLocationId(blankZipAdd);
  if (blankZipLocationId !== null) {
    createdLocationIds.push(blankZipLocationId);
  }

  const tooLongNameAdd = await runCase('tooLongNameAdd', locationAddMutation, {
    input: {
      name: longName,
      fulfillsOnlineOrders: false,
      address: completeAddress('5 Test St', '02113'),
    },
  });
  assertUserErrorCode('tooLongNameAdd', tooLongNameAdd, 'locationAdd', 'TOO_LONG');

  const tooLongAddressAdd = await runCase('tooLongAddressAdd', locationAddMutation, {
    input: {
      name: `Proxy validation long address ${uniqueSuffix}`,
      fulfillsOnlineOrders: false,
      address: completeAddress(longAddress, '02114'),
    },
  });
  assertUserErrorCode('tooLongAddressAdd', tooLongAddressAdd, 'locationAdd', 'TOO_LONG');

  const tooLongZipAdd = await runCase('tooLongZipAdd', locationAddMutation, {
    input: {
      name: `Proxy validation long zip ${uniqueSuffix}`,
      fulfillsOnlineOrders: false,
      address: completeAddress('5 Test St', longAddress),
    },
  });
  assertUserErrorCode('tooLongZipAdd', tooLongZipAdd, 'locationAdd', 'TOO_LONG');

  const tooLongNameEdit = await runCase('tooLongNameEdit', locationEditMutation, {
    id: backupLocationId,
    input: {
      name: longName,
    },
  });
  assertUserErrorCode('tooLongNameEdit', tooLongNameEdit, 'locationEdit', 'TOO_LONG');

  const tooLongCityEdit = await runCase('tooLongCityEdit', locationEditMutation, {
    id: backupLocationId,
    input: {
      address: {
        city: longAddress,
      },
    },
  });
  assertUserErrorCode('tooLongCityEdit', tooLongCityEdit, 'locationEdit', 'TOO_LONG');

  const tooLongZipEdit = await runCase('tooLongZipEdit', locationEditMutation, {
    id: backupLocationId,
    input: {
      address: {
        zip: longAddress,
      },
    },
  });
  assertUserErrorCode('tooLongZipEdit', tooLongZipEdit, 'locationEdit', 'TOO_LONG');

  const invalidUsZipAdd = await runCase('invalidUsZipAdd', locationAddMutation, {
    input: {
      name: `Proxy validation invalid zip ${uniqueSuffix}`,
      fulfillsOnlineOrders: false,
      address: completeAddress('6 Test St', 'not-a-zip'),
    },
  });
  const invalidUsZipLocationId = maybeReadAddedLocationId(invalidUsZipAdd);
  if (invalidUsZipLocationId !== null) {
    createdLocationIds.push(invalidUsZipLocationId);
  }

  for (const locationId of [...createdLocationIds].reverse()) {
    await cleanupLocation(locationId, cleanup, uniqueSuffix);
  }

  const capture = {
    capturedAt,
    storeDomain,
    apiVersion,
    setup: {
      primaryCreate,
      backupCreate,
    },
    cases: {
      duplicateAdd,
      duplicateEdit,
      missingRequiredAdd,
      blankZipAdd,
      tooLongNameAdd,
      tooLongAddressAdd,
      tooLongZipAdd,
      tooLongNameEdit,
      tooLongCityEdit,
      tooLongZipEdit,
      invalidUsZipAdd,
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
      await cleanupLocation(locationId, cleanup, `error-${index}-${uniqueSuffix}`);
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
