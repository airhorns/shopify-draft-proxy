import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type RecordedOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: JsonRecord;
};

const scenarioId = 'b2b-location-address-management';
const timestamp = Date.now();
const companyName = `HAR-623 B2B address ${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BLocationAddressManagementCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        mainContact {
          id
          roleAssignments(first: 5) {
            nodes {
              id
              companyLocation { id }
            }
          }
        }
        contacts(first: 5) {
          nodes {
            id
            roleAssignments(first: 5) {
              nodes {
                id
                companyLocation { id }
              }
            }
          }
        }
        locations(first: 5) { nodes { id name } }
        contactRoles(first: 5) { nodes { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const locationCreateDocument = `#graphql
  mutation B2BLocationAddressManagementLocationCreate($companyId: ID!, $input: CompanyLocationInput!) {
    companyLocationCreate(companyId: $companyId, input: $input) {
      companyLocation {
        id
        name
        billingAddress { id address1 }
        shippingAddress { id address1 }
        company { id name }
      }
      userErrors { field message code }
    }
  }
`;

const assignAddressDocument = `#graphql
  mutation B2BLocationAddressManagementAssignAddress(
    $locationId: ID!
    $address: CompanyAddressInput!
    $addressTypes: [CompanyAddressType!]!
  ) {
    companyLocationAssignAddress(locationId: $locationId, address: $address, addressTypes: $addressTypes) {
      addresses { id address1 }
      userErrors { field message code }
    }
  }
`;

const addressDeleteDocument = `#graphql
  mutation B2BLocationAddressManagementAddressDelete($addressId: ID!) {
    companyAddressDelete(addressId: $addressId) {
      deletedAddressId
      userErrors { field message code }
    }
  }
`;

const readAfterSharedAddressDeleteDocument = `#graphql
  query B2BLocationAddressManagementReadSharedDelete($companyLocationId: ID!) {
    companyLocation(id: $companyLocationId) {
      id
      billingAddress { id address1 }
      shippingAddress { id address1 }
    }
  }
`;

const locationDeleteDocument = `#graphql
  mutation B2BLocationAddressManagementLocationDelete($companyLocationId: ID!) {
    companyLocationDelete(companyLocationId: $companyLocationId) {
      deletedCompanyLocationId
      userErrors { field message code }
    }
  }
`;

const readAfterLocationDeleteDocument = `#graphql
  query B2BLocationAddressManagementReadLocationDelete($companyContactId: ID!, $companyLocationId: ID!) {
    companyContact(id: $companyContactId) {
      id
      roleAssignments(first: 5) {
        nodes {
          id
          companyLocation { id }
        }
      }
    }
    companyLocation(id: $companyLocationId) { id }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BLocationAddressManagementCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors { field message code }
    }
  }
`;

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      const index = Number(segment);
      if (!Number.isInteger(index) || index < 0) {
        return undefined;
      }
      current = current[index];
      continue;
    }
    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[segment];
  }
  return current;
}

function readStringAtPath(value: unknown, pathSegments: string[], label: string): string {
  const pathValue = readPath(value, pathSegments);
  if (typeof pathValue !== 'string' || pathValue.length === 0) {
    throw new Error(`${label} did not return a string at ${pathSegments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return pathValue;
}

