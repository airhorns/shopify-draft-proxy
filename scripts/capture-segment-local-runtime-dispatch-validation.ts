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
  getLog: () => unknown;
  dispose: () => void;
};

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2026-04';
const scenarioId = 'segment-local-runtime-dispatch-validation';
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'segments',
  `${scenarioId}.json`,
);
const createRequestPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'segments',
  'segment-local-runtime-dispatch-validation.graphql',
);

function readObject(value: unknown, context: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function assertResponseOk(response: ProxyResponse, context: string): JsonRecord {
  if (response.status !== 200) {
    throw new Error(`${context} returned HTTP ${response.status}: ${JSON.stringify(response.body)}`);
  }
  const body = readObject(response.body, `${context} response body`);
  if ('errors' in body) {
    throw new Error(`${context} returned top-level GraphQL errors: ${JSON.stringify(body['errors'])}`);
  }
  return body;
}

const createQuery = await readFile(createRequestPath, 'utf8');
const existingFixture = readObject(JSON.parse(await readFile(fixturePath, 'utf8')) as unknown, 'existing fixture');
const createVariables = readObject(
  readObject(existingFixture['create'], 'fixture.create')['variables'],
  'fixture.create.variables',
);
const { createDraftProxy } = (await import('../js/src/index.js')) as {
  createDraftProxy: (options: { readMode: string; port: number; shopifyAdminOrigin: string }) => DraftProxyInstance;
};

const proxy = createDraftProxy({
  readMode: 'live-hybrid',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

try {
  const create = assertResponseOk(
    await proxy.processGraphQLRequest({ query: createQuery, variables: createVariables }, { apiVersion }),
    'segment create',
  );
  const log = proxy.getLog();

  const fixture = {
    fixtureKind: 'local-runtime-segment-dispatch-validation',
    scenarioId,
    storeDomain: 'local-runtime',
    apiVersion,
    capturedAt: existingFixture['capturedAt'],
    notes: existingFixture['notes'],
    create: {
      variables: createVariables,
      response: create,
      log,
    },
    upstreamCalls: [],
  };

  await mkdir(path.dirname(fixturePath), { recursive: true });
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
} finally {
  proxy.dispose();
}
