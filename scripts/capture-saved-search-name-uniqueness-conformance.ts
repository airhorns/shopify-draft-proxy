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

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readMutationPayload(
  payload: ConformanceGraphqlPayload,
  root: 'savedSearchCreate' | 'savedSearchUpdate' | 'savedSearchDelete',
): Record<string, unknown> {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.[root]);
  if (!mutationPayload) {
    throw new Error(`Expected ${root} payload.`);
  }

  return mutationPayload;
}

function readCreatedSavedSearchId(payload: ConformanceGraphqlPayload): string {
  const mutationPayload = readMutationPayload(payload, 'savedSearchCreate');
  const savedSearch = readObject(mutationPayload['savedSearch']);
  const id = savedSearch?.['id'];
  if (typeof id !== 'string') {
    throw new Error('Expected savedSearchCreate to return a savedSearch id.');
  }

  return id;
}

function readOptionalSavedSearchId(payload: ConformanceGraphqlPayload): string | null {
  const mutationPayload = readMutationPayload(payload, 'savedSearchCreate');
  const savedSearch = readObject(mutationPayload['savedSearch']);
  return typeof savedSearch?.['id'] === 'string' ? savedSearch['id'] : null;
}

function assertDuplicateNameUserError(
  payload: ConformanceGraphqlPayload,
  root: 'savedSearchCreate' | 'savedSearchUpdate',
  context: string,
  options: { expectNullSavedSearch: boolean },
): void {
  const mutationPayload = readMutationPayload(payload, root);
  if (options.expectNullSavedSearch && mutationPayload['savedSearch'] !== null) {
    throw new Error(
      `Expected ${context} savedSearch to be null; got ${JSON.stringify(mutationPayload['savedSearch'])}.`,
    );
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`Expected ${context} to return exactly one userError; got ${JSON.stringify(userErrors)}.`);
  }
  const error = readObject(userErrors[0]);
  const field = error?.['field'];
  if (
    !Array.isArray(field) ||
    field.length !== 2 ||
    field[0] !== 'input' ||
    field[1] !== 'name' ||
    error?.['message'] !== 'Name has already been taken'
  ) {
    throw new Error(`Expected ${context} duplicate-name userError; got ${JSON.stringify(error)}.`);
  }
}

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'saved-searches', name), 'utf8');
}

async function cleanup(id: string, deleteDocument: string): Promise<void> {
  try {
    await client.runGraphqlRequest(deleteDocument, { input: { id } });
  } catch (error) {
    console.error(`Failed to cleanup saved search ${id}:`, error);
  }
}

const createDocument = await readRequest('saved-search-local-staging-create.graphql');
const updateDocument = await readRequest('saved-search-name-uniqueness-update-conflict.graphql');
const deleteDocument = await readRequest('saved-search-delete-shop-payload-delete.graphql');
const token = `H720-${Date.now().toString(36)}`;
const nameA = `Conflict A ${token}`.slice(0, 40);
const nameB = `Conflict B ${token}`.slice(0, 40);
const cleanupIds: string[] = [];

const createAVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: nameA,
    query: `title:${token}-a`,
  },
};
const duplicateCreateVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: nameA,
    query: `title:${token}-duplicate`,
  },
};
const createBVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: nameB,
    query: `title:${token}-b`,
  },
};

let createA: ConformanceGraphqlResult | null = null;
let savedSearchCreateDuplicate: ConformanceGraphqlResult | null = null;
let createB: ConformanceGraphqlResult | null = null;
let savedSearchUpdateRenameConflict: ConformanceGraphqlResult | null = null;

try {
  createA = await client.runGraphqlRequest(createDocument, createAVariables);
  assertNoTopLevelErrors(createA, 'saved-search name uniqueness create A capture');
  const idA = readCreatedSavedSearchId(createA.payload);
  cleanupIds.push(idA);

  savedSearchCreateDuplicate = await client.runGraphqlRequest(createDocument, duplicateCreateVariables);
  assertNoTopLevelErrors(savedSearchCreateDuplicate, 'saved-search name uniqueness duplicate-create capture');
  const unexpectedDuplicateId = readOptionalSavedSearchId(savedSearchCreateDuplicate.payload);
  if (unexpectedDuplicateId) {
    cleanupIds.push(unexpectedDuplicateId);
  }
  assertDuplicateNameUserError(savedSearchCreateDuplicate.payload, 'savedSearchCreate', 'duplicate-create', {
    expectNullSavedSearch: true,
  });

  createB = await client.runGraphqlRequest(createDocument, createBVariables);
  assertNoTopLevelErrors(createB, 'saved-search name uniqueness create B capture');
  const idB = readCreatedSavedSearchId(createB.payload);
  cleanupIds.push(idB);

  const updateRenameConflictVariables = {
    input: {
      id: idB,
      name: nameA,
      query: `title:${token}-rename-conflict`,
    },
  };
  savedSearchUpdateRenameConflict = await client.runGraphqlRequest(updateDocument, updateRenameConflictVariables);
  assertNoTopLevelErrors(
    savedSearchUpdateRenameConflict,
    'saved-search name uniqueness update-rename-conflict capture',
  );
  assertDuplicateNameUserError(savedSearchUpdateRenameConflict.payload, 'savedSearchUpdate', 'update-rename-conflict', {
    expectNullSavedSearch: false,
  });

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    token,
    notes: [
      'HAR-720 capture for saved-search name uniqueness.',
      'Created disposable PRODUCT saved search A, attempted duplicate create with the same case-sensitive name, created disposable saved search B, then attempted to rename B to A.',
      'Shopify returned savedSearch: null for duplicate create, and returned a non-null savedSearch echo with the existing name plus submitted valid query for duplicate update; both branches returned userErrors[{ field: ["input", "name"], message: "Name has already been taken" }].',
      'The proxy parity runner stages the setup creates locally; no upstream cassette calls are required.',
    ],
    savedSearchCreateA: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: createAVariables,
      payload: createA.payload,
    },
    savedSearchCreateDuplicate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: duplicateCreateVariables,
      payload: savedSearchCreateDuplicate.payload,
    },
    savedSearchCreateB: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: createBVariables,
      payload: createB.payload,
    },
    savedSearchUpdateRenameConflict: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-name-uniqueness-update-conflict.graphql',
      variables: updateRenameConflictVariables,
      payload: savedSearchUpdateRenameConflict.payload,
    },
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-name-uniqueness.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  for (const id of cleanupIds.reverse()) {
    await cleanup(id, deleteDocument);
  }
}
