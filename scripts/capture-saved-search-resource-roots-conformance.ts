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

type SavedSearchAlias = 'product' | 'collection' | 'order' | 'draftOrder' | 'file' | 'discountRedeemCode';

const SUCCESS_ALIASES: Array<{ alias: SavedSearchAlias; resourceType: string }> = [
  { alias: 'product', resourceType: 'PRODUCT' },
  { alias: 'collection', resourceType: 'COLLECTION' },
  { alias: 'order', resourceType: 'ORDER' },
  { alias: 'draftOrder', resourceType: 'DRAFT_ORDER' },
  { alias: 'file', resourceType: 'FILE' },
  { alias: 'discountRedeemCode', resourceType: 'DISCOUNT_REDEEM_CODE' },
];

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
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

function readAliasSavedSearchId(payload: ConformanceGraphqlPayload, alias: string): string | null {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.[alias]);
  const savedSearch = readObject(mutationPayload?.['savedSearch']);
  const id = savedSearch?.['id'];
  return typeof id === 'string' ? id : null;
}

function assertCreatedIds(payload: ConformanceGraphqlPayload): Record<SavedSearchAlias, string> {
  const ids: Partial<Record<SavedSearchAlias, string>> = {};
  for (const { alias } of SUCCESS_ALIASES) {
    const id = readAliasSavedSearchId(payload, alias);
    if (!id) {
      throw new Error(`Expected ${alias} savedSearchCreate to return a savedSearch id.`);
    }
    ids[alias] = id;
  }

  return ids as Record<SavedSearchAlias, string>;
}

function assertDeleteSucceeded(payload: ConformanceGraphqlPayload, ids: Record<SavedSearchAlias, string>): void {
  const data = readObject(payload.data);
  for (const { alias } of SUCCESS_ALIASES) {
    const mutationPayload = readObject(data?.[alias]);
    if (mutationPayload?.['deletedSavedSearchId'] !== ids[alias]) {
      throw new Error(`Expected ${alias} savedSearchDelete to delete ${ids[alias]}.`);
    }
    const userErrors = mutationPayload?.['userErrors'];
    if (!Array.isArray(userErrors) || userErrors.length !== 0) {
      throw new Error(`Expected ${alias} savedSearchDelete to have no userErrors.`);
    }
  }
}

function makeInput(token: string, resourceType: string): Record<string, string> {
  const value = `${token} ${resourceType}`;
  return {
    resourceType,
    name: value.slice(0, 40),
    query: value,
  };
}

function makeCreateVariables(token: string): Record<string, unknown> {
  return {
    ...Object.fromEntries(SUCCESS_ALIASES.map(({ alias, resourceType }) => [alias, makeInput(token, resourceType)])),
    customer: makeInput(token, 'CUSTOMER'),
  };
}

function makeDeleteVariables(ids: Record<SavedSearchAlias, string>): Record<string, unknown> {
  return Object.fromEntries(SUCCESS_ALIASES.map(({ alias }) => [alias, { id: ids[alias] }]));
}

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'saved-searches', name), 'utf8');
}

async function cleanupIndividually(ids: Record<SavedSearchAlias, string>): Promise<void> {
  const document = `#graphql
    mutation CleanupSavedSearch($input: SavedSearchDeleteInput!) {
      savedSearchDelete(input: $input) {
        deletedSavedSearchId
        userErrors {
          field
          message
        }
      }
    }
  `;
  for (const id of Object.values(ids).reverse()) {
    try {
      await client.runGraphqlRequest(document, { input: { id } });
    } catch (error) {
      console.error(`Failed to cleanup saved search ${id}:`, error);
    }
  }
}

const createDocument = await readRequest('saved-search-resource-roots-create.graphql');
const readDocument = await readRequest('saved-search-resource-roots-read.graphql');
const deleteDocument = await readRequest('saved-search-resource-roots-delete.graphql');
const token = `H402-${Date.now().toString(36)}`;
const createVariables = makeCreateVariables(token);
const queryVariables = {};

const create = await client.runGraphqlRequest(createDocument, createVariables);
assertNoTopLevelErrors(create, 'saved-search resource create capture');
const createdIds = assertCreatedIds(create.payload);

let cleanupComplete = false;
let cleanupDelete: ConformanceGraphqlResult | null = null;
try {
  const readAfterCreate = await client.runGraphqlRequest(readDocument, queryVariables);
  assertNoTopLevelErrors(readAfterCreate, 'saved-search resource read-after-create capture');

  const deleteVariables = makeDeleteVariables(createdIds);
  cleanupDelete = await client.runGraphqlRequest(deleteDocument, deleteVariables);
  assertNoTopLevelErrors(cleanupDelete, 'saved-search cleanup delete capture');
  assertDeleteSucceeded(cleanupDelete.payload, createdIds);
  cleanupComplete = true;

  const readAfterDelete = await client.runGraphqlRequest(readDocument, queryVariables);
  assertNoTopLevelErrors(readAfterDelete, 'saved-search resource read-after-delete capture');

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    token,
    notes: [
      'HAR-402 capture for executable saved-search resource-root parity.',
      'PRODUCT, COLLECTION, ORDER, DRAFT_ORDER, FILE, and DISCOUNT_REDEEM_CODE savedSearchCreate calls succeeded and were visible only through their matching saved-search roots.',
      'CUSTOMER savedSearchCreate returned Shopify deprecation userErrors with field null.',
      'PRICE_RULE savedSearchCreate was intentionally excluded from this live fixture because the current conformance token returns ACCESS_DENIED for that resource type; local PRICE_RULE routing remains covered by runtime tests.',
      'codeDiscountSavedSearches and automaticDiscountSavedSearches were captured without query arguments because Shopify 2026-04 rejects query: on those roots.',
      'The fixture includes cleanup delete responses and post-delete empty reads for all successfully created saved searches.',
    ],
    create: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-resource-roots-create.graphql',
      variables: createVariables,
      response: create.payload,
    },
    readAfterCreate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-resource-roots-read.graphql',
      variables: queryVariables,
      response: readAfterCreate.payload,
    },
    cleanupDelete: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-resource-roots-delete.graphql',
      variables: deleteVariables,
      response: cleanupDelete.payload,
    },
    readAfterDelete: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-resource-roots-read.graphql',
      variables: queryVariables,
      response: readAfterDelete.payload,
    },
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-resource-roots.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  if (!cleanupComplete) {
    await cleanupIndividually(createdIds);
  }
}
