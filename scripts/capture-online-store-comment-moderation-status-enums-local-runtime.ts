/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createDraftProxy, type DraftProxyHttpResponse } from '../js/src/index.js';

type JsonRecord = Record<string, unknown>;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2026-04';
const commentId = 'gid://shopify/Comment/local-moderation';
const hydrateQuery =
  'query OnlineStoreCommentHydrate($id: ID!) { comment(id: $id) { __typename id status body bodyHtml isPublished publishedAt createdAt updatedAt article { id } } }';
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'online-store',
  'comment-moderation-status-enums.json',
);

const requestPaths = {
  spam: path.join(repoRoot, 'config', 'parity-requests', 'online-store', 'comment-moderation-status-spam.graphql'),
  notSpam: path.join(
    repoRoot,
    'config',
    'parity-requests',
    'online-store',
    'comment-moderation-status-not-spam.graphql',
  ),
  approve: path.join(
    repoRoot,
    'config',
    'parity-requests',
    'online-store',
    'comment-moderation-status-approve.graphql',
  ),
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

async function readRequestBody(request: IncomingMessage): Promise<string> {
  return await new Promise<string>((resolve, reject) => {
    let body = '';
    request.setEncoding('utf8');
    request.on('data', (chunk) => (body += chunk));
    request.on('end', () => resolve(body));
    request.on('error', reject);
  });
}

async function startHydrateServer(comment: unknown): Promise<{ origin: string; close: () => Promise<void> }> {
  const server = createServer((request: IncomingMessage, response: ServerResponse) => {
    void (async () => {
      const rawBody = await readRequestBody(request);
      const body = readObject(JSON.parse(rawBody) as unknown, 'upstream request body');
      const query = body['query'];
      const variables = readObject(body['variables'], 'upstream request variables');
      if (query !== hydrateQuery || variables['id'] !== commentId) {
        response.statusCode = 500;
        response.setHeader('content-type', 'application/json');
        response.end(JSON.stringify({ errors: [{ message: `Unexpected upstream hydrate request: ${rawBody}` }] }));
        return;
      }
      response.statusCode = 200;
      response.setHeader('content-type', 'application/json');
      response.end(JSON.stringify({ data: { comment } }));
    })().catch((error) => {
      response.statusCode = 500;
      response.setHeader('content-type', 'application/json');
      response.end(JSON.stringify({ errors: [{ message: String(error) }] }));
    });
  });
  await new Promise<void>((resolveListen) => server.listen(0, '127.0.0.1', resolveListen));
  const address = server.address();
  if (address === null || typeof address === 'string') throw new Error('Failed to start local hydrate server');
  return {
    origin: `http://127.0.0.1:${address.port}`,
    close: async () =>
      await new Promise<void>((resolveClose, reject) =>
        server.close((error) => (error ? reject(error) : resolveClose())),
      ),
  };
}

const existingFixture = readObject(JSON.parse(await readFile(fixturePath, 'utf8')) as unknown, 'existing fixture');
const variables = readObject(existingFixture['variables'], 'fixture variables');
const existingUpstreamCall = readObject(
  readObject(
    Array.isArray(existingFixture['upstreamCalls']) ? existingFixture['upstreamCalls'][0] : undefined,
    'fixture upstreamCalls[0]',
  )['response'],
  'fixture upstreamCalls[0].response',
);
const hydratedComment = readObject(existingUpstreamCall['body'], 'fixture hydrate body')['data'];
const comment = readObject(hydratedComment, 'fixture hydrate data')['comment'];
const queries = {
  spam: await readFile(requestPaths.spam, 'utf8'),
  notSpam: await readFile(requestPaths.notSpam, 'utf8'),
  approve: await readFile(requestPaths.approve, 'utf8'),
};
const upstream = await startHydrateServer(comment);
const proxy = createDraftProxy({
  readMode: 'live-hybrid',
  unsupportedMutationMode: 'passthrough',
  port: 0,
  shopifyAdminOrigin: upstream.origin,
});

try {
  const spamResponse = assertResponseOk(
    await proxy.processGraphQLRequest({ query: queries.spam, variables }, { apiVersion }),
    'commentSpam',
  );
  const notSpamResponse = assertResponseOk(
    await proxy.processGraphQLRequest({ query: queries.notSpam, variables }, { apiVersion }),
    'commentNotSpam',
  );
  const approveResponse = assertResponseOk(
    await proxy.processGraphQLRequest({ query: queries.approve, variables }, { apiVersion }),
    'commentApprove',
  );

  const fixture = {
    ...existingFixture,
    upstreamCalls: [
      {
        operationName: 'OnlineStoreCommentHydrate',
        query: hydrateQuery,
        variables,
        response: {
          status: 200,
          body: { data: { comment } },
        },
      },
    ],
    spam: { response: spamResponse },
    notSpam: { response: notSpamResponse },
    approve: { response: approveResponse },
  };

  await mkdir(path.dirname(fixturePath), { recursive: true });
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
  formatGeneratedJson();
  console.log(`Wrote ${path.relative(repoRoot, fixturePath)}`);
} finally {
  proxy.dispose();
  await upstream.close();
}
