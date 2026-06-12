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
const scenarioId = 'location-activate-limit-and-relocation';
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'store-properties',
  `${scenarioId}.json`,
);
const requestPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'store-properties',
  'location-activate-limit-and-relocation.graphql',
);
const relocationMessage =
  'This location currently cannot be activated as inventory, pending orders or transfers are being relocated from this location. Please try again later.';

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

async function runProxyRequest(
  proxy: DraftProxyInstance,
  query: string,
  variables: JsonRecord,
  context: string,
): Promise<JsonRecord> {
  return assertResponseOk(await proxy.processGraphQLRequest({ query, variables }, { apiVersion }), context);
}

function readVariables(fixture: JsonRecord, name: string): JsonRecord {
  return readObject(
    readObject(fixture['proxyVariables'], 'fixture.proxyVariables')[name],
    `fixture.proxyVariables.${name}`,
  );
}

function assertRelocationMessage(body: JsonRecord): void {
  const data = readObject(body['data'], 'relocation response data');
  const payload = readObject(data['locationActivate'], 'relocation response payload');
  const errors = payload['locationActivateUserErrors'];
  if (!Array.isArray(errors)) {
    throw new Error(`relocation userErrors was not an array: ${JSON.stringify(errors)}`);
  }

  const firstError = readObject(errors[0], 'relocation first userError');
  if (firstError['message'] !== relocationMessage) {
    throw new Error(`relocation message diverged from Core i18n: ${JSON.stringify(firstError['message'])}`);
  }
}

const query = await readFile(requestPath, 'utf8');
const existingFixture = readObject(JSON.parse(await readFile(fixturePath, 'utf8')) as unknown, 'existing fixture');
const { createDraftProxy } = (await import('../js/src/index.js')) as {
  createDraftProxy: (options: { readMode: string; port: number; shopifyAdminOrigin: string }) => DraftProxyInstance;
};

const proxy = createDraftProxy({
  readMode: 'snapshot',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

try {
  const limit = await runProxyRequest(proxy, query, readVariables(existingFixture, 'limit'), 'limit activation');
  const relocation = await runProxyRequest(
    proxy,
    query,
    readVariables(existingFixture, 'relocation'),
    'relocation activation',
  );
  const control = await runProxyRequest(proxy, query, readVariables(existingFixture, 'control'), 'control activation');
  assertRelocationMessage(relocation);

  const fixture = {
    scenarioId,
    storeDomain: 'local-runtime',
    apiVersion,
    capturedAt: existingFixture['capturedAt'],
    notes: [
      'Executable local-runtime fixture for Core locationActivate validation flags that are not exposed as public Admin GraphQL Location fields.',
      'The configured live conformance credential probes successfully, but the public test shop is not at its location limit and does not expose an incomplete mass-relocation job. The cassette-backed hydrate responses encode the Core validation flags described by the source evidence.',
      "The HAS_ONGOING_RELOCATION message text is sourced from Shopify Core's shop_identity activate.has_ongoing_relocation i18n string, not from this synthetic local-runtime fixture.",
      'State is earned by replay: each locationActivate request hydrates the target location through StorePropertiesLocationHydrate before applying the supported mutation locally.',
    ],
    proxyVariables: existingFixture['proxyVariables'],
    expected: {
      limit,
      relocation,
      control,
    },
    upstreamCalls: existingFixture['upstreamCalls'],
  };

  await mkdir(path.dirname(fixturePath), { recursive: true });
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
} finally {
  proxy.dispose();
}
