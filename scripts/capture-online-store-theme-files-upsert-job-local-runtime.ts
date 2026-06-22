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
const scenarioId = 'online-store/theme-files-upsert-job';
const requestPath = path.join(repoRoot, 'config', 'parity-requests', 'online-store', 'theme-files-upsert-job.graphql');
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'online-store',
  'theme-files-upsert-job.json',
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

function assertJobPayload(body: JsonRecord): void {
  const data = readObject(body['data'], 'response.data');
  const inline = readObject(data['inline'], 'response.data.inline');
  if (!Object.hasOwn(inline, 'job') || inline['job'] !== null) {
    throw new Error(`Inline upsert should include job: null, got ${JSON.stringify(inline['job'])}`);
  }

  const remote = readObject(data['remote'], 'response.data.remote');
  const remoteJob = readObject(remote['job'], 'response.data.remote.job');
  if (typeof remoteJob['id'] !== 'string' || !remoteJob['id'].startsWith('gid://shopify/Job/')) {
    throw new Error(`URL-body upsert should include a Job gid, got ${JSON.stringify(remoteJob)}`);
  }
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
const variables = {};
const proxy = createDraftProxy({
  readMode: 'live-hybrid',
  unsupportedMutationMode: 'reject',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

try {
  const body = assertResponseOk(await proxy.processGraphQLRequest({ query, variables }, { apiVersion }));
  assertJobPayload(body);

  const fixture = {
    fixtureKind: 'local-runtime-theme-files-upsert-job',
    scenarioId,
    storeDomain: 'local-runtime',
    apiVersion,
    capturedAt: '2026-06-22T00:00:00.000Z',
    summary: 'Executable local-runtime fixture for themeFilesUpsert job payload shape.',
    primary: {
      variables,
    },
    expected: {
      primary: body,
    },
    evidence: {
      source: 'local-runtime',
      notes: [
        'The request stages one inline TEXT theme file and one URL-body theme file through the public GraphQL mutation path.',
        'Inline theme file writes must include job: null; URL-body writes must include a synthetic Job GID.',
        'No upstream Shopify calls are expected because themeFilesUpsert is handled locally.',
      ],
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
