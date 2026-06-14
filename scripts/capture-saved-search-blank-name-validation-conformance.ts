/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'saved-searches');

type UserErrorExpectation = {
  field: string[];
  message: string;
};

function readObject(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readPayloadAlias(payload: ConformanceGraphqlPayload, alias: string): Record<string, unknown> {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.[alias]);
  if (!mutationPayload) {
    throw new Error(`Expected ${alias} payload: ${JSON.stringify(payload, null, 2)}`);
  }

  return mutationPayload;
}

function assertUserErrors(payload: ConformanceGraphqlPayload, alias: string, expected: UserErrorExpectation[]): void {
  const mutationPayload = readPayloadAlias(payload, alias);
  if (mutationPayload['savedSearch'] !== null) {
    throw new Error(`Expected ${alias} savedSearch null: ${JSON.stringify(mutationPayload, null, 2)}`);
  }

  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== expected.length) {
    throw new Error(`Expected ${alias} ${expected.length} userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }

  for (const [index, expectation] of expected.entries()) {
    const actual = readObject(userErrors[index]);
    const keys = actual ? Object.keys(actual).sort() : [];
    if (
      JSON.stringify(keys) !== JSON.stringify(['field', 'message']) ||
      JSON.stringify(actual?.['field']) !== JSON.stringify(expectation.field) ||
      actual?.['message'] !== expectation.message
    ) {
      throw new Error(
        `Unexpected ${alias} userError[${index}]: expected ${JSON.stringify(
          expectation,
        )}, got ${JSON.stringify(actual, null, 2)}`,
      );
    }
  }
}

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'saved-searches', name), 'utf8');
}

const document = await readRequest('saved-search-blank-name-validation-create.graphql');
const variables = {
  blankEmpty: {
    resourceType: 'PRODUCT',
    name: '',
    query: '',
  },
  blankInvalidQuery: {
    resourceType: 'PRODUCT',
    name: '',
    query: 'made_up_filter:foo',
  },
};

const savedSearchCreateBlankNameValidation = await client.runGraphqlRequest(document, variables);
assertNoTopLevelErrors(savedSearchCreateBlankNameValidation, 'saved-search blank-name validation capture');
assertUserErrors(savedSearchCreateBlankNameValidation.payload, 'blankEmpty', [
  { field: ['input', 'name'], message: "Name can't be blank" },
]);
assertUserErrors(savedSearchCreateBlankNameValidation.payload, 'blankInvalidQuery', [
  { field: ['input', 'name'], message: "Name can't be blank" },
  { field: ['input', 'query'], message: "Query is invalid, 'made_up_filter' is not a valid filter" },
]);

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  notes: [
    'SavedSearchCreate with an empty string name returns base UserError objects with field and message only.',
    'An empty string name does not short-circuit query validation; blank name and invalid PRODUCT query filters aggregate in one payload.',
  ],
  savedSearchCreateBlankNameValidation: {
    documentPath: 'config/parity-requests/saved-searches/saved-search-blank-name-validation-create.graphql',
    variables,
    payload: savedSearchCreateBlankNameValidation.payload,
  },
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
const fixturePath = path.join(outputDir, 'saved-search-blank-name-validation.json');
await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
