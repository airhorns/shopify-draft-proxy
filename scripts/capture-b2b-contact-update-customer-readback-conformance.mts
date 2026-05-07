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

const scenarioId = 'b2b-contact-update-customer-readback';
const timestamp = Date.now();
const companyName = `B2B contact customer readback ${timestamp}`;
const originalEmail = `b2b-contact-customer-old-${timestamp}@example.com`;
const updatedEmail = `b2b-contact-customer-new-${timestamp}@example.com`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const schemaProbeDocument = `#graphql
  query B2BContactUpdateCustomerReadbackSchema {
    contactType: __type(name: "CompanyContact") {
      fields { name }
    }
    customerType: __type(name: "Customer") {
      fields { name }
    }
    contactInputType: __type(name: "CompanyContactInput") {
      inputFields { name }
    }
  }
`;

const companyCreateDocument = `#graphql
  mutation B2BContactUpdateCustomerReadbackCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const contactCreateDocument = `#graphql
  mutation B2BContactUpdateCustomerReadbackContactCreate($companyId: ID!, $input: CompanyContactInput!) {
    companyContactCreate(companyId: $companyId, input: $input) {
      companyContact {
        id
        customer {
          id
          email
          firstName
          lastName
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

const contactUpdateDocument = `#graphql
  mutation B2BContactUpdateCustomerReadbackContactUpdate($companyContactId: ID!, $input: CompanyContactInput!) {
    companyContactUpdate(companyContactId: $companyContactId, input: $input) {
      companyContact {
        id
        customer {
          id
          email
          firstName
          lastName
          phone
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

const contactReadDocument = `#graphql
  query B2BContactUpdateCustomerReadbackContactRead($companyContactId: ID!) {
    companyContact(id: $companyContactId) {
      id
      customer {
        id
        email
        firstName
        lastName
        phone
      }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BContactUpdateCustomerReadbackCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors {
        field
        message
        code
      }
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

function readUserErrors(payload: unknown, root: string): JsonRecord[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value.filter((item): item is JsonRecord => readRecord(item) !== null) : [];
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

function assertCustomerValues(payload: JsonRecord, rootPath: string[], expected: JsonRecord, label: string): void {
  const contact = readRecord(readPath(payload, rootPath));
  if (!contact) {
    throw new Error(`${label} did not return a contact object: ${JSON.stringify(payload, null, 2)}`);
  }
  const customer = readRecord(contact['customer']);
  if (!customer) {
    throw new Error(`${label} did not return a customer object: ${JSON.stringify(payload, null, 2)}`);
  }
  for (const key of Object.keys(expected)) {
    if (customer[key] !== expected[key]) {
      throw new Error(
        `${label} customer ${key} mismatch: ${JSON.stringify({ expected: expected[key], customer: customer[key], payload }, null, 2)}`,
      );
    }
  }
}

function recordOperation(query: string, variables: JsonRecord, result: ConformanceGraphqlResult): RecordedOperation {
  return {
    request: {
      query,
      variables,
    },
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

async function runRead(query: string, variables: JsonRecord, label: string): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertNoTopLevelErrors(result, label);
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
  const schemaProbeResult = await runGraphqlRequest(schemaProbeDocument, {});
  assertNoTopLevelErrors(schemaProbeResult, 'B2B contact customer readback schema probe');
  const schemaProbe = recordOperation(schemaProbeDocument, {}, schemaProbeResult);

  const companyCreate = await runRequired(
    companyCreateDocument,
    {
      input: {
        company: {
          name: companyName,
        },
        companyLocation: {
          name: `${companyName} HQ`,
        },
      },
    },
    'companyCreate',
    'B2B contact customer readback company create',
  );
  companyId = readStringAtPath(
    companyCreate.response,
    ['data', 'companyCreate', 'company', 'id'],
    'B2B contact customer readback company create',
  );

  const contactCreate = await runRequired(
    contactCreateDocument,
    {
      companyId,
      input: {
        email: originalEmail,
        firstName: 'Old',
        lastName: 'Buyer',
        phone: '(415) 555-0100',
      },
    },
    'companyContactCreate',
    'B2B contact customer readback contact create',
  );
  assertCustomerValues(
    contactCreate.response,
    ['data', 'companyContactCreate', 'companyContact'],
    {
      email: originalEmail,
      firstName: 'Old',
      lastName: 'Buyer',
    },
    'B2B contact customer readback contact create',
  );
  const companyContactId = readStringAtPath(
    contactCreate.response,
    ['data', 'companyContactCreate', 'companyContact', 'id'],
    'B2B contact customer readback contact create',
  );

  const contactUpdate = await runRequired(
    contactUpdateDocument,
    {
      companyContactId,
      input: {
        email: updatedEmail,
        firstName: 'New',
        lastName: 'Name',
        phone: '(650) 555-0101',
      },
    },
    'companyContactUpdate',
    'B2B contact customer readback contact update',
  );
  assertCustomerValues(
    contactUpdate.response,
    ['data', 'companyContactUpdate', 'companyContact'],
    {
      email: updatedEmail,
      firstName: 'New',
      lastName: 'Name',
      phone: '+16505550101',
    },
    'B2B contact customer readback contact update',
  );

  const contactReadAfterUpdate = await runRead(
    contactReadDocument,
    { companyContactId },
    'B2B contact customer readback read after update',
  );
  assertCustomerValues(
    contactReadAfterUpdate.response,
    ['data', 'companyContact'],
    {
      email: updatedEmail,
      firstName: 'New',
      lastName: 'Name',
      phone: '+16505550101',
    },
    'B2B contact customer readback read after update',
  );

  cleanup['companyDelete'] = await runCleanup(
    companyDeleteDocument,
    { id: companyId },
    'companyDelete',
    'B2B contact customer readback company cleanup',
  );
  companyDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      plan: 'Create a disposable B2B company and contact, update firstName/lastName/email/phone, verify the linked customer subobject changes in the mutation response and downstream companyContact readback, then delete the company.',
    },
    schemaProbe,
    companyCreate,
    contactCreate,
    contactUpdate,
    contactReadAfterUpdate,
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
      'B2B contact customer readback company finally cleanup',
    );
  }
}
