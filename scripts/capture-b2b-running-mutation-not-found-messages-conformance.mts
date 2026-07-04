/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
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

type ExpectedUserError = {
  field: string[];
  message: string;
  code: string;
};

const scenarioId = 'b2b-running-mutation-not-found-messages';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixturePath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
const documentPath = 'config/parity-requests/b2b/b2b-running-mutation-not-found-messages.graphql';

const variables: JsonRecord = {
  companyId: 'gid://shopify/Company/999999999999',
  addressId: 'gid://shopify/CompanyAddress/999999999999',
  companyLocationId: 'gid://shopify/CompanyLocation/999999999999',
  contactInput: {
    title: 'Unknown company buyer',
    email: 'unknown-company-buyer@example.test',
  },
  rolesToAssign: [],
  rolesToRevoke: [],
};

const expectedUserErrors: Record<string, ExpectedUserError> = {
  companyDelete: {
    field: ['id'],
    message: 'Company does not exist.',
    code: 'RESOURCE_NOT_FOUND',
  },
  companyContactCreate: {
    field: ['companyId'],
    message: 'Company does not exist.',
    code: 'RESOURCE_NOT_FOUND',
  },
  companyAddressDelete: {
    field: ['addressId'],
    message: 'Company address was not found.',
    code: 'RESOURCE_NOT_FOUND',
  },
  companyLocationAssignRoles: {
    field: ['companyLocationId'],
    message: 'Location does not exist.',
    code: 'RESOURCE_NOT_FOUND',
  },
  companyLocationRevokeRoles: {
    field: ['companyLocationId'],
    message: 'Location does not exist.',
    code: 'RESOURCE_NOT_FOUND',
  },
};

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

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertExpectedUserErrors(result: ConformanceGraphqlResult): void {
  for (const [root, expected] of Object.entries(expectedUserErrors)) {
    const userErrors = readPath(result.payload, ['data', root, 'userErrors']);
    if (!Array.isArray(userErrors) || userErrors.length !== 1) {
      throw new Error(`${root} did not return exactly one userError: ${JSON.stringify(result.payload, null, 2)}`);
    }
    const actual = readRecord(userErrors[0]);
    if (
      !actual ||
      JSON.stringify(actual['field']) !== JSON.stringify(expected.field) ||
      actual['message'] !== expected.message ||
      actual['code'] !== expected.code
    ) {
      throw new Error(`${root} returned unexpected userError: ${JSON.stringify({ expected, actual }, null, 2)}`);
    }
  }
}

async function readDocument(): Promise<string> {
  return await readFile(documentPath, 'utf8');
}

async function runOperation(): Promise<RecordedOperation> {
  const query = await readDocument();
  const response = await runGraphqlRequest(query, variables);
  assertGraphqlOk(response, scenarioId);
  assertExpectedUserErrors(response);
  return {
    request: { query, variables },
    response: response.payload as JsonRecord,
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const notFoundMessages = await runOperation();

await mkdir(path.dirname(fixturePath), { recursive: true });
await writeFile(
  fixturePath,
  `${JSON.stringify(
    {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notFoundMessages,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, scenarioId, fixturePath }, null, 2));
