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
const ownerId = 'gid://shopify/Product/1';
const neverCreatedMetafieldId = 'gid://shopify/Metafield/never-created';

const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'metafield-definitions',
  'metafield-delete-not-found.json',
);
const setupDocumentPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'metafield-definitions',
  'metafield-delete-not-found-setup.graphql',
);
const deleteDocumentPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'metafield-definitions',
  'metafield-delete-by-id.graphql',
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
        key: 'delete_me',
        type: 'single_line_text_field',
        value: 'staged',
      },
    ],
  };
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

  const happyDelete = await graphql(
    proxy,
    deleteDocument,
    { input: { id: metafieldId } },
    'metafieldDelete happy delete',
  );
  const repeatDelete = await graphql(
    proxy,
    deleteDocument,
    { input: { id: metafieldId } },
    'metafieldDelete repeat delete',
  );
  const neverCreatedDelete = await graphql(
    proxy,
    deleteDocument,
    { input: { id: neverCreatedMetafieldId } },
    'metafieldDelete never-created delete',
  );

  const fixture = {
    fixtureKind: 'local-runtime-metafield-delete-not-found',
    apiVersion,
    capturedAt: new Date().toISOString(),
    storeDomain: 'local-runtime.myshopify.com',
    summary:
      'Executable local-runtime fixture for singular metafieldDelete compatibility behavior when the public Admin schema exposes only metafieldsDelete.',
    expected: {
      setup: {
        response: setup,
      },
      happyDelete: {
        response: happyDelete,
      },
      repeatDelete: {
        response: repeatDelete,
      },
      neverCreatedDelete: {
        response: neverCreatedDelete,
      },
    },
    upstreamCalls: [],
  };

  await mkdir(path.dirname(fixturePath), { recursive: true });
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
} finally {
  proxy.dispose();
}
