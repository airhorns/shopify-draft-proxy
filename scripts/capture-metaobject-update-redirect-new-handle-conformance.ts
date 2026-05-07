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
  oldPath: string;
  newPath: string;
  definitionCreate: Capture;
  metaobjectCreate: Capture;
  metaobjectUpdate: Capture;
  urlRedirects: Capture;
  urlRedirect?: Capture;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-update-redirect-new-handle.json');
const requestDir = path.join('config', 'parity-requests', 'metaobjects');
const requestPaths = {
  definitionCreate: path.join(requestDir, 'metaobject-update-redirect-new-handle-definition-create.graphql'),
  entryCreate: path.join(requestDir, 'metaobject-update-redirect-new-handle-entry-create.graphql'),
  update: path.join(requestDir, 'metaobject-update-redirect-new-handle-update.graphql'),
  urlRedirects: path.join(requestDir, 'metaobject-update-redirect-new-handle-url-redirects.graphql'),
  urlRedirect: path.join(requestDir, 'metaobject-update-redirect-new-handle-url-redirect.graphql'),
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const metaobjectDeleteMutation = `#graphql
  mutation MetaobjectUpdateRedirectNewHandleCleanupMetaobject($id: ID!) {
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
  mutation MetaobjectUpdateRedirectNewHandleCleanupDefinition($id: ID!) {
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
  mutation MetaobjectUpdateRedirectNewHandleCleanupUrlRedirect($id: ID!) {
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

function renderableDefinition(type: string, name: string, urlHandle: string): Record<string, unknown> {
  return {
    type,
    name,
    displayNameKey: 'title',
    access: { storefront: 'PUBLIC_READ' },
    capabilities: {
      publishable: { enabled: true },
      renderable: { enabled: true, data: { metaTitleKey: 'title' } },
      onlineStore: { enabled: true, data: { urlHandle } },
    },
    fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field', required: true }],
  };
}

function nonRenderableDefinition(type: string, name: string): Record<string, unknown> {
  return {
    type,
    name,
    displayNameKey: 'title',
    fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field', required: true }],
  };
}

function metaobjectInput(
  type: string,
  handle: string,
  title: string,
  includeOnlineStore: boolean,
): Record<string, unknown> {
  return {
    type,
    handle,
    ...(includeOnlineStore
      ? { capabilities: { publishable: { status: 'ACTIVE' }, onlineStore: { templateSuffix: '' } } }
      : {}),
    fields: [{ key: 'title', value: title }],
  };
}

function firstRedirectNodeId(payload: unknown): string | null {
  const nodes = readPath(payload, ['data', 'urlRedirects', 'nodes']);
  if (!Array.isArray(nodes) || nodes.length === 0) {
    return null;
  }
  const first = nodes[0];
  return isRecord(first) && typeof first['id'] === 'string' ? first['id'] : null;
}

function firstRedirectNodeTarget(payload: unknown): string | null {
  const nodes = readPath(payload, ['data', 'urlRedirects', 'nodes']);
  if (!Array.isArray(nodes) || nodes.length === 0) {
    return null;
  }
  const first = nodes[0];
  return isRecord(first) && typeof first['target'] === 'string' ? first['target'] : null;
}

async function waitForRedirectsCapture(name: string, queryValue: string, expectedTarget: string): Promise<Capture> {
  const variables = { query: queryValue };
  let lastResult: ConformanceGraphqlResult | null = null;
  for (let attempt = 1; attempt <= 10; attempt += 1) {
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

async function captureScenario(
  name: string,
  definition: Record<string, unknown>,
  urlHandle: string,
  includeOnlineStoreCapability: boolean,
  redirectNewHandle: boolean,
  expectRedirect: boolean,
): Promise<ScenarioCaptures> {
  const type = `codex_redirect_${name}_${runId}`;
  const oldHandle = `${name}-old-${runId}`;
  const newHandle = `${name}-new-${runId}`;
  const oldPath = `/pages/${urlHandle}/${oldHandle}`;
  const newPath = `/pages/${urlHandle}/${newHandle}`;

  const definitionCreate = await captureGraphql(`${name}-definition-create`, queries.definitionCreate, {
    definition: { ...definition, type },
  });
  assertNoUserErrors(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    `${name} definition create`,
  );
  createdDefinitionIds.push(
    readStringPath(
      definitionCreate.response,
      ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
      `${name} definition create`,
    ),
  );

  const metaobjectCreate = await captureGraphql(`${name}-metaobject-create`, queries.entryCreate, {
    metaobject: metaobjectInput(type, oldHandle, `${name} old title`, includeOnlineStoreCapability),
  });
  assertNoUserErrors(
    metaobjectCreate.response,
    ['data', 'metaobjectCreate', 'userErrors'],
    `${name} metaobject create`,
  );
  createdMetaobjectIds.push(
    readStringPath(metaobjectCreate.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], `${name} create`),
  );

  const metaobjectUpdate = await captureGraphql(`${name}-metaobject-update`, queries.update, {
    id: createdMetaobjectIds[createdMetaobjectIds.length - 1],
    metaobject: {
      handle: newHandle,
      redirectNewHandle,
      fields: [{ key: 'title', value: `${name} new title` }],
    },
  });
  assertNoUserErrors(
    metaobjectUpdate.response,
    ['data', 'metaobjectUpdate', 'userErrors'],
    `${name} metaobject update`,
  );

  const redirectQuery = `path:${oldPath}`;
  const urlRedirects = expectRedirect
    ? await waitForRedirectsCapture(`${name}-url-redirects`, redirectQuery, newPath)
    : await captureGraphql(`${name}-url-redirects`, queries.urlRedirects, { query: redirectQuery });

  if (!expectRedirect && firstRedirectNodeId(urlRedirects.response)) {
    throw new Error(`${name} unexpectedly created a redirect: ${JSON.stringify(urlRedirects.response, null, 2)}`);
  }

  const redirectId = firstRedirectNodeId(urlRedirects.response);
  if (!expectRedirect || !redirectId) {
    return { oldPath, newPath, definitionCreate, metaobjectCreate, metaobjectUpdate, urlRedirects };
  }

  createdRedirectIds.push(redirectId);
  const urlRedirect = await captureGraphql(`${name}-url-redirect`, queries.urlRedirect, { id: redirectId });
  return { oldPath, newPath, definitionCreate, metaobjectCreate, metaobjectUpdate, urlRedirects, urlRedirect };
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
  const renderableWithRedirectTrue = await captureScenario(
    'renderable-true',
    renderableDefinition('', `Redirect true ${runId}`, `redirect-true-${runId}`),
    `redirect-true-${runId}`,
    true,
    true,
    true,
  );
  const renderableWithRedirectFalse = await captureScenario(
    'renderable-false',
    renderableDefinition('', `Redirect false ${runId}`, `redirect-false-${runId}`),
    `redirect-false-${runId}`,
    true,
    false,
    false,
  );
  const nonRenderableWithRedirectTrue = await captureScenario(
    'non-renderable-true',
    nonRenderableDefinition('', `Non-renderable redirect ${runId}`),
    `non-renderable-${runId}`,
    false,
    true,
    false,
  );

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        apiVersion,
        storeDomain,
        capturedAt: new Date().toISOString(),
        scenarioId: 'metaobject-update-redirect-new-handle',
        notes:
          'Captured live 2026-04 evidence for metaobjectUpdate redirectNewHandle nested in MetaobjectUpdateInput. Online-store renderable handle updates create URL redirects, while redirectNewHandle false and non-renderable definitions do not.',
        cases: {
          renderableWithRedirectTrue,
          renderableWithRedirectFalse,
          nonRenderableWithRedirectTrue,
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
