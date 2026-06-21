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
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'online-store',
  'theme-publish-demotes-previous-main.json',
);

const requestPaths = {
  main: path.join(repoRoot, 'config', 'parity-requests', 'online-store', 'theme-publish-create-main.graphql'),
  next: path.join(repoRoot, 'config', 'parity-requests', 'online-store', 'theme-publish-create-unpublished.graphql'),
  publish: path.join(repoRoot, 'config', 'parity-requests', 'online-store', 'theme-publish-publish.graphql'),
  downstreamRead: path.join(repoRoot, 'config', 'parity-requests', 'online-store', 'theme-publish-read.graphql'),
};

function readObject(value: unknown, context: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function assertResponseOk(response: DraftProxyHttpResponse, context: string): JsonRecord {
  if (response.status !== 200) {
    throw new Error(`${context} returned HTTP ${response.status}: ${JSON.stringify(response.body)}`);
  }
  const body = readObject(response.body, `${context} response body`);
  if ('errors' in body) {
    throw new Error(`${context} returned top-level GraphQL errors: ${JSON.stringify(body['errors'])}`);
  }
  return body;
}

function readPath(value: unknown, parts: string[], context: string): unknown {
  let cursor = value;
  for (const part of parts) cursor = readObject(cursor, context)[part];
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

const existingFixture = readObject(JSON.parse(await readFile(fixturePath, 'utf8')) as unknown, 'existing fixture');
const queries = {
  main: await readFile(requestPaths.main, 'utf8'),
  next: await readFile(requestPaths.next, 'utf8'),
  publish: await readFile(requestPaths.publish, 'utf8'),
  downstreamRead: await readFile(requestPaths.downstreamRead, 'utf8'),
};
const mainVariables = readObject(readObject(existingFixture['main'], 'fixture.main')['variables'], 'main variables');
const nextVariables = readObject(readObject(existingFixture['next'], 'fixture.next')['variables'], 'next variables');

const proxy = createDraftProxy({
  readMode: 'snapshot',
  unsupportedMutationMode: 'reject',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

try {
  const mainResponse = assertResponseOk(
    await proxy.processGraphQLRequest({ query: queries.main, variables: mainVariables }, { apiVersion }),
    'create current main',
  );
  const nextResponse = assertResponseOk(
    await proxy.processGraphQLRequest({ query: queries.next, variables: nextVariables }, { apiVersion }),
    'create next theme',
  );
  const previousId = readPath(mainResponse, ['data', 'themeCreate', 'theme', 'id'], 'main response');
  const nextId = readPath(nextResponse, ['data', 'themeCreate', 'theme', 'id'], 'next response');
  if (typeof previousId !== 'string' || typeof nextId !== 'string') {
    throw new Error(`Theme IDs were not strings: ${JSON.stringify({ previousId, nextId })}`);
  }

  const publishVariables = { id: nextId };
  const publishResponse = assertResponseOk(
    await proxy.processGraphQLRequest({ query: queries.publish, variables: publishVariables }, { apiVersion }),
    'publish next theme',
  );
  const downstreamReadVariables = { previousId };
  const downstreamReadResponse = assertResponseOk(
    await proxy.processGraphQLRequest(
      { query: queries.downstreamRead, variables: downstreamReadVariables },
      { apiVersion },
    ),
    'downstream read',
  );

  const fixture = {
    ...existingFixture,
    main: {
      variables: mainVariables,
      response: { payload: mainResponse },
    },
    next: {
      variables: nextVariables,
      response: { payload: nextResponse },
    },
    publish: {
      variables: publishVariables,
      response: { payload: publishResponse },
    },
    downstreamRead: {
      variables: downstreamReadVariables,
      response: { payload: downstreamReadResponse },
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
