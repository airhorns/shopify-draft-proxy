/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
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

type Seed = {
  runId: string;
  requestingApiClientId: string;
  appNamespaceInput: string;
  appNamespaceResolved: string;
  crossAppNamespace: string;
  key: string;
  definitionId?: string;
};

const requestPaths = {
  create: 'config/parity-requests/metafields/metafield-definition-app-namespace-create.graphql',
  update: 'config/parity-requests/metafields/metafield-definition-app-namespace-update.graphql',
  read: 'config/parity-requests/metafields/metafield-definition-app-namespace-read.graphql',
  delete: 'config/parity-requests/metafields/metafield-definition-app-namespace-delete.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-app-namespace-resolution.json');
const runId = Date.now().toString(36);
const requestingApiClientId = process.env.SHOPIFY_CONFORMANCE_API_CLIENT_ID ?? '347082227713';
const suffix = `app_namespace_${runId}`;
const seed: Seed = {
  runId,
  requestingApiClientId,
  appNamespaceInput: `$app:${suffix}`,
  appNamespaceResolved: `app--${requestingApiClientId}--${suffix}`,
  crossAppNamespace: `app--999999999999--${suffix}`,
  key: 'tier',
};

const deleteByIdMutation = `#graphql
  mutation DeleteMetafieldDefinitionById($id: ID!, $deleteAllAssociatedMetafields: Boolean!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: $deleteAllAssociatedMetafields) {
      deletedDefinitionId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    if (object === null) {
      return undefined;
    }
    current = object[part];
  }

  return current;
}

function readUserErrors(payload: unknown, pathParts: string[]): unknown[] {
  const value = readPath(payload, pathParts);
  return Array.isArray(value) ? value : [];
}

function readTopErrors(payload: unknown): unknown[] {
  const value = readPath(payload, ['errors']);
  return Array.isArray(value) ? value : [];
}

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} failed HTTP status: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoGraphqlErrors(result: ConformanceGraphqlResult, label: string): void {
  assertHttpOk(result, label);
  if (readTopErrors(result.payload).length > 0) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertHasAccessDenied(payload: unknown, rootName: string, label: string): void {
  const errors = readTopErrors(payload);
  if (
    !errors.some((error) => {
      const object = readObject(error);
      const extensions = readObject(object?.['extensions']);
      const pathValue = object?.['path'];
      return (
        extensions?.['code'] === 'ACCESS_DENIED' &&
        Array.isArray(pathValue) &&
        pathValue.length === 1 &&
        pathValue[0] === rootName
      );
    })
  ) {
    throw new Error(`${label} did not return the expected ACCESS_DENIED top-level error: ${JSON.stringify(payload)}`);
  }
}

function readStringPath(payload: unknown, pathParts: string[], label: string): string {
  const value = readPath(payload, pathParts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(payload, null, 2)}`);
  }

  return value;
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
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertNoGraphqlErrors(result, name);
  return captureFromResult(name, query, variables, result);
}

async function captureGraphqlWithTopError(
  name: string,
  rootName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, name);
  assertHasAccessDenied(result.payload, rootName, name);
  return captureFromResult(name, query, variables, result);
}

function definitionCreateInput(namespace: string, name: string): Record<string, unknown> {
  return {
    name,
    namespace,
    key: seed.key,
    ownerType: 'PRODUCT',
    type: 'single_line_text_field',
    description: `Temporary app namespace definition ${runId}`,
    validations: [{ name: 'max', value: '32' }],
  };
}

function definitionUpdateInput(namespace: string, name: string): Record<string, unknown> {
  return {
    name,
    namespace,
    key: seed.key,
    ownerType: 'PRODUCT',
    description: `Updated app namespace definition ${runId}`,
  };
}

