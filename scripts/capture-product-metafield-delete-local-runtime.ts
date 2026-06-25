import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

type JsonRecord = Record<string, unknown>;
type ProxyResponse = {
  status: number;
  body: unknown;
};
type DraftProxyInstance = {
  processGraphQLRequest: (
    body: { query: string; variables?: JsonRecord },
    options?: { apiVersion?: string },
  ) => Promise<ProxyResponse>;
  dispose: () => void;
};

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2026-04';
const ownerId = 'gid://shopify/Product/170004';
const neverCreatedMetafieldId = 'gid://shopify/Metafield/170099';

const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'products',
  'metafield-delete-product-owner-local-runtime.json',
);
const setupDocumentPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'products',
  'metafieldDelete-product-owner-setup.graphql',
);
const deleteDocumentPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'products',
  'metafieldDelete-product-owner.graphql',
);
const readDocumentPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'products',
  'metafieldDelete-product-owner-read.graphql',
);

function readObject(value: unknown, context: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function readArray(value: unknown, context: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${context} was not an array: ${JSON.stringify(value)}`);
  }
  return value;
}

function pathValue(value: unknown, parts: string[], context: string): unknown {
  let cursor = value;
  for (const part of parts) {
    if (!cursor || typeof cursor !== 'object' || Array.isArray(cursor)) {
      throw new Error(`${context} missing ${parts.join('.')}: ${JSON.stringify(value)}`);
    }
    cursor = (cursor as JsonRecord)[part];
  }
  return cursor;
}

function assertHttpOk(response: ProxyResponse, context: string): JsonRecord {
  if (response.status !== 200) {
    throw new Error(`${context} returned HTTP ${response.status}: ${JSON.stringify(response.body)}`);
  }
  return readObject(response.body, `${context} response body`);
}

function assertNoTopLevelErrors(body: JsonRecord, context: string): void {
  if (body['errors'] !== undefined) {
    throw new Error(`${context} returned top-level errors: ${JSON.stringify(body['errors'])}`);
  }
}

async function graphql(
  proxy: DraftProxyInstance,
  query: string,
  variables: JsonRecord,
  context: string,
): Promise<JsonRecord> {
  const body = assertHttpOk(await proxy.processGraphQLRequest({ query, variables }, { apiVersion }), context);
  assertNoTopLevelErrors(body, context);
  return body;
}

const setupDocument = await readFile(setupDocumentPath, 'utf8');
const deleteDocument = await readFile(deleteDocumentPath, 'utf8');
const readDocument = await readFile(readDocumentPath, 'utf8');
const { createDraftProxy } = (await import('../js/src/index.js')) as {
  createDraftProxy: (options: { readMode: string; port: number; shopifyAdminOrigin: string }) => DraftProxyInstance;
};

const proxy = createDraftProxy({
  readMode: 'live-hybrid',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

try {
  const setupVariables = {
    metafields: [
      {
        ownerId,
        namespace: 'custom',
        key: 'singular_delete_me',
        type: 'single_line_text_field',
        value: 'staged singular delete',
      },
    ],
  };
  const readVariables = { id: ownerId };
  const setup = await graphql(proxy, setupDocument, setupVariables, 'metafieldsSet setup');
  const metafields = readArray(
    pathValue(setup, ['data', 'metafieldsSet', 'metafields'], 'metafieldsSet setup'),
    'metafieldsSet setup metafields',
  );
  const firstMetafield = readObject(metafields[0], 'metafieldsSet setup first metafield');
  const metafieldId = firstMetafield['id'];
  if (typeof metafieldId !== 'string') {
    throw new Error(`metafieldsSet setup did not return a metafield id: ${JSON.stringify(setup)}`);
  }

  const readAfterSet = await graphql(proxy, readDocument, readVariables, 'read after set');
  const deleteVariables = { input: { id: metafieldId } };
  const deleteResponse = await graphql(proxy, deleteDocument, deleteVariables, 'metafieldDelete success');
  const readAfterDelete = await graphql(proxy, readDocument, readVariables, 'read after delete');
  const deleteNotFoundVariables = { input: { id: neverCreatedMetafieldId } };
  const deleteNotFound = await graphql(proxy, deleteDocument, deleteNotFoundVariables, 'metafieldDelete not found');

  const fixture = {
    fixtureKind: 'local-runtime-product-metafield-delete',
    apiVersion,
    storeDomain: 'local-runtime.myshopify.com',
    capturedAt: new Date().toISOString(),
    summary:
      'Executable local-runtime fixture for product-owner singular metafieldDelete compatibility behavior when public Admin GraphQL exposes only metafieldsDelete.',
    workflow: {
      setup: {
        query: setupDocument,
        variables: setupVariables,
        response: setup,
      },
      readAfterSet: {
        query: readDocument,
        variables: readVariables,
        response: readAfterSet,
      },
      delete: {
        query: deleteDocument,
        variables: deleteVariables,
        response: deleteResponse,
      },
      readAfterDelete: {
        query: readDocument,
        variables: readVariables,
        response: readAfterDelete,
      },
      deleteNotFound: {
        query: deleteDocument,
        variables: deleteNotFoundVariables,
        response: deleteNotFound,
      },
    },
    upstreamCalls: [],
  };

  await mkdir(path.dirname(fixturePath), { recursive: true });
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
} finally {
  proxy.dispose();
}
