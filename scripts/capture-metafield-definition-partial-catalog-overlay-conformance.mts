/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-partial-catalog-overlay.json');

const requestPaths = {
  create: 'config/parity-requests/metafields/metafield-definition-owner-scoped-create.graphql',
  delete: 'config/parity-requests/metafields/metafield-definition-lifecycle-delete.graphql',
  read: 'config/parity-requests/metafields/metafield-definition-partial-catalog-read.graphql',
  readAfterDelete: 'config/parity-requests/metafields/metafield-definition-partial-catalog-read-after-delete.graphql',
  hydrateOwnerCatalog: 'config/parity-requests/metafields/metafield-definition-hydrate-owner-catalog.graphql',
};

const createDocument = await readFile(requestPaths.create, 'utf8');
const deleteDocument = await readFile(requestPaths.delete, 'utf8');
const readDocument = await readFile(requestPaths.read, 'utf8');
const readAfterDeleteDocument = await readFile(requestPaths.readAfterDelete, 'utf8');
const hydrateOwnerCatalogDocument = await readFile(requestPaths.hydrateOwnerCatalog, 'utf8');

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const suffix = Date.now().toString(36);
const namespace = `partial_${suffix}`;
const otherNamespace = `partial_other_${suffix}`;

function readObject(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let cursor = value;
  for (const part of pathParts) {
    cursor = readObject(cursor)?.[part];
  }
  return cursor;
}

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function userErrorsFor(response: unknown, root: string): unknown[] {
  const userErrors = readPath(response, ['data', root, 'userErrors']);
  return Array.isArray(userErrors) ? userErrors : [];
}