function readUserErrors(payload: unknown, root: string): unknown[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value : [];
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertSuccessful(result: ConformanceGraphqlResult, root: string, label: string): void {
  assertNoTopLevelErrors(result, label);
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertUserErrorCode(result: ConformanceGraphqlResult, root: string, code: string, label: string): void {
  assertNoTopLevelErrors(result, label);
  const userErrors = readUserErrors(result.payload, root);
  const matched = userErrors.some((error) => readRecord(error)?.['code'] === code);
  if (!matched) {
    throw new Error(`${label} did not return ${code}: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function recordOperation(query: string, variables: JsonRecord, result: ConformanceGraphqlResult): RecordedOperation {
  return {
    request: { query, variables },
    response: {
      status: result.status,
      ...result.payload,
    },
  };
}

async function runRequired(
  query: string,
  variables: JsonRecord,
  root: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertSuccessful(result, root, label);
  return recordOperation(query, variables, result);
}

async function runValidation(
  query: string,
  variables: JsonRecord,
  root: string,
  code: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertUserErrorCode(result, root, code, label);
  return recordOperation(query, variables, result);
}

async function runCleanup(
  query: string,
  variables: JsonRecord,
  root: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status >= 200 && result.status < 300 && !result.payload.errors) {
    return recordOperation(query, variables, result);
  }
  return recordOperation(query, variables, {
    status: result.status,
    payload: {
      ...result.payload,
      cleanupWarning: `${label} cleanup failed before userErrors could be inspected.`,
      cleanupRoot: root,
    },
  });
}

let companyId: string | null = null;
let companyDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const companyCreateVariables = {
    input: {
      company: {
        name: companyName,
        note: 'HAR-623 B2B location address management parity',
        externalId: `har-623-address-${timestamp}`,
      },
      companyContact: {
        firstName: 'Har',
        lastName: 'Address',
        email: `har-623-address-${timestamp}@example.com`,
        title: 'Buyer',
      },
      companyLocation: {
        name: `${companyName} HQ`,
      },
    },
  };
  const companyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables,
    'companyCreate',
    'companyCreate setup',
  );
  companyId = readStringAtPath(companyCreate.response, ['data', 'companyCreate', 'company', 'id'], 'companyCreate');
  const contactId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'mainContact', 'id'],
    'companyCreate main contact',
  );
  const mainLocationId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
    'companyCreate main location',
  );

  const locationNameFallback = await runRequired(
    locationCreateDocument,
    {
      companyId,
      input: {
        phone: '+16135550623',
        shippingAddress: {
          address1: '123 Main',
          countryCode: 'CA',
        },
      },
    },
    'companyLocationCreate',
    'companyLocationCreate location name fallback',
  );
  const fallbackLocationId = readStringAtPath(
    locationNameFallback.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'location name fallback',
  );

  const duplicateAddressTypes = await runValidation(
    assignAddressDocument,
    {
      locationId: fallbackLocationId,
      address: {
        address1: 'Duplicate Type Way',
      },
      addressTypes: ['BILLING', 'BILLING'],
    },
    'companyLocationAssignAddress',
    'INVALID_INPUT',
    'companyLocationAssignAddress duplicate addressTypes',
  );

  const sharedLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${companyName} shared address`,
        billingSameAsShipping: true,
        shippingAddress: {
          address1: '623 Shared Anchor',
          countryCode: 'CA',
        },
      },
    },
    'companyLocationCreate',
    'companyLocationCreate shared address',
  );
  const sharedLocationId = readStringAtPath(
    sharedLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'shared location create',
  );
  const sharedAddressId = readStringAtPath(
    sharedLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'shippingAddress', 'id'],
    'shared location shipping address',
  );

  const sharedAddressDelete = await runRequired(
    addressDeleteDocument,
    { addressId: sharedAddressId },
    'companyAddressDelete',
    'companyAddressDelete shared address',
  );

  const readAfterSharedAddressDelete = await runRequired(
    readAfterSharedAddressDeleteDocument,
    { companyLocationId: sharedLocationId },
    'companyLocation',
    'read after shared address delete',
  );

  const locationDelete = await runRequired(
    locationDeleteDocument,
    { companyLocationId: mainLocationId },
    'companyLocationDelete',
    'companyLocationDelete cascade',
  );

  const readAfterLocationDelete = await runRequired(
    readAfterLocationDeleteDocument,
    { companyContactId: contactId, companyLocationId: mainLocationId },
    'companyContact',
    'read after location delete cascade',
  );

  const companyDelete = await runRequired(
    companyDeleteDocument,
    { id: companyId },
    'companyDelete',
    'companyDelete cleanup',
  );
  companyDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      ticket: 'HAR-623',
      plan: 'Create a disposable B2B company, capture location name fallback, duplicate address-type validation, shared-address delete readback, location-delete role-assignment cascade, and delete the company.',
    },
    companyCreate,
    locationNameFallback,
    duplicateAddressTypes,
    sharedLocationCreate,
    sharedAddressDelete,
    readAfterSharedAddressDelete,
    locationDelete,
    readAfterLocationDelete,
    companyDelete,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- capture scripts report their output path.
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (companyId && !companyDeleted) {
    cleanup['companyDelete'] = await runCleanup(
      companyDeleteDocument,
      { id: companyId },
      'companyDelete',
      'companyDelete finally cleanup',
    );
  }
}
