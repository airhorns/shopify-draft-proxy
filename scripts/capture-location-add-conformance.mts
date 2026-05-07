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

const scenarioId = 'location-add-validation-and-defaults';
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
  mutation LocationAddDefaults($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        fulfillsOnlineOrders
        address {
          address1
          city
          countryCode
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

const locationReadQuery = `#graphql
  query LocationAddRead($id: ID!) {
    location(id: $id) {
      id
      name
      fulfillsOnlineOrders
      address {
        address1
        city
        countryCode
        zip
      }
    }
  }
`;

const blankNameMutation = `#graphql
  mutation LocationAddBlankName($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const missingAddressMutation = `#graphql
  mutation LocationAddMissingAddress {
    locationAdd(input: { name: "Bad" }) {
      location {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const missingCountryCodeMutation = `#graphql
  mutation LocationAddMissingCountryCode($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const inlineMissingCountryCodeMutation = `#graphql
  mutation LocationAddInlineMissingCountryCode {
    locationAdd(input: { name: "Bad", address: { address1: "1 Infinite Loop" } }) {
      location {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const invalidCountryCodeMutation = `#graphql
  mutation LocationAddInvalidCountryCode($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const capabilitiesVariableMutation = `#graphql
  mutation LocationAddCapabilitiesVariable($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const inlineCapabilitiesMutation = `#graphql
  mutation LocationAddInlineCapabilities {
    locationAdd(input: { name: "Cap", address: { countryCode: CA }, capabilitiesToAdd: [PICKUP] }) {
      location {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const locationDeactivateWithDirectiveMutation = `#graphql
  mutation LocationAddCleanupDeactivate($locationId: ID!, $idempotencyKey: String!) {
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
  mutation LocationAddCleanupDelete($locationId: ID!) {
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

async function cleanupLocation(
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

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const uniqueSuffix = capturedAt.replace(/\D/gu, '').slice(0, 14);
const cleanup: CaptureCase[] = [];
const createdLocationIds: string[] = [];

try {
  const defaultAdd = await runCase(client, 'defaultAdd', locationAddMutation, {
    input: {
      name: `Proxy Main ${uniqueSuffix}`,
      address: {
        address1: '1 Spadina',
        city: 'Toronto',
        countryCode: 'CA',
        zip: 'M5T 2C2',
      },
    },
  });
  const defaultLocationId = readAddedLocationId(defaultAdd);
  createdLocationIds.push(defaultLocationId);
  const defaultRead = await runCase(client, 'defaultRead', locationReadQuery, {
    id: defaultLocationId,
  });

  const explicitFalseAdd = await runCase(client, 'explicitFalseAdd', locationAddMutation, {
    input: {
      name: `Proxy Sub ${uniqueSuffix}`,
      fulfillsOnlineOrders: false,
      address: {
        address1: '2 Spadina',
        city: 'Toronto',
        countryCode: 'CA',
        zip: 'M5T 2C3',
      },
    },
  });
  const explicitFalseLocationId = readAddedLocationId(explicitFalseAdd);
  createdLocationIds.push(explicitFalseLocationId);
  const explicitFalseRead = await runCase(client, 'explicitFalseRead', locationReadQuery, {
    id: explicitFalseLocationId,
  });

  const blankName = await runCase(client, 'blankName', blankNameMutation, {
    input: {
      name: '',
      address: {
        countryCode: 'CA',
      },
    },
  });
  const missingAddress = await runCase(client, 'missingAddress', missingAddressMutation);
  const missingCountryCode = await runCase(client, 'missingCountryCode', missingCountryCodeMutation, {
    input: {
      name: 'Missing Country',
      address: {
        address1: '1 Infinite Loop',
      },
    },
  });
  const inlineMissingCountryCode = await runCase(client, 'inlineMissingCountryCode', inlineMissingCountryCodeMutation);
  const invalidCountryCode = await runCase(client, 'invalidCountryCode', invalidCountryCodeMutation, {
    input: {
      name: 'Invalid Country',
      address: {
        countryCode: 'QQ',
      },
    },
  });
  const capabilitiesVariable = await runCase(client, 'capabilitiesVariable', capabilitiesVariableMutation, {
    input: {
      name: 'Capabilities Variable',
      address: {
        countryCode: 'CA',
      },
      capabilities: {
        pickupEnabled: true,
      },
    },
  });
  const inlineCapabilities = await runCase(client, 'inlineCapabilities', inlineCapabilitiesMutation);

  await cleanupLocation(defaultLocationId, cleanup, 'default', uniqueSuffix);
  await cleanupLocation(explicitFalseLocationId, cleanup, 'explicitFalse', uniqueSuffix);

  const capture = {
    capturedAt,
    storeDomain,
    apiVersion,
    cases: {
      defaultAdd,
      defaultRead,
      explicitFalseAdd,
      explicitFalseRead,
      blankName,
      missingAddress,
      missingCountryCode,
      inlineMissingCountryCode,
      invalidCountryCode,
      capabilitiesVariable,
      inlineCapabilities,
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
      await cleanupLocation(locationId, cleanup, `error-${index}`, uniqueSuffix);
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
