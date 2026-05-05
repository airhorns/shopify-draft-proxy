/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { createHash } from 'node:crypto';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const scenarioId = 'location-edit-fields-and-state-machine';
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

const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: adminHeaders,
});

type GraphqlVariables = Record<string, unknown>;

type CapturedCase = {
  name: string;
  query: string;
  variables: GraphqlVariables;
  response: ConformanceGraphqlResult;
};

type LocationAddress = {
  address1?: string | null;
  city?: string | null;
  countryCode?: string | null;
  provinceCode?: string | null;
  zip?: string | null;
};

type LocationCatalogNode = {
  id: string;
  isActive?: boolean | null;
  fulfillsOnlineOrders?: boolean | null;
  address?: LocationAddress | null;
};

const locationEditFields = `#graphql
  fragment LocationEditFields on Location {
    id
    name
    fulfillsOnlineOrders
    updatedAt
    address {
      address1
      address2
      city
      country
      countryCode
      provinceCode
      zip
    }
    metafield(namespace: "custom", key: "har661") {
      id
      namespace
      key
      value
      type
    }
    metafields(first: 5, namespace: "custom") {
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
      }
    }
  }
`;

const locationHydrateQuery = `query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }`;

const locationCatalogQuery = `#graphql
  query LocationEditCatalog {
    locations(first: 100) {
      nodes {
        id
        name
        isActive
        fulfillsOnlineOrders
        hasUnfulfilledOrders
        isFulfillmentService
        address {
          address1
          address2
          city
          country
          countryCode
          provinceCode
          zip
        }
      }
    }
  }
`;

const locationAddMutation = `#graphql
  mutation LocationEditFixtureAdd($input: LocationAddInput!) {
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
      }
    }
  }
`;

const locationEditMutation = `#graphql
  ${locationEditFields}

  mutation LocationEditFieldsAndStateMachine($id: ID!, $input: LocationEditInput!) {
    locationEdit(id: $id, input: $input) {
      location {
        ...LocationEditFields
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
  ${locationEditFields}

  query LocationEditRead($id: ID!) {
    location(id: $id) {
      ...LocationEditFields
    }
  }
`;

