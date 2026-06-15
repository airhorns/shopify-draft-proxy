/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createDraftProxy, type DraftProxyHttpResponse } from '../js/src/index.js';

type JsonRecord = Record<string, unknown>;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2026-04';
const scenarioId = 'online-store-integration-root-dispatch-local-runtime';
const requestPath = path.join(repoRoot, 'config', 'parity-requests', 'online-store', `${scenarioId}.graphql`);
const deleteRequestPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'online-store',
  'online-store-integration-root-dispatch-delete-local-runtime.graphql',
);
const readRequestPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'online-store',
  'online-store-integration-root-dispatch-read-local-runtime.graphql',
);
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'online-store',
  `${scenarioId}.json`,
);

function readObject(value: unknown, context: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function assertResponseOk(response: DraftProxyHttpResponse): JsonRecord {
  if (response.status !== 200) {
    throw new Error(`Proxy request returned HTTP ${response.status}: ${JSON.stringify(response.body)}`);
  }
  const body = readObject(response.body, 'proxy response body');
  if ('errors' in body) {
    throw new Error(`Proxy request returned top-level GraphQL errors: ${JSON.stringify(body['errors'])}`);
  }
  return body;
}

function readPath(value: unknown, parts: string[], context: string): unknown {
  let cursor = value;
  for (const part of parts) {
    cursor = readObject(cursor, context)[part];
  }
  return cursor;
}

function formatGeneratedJson(): void {
  const result = spawnSync('corepack', ['pnpm', 'exec', 'oxfmt', fixturePath], {
    cwd: repoRoot,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });

  if (result.status !== 0) {
    throw new Error(`Generated JSON formatting failed with status ${String(result.status)}`);
  }
}

const query = await readFile(requestPath, 'utf8');
const deleteQuery = await readFile(deleteRequestPath, 'utf8');
const readQuery = await readFile(readRequestPath, 'utf8');
const existingFixture = readObject(JSON.parse(await readFile(fixturePath, 'utf8')) as unknown, 'existing fixture');
const variables = readObject(
  readObject(existingFixture['primary'], 'fixture.primary')['variables'],
  'fixture.primary.variables',
);

const proxy = createDraftProxy({
  readMode: 'live-hybrid',
  unsupportedMutationMode: 'passthrough',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

try {
  const response = assertResponseOk(await proxy.processGraphQLRequest({ query, variables }, { apiVersion }));
  const scriptId = readPath(response, ['data', 'createdScript', 'scriptTag', 'id'], 'create response');
  if (typeof scriptId !== 'string') {
    throw new Error(`created script id was not a string: ${JSON.stringify(scriptId)}`);
  }
  const deleteVariables = { scriptId };
  const deleteResponse = assertResponseOk(
    await proxy.processGraphQLRequest({ query: deleteQuery, variables: deleteVariables }, { apiVersion }),
  );
  const readResponse = assertResponseOk(
    await proxy.processGraphQLRequest({ query: readQuery, variables: deleteVariables }, { apiVersion }),
  );
  const log = readObject(proxy.getLog(), 'proxy log');
  const entries = Array.isArray(log['entries']) ? log['entries'] : [];
  const selectedLog = {
    entries: entries.map((entry) => {
      const record = readObject(entry, 'log entry');
      return {
        interpreted: record['interpreted'],
        operationName: record['operationName'],
        stagedResourceIds: record['stagedResourceIds'],
        status: record['status'],
      };
    }),
  };

  const fixture = {
    fixtureKind: existingFixture['fixtureKind'],
    scenarioId,
    storeDomain: existingFixture['storeDomain'],
    apiVersion,
    capturedAt: existingFixture['capturedAt'],
    notes: existingFixture['notes'],
    primary: {
      variables,
      response,
    },
    delete: {
      variables: deleteVariables,
      response: deleteResponse,
    },
    readAfterDelete: {
      variables: deleteVariables,
      response: readResponse,
      log: selectedLog,
    },
    upstreamCalls: [],
  };

  await mkdir(path.dirname(fixturePath), { recursive: true });
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
  formatGeneratedJson();
  console.log(`Wrote ${path.relative(repoRoot, fixturePath)}`);
} finally {
  proxy.dispose();
}