function assertNoUserErrors(response: unknown, root: string, label: string): void {
  const userErrors = userErrorsFor(response, root);
  if (userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
}

function assertTaken(response: unknown): void {
  const payload = readObject(readPath(response, ['data', 'metafieldDefinitionCreate']));
  const userErrors = Array.isArray(payload?.['userErrors']) ? payload['userErrors'] : [];
  const firstError = readObject(userErrors[0]);
  if (payload?.['createdDefinition'] === null && firstError?.['code'] === 'TAKEN') return;
  throw new Error(`Duplicate create did not return TAKEN: ${JSON.stringify(payload, null, 2)}`);
}

function createdDefinitionId(response: unknown): string {
  const id = readPath(response, ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id']);
  if (typeof id !== 'string') {
    throw new Error(`metafieldDefinitionCreate did not return an id: ${JSON.stringify(response, null, 2)}`);
  }
  return id;
}

function assertMergedRead(response: unknown, expectReal: boolean): void {
  const data = readObject(readObject(response)?.['data']);
  const real = data?.['realDetail'];
  const local = readObject(data?.['localDetail']);
  const other = readObject(data?.['otherDetail']);
  const realNodes = readPath(data, ['realCatalog', 'nodes']);
  const localNodes = readPath(data, ['localCatalog', 'nodes']);
  const otherNodes = readPath(data, ['otherCatalog', 'nodes']);
  if ((expectReal ? readObject(real)?.['key'] === 'real' : real === null) && local?.['key'] === 'local') {
    if (
      other?.['key'] === 'other' &&
      Array.isArray(realNodes) &&
      (expectReal ? realNodes.some((node) => readObject(node)?.['key'] === 'real') : realNodes.length === 0) &&
      Array.isArray(localNodes) &&
      localNodes.some((node) => readObject(node)?.['key'] === 'local') &&
      Array.isArray(otherNodes) &&
      otherNodes.some((node) => readObject(node)?.['key'] === 'other')
    ) {
      return;
    }
  }
  throw new Error(`Merged read shape did not match expectation: ${JSON.stringify(response, null, 2)}`);
}

async function capture(label: string, query: string, variables: JsonRecord) {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    label,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

async function recordOwnerCatalogHydrate() {
  const calls = [];
  let after: string | null = null;
  for (let page = 0; page < 20; page += 1) {
    const variables = { ownerType: 'PRODUCT', first: 250, after };
    const result = await runGraphqlRaw(hydrateOwnerCatalogDocument, variables);
    assertHttpOk(result, `hydrate owner catalog page ${page + 1}`);
    calls.push({
      operationName: 'MetafieldDefinitionsHydrateOwnerCatalog',
      variables,
      query: hydrateOwnerCatalogDocument,
      response: { status: result.status, body: result.payload },
    });
    const pageInfo = readObject(readPath(result.payload, ['data', 'metafieldDefinitions', 'pageInfo']));
    if (pageInfo?.['hasNextPage'] !== true) break;
    const endCursor = pageInfo?.['endCursor'];
    if (typeof endCursor !== 'string') {
      throw new Error(`Owner catalog hydrate page ${page + 1} did not return endCursor`);
    }
    after = endCursor;
  }
  return calls;
}

async function cleanupDefinition(id: string): Promise<unknown> {
  return await capture('cleanup metafieldDefinitionDelete', deleteDocument, {
    id,
    deleteAllAssociatedMetafields: true,
  }).catch((error: unknown) => ({ label: 'cleanup metafieldDefinitionDelete', error: String(error) }));
}

let realDefinitionId: string | null = null;
let localDefinitionId: string | null = null;
let otherDefinitionId: string | null = null;
const cleanup: unknown[] = [];
let fixture: JsonRecord | null = null;

try {
  const realCreate = await capture('setup real metafieldDefinitionCreate', createDocument, {
    definition: {
      namespace,
      key: 'real',
      ownerType: 'PRODUCT',
      name: 'Alpha partial real definition',
      type: 'single_line_text_field',
    },
  });
  assertNoUserErrors(realCreate.response, 'metafieldDefinitionCreate', 'real create');
  realDefinitionId = createdDefinitionId(realCreate.response);

  const otherCreate = await capture('setup unrelated metafieldDefinitionCreate', createDocument, {
    definition: {
      namespace: otherNamespace,
      key: 'other',
      ownerType: 'PRODUCT',
      name: 'Unrelated partial definition',
      type: 'single_line_text_field',
    },
  });
  assertNoUserErrors(otherCreate.response, 'metafieldDefinitionCreate', 'unrelated create');
  otherDefinitionId = createdDefinitionId(otherCreate.response);

  const upstreamCalls = await recordOwnerCatalogHydrate();

  const duplicateReal = await capture('duplicate real metafieldDefinitionCreate TAKEN', createDocument, {
    definition: {
      namespace,
      key: 'real',
      ownerType: 'PRODUCT',
      name: 'Duplicate partial real definition',
      type: 'single_line_text_field',
    },
  });
  assertTaken(duplicateReal.response);

  const localCreate = await capture('local overlay metafieldDefinitionCreate', createDocument, {
    definition: {
      namespace,
      key: 'local',
      ownerType: 'PRODUCT',
      name: 'Omega partial local definition',
      type: 'single_line_text_field',
    },
  });
  assertNoUserErrors(localCreate.response, 'metafieldDefinitionCreate', 'local create');
  localDefinitionId = createdDefinitionId(localCreate.response);

  const mergedRead = await capture('merged definition read', readDocument, { namespace, otherNamespace });
  assertMergedRead(mergedRead.response, true);

  const deleteReal = await capture('delete real metafieldDefinitionDelete', deleteDocument, {
    id: realDefinitionId,
    deleteAllAssociatedMetafields: true,
  });
  assertNoUserErrors(deleteReal.response, 'metafieldDefinitionDelete', 'real delete');
  realDefinitionId = null;

  const readAfterDelete = await capture('read after real definition delete', readAfterDeleteDocument, {
    namespace,
    otherNamespace,
  });
  assertMergedRead(readAfterDelete.response, false);

  fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    namespace,
    otherNamespace,
    captures: {
      realCreate,
      otherCreate,
      duplicateReal,
      localCreate,
      mergedRead,
      deleteReal,
      readAfterDelete,
    },
    upstreamCalls,
  };
} finally {
  if (realDefinitionId) cleanup.push(await cleanupDefinition(realDefinitionId));
  if (localDefinitionId) cleanup.push(await cleanupDefinition(localDefinitionId));
  if (otherDefinitionId) cleanup.push(await cleanupDefinition(otherDefinitionId));
}

if (fixture) {
  fixture['cleanup'] = cleanup;
  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath, namespace, otherNamespace }, null, 2));
}
