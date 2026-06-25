import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

/* oxlint-disable no-console -- CLI capture scripts report output path and best-effort cleanup failures. */

type JsonRecord = Record<string, unknown>;
type RecordedOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: JsonRecord;
};

const scenarioId = 'b2b-contact-missing-email-validation';
const timestamp = Date.now();
const setupCompanyName = `B2B contact missing email setup ${timestamp}`;
const nestedCompanyName = `B2B nested contact missing email ${timestamp}`;
const setupExternalId = `missing-email-setup-${timestamp}`;
const nestedExternalId = `missing-email-nested-${timestamp}`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BContactMissingEmailCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        externalId
        contacts(first: 5) {
          nodes {
            id
            title
            locale
            isMainContact
          }
        }
        locations(first: 5) {
          nodes {
            id
            name
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
  mutation B2BContactMissingEmailContactCreate($companyId: ID!, $input: CompanyContactInput!) {
    companyContactCreate(companyId: $companyId, input: $input) {
      companyContact {
        id
        customer {
          id
        }
        company {
          id
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

const companyReadDocument = `#graphql
  query B2BContactMissingEmailCompanyRead($companyId: ID!) {
    company(id: $companyId) {
      id
      name
      externalId
      contacts(first: 5) {
        nodes {
          id
          title
          locale
          isMainContact
        }
      }
      locations(first: 5) {
        nodes {
          id
          name
        }
      }
    }
  }
`;

const companiesSearchDocument = `#graphql
  query B2BContactMissingEmailCompaniesSearch($companyQuery: String!) {
    companies(first: 5, query: $companyQuery) {
      nodes {
        id
        name
        externalId
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BContactMissingEmailCompanyDelete($id: ID!) {
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
    if (Array.isArray(current)) {
      const index = Number(segment);
      if (!Number.isInteger(index)) return undefined;
      current = current[index];
      continue;
    }
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
  expected: { field: string[]; message: string; code: string },
  label: string,
): void {
  assertNoTopLevelErrors(result, label);
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length !== 1) {
    throw new Error(`${label} expected exactly one userError: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const [error] = userErrors;
  const actualField = error['field'];
  const fieldMatches = Array.isArray(actualField) && JSON.stringify(actualField) === JSON.stringify(expected.field);
  if (!fieldMatches || error['message'] !== expected.message || error['code'] !== expected.code) {
    throw new Error(
      `${label} userError mismatch: ${JSON.stringify({ expected, actual: error, payload: result.payload }, null, 2)}`,
    );
  }
}

function assertEmptyNodes(result: ConformanceGraphqlResult, pathSegments: string[], label: string): void {
  assertNoTopLevelErrors(result, label);
  const nodes = readPath(result.payload, pathSegments);
  if (!Array.isArray(nodes) || nodes.length !== 0) {
    throw new Error(`${label} expected empty nodes: ${JSON.stringify(result.payload, null, 2)}`);
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
  expected: { field: string[]; message: string; code: string },
  label: string,
): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertSingleUserError(result, root, expected, label);
  return recordOperation(query, variables, result);
}

async function runRead(query: string, variables: JsonRecord, label: string): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(query, variables);
  assertNoTopLevelErrors(result, label);
  return recordOperation(query, variables, result);
}

async function runCleanup(companyId: string): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(companyDeleteDocument, { id: companyId });
  return recordOperation(companyDeleteDocument, { id: companyId }, result);
}

let setupCompanyId: string | null = null;
let setupCompanyDeleted = false;
const cleanup: Record<string, RecordedOperation> = {};

try {
  const setupCompany = await runRequired(
    companyCreateDocument,
    {
      input: {
        company: {
          name: setupCompanyName,
          externalId: setupExternalId,
        },
      },
    },
    'companyCreate',
    'B2B contact missing email setup companyCreate',
  );

  setupCompanyId = readStringAtPath(
    setupCompany.response,
    ['data', 'companyCreate', 'company', 'id'],
    'B2B contact missing email setup company',
  );

  const standaloneMissingEmailContactCreate = await runValidation(
    contactCreateDocument,
    {
      companyId: setupCompanyId,
      input: {
        firstName: 'Jane',
        lastName: 'Doe',
      },
    },
    'companyContactCreate',
    {
      field: ['input'],
      message: 'Either the attribute email or customer_id must be provided',
      code: 'INVALID',
    },
    'B2B contact missing email standalone companyContactCreate',
  );

  const readAfterStandaloneRejection = await runRead(
    companyReadDocument,
    { companyId: setupCompanyId },
    'B2B contact missing email read after standalone rejection',
  );
  assertEmptyNodes(
    { status: 200, payload: readAfterStandaloneRejection.response },
    ['data', 'company', 'contacts', 'nodes'],
    'B2B contact missing email read after standalone rejection',
  );

  const nestedMissingEmailCompanyCreate = await runValidation(
    companyCreateDocument,
    {
      input: {
        company: {
          name: nestedCompanyName,
          externalId: nestedExternalId,
        },
        companyContact: {
          firstName: 'Jane',
        },
      },
    },
    'companyCreate',
    {
      field: ['input', 'companyContact'],
      message: 'Either the attribute email or customer_id must be provided',
      code: 'INVALID',
    },
    'B2B nested contact missing email companyCreate',
  );

  const nestedReadAfterRejection = await runRead(
    companiesSearchDocument,
    { companyQuery: `name:"${nestedCompanyName}"` },
    'B2B nested contact missing email companies search',
  );
  assertEmptyNodes(
    { status: 200, payload: nestedReadAfterRejection.response },
    ['data', 'companies', 'nodes'],
    'B2B nested contact missing email companies search',
  );

  cleanup[`companyDelete:${setupCompanyId}`] = await runCleanup(setupCompanyId);
  setupCompanyDeleted = true;

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      plan: 'Create a disposable B2B company, record standalone companyContactCreate missing-email rejection and unchanged contact readback, then record nested companyCreate companyContact missing-email rejection plus empty company search by name.',
    },
    setupCompany,
    standaloneMissingEmailContactCreate,
    readAfterStandaloneRejection,
    nestedMissingEmailCompanyCreate,
    nestedReadAfterRejection,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (setupCompanyId && !setupCompanyDeleted) {
    try {
      cleanup[`companyDelete:${setupCompanyId}`] = await runCleanup(setupCompanyId);
    } catch (error) {
      console.error(`Cleanup failed for ${setupCompanyId}:`, error);
    }
  }
}