const locationDeactivateMutation = `#graphql
  mutation LocationEditFixtureDeactivate($locationId: ID!, $idempotencyKey: String!) {
    locationDeactivate(locationId: $locationId) @idempotent(key: $idempotencyKey) {
      location {
        id
        isActive
        fulfillsOnlineOrders
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
  mutation LocationEditFixtureDelete($locationId: ID!) {
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

async function runCase(name: string, query: string, variables: GraphqlVariables): Promise<CapturedCase> {
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

function locationEditUserErrors(editCase: CapturedCase): unknown[] {
  const data = caseData(editCase);
  const locationEdit = asRecord(data?.['locationEdit']);
  return asArray(locationEdit?.['userErrors']);
}

function assertNoLocationEditErrors(label: string, editCase: CapturedCase): void {
  const errors = locationEditUserErrors(editCase);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertLocationEditErrorCode(label: string, editCase: CapturedCase, code: string): void {
  const errors = locationEditUserErrors(editCase);
  if (!errors.some((error) => asRecord(error)?.['code'] === code)) {
    throw new Error(`${label} did not return ${code}: ${JSON.stringify(editCase.response.payload)}`);
  }
}

function fullAddress(location: LocationCatalogNode, fallbackAddress1: string): Required<LocationAddress> {
  const address = location?.address ?? {};
  return {
    address1: address.address1 || fallbackAddress1,
    city: address.city || 'Toronto',
    countryCode: address.countryCode || 'CA',
    provinceCode: address.provinceCode || 'ON',
    zip: address.zip || 'M5T 2C2',
  };
}

async function editFulfillsOnlineOrders(
  location: LocationCatalogNode,
  value: boolean,
  name: string,
): Promise<CapturedCase> {
  return runCase(name, locationEditMutation, {
    id: location.id,
    input: {
      fulfillsOnlineOrders: value,
      address: fullAddress(location, '1 Restore St'),
    },
  });
}

async function cleanupLocation(locationId: string, cleanup: CapturedCase[], uniqueSuffix: string): Promise<void> {
  const disable = await runCase('cleanupDisableOnlineFulfillment', locationEditMutation, {
    id: locationId,
    input: {
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
  cleanup.push(disable);
  const deactivate = await runCase('cleanupDeactivate', locationDeactivateMutation, {
    locationId,
    idempotencyKey: `${scenarioId}-cleanup-deactivate-${uniqueSuffix}-${locationId.split('/').at(-1)}`,
  });
  cleanup.push(deactivate);
  const deleted = await runCase('cleanupDelete', locationDeleteMutation, { locationId });
  cleanup.push(deleted);
}

function catalogLocations(catalogCase: CapturedCase): LocationCatalogNode[] {
  const data = caseData(catalogCase);
  const locations = asRecord(data?.['locations']);
  return asArray(locations?.['nodes']).filter((node): node is LocationCatalogNode => {
    const record = asRecord(node);
    return typeof record?.['id'] === 'string';
  }) as LocationCatalogNode[];
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const uniqueSuffix = capturedAt.replace(/\D/gu, '').slice(0, 14);
const cleanup: CapturedCase[] = [];
const externalRestoreCases: CapturedCase[] = [];
let initialOnlineLocations: LocationCatalogNode[] = [];
let primaryLocationId: string | null = null;
let backupLocationId: string | null = null;

try {
  const initialCatalog = await runCase('initialLocationCatalog', locationCatalogQuery, {});
  initialOnlineLocations = catalogLocations(initialCatalog).filter(
    (location) => location?.isActive === true && location?.fulfillsOnlineOrders === true,
  );

  const primaryCreate = await runCase('primaryLocationAdd', locationAddMutation, {
    input: {
      name: `HAR-661 Primary ${uniqueSuffix}`,
      fulfillsOnlineOrders: true,
      address: {
        address1: '1 Test St',
        city: 'Boston',
        provinceCode: 'MA',
        countryCode: 'US',
        zip: '02110',
      },
    },
  });
  primaryLocationId = readAddedLocationId(primaryCreate);

  const backupCreate = await runCase('backupLocationAdd', locationAddMutation, {
    input: {
      name: `HAR-661 Backup ${uniqueSuffix}`,
      fulfillsOnlineOrders: true,
      address: {
        address1: '2 Test St',
        city: 'Boston',
        provinceCode: 'MA',
        countryCode: 'US',
        zip: '02111',
      },
    },
  });
  backupLocationId = readAddedLocationId(backupCreate);
  if (primaryLocationId === null || backupLocationId === null) {
    throw new Error('locationAdd did not produce both fixture location ids.');
  }

  const primaryHydrate = await runCase('primaryLocationHydrateForProxyCassette', locationHydrateQuery, {
    id: primaryLocationId,
  });
  const backupHydrate = await runCase('backupLocationHydrateForProxyCassette', locationHydrateQuery, {
    id: backupLocationId,
  });

  const editName = await runCase('editName', locationEditMutation, {
    id: primaryLocationId,
    input: { name: `HAR-661 Edited ${uniqueSuffix}` },
  });
  assertNoLocationEditErrors('editName', editName);

  const editAddress = await runCase('editAddress', locationEditMutation, {
    id: primaryLocationId,
    input: {
      address: {
        city: 'Toronto',
        countryCode: 'CA',
        provinceCode: 'ON',
        zip: 'M5T 2C2',
      },
    },
  });
  assertNoLocationEditErrors('editAddress', editAddress);

  const invalidCountry = await runCase('invalidCountryEnum', locationEditMutation, {
    id: primaryLocationId,
    input: {
      address: {
        countryCode: 'XX',
      },
    },
  });

  const metafieldSet = await runCase('metafieldSet', locationEditMutation, {
    id: primaryLocationId,
    input: {
      metafields: [
        {
          namespace: 'custom',
          key: 'har661',
          value: '1',
          type: 'single_line_text_field',
        },
      ],
    },
  });
  assertNoLocationEditErrors('metafieldSet', metafieldSet);

  const readAfterMetafield = await runCase('readAfterMetafield', locationReadQuery, {
    id: primaryLocationId,
  });

  const invalidMetafieldType = await runCase('invalidMetafieldType', locationEditMutation, {
    id: primaryLocationId,
    input: {
      metafields: [
        {
          namespace: 'custom',
          key: 'har661-bad',
          value: '1',
          type: 'not_a_real_type',
        },
      ],
    },
  });
  assertLocationEditErrorCode('invalidMetafieldType', invalidMetafieldType, 'INVALID_TYPE');

  const nonblockingDisable = await runCase('nonblockingDisableOnlineFulfillment', locationEditMutation, {
    id: backupLocationId,
    input: { fulfillsOnlineOrders: false },
  });
  assertNoLocationEditErrors('nonblockingDisableOnlineFulfillment', nonblockingDisable);

  const externalOnlineLocations = catalogLocations(initialCatalog).filter(
    (location) =>
      location?.isActive === true &&
      location?.fulfillsOnlineOrders === true &&
      location?.id !== primaryLocationId &&
      location?.id !== backupLocationId,
  );

  const onlyOnlineSetupDisables: CapturedCase[] = [];
  for (const location of externalOnlineLocations) {
    const disable = await editFulfillsOnlineOrders(location, false, `disableExternalOnline:${location.id}`);
    onlyOnlineSetupDisables.push(disable);
    assertNoLocationEditErrors(`disableExternalOnline:${location.id}`, disable);
  }

  const onlyOnlineDisable = await runCase('onlyOnlineDisableRejected', locationEditMutation, {
    id: primaryLocationId,
    input: { fulfillsOnlineOrders: false },
  });
  assertLocationEditErrorCode(
    'onlyOnlineDisableRejected',
    onlyOnlineDisable,
    'CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT',
  );

  for (const location of externalOnlineLocations) {
    const restore = await editFulfillsOnlineOrders(location, true, `restoreExternalOnline:${location.id}`);
    externalRestoreCases.push(restore);
    assertNoLocationEditErrors(`restoreExternalOnline:${location.id}`, restore);
  }

  await cleanupLocation(backupLocationId, cleanup, uniqueSuffix);
  await cleanupLocation(primaryLocationId, cleanup, uniqueSuffix);

  const capture = {
    capturedAt,
    storeDomain,
    apiVersion,
    setup: {
      initialCatalog,
      primaryCreate,
      backupCreate,
      primaryHydrate,
      backupHydrate,
      onlyOnlineSetupDisables,
      externalRestoreCases,
    },
    cases: {
      editName,
      editAddress,
      invalidCountry,
      metafieldSet,
      readAfterMetafield,
      invalidMetafieldType,
      nonblockingDisable,
      onlyOnlineDisable,
    },
    cleanup,
    upstreamCalls: [
      {
        operationName: 'StorePropertiesLocationHydrate',
        variables: { id: primaryLocationId },
        query: digestDocument(locationHydrateQuery),
        response: {
          status: primaryHydrate.response.status,
          body: primaryHydrate.response.payload,
        },
      },
      {
        operationName: 'StorePropertiesLocationHydrate',
        variables: { id: backupLocationId },
        query: digestDocument(locationHydrateQuery),
        response: {
          status: backupHydrate.response.status,
          body: backupHydrate.response.payload,
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
        primaryLocationId,
        backupLocationId,
        disabledExternalLocations: externalOnlineLocations.length,
      },
      null,
      2,
    ),
  );
} catch (error) {
  for (const location of initialOnlineLocations) {
    if (location?.id && location?.fulfillsOnlineOrders === true) {
      try {
        cleanup.push(await editFulfillsOnlineOrders(location, true, `catchRestoreExternalOnline:${location.id}`));
      } catch (restoreError) {
        console.error(
          JSON.stringify(
            {
              ok: false,
              restoreFailed: location.id,
              error: restoreError instanceof Error ? restoreError.message : String(restoreError),
            },
            null,
            2,
          ),
        );
      }
    }
  }
  for (const locationId of [backupLocationId, primaryLocationId]) {
    if (typeof locationId === 'string' && locationId.length > 0) {
      try {
        await cleanupLocation(locationId, cleanup, uniqueSuffix);
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
  }
  throw error;
}
