/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type ScenarioCaptures = {
  oldUrlHandle: string;
  newUrlHandle: string;
  firstOldPath: string;
  firstNewPath: string;
  secondOldPath: string;
  secondNewPath: string;
  definitionCreate: Capture;
  firstMetaobjectCreate: Capture;
  secondMetaobjectCreate: Capture;
  definitionUpdate: Capture;
  firstUrlRedirects: Capture;
  secondUrlRedirects: Capture;
  firstUrlRedirect: Capture;
  definitionRead: Capture;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobjectDefinitionUpdate-url-handle-redirect.json');
const requestDir = path.join('config', 'parity-requests', 'metaobjects');
const requestPaths = {
  definitionCreate: path.join(requestDir, 'metaobject-definition-update-url-handle-redirect-definition-create.graphql'),
  entryCreate: path.join(requestDir, 'metaobject-definition-update-url-handle-redirect-entry-create.graphql'),
  definitionUpdate: path.join(requestDir, 'metaobject-definition-update-url-handle-redirect-update.graphql'),
  definitionRead: path.join(requestDir, 'metaobject-definition-update-url-handle-redirect-definition-read.graphql'),
  urlRedirects: path.join(requestDir, 'metaobject-definition-update-url-handle-redirect-url-redirects.graphql'),
  urlRedirect: path.join(requestDir, 'metaobject-definition-update-url-handle-redirect-url-redirect.graphql'),
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const metaobjectDeleteMutation = `#graphql
  mutation MetaobjectDefinitionUpdateUrlHandleRedirectCleanupMetaobject($id: ID!) {
    metaobjectDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const definitionDeleteMutation = `#graphql
  mutation MetaobjectDefinitionUpdateUrlHandleRedirectCleanupDefinition($id: ID!) {
    metaobjectDefinitionDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const urlRedirectDeleteMutation = `#graphql
  mutation MetaobjectDefinitionUpdateUrlHandleRedirectCleanupUrlRedirect($id: ID!) {
    urlRedirectDelete(id: $id) {
      userErrors {
        field
        message
      }
    }
  }
`;

const runId = Date.now().toString();
const createdDefinitionIds: string[] = [];
const createdMetaobjectIds: string[] = [];
const createdRedirectIds: string[] = [];

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function readPath(value: unknown, parts: string[]): unknown {
  let current = value;
  for (const part of parts) {
    if (!isRecord(current)) {
      return undefined;
    }
    current = current[part];
  }
  return current;
}

function readStringPath(value: unknown, parts: string[], label: string): string {
  const found = readPath(value, parts);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`${label} did not return a string at ${parts.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return found;
}

function readBoolPath(value: unknown, parts: string[], label: string): boolean {
  const found = readPath(value, parts);
  if (typeof found !== 'boolean') {
    throw new Error(`${label} did not return a boolean at ${parts.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return found;
}

function readUserErrors(value: unknown, parts: string[]): unknown[] {
  const found = readPath(value, parts);
  return Array.isArray(found) ? found : [];
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || (isRecord(result.payload) && result.payload['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, parts: string[], label: string): void {
  const userErrors = readUserErrors(payload, parts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Capture {
  return {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await client.runGraphqlRequest(query, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

function definitionInput(type: string, oldUrlHandle: string): Record<string, unknown> {
  return {
    type,
    name: `Definition URL redirects ${runId}`,
    displayNameKey: 'title',
    access: { storefront: 'PUBLIC_READ' },
    capabilities: {
      publishable: { enabled: true },
      renderable: { enabled: true, data: { metaTitleKey: 'title' } },
      onlineStore: { enabled: true, data: { urlHandle: oldUrlHandle } },
    },
    fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field', required: true }],
  };
}

function activeMetaobjectInput(type: string, handle: string, title: string): Record<string, unknown> {
  return {
    type,
    handle,
    capabilities: {
      publishable: { status: 'ACTIVE' },
      onlineStore: { templateSuffix: '' },
    },
    fields: [{ key: 'title', value: title }],
  };
}

function definitionUpdateInput(newUrlHandle: string): Record<string, unknown> {
  return {
    capabilities: {
      onlineStore: {
        enabled: true,
        data: { urlHandle: newUrlHandle, createRedirects: true },
      },
    },
  };
}

function firstRedirectNode(payload: unknown): Record<string, unknown> | null {
  const nodes = readPath(payload, ['data', 'urlRedirects', 'nodes']);
  if (!Array.isArray(nodes) || nodes.length === 0) {
    return null;
  }
  const first = nodes[0];
  return isRecord(first) ? first : null;
}

function firstRedirectNodeId(payload: unknown): string | null {
  const first = firstRedirectNode(payload);
  return first && typeof first['id'] === 'string' ? first['id'] : null;
}

function firstRedirectNodeTarget(payload: unknown): string | null {
  const first = firstRedirectNode(payload);
  return first && typeof first['target'] === 'string' ? first['target'] : null;
}

async function waitForRedirectsCapture(name: string, queryValue: string, expectedTarget: string): Promise<Capture> {
  const variables = { query: queryValue };
  let lastResult: ConformanceGraphqlResult | null = null;
  for (let attempt = 1; attempt <= 12; attempt += 1) {
    const result = await client.runGraphqlRequest(queries.urlRedirects, variables);
    assertGraphqlOk(result, name);
    lastResult = result;
    if (firstRedirectNodeTarget(result.payload) === expectedTarget) {
      return captureFromResult(name, queries.urlRedirects, variables, result);
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }

  throw new Error(
    `${name} did not observe redirect target ${expectedTarget}: ${JSON.stringify(lastResult?.payload, null, 2)}`,
  );
}

async function captureScenario(): Promise<ScenarioCaptures> {
  const type = `codex_definition_redirect_${runId}`;
  const oldUrlHandle = `old-definition-${runId}`;
  const newUrlHandle = `new-definition-${runId}`;
  const firstHandle = `first-row-${runId}`;
  const secondHandle = `second-row-${runId}`;
  const firstOldPath = `/pages/${oldUrlHandle}/${firstHandle}`;
  const firstNewPath = `/pages/${newUrlHandle}/${firstHandle}`;
  const secondOldPath = `/pages/${oldUrlHandle}/${secondHandle}`;
  const secondNewPath = `/pages/${newUrlHandle}/${secondHandle}`;

  const definitionCreate = await captureGraphql('definition-create', queries.definitionCreate, {
    definition: definitionInput(type, oldUrlHandle),
  });
  assertNoUserErrors(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'definition create',
  );
  const definitionId = readStringPath(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'definition create',
  );
  createdDefinitionIds.push(definitionId);

  const firstMetaobjectCreate = await captureGraphql('first-metaobject-create', queries.entryCreate, {
    metaobject: activeMetaobjectInput(type, firstHandle, 'First title'),
  });
  assertNoUserErrors(
    firstMetaobjectCreate.response,
    ['data', 'metaobjectCreate', 'userErrors'],
    'first metaobject create',
  );
  createdMetaobjectIds.push(
    readStringPath(firstMetaobjectCreate.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], 'first create'),
  );

  const secondMetaobjectCreate = await captureGraphql('second-metaobject-create', queries.entryCreate, {
    metaobject: activeMetaobjectInput(type, secondHandle, 'Second title'),
  });
  assertNoUserErrors(
    secondMetaobjectCreate.response,
    ['data', 'metaobjectCreate', 'userErrors'],
    'second metaobject create',
  );
  createdMetaobjectIds.push(
    readStringPath(secondMetaobjectCreate.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], 'second create'),
  );

  const definitionUpdate = await captureGraphql('definition-update', queries.definitionUpdate, {
    id: definitionId,
    definition: definitionUpdateInput(newUrlHandle),
  });
  assertNoUserErrors(
    definitionUpdate.response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'definition update',
  );
  readBoolPath(
    definitionUpdate.response,
    [
      'data',
      'metaobjectDefinitionUpdate',
      'metaobjectDefinition',
      'capabilities',
      'onlineStore',
      'data',
      'canCreateRedirects',
    ],
    'definition update canCreateRedirects',
  );

  const firstUrlRedirects = await waitForRedirectsCapture('first-url-redirects', `path:${firstOldPath}`, firstNewPath);
  const secondUrlRedirects = await waitForRedirectsCapture(
    'second-url-redirects',
    `path:${secondOldPath}`,
    secondNewPath,
  );

  const firstRedirectId = firstRedirectNodeId(firstUrlRedirects.response);
  if (!firstRedirectId) {
    throw new Error(`first-url-redirects did not return an id: ${JSON.stringify(firstUrlRedirects.response, null, 2)}`);
  }
  createdRedirectIds.push(firstRedirectId);

  const secondRedirectId = firstRedirectNodeId(secondUrlRedirects.response);
  if (!secondRedirectId) {
    throw new Error(
      `second-url-redirects did not return an id: ${JSON.stringify(secondUrlRedirects.response, null, 2)}`,
    );
  }
  createdRedirectIds.push(secondRedirectId);

  const firstUrlRedirect = await captureGraphql('first-url-redirect', queries.urlRedirect, { id: firstRedirectId });
  const definitionRead = await captureGraphql('definition-read', queries.definitionRead, { id: definitionId });
  readBoolPath(
    definitionRead.response,
    ['data', 'metaobjectDefinition', 'capabilities', 'onlineStore', 'data', 'canCreateRedirects'],
    'definition read canCreateRedirects',
  );

  return {
    oldUrlHandle,
    newUrlHandle,
    firstOldPath,
    firstNewPath,
    secondOldPath,
    secondNewPath,
    definitionCreate,
    firstMetaobjectCreate,
    secondMetaobjectCreate,
    definitionUpdate,
    firstUrlRedirects,
    secondUrlRedirects,
    firstUrlRedirect,
    definitionRead,
  };
}

async function cleanup(): Promise<void> {
  for (const id of [...createdRedirectIds].reverse()) {
    try {
      await client.runGraphqlRequest(urlRedirectDeleteMutation, { id });
    } catch (error) {
      console.warn(`Failed to cleanup URL redirect ${id}:`, error);
    }
  }
  for (const id of [...createdMetaobjectIds].reverse()) {
    try {
      await client.runGraphqlRequest(metaobjectDeleteMutation, { id });
    } catch (error) {
      console.warn(`Failed to cleanup metaobject ${id}:`, error);
    }
  }
  for (const id of [...createdDefinitionIds].reverse()) {
    try {
      await client.runGraphqlRequest(definitionDeleteMutation, { id });
    } catch (error) {
      console.warn(`Failed to cleanup metaobject definition ${id}:`, error);
    }
  }
}

try {
  const urlHandleRedirect = await captureScenario();

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        apiVersion,
        storeDomain,
        capturedAt: new Date().toISOString(),
        scenarioId: 'metaobjectDefinitionUpdate-url-handle-redirect',
        notes:
          'Captured live 2026-04 evidence for metaobjectDefinitionUpdate onlineStore.data.urlHandle changes with createRedirects true. Updating a definition with two ACTIVE rows creates one /pages URL redirect per row and definition reads expose onlineStore.data.canCreateRedirects.',
        cases: {
          urlHandleRedirect,
        },
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} finally {
  await cleanup();
}