async function cleanupDefinition(cleanup: Capture[]): Promise<void> {
  if (!seed.definitionId) {
    return;
  }

  const cleanupCapture = await captureGraphql('cleanup-definition-delete', deleteByIdMutation, {
    id: seed.definitionId,
    deleteAllAssociatedMetafields: true,
  });
  cleanup.push(cleanupCapture);
}

async function writeBlocker(stage: string, error: unknown, captures: Capture[], cleanup: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metafield-definition-app-namespace-resolution-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command: 'corepack pnpm conformance:capture -- --run metafield-definition-app-namespace-resolution',
        blocker: { stage, message },
        seed,
        partialCaptures: captures,
        cleanup,
      },
      null,
      2,
    )}\n`,
  );
  console.error(`Wrote blocker evidence to ${blockerPath}`);
}

const cleanup: Capture[] = [];
const captures: Capture[] = [];

try {
  const appPrefixedCreate = await captureGraphql('app-prefixed-create', queries.create, {
    definition: definitionCreateInput(seed.appNamespaceInput, `App namespace definition ${runId}`),
  });
  captures.push(appPrefixedCreate);
  assertNoUserErrors(
    appPrefixedCreate.response,
    ['data', 'metafieldDefinitionCreate', 'userErrors'],
    'app-prefixed-create',
  );
  seed.definitionId = readStringPath(
    appPrefixedCreate.response,
    ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id'],
    'app-prefixed-create',
  );

  const appPrefixedUpdate = await captureGraphql('app-prefixed-update', queries.update, {
    definition: definitionUpdateInput(seed.appNamespaceInput, `App namespace definition updated ${runId}`),
  });
  captures.push(appPrefixedUpdate);
  assertNoUserErrors(
    appPrefixedUpdate.response,
    ['data', 'metafieldDefinitionUpdate', 'userErrors'],
    'app-prefixed-update',
  );

  const appNamespaceRead = await captureGraphql('read-by-canonical-and-app-prefix', queries.read, {
    canonicalNamespace: seed.appNamespaceResolved,
    appNamespace: seed.appNamespaceInput,
    key: seed.key,
  });
  captures.push(appNamespaceRead);

  const canonicalDelete = await captureGraphql('delete-by-canonical-identifier', queries.delete, {
    namespace: seed.appNamespaceResolved,
    key: seed.key,
    deleteAllAssociatedMetafields: true,
  });
  captures.push(canonicalDelete);
  assertNoUserErrors(
    canonicalDelete.response,
    ['data', 'metafieldDefinitionDelete', 'userErrors'],
    'delete-by-canonical-identifier',
  );
  seed.definitionId = undefined;

  const postDeleteRead = await captureGraphql('post-delete-read', queries.read, {
    canonicalNamespace: seed.appNamespaceResolved,
    appNamespace: seed.appNamespaceInput,
    key: seed.key,
  });
  captures.push(postDeleteRead);

  const crossAppCreate = await captureGraphqlWithTopError(
    'cross-app-create',
    'metafieldDefinitionCreate',
    queries.create,
    {
      definition: definitionCreateInput(seed.crossAppNamespace, `Cross app namespace definition ${runId}`),
    },
  );
  captures.push(crossAppCreate);

  const crossAppUpdate = await captureGraphqlWithTopError(
    'cross-app-update',
    'metafieldDefinitionUpdate',
    queries.update,
    {
      definition: definitionUpdateInput(seed.crossAppNamespace, `Cross app namespace update ${runId}`),
    },
  );
  captures.push(crossAppUpdate);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary:
          'MetafieldDefinition app namespace resolution for create, update, identifier reads, canonical delete, and cross-app access denial.',
        seed,
        appPrefixedCreate,
        appPrefixedUpdate,
        appNamespaceRead,
        canonicalDelete,
        postDeleteRead,
        crossAppCreate,
        crossAppUpdate,
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  try {
    await cleanupDefinition(cleanup);
  } finally {
    await writeBlocker('capture', error, captures, cleanup);
  }
  throw error;
}
