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

const createAliases = ['upperOnly', 'lowerOnly', 'boundedRange', 'existsFilter', 'negatedRange'] as const;

type CreateAlias = (typeof createAliases)[number];

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

function readData(payload: ConformanceGraphqlPayload): Record<string, unknown> {
  const data = readObject(payload.data);
  if (!data) {
    throw new Error('Expected GraphQL payload data object.');
  }

  return data;
}

function readCreatedSavedSearchId(payload: ConformanceGraphqlPayload, alias: CreateAlias): string {
  const data = readData(payload);
  const mutationPayload = readObject(data[alias]);
  const savedSearch = readObject(mutationPayload?.['savedSearch']);
  const id = savedSearch?.['id'];
  const userErrors = mutationPayload?.['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`Expected ${alias} savedSearchCreate to have no userErrors.`);
  }
  if (typeof id !== 'string') {
    throw new Error(`Expected ${alias} savedSearchCreate to return a savedSearch id.`);
  }

  return id;
}

function assertDeleteSucceeded(payload: ConformanceGraphqlPayload, ids: Record<CreateAlias, string>): void {
  const data = readData(payload);
  for (const alias of createAliases) {
    const mutationPayload = readObject(data[alias]);
    if (mutationPayload?.['deletedSavedSearchId'] !== ids[alias]) {
      throw new Error(`Expected ${alias} savedSearchDelete to delete ${ids[alias]}.`);
    }
    const userErrors = mutationPayload?.['userErrors'];
    if (!Array.isArray(userErrors) || userErrors.length !== 0) {
      throw new Error(`Expected ${alias} savedSearchDelete to have no userErrors.`);
    }
  }
}

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'saved-searches', name), 'utf8');
}

async function cleanup(ids: Partial<Record<CreateAlias, string>>): Promise<void> {
  const document = await readRequest('saved-search-query-grammar-delete.graphql');
  for (const [alias, id] of Object.entries(ids)) {
    if (!id) {
      continue;
    }
    try {
      await client.runGraphqlRequest(document, { input: { id } });
    } catch (error) {
      console.error(`Failed to cleanup ${alias} saved search ${id}:`, error);
    }
  }
}

const createDocument = await readRequest('saved-search-filter-projection-create.graphql');
const readDocument = await readRequest('saved-search-filter-projection-read-after-create.graphql');
const deleteDocument = await readRequest('saved-search-filter-projection-delete.graphql');
const token = `SFP-${Date.now().toString(36)}`;
const createVariables = {
  upperOnly: {
    resourceType: 'PRODUCT',
    name: `SFP Upper ${token}`.slice(0, 40),
    query: 'inventory_total:<10',
  },
  lowerOnly: {
    resourceType: 'PRODUCT',
    name: `SFP Lower ${token}`.slice(0, 40),
    query: 'inventory_total:>2',
  },
  boundedRange: {
    resourceType: 'PRODUCT',
    name: `SFP Bounded ${token}`.slice(0, 40),
    query: 'inventory_total:>2 inventory_total:<10',
  },
  existsFilter: {
    resourceType: 'PRODUCT',
    name: `SFP Exists ${token}`.slice(0, 40),
    query: 'sku:*',
  },
  negatedRange: {
    resourceType: 'PRODUCT',
    name: `SFP Negated ${token}`.slice(0, 40),
    query: '-inventory_total:<3',
  },
};

const create = await client.runGraphqlRequest(createDocument, createVariables);
assertNoTopLevelErrors(create, 'saved-search filter projection create capture');

const createdIds = Object.fromEntries(
  createAliases.map((alias) => [alias, readCreatedSavedSearchId(create.payload, alias)]),
) as Record<CreateAlias, string>;

let cleanupComplete = false;
try {
  const readVariables = {};
  const readAfterCreate = await client.runGraphqlRequest(readDocument, readVariables);
  assertNoTopLevelErrors(readAfterCreate, 'saved-search filter projection read-after-create capture');

  const deleteVariables = Object.fromEntries(
    createAliases.map((alias) => [alias, { id: createdIds[alias] }]),
  ) as Record<CreateAlias, { id: string }>;
  const cleanupDelete = await client.runGraphqlRequest(deleteDocument, deleteVariables);
  assertNoTopLevelErrors(cleanupDelete, 'saved-search filter projection cleanup delete capture');
  assertDeleteSucceeded(cleanupDelete.payload, createdIds);
  cleanupComplete = true;

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    token,
    notes: [
      'Live capture for saved-search filter projection shapes.',
      'Range upper-only syntax inventory_total:<10 projected to filters[{ key: "inventory_total_max", value: "10" }] with empty searchTerms.',
      'Range lower-only syntax inventory_total:>2 projected to filters[{ key: "inventory_total_min", value: "2" }] with empty searchTerms.',
      'Bounded ranges are represented by two range tokens and project to both inventory_total_min and inventory_total_max filters.',
      'Exists syntax sku:* projected to filters[{ key: "sku", value: "true" }] with empty searchTerms.',
      'Negated range syntax -inventory_total:<3 projected to inventory_total_min with canonical downstream query inventory_total:>=3.',
      'The fixture includes cleanup delete evidence for every created saved search.',
    ],
    savedSearchFilterProjectionCreate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-filter-projection-create.graphql',
      variables: createVariables,
      payload: create.payload,
    },
    productSavedSearchesAfterFilterProjectionCreate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-filter-projection-read-after-create.graphql',
      variables: readVariables,
      payload: readAfterCreate.payload,
    },
    cleanupDelete: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-filter-projection-delete.graphql',
      variables: deleteVariables,
      payload: cleanupDelete.payload,
    },
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-filter-projection.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  if (!cleanupComplete) {
    await cleanup(createdIds);
  }
}
