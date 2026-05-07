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

const scenarioId = 'location_assign_address_preserves_id';
const timestamp = Date.now();
const companyName = `B2B assign address preserves id ${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BLocationAssignAddressPreservesIdCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        locations(first: 5) {
          nodes {
            id
            name
            billingAddress { id address1 }
            shippingAddress { id address1 }
          }
        }
      }
      userErrors { field message code }
    }
  }
`;

const locationCreateDocument = `#graphql
  mutation B2BLocationAssignAddressPreservesIdLocationCreate($companyId: ID!, $input: CompanyLocationInput!) {
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
  mutation B2BLocationAssignAddressPreservesIdAssign(
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

const readLocationDocument = `#graphql
  query B2BLocationAssignAddressPreservesIdRead($locationId: ID!) {
    companyLocation(id: $locationId) {
      id
      billingAddress { id address1 }
      shippingAddress { id address1 }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BLocationAssignAddressPreservesIdCompanyDelete($id: ID!) {
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

function assertEqual(actual: string, expected: string, label: string): void {
  if (actual !== expected) {
    throw new Error(`${label}: expected ${expected}, got ${actual}`);
  }
}

function assertNotEqual(left: string, right: string, label: string): void {
  if (left === right) {
    throw new Error(`${label}: expected distinct ids, got ${left}`);
  }
}

function assertIncludes(values: string[], expected: string, label: string): void {
  if (!values.includes(expected)) {
    throw new Error(`${label}: expected ${expected} in ${JSON.stringify(values)}`);
  }
}

let companyId: string | null = null;
let companyDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const companyCreateVariables = {
    input: {
      company: {
        name: companyName,
        note: 'B2B location assign-address preserves CompanyAddress id parity',
        externalId: `b2b-assign-address-preserves-id-${timestamp}`,
      },
      companyLocation: {
        name: `${companyName} first assign`,
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
  const firstAssignLocationId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
    'first assign location',
  );

  const firstBillingAssign = await runRequired(
    assignAddressDocument,
    {
      locationId: firstAssignLocationId,
      address: {
        address1: 'First Billing Anchor',
        countryCode: 'CA',
      },
      addressTypes: ['BILLING'],
    },
    'companyLocationAssignAddress',
    'companyLocationAssignAddress first billing assignment',
  );
  const firstBillingAddressId = readStringAtPath(
    firstBillingAssign.response,
    ['data', 'companyLocationAssignAddress', 'addresses', '0', 'id'],
    'first billing address',
  );

  const updateBillingAssign = await runRequired(
    assignAddressDocument,
    {
      locationId: firstAssignLocationId,
      address: {
        address1: 'Updated Billing Anchor',
        countryCode: 'CA',
      },
      addressTypes: ['BILLING'],
    },
    'companyLocationAssignAddress',
    'companyLocationAssignAddress update billing assignment',
  );
  const updatedBillingAddressId = readStringAtPath(
    updateBillingAssign.response,
    ['data', 'companyLocationAssignAddress', 'addresses', '0', 'id'],
    'updated billing address',
  );
  assertEqual(updatedBillingAddressId, firstBillingAddressId, 'billing update preserved CompanyAddress id');

  const dualLocationCreate = await runRequired(
    locationCreateDocument,
    {
      companyId,
      input: {
        name: `${companyName} dual prior`,
        billingAddress: {
          address1: 'Dual Old Billing',
          countryCode: 'CA',
        },
        shippingAddress: {
          address1: 'Dual Old Shipping',
          countryCode: 'CA',
        },
      },
    },
    'companyLocationCreate',
    'companyLocationCreate dual-address setup',
  );
  const dualLocationId = readStringAtPath(
    dualLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'id'],
    'dual location',
  );
  const dualBillingAddressId = readStringAtPath(
    dualLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'billingAddress', 'id'],
    'dual billing address',
  );
  const dualShippingAddressId = readStringAtPath(
    dualLocationCreate.response,
    ['data', 'companyLocationCreate', 'companyLocation', 'shippingAddress', 'id'],
    'dual shipping address',
  );
  assertNotEqual(dualBillingAddressId, dualShippingAddressId, 'dual setup addresses');

  const dualAssign = await runRequired(
    assignAddressDocument,
    {
      locationId: dualLocationId,
      address: {
        address1: 'Dual Updated',
        countryCode: 'CA',
      },
      addressTypes: ['BILLING', 'SHIPPING'],
    },
    'companyLocationAssignAddress',
    'companyLocationAssignAddress dual update assignment',
  );
  const dualAssignedBillingAddressId = readStringAtPath(
    dualAssign.response,
    ['data', 'companyLocationAssignAddress', 'addresses', '0', 'id'],
    'dual assigned first address',
  );
  const dualAssignedShippingAddressId = readStringAtPath(
    dualAssign.response,
    ['data', 'companyLocationAssignAddress', 'addresses', '1', 'id'],
    'dual assigned second address',
  );
  const dualAssignedAddressIds = [dualAssignedBillingAddressId, dualAssignedShippingAddressId];
  assertIncludes(
    dualAssignedAddressIds,
    dualBillingAddressId,
    'dual assign payload includes prior billing CompanyAddress id',
  );
  assertIncludes(
    dualAssignedAddressIds,
    dualShippingAddressId,
    'dual assign payload includes prior shipping CompanyAddress id',
  );

  const readAfterDualAssign = await runRequired(
    readLocationDocument,
    { locationId: dualLocationId },
    'companyLocation',
    'read after dual assign',
  );
  assertEqual(
    readStringAtPath(
      readAfterDualAssign.response,
      ['data', 'companyLocation', 'billingAddress', 'id'],
      'read dual billing',
    ),
    dualBillingAddressId,
    'dual read billing id',
  );
  assertEqual(
    readStringAtPath(
      readAfterDualAssign.response,
      ['data', 'companyLocation', 'shippingAddress', 'id'],
      'read dual shipping',
    ),
    dualShippingAddressId,
    'dual read shipping id',
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
      plan: 'Create a disposable B2B company, prove first assign creates a CompanyAddress id, prove later assign preserves that id, prove dual billing/shipping update preserves both prior ids, read back the dual location, then delete the company.',
    },
    companyCreate,
    firstBillingAssign,
    updateBillingAssign,
    dualLocationCreate,
    dualAssign,
    readAfterDualAssign,
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
