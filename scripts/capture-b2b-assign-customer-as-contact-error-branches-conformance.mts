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

const scenarioId = 'assign-customer-as-contact-no-email-invalid-input';
const timestamp = Date.now();
const companyName = `Assign customer error branches ${timestamp}`;
const knownCustomerEmail = `assign-contact-known-${timestamp}@example.com`;
const noEmailPhone = `+1613${String(timestamp).slice(-7)}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({
  adminOrigin,
  apiVersion,
});
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation AssignCustomerAsContactErrorBranchCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        contactsCount { count }
        mainContact { id isMainContact }
        contacts(first: 5) { nodes { id isMainContact } }
        locations(first: 5) { nodes { id name } }
        contactRoles(first: 5) { nodes { id name } }
      }
      userErrors { field message code }
    }
  }
`;

const customerCreateDocument = `#graphql
  mutation AssignCustomerAsContactErrorBranchCustomerCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer { id email defaultPhoneNumber { phoneNumber } }
      userErrors { field message }
    }
  }
`;

const customerDeleteDocument = `#graphql
  mutation AssignCustomerAsContactErrorBranchCustomerDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors { field message }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation AssignCustomerAsContactErrorBranchCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors { field message code }
    }
  }
`;

const assignCustomerDocument = `#graphql
  mutation AssignCustomerAsContactErrorBranchAssign($companyId: ID!, $customerId: ID!) {
    companyAssignCustomerAsContact(companyId: $companyId, customerId: $customerId) {
      companyContact {
        id
        isMainContact
        customer { id email }
        company { id name }
      }
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
    const record = readRecord(current);
    if (!record) return undefined;
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

function assertSuccessful(result: ConformanceGraphqlResult, root: string, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertUserError(result: ConformanceGraphqlResult, root: string, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length === 0) {
    throw new Error(`${label} did not return userErrors: ${JSON.stringify(result, null, 2)}`);
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

async function runExpectedUserError(
  query: string,
  variables: JsonRecord,
  root: string,
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertUserError(result, root, label);
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
let knownCustomerId: string | null = null;
let noEmailCustomerId: string | null = null;
let companyDeleted = false;
let knownCustomerDeleted = false;
let noEmailCustomerDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const companyCreateVariables = {
    input: {
      company: {
        name: companyName,
        externalId: `assign-contact-errors-${timestamp}`,
      },
      companyContact: {
        firstName: 'Assign',
        lastName: 'Primary',
        email: `assign-contact-primary-${timestamp}@example.com`,
      },
      companyLocation: {
        name: `${companyName} HQ`,
        billingAddress: {
          address1: '1 Error Branch Way',
          city: 'Ottawa',
          countryCode: 'CA',
        },
      },
    },
  };
  const companyCreate = await runRequired(
    companyCreateDocument,
    companyCreateVariables,
    'companyCreate',
    'companyCreate setup for assign-customer error branches',
  );
  companyId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'id'],
    'companyCreate setup for assign-customer error branches',
  );

  const knownCustomerCreateVariables = {
    input: {
      email: knownCustomerEmail,
      firstName: 'Assign',
      lastName: 'Known',
      tags: ['assign-contact-error-branches'],
    },
  };
  const knownCustomerCreate = await runRequired(
    customerCreateDocument,
    knownCustomerCreateVariables,
    'customerCreate',
    'customerCreate setup for duplicate assign-customer branch',
  );
  knownCustomerId = readStringAtPath(
    knownCustomerCreate.response,
    ['data', 'customerCreate', 'customer', 'id'],
    'customerCreate setup for duplicate assign-customer branch',
  );

  const unknownCustomerAssignVariables = {
    companyId,
    customerId: 'gid://shopify/Customer/999999999999999',
  };
  const unknownCustomerAssign = await runExpectedUserError(
    assignCustomerDocument,
    unknownCustomerAssignVariables,
    'companyAssignCustomerAsContact',
    'companyAssignCustomerAsContact unknown customer validation',
  );

  const knownCustomerAssignVariables = { companyId, customerId: knownCustomerId };
  const knownCustomerAssign = await runRequired(
    assignCustomerDocument,
    knownCustomerAssignVariables,
    'companyAssignCustomerAsContact',
    'companyAssignCustomerAsContact known customer setup',
  );

  const duplicateCustomerAssign = await runExpectedUserError(
    assignCustomerDocument,
    knownCustomerAssignVariables,
    'companyAssignCustomerAsContact',
    'companyAssignCustomerAsContact duplicate customer validation',
  );

  const noEmailCustomerCreateVariables = {
    input: {
      firstName: 'Assign',
      lastName: 'No Email',
      phone: noEmailPhone,
      tags: ['assign-contact-error-branches'],
    },
  };
  const noEmailCustomerCreate = await runRequired(
    customerCreateDocument,
    noEmailCustomerCreateVariables,
    'customerCreate',
    'customerCreate setup for no-email assign-customer branch',
  );
  noEmailCustomerId = readStringAtPath(
    noEmailCustomerCreate.response,
    ['data', 'customerCreate', 'customer', 'id'],
    'customerCreate setup for no-email assign-customer branch',
  );

  const noEmailCustomerAssignVariables = { companyId, customerId: noEmailCustomerId };
  const noEmailCustomerAssign = await runExpectedUserError(
    assignCustomerDocument,
    noEmailCustomerAssignVariables,
    'companyAssignCustomerAsContact',
    'companyAssignCustomerAsContact no-email customer validation',
  );

  cleanup.companyDelete = await runCleanup(
    companyDeleteDocument,
    { id: companyId },
    'companyDelete',
    'companyDelete cleanup for assign-customer error branches',
  );
  companyDeleted = true;

  cleanup.knownCustomerDelete = await runCleanup(
    customerDeleteDocument,
    { input: { id: knownCustomerId } },
    'customerDelete',
    'known customer cleanup for assign-customer error branches',
  );
  knownCustomerDeleted = true;

  cleanup.noEmailCustomerDelete = await runCleanup(
    customerDeleteDocument,
    { input: { id: noEmailCustomerId } },
    'customerDelete',
    'no-email customer cleanup for assign-customer error branches',
  );
  noEmailCustomerDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    upstreamCalls: [],
    intent: {
      plan: 'Create a disposable B2B company, a normal disposable customer, and a disposable customer with phone but no email; record companyAssignCustomerAsContact unknown-customer, duplicate-customer, and no-email-customer userError branches.',
    },
    companyCreate,
    knownCustomerCreate,
    unknownCustomerAssign,
    knownCustomerAssign,
    duplicateCustomerAssign,
    noEmailCustomerCreate,
    noEmailCustomerAssign,
    cleanup,
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- capture scripts report their output path.
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (companyId && !companyDeleted) {
    cleanup.companyDelete = await runCleanup(
      companyDeleteDocument,
      { id: companyId },
      'companyDelete',
      'companyDelete finally cleanup for assign-customer error branches',
    );
  }
  if (knownCustomerId && !knownCustomerDeleted) {
    cleanup.knownCustomerDelete = await runCleanup(
      customerDeleteDocument,
      { input: { id: knownCustomerId } },
      'customerDelete',
      'known customer finally cleanup for assign-customer error branches',
    );
  }
  if (noEmailCustomerId && !noEmailCustomerDeleted) {
    cleanup.noEmailCustomerDelete = await runCleanup(
      customerDeleteDocument,
      { input: { id: noEmailCustomerId } },
      'customerDelete',
      'no-email customer finally cleanup for assign-customer error branches',
    );
  }
}
