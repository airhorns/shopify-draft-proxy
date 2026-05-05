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

function readCreatedSavedSearchId(payload: ConformanceGraphqlPayload): string {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.['savedSearchCreate']);
  const savedSearch = readObject(mutationPayload?.['savedSearch']);
  const id = savedSearch?.['id'];
  if (typeof id !== 'string') {
    throw new Error('Expected savedSearchCreate to return a savedSearch id.');
  }

  return id;
}

function assertDeletePayload(
  payload: ConformanceGraphqlPayload,
  context: string,
  expectedDeletedId: string | null,
): void {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.['savedSearchDelete']);
  if (!mutationPayload) {
    throw new Error(`Expected ${context} savedSearchDelete payload.`);
  }
  if (mutationPayload['deletedSavedSearchId'] !== expectedDeletedId) {
    throw new Error(
      `Expected ${context} deletedSavedSearchId ${expectedDeletedId}; got ${String(
        mutationPayload['deletedSavedSearchId'],
      )}.`,
    );
  }
  const shop = readObject(mutationPayload['shop']);
  if (typeof shop?.['id'] !== 'string' || !shop['id'].startsWith('gid://shopify/Shop/')) {
    throw new Error(`Expected ${context} payload to include non-null shop.id.`);
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors)) {
    throw new Error(`Expected ${context} userErrors array.`);
  }
}

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'saved-searches', name), 'utf8');
}

async function cleanup(id: string): Promise<void> {
  const document = await readRequest('saved-search-delete-shop-payload-delete.graphql');
  try {
    await client.runGraphqlRequest(document, { input: { id } });
  } catch (error) {
    console.error(`Failed to cleanup saved search ${id}:`, error);
  }
}

const createDocument = await readRequest('saved-search-local-staging-create.graphql');
const deleteDocument = await readRequest('saved-search-delete-shop-payload-delete.graphql');
const token = `H716-${Date.now().toString(36)}`;
const createVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: `H716 Shop Payload ${token}`.slice(0, 40),
    query: `title:${token}`,
  },
};

const create = await client.runGraphqlRequest(createDocument, createVariables);
assertNoTopLevelErrors(create, 'saved-search delete shop payload create capture');
const createdId = readCreatedSavedSearchId(create.payload);

let cleanupComplete = false;
try {
  const deleteVariables = { input: { id: createdId } };
  const savedSearchDelete = await client.runGraphqlRequest(deleteDocument, deleteVariables);
  assertNoTopLevelErrors(savedSearchDelete, 'saved-search delete shop payload success-delete capture');
  assertDeletePayload(savedSearchDelete.payload, 'success-delete', createdId);
  cleanupComplete = true;

  const missingVariables = { input: { id: 'gid://shopify/SavedSearch/0' } };
  const missingSavedSearchDelete = await client.runGraphqlRequest(deleteDocument, missingVariables);
  assertNoTopLevelErrors(missingSavedSearchDelete, 'saved-search delete shop payload missing-id capture');
  assertDeletePayload(missingSavedSearchDelete.payload, 'missing-id', null);

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    token,
    notes: [
      'HAR-716 capture for savedSearchDelete.shop payload parity.',
      'A disposable PRODUCT saved search was created, then deleted with shop { id } selected.',
      'Shopify populated shop.id on the success payload and on a missing-id userError payload.',
      'The proxy parity runner uses local staging for the create/delete operations; no upstream cassette calls are required.',
    ],
    savedSearchCreate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: createVariables,
      payload: create.payload,
    },
    savedSearchDelete: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-delete-shop-payload-delete.graphql',
      variables: deleteVariables,
      payload: savedSearchDelete.payload,
    },
    missingSavedSearchDelete: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-delete-shop-payload-delete.graphql',
      variables: missingVariables,
      payload: missingSavedSearchDelete.payload,
    },
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-delete-shop-payload.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  if (!cleanupComplete) {
    await cleanup(createdId);
  }
}
