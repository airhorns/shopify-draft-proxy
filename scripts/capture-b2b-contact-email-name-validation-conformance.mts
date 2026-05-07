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

const scenarioId = 'b2b-contact-email-name-validation';
const timestamp = Date.now();
const setupEmail = `b2b-contact-validation-${timestamp}@example.com`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const schemaProbeDocument = `#graphql
  query B2BContactEmailNameValidationSchema {
    userErrorType: __type(name: "BusinessCustomerUserError") {
      fields { name }
    }
    contactInputType: __type(name: "CompanyContactInput") {
      inputFields { name }
    }
  }
`;

const companyCreateDocument = `#graphql
  mutation B2BContactEmailNameValidationCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        mainContact {
          id
          customer {
            id
            email
            firstName
            lastName
          }
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

const contactCreateDocument = `#graphql
  mutation B2BContactEmailNameValidationContactCreate($companyId: ID!, $input: CompanyContactInput!) {
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
  mutation B2BContactEmailNameValidationContactUpdate($companyContactId: ID!, $input: CompanyContactInput!) {
    companyContactUpdate(companyContactId: $companyContactId, input: $input) {
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

const companyDeleteDocument = `#graphql
  mutation B2BContactEmailNameValidationCompanyDelete($id: ID!) {
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

function assertSingleUserError(
  result: ConformanceGraphqlResult,
  root: string,
  expected: { field: string[] | null; message: string; code: string },
  label: string,
): void {
  assertNoTopLevelErrors(result, label);
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length !== 1) {
    throw new Error(`${label} expected exactly one userError: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const [error] = userErrors;
  const actualField = error['field'];
  const expectedField = expected.field;
  const fieldMatches =
    expectedField === null
      ? actualField === null
      : Array.isArray(actualField) && JSON.stringify(actualField) === JSON.stringify(expectedField);
  if (!fieldMatches || error['message'] !== expected.message || error['code'] !== expected.code) {
    throw new Error(
      `${label} userError mismatch: ${JSON.stringify({ expected, actual: error, payload: result.payload }, null, 2)}`,
    );
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

async function runValidation(
  query: string,
  variables: JsonRecord,
  root: string,
  expected: { field: string[] | null; message: string; code: string },
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertSingleUserError(result, root, expected, label);
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
  assertNoTopLevelErrors(schemaProbeResult, 'B2B contact validation schema probe');
  const schemaProbe = recordOperation(schemaProbeDocument, {}, schemaProbeResult);

  const setupCompany = await runRequired(
    companyCreateDocument,
    {
      input: {
        company: {
          name: `B2B contact validation ${timestamp}`,
        },
        companyContact: {
          email: setupEmail,
          firstName: 'Safe',
          lastName: 'Buyer',
        },
        companyLocation: {
          name: 'B2B contact validation HQ',
        },
      },
    },
    'companyCreate',
    'B2B contact validation setup company',
  );
  companyId = readStringAtPath(
    setupCompany.response,
    ['data', 'companyCreate', 'company', 'id'],
    'B2B contact validation setup company',
  );
  const companyContactId = readStringAtPath(
    setupCompany.response,
    ['data', 'companyCreate', 'company', 'mainContact', 'id'],
    'B2B contact validation setup company',
  );

  const invalidEmailCreate = await runValidation(
    contactCreateDocument,
    {
      companyId,
      input: {
        email: 'not-an-email',
        firstName: 'Invalid',
        lastName: 'Email',
      },
    },
    'companyContactCreate',
    { field: ['input', 'email'], message: 'Email is invalid', code: 'INVALID' },
    'B2B contact create invalid email',
  );

  const htmlNameCreate = await runValidation(
    contactCreateDocument,
    {
      companyId,
      input: {
        email: `b2b-contact-html-name-${timestamp}@example.com`,
        firstName: '<b>Jane</b>',
        lastName: 'Buyer',
      },
    },
    'companyContactCreate',
    { field: ['input'], message: 'Invalid input.', code: 'INVALID_INPUT' },
    'B2B contact create HTML name',
  );

  const invalidEmailUpdate = await runValidation(
    contactUpdateDocument,
    {
      companyContactId,
      input: {
        email: 'stillbad@',
      },
    },
    'companyContactUpdate',
    { field: ['input', 'email'], message: 'Email address is invalid', code: 'INVALID' },
    'B2B contact update invalid email',
  );

  const nestedInvalidEmailCompanyCreate = await runValidation(
    companyCreateDocument,
    {
      input: {
        company: {
          name: `B2B nested contact validation ${timestamp}`,
        },
        companyContact: {
          email: 'not-an-email',
          firstName: 'Nested',
          lastName: 'Buyer',
        },
        companyLocation: {
          name: 'B2B nested contact validation HQ',
        },
      },
    },
    'companyCreate',
    { field: ['input', 'companyContact', 'email'], message: 'Email is invalid', code: 'INVALID' },
    'B2B nested contact invalid email',
  );

  cleanup['companyDelete'] = await runCleanup(
    companyDeleteDocument,
    { id: companyId },
    'companyDelete',
    'B2B contact validation company cleanup',
  );
  companyDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      plan: 'Create a disposable B2B company and record public Admin validation payloads for contact email format, HTML contact names, update email format, and nested companyCreate contact email rejection.',
    },
    schemaProbe,
    setupCompany,
    invalidEmailCreate,
    htmlNameCreate,
    invalidEmailUpdate,
    nestedInvalidEmailCompanyCreate,
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
      'B2B contact validation company finally cleanup',
    );
  }
}
