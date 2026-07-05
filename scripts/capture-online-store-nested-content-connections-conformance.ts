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

type RestComment = {
  id: number;
  body: string;
  article_id: number;
  blog_id: number;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const authHeaders = buildAdminAuthHeaders(adminAccessToken);
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'online-store');
const outputPath = path.join(outputDir, 'online-store-nested-content-connections.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: authHeaders,
});

const hydrateQuery = await readFile(
  'config/parity-requests/online-store/online-store-nested-content-connections-hydrate.graphql',
  'utf8',
);
const blogCreateMutation = await readFile(
  'config/parity-requests/online-store/online-store-nested-content-connections-blog-create.graphql',
  'utf8',
);
const articleCreateMutation = await readFile(
  'config/parity-requests/online-store/online-store-nested-content-connections-article-create.graphql',
  'utf8',
);
const commentSpamMutation = await readFile(
  'config/parity-requests/online-store/online-store-nested-content-connections-comment-spam.graphql',
  'utf8',
);
const readWindowsQuery = await readFile(
  'config/parity-requests/online-store/online-store-nested-content-connections-read-windows.graphql',
  'utf8',
);
const readCursorsQuery = await readFile(
  'config/parity-requests/online-store/online-store-nested-content-connections-read-cursors.graphql',
  'utf8',
);

const articleDeleteMutation = `#graphql
  mutation OnlineStoreNestedContentConnectionsArticleCleanup($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation OnlineStoreNestedContentConnectionsBlogCleanup($id: ID!) {
    blogDelete(id: $id) {
      deletedBlogId
      userErrors { field message code }
    }
  }
`;

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      current = current[Number(segment)];
      continue;
    }
    if (!current || typeof current !== 'object') {
      return undefined;
    }
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}

function readObject(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`Expected object at ${label}.`);
  }
  return value as Record<string, unknown>;
}

function readString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Expected non-empty string at ${label}.`);
  }
  return value;
}

function readData(capture: Capture): Record<string, unknown> {
  return readObject(readPath(capture.response, ['data']), `${capture.name}.response.data`);
}

function assertNoUserErrors(payload: Record<string, unknown>, label: string): void {
  const userErrors = payload['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function readCreatedId(capture: Capture, mutationName: string, resourceName: string): string {
  const payload = readObject(readData(capture)[mutationName], `${capture.name}.${mutationName}`);
  assertNoUserErrors(payload, `${capture.name}.${mutationName}`);
  const resource = readObject(payload[resourceName], `${capture.name}.${mutationName}.${resourceName}`);
  return readString(resource['id'], `${capture.name}.${mutationName}.${resourceName}.id`);
}

function readNodes(capture: Capture, pathSegments: string[]): Record<string, unknown>[] {
  const nodes = readPath(capture.response, pathSegments);
  if (!Array.isArray(nodes)) {
    throw new Error(`Expected nodes array at ${capture.name}.${pathSegments.join('.')}.`);
  }
  return nodes.map((node, index) => readObject(node, `${capture.name}.${pathSegments.join('.')}[${index}]`));
}

function readCursor(capture: Capture, pathSegments: string[]): string {
  return readString(readPath(capture.response, pathSegments), `${capture.name}.${pathSegments.join('.')}`);
}

function hasNodeValue(nodes: Record<string, unknown>[], field: string, expected: string): boolean {
  return nodes.some((node) => node[field] === expected);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function capture(label: string, query: string, variables: Record<string, unknown> = {}): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    name: label,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

async function cleanup(label: string, query: string, id: string | null, captures: Capture[]): Promise<void> {
  if (!id) {
    return;
  }
  captures.push(await capture(label, query, { id }));
}

function numericId(gid: string): string {
  const id = gid.split('/').at(-1);
  if (!id) {
    throw new Error(`Could not read numeric id from ${gid}`);
  }
  return id;
}

function commentGid(comment: RestComment): string {
  return `gid://shopify/Comment/${comment.id}`;
}

async function createComment(blogId: string, articleId: string, body: string): Promise<RestComment> {
  const response = await fetch(
    `${adminOrigin}/admin/api/${apiVersion}/blogs/${numericId(blogId)}/articles/${numericId(articleId)}/comments.json`,
    {
      method: 'POST',
      headers: {
        ...authHeaders,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        comment: {
          body,
          body_html: `<p>${body}</p>`,
          author: 'Nested Content Connections Fixture',
          email: 'nested-content-connections@example.com',
          ip: '127.0.0.1',
          status: 'unapproved',
        },
      }),
    },
  );
  const payload = (await response.json()) as { comment?: RestComment; errors?: unknown };
  if (response.status < 200 || response.status >= 300 || !payload.comment) {
    throw new Error(`Comment setup failed: HTTP ${response.status} ${JSON.stringify(payload)}`);
  }
  return payload.comment;
}

async function waitForNestedWindows(
  variables: Record<string, unknown>,
  expected: {
    alphaArticleTitle: string;
    alphaCommentBody: string;
    bravoCommentBody: string;
  },
): Promise<void> {
  let lastMisses: string[] = [];
  for (let attempt = 1; attempt <= 8; attempt += 1) {
    const read = await capture('nested windows probe', readWindowsQuery, variables);
    const articleFirst = readNodes(read, ['data', 'blog', 'articlesFirst', 'nodes']);
    const articleAll = readNodes(read, ['data', 'blog', 'articlesAll', 'nodes']);
    const articleReverse = readNodes(read, ['data', 'blog', 'articlesReverse', 'nodes']);
    const commentFirst = readNodes(read, ['data', 'article', 'commentsFirst', 'nodes']);
    const commentAll = readNodes(read, ['data', 'article', 'commentsAll', 'nodes']);
    const commentReverse = readNodes(read, ['data', 'article', 'commentsReverse', 'nodes']);
    const commentFiltered = readNodes(read, ['data', 'article', 'commentsFiltered', 'nodes']);

    lastMisses = [
      articleFirst.length === 2 ? null : `articlesFirst returned ${articleFirst.length} nodes`,
      articleAll.length === 3 ? null : `articlesAll returned ${articleAll.length} nodes`,
      articleReverse.length === 2 ? null : `articlesReverse returned ${articleReverse.length} nodes`,
      hasNodeValue(commentFirst, 'body', expected.alphaCommentBody)
        ? null
        : `commentsFirst missing ${expected.alphaCommentBody}`,
      hasNodeValue(commentFirst, 'body', expected.bravoCommentBody)
        ? null
        : `commentsFirst missing ${expected.bravoCommentBody}`,
      commentAll.length === 3 ? null : `commentsAll returned ${commentAll.length} nodes`,
      commentReverse.length === 2 ? null : `commentsReverse returned ${commentReverse.length} nodes`,
      hasNodeValue(commentFiltered, 'body', expected.alphaCommentBody)
        ? null
        : `commentsFiltered missing spammed alpha comment`,
    ].filter((miss): miss is string => typeof miss === 'string');

    if (lastMisses.length === 0) {
      return;
    }
    await sleep(attempt * 1000);
  }

  throw new Error(`Nested connection reads did not contain expected setup rows: ${lastMisses.join('; ')}`);
}

function upstreamCall(operationName: string, variables: Record<string, unknown>, captureResult: Capture): unknown {
  return {
    operationName,
    variables,
    query: captureResult.request.query,
    response: {
      status: captureResult.status,
      body: captureResult.response,
    },
  };
}

const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const blogTitle = `Nested Content Connections Blog ${suffix}`;
const alphaArticleTitle = `Nested Content Connections Alpha ${suffix}`;
const bravoArticleTitle = `Nested Content Connections Bravo ${suffix}`;
const charlieArticleTitle = `Nested Content Connections Charlie ${suffix}`;
const alphaTag = `nested-content-alpha-${suffix}`;
const sharedTag = `nested-content-${suffix}`;
const alphaCommentBody = `Nested Content Connections Comment Alpha ${suffix}`;
const bravoCommentBody = `Nested Content Connections Comment Bravo ${suffix}`;
const charlieCommentBody = `Nested Content Connections Comment Charlie ${suffix}`;
const captures: Capture[] = [];
let blogId: string | null = null;
let alphaArticleId: string | null = null;
let bravoArticleId: string | null = null;
let charlieArticleId: string | null = null;
let cleanupCaptured = false;

try {
  const blogCreateVariables = {
    blog: {
      title: blogTitle,
      commentPolicy: 'MODERATED',
    },
  };
  const blogCreate = await capture('blogCreate setup', blogCreateMutation, blogCreateVariables);
  captures.push(blogCreate);
  blogId = readCreatedId(blogCreate, 'blogCreate', 'blog');

  const alphaArticleCreateVariables = {
    article: {
      blogId,
      title: alphaArticleTitle,
      body: '<p>Nested content connections alpha body.</p>',
      summary: '<p>Nested content connections alpha summary.</p>',
      isPublished: true,
      tags: [sharedTag, alphaTag],
      author: { name: `Nested Connections Author ${suffix}` },
    },
  };
  const alphaArticleCreate = await capture(
    'articleCreate setup alpha',
    articleCreateMutation,
    alphaArticleCreateVariables,
  );
  captures.push(alphaArticleCreate);
  alphaArticleId = readCreatedId(alphaArticleCreate, 'articleCreate', 'article');

  const bravoArticleCreateVariables = {
    article: {
      blogId,
      title: bravoArticleTitle,
      body: '<p>Nested content connections bravo body.</p>',
      summary: '<p>Nested content connections bravo summary.</p>',
      isPublished: true,
      tags: [sharedTag],
      author: { name: `Nested Connections Author ${suffix}` },
    },
  };
  const bravoArticleCreate = await capture(
    'articleCreate setup bravo',
    articleCreateMutation,
    bravoArticleCreateVariables,
  );
  captures.push(bravoArticleCreate);
  bravoArticleId = readCreatedId(bravoArticleCreate, 'articleCreate', 'article');

  const charlieArticleCreateVariables = {
    article: {
      blogId,
      title: charlieArticleTitle,
      body: '<p>Nested content connections charlie body.</p>',
      summary: '<p>Nested content connections charlie summary.</p>',
      isPublished: true,
      tags: [sharedTag],
      author: { name: `Nested Connections Author ${suffix}` },
    },
  };
  const charlieArticleCreate = await capture(
    'articleCreate setup charlie',
    articleCreateMutation,
    charlieArticleCreateVariables,
  );
  captures.push(charlieArticleCreate);
  charlieArticleId = readCreatedId(charlieArticleCreate, 'articleCreate', 'article');

  const alphaComment = await createComment(blogId, alphaArticleId, alphaCommentBody);
  await sleep(1100);
  const bravoComment = await createComment(blogId, alphaArticleId, bravoCommentBody);
  await sleep(1100);
  const charlieComment = await createComment(blogId, alphaArticleId, charlieCommentBody);

  const hydrateVariables = { articleId: alphaArticleId };
  const hydrate = await capture('article comments hydrate', hydrateQuery, hydrateVariables);
  captures.push(hydrate);

  const commentSpamVariables = { id: commentGid(alphaComment) };
  const commentSpam = await capture('commentSpam setup alpha', commentSpamMutation, commentSpamVariables);
  captures.push(commentSpam);

  const readWindowsVariables = {
    blogId,
    articleId: alphaArticleId,
    commentFilterQuery: 'status:SPAM',
  };
  await waitForNestedWindows(readWindowsVariables, {
    alphaArticleTitle,
    alphaCommentBody,
    bravoCommentBody,
  });
  const readWindows = await capture('nested connection windows', readWindowsQuery, readWindowsVariables);
  captures.push(readWindows);

  const readCursorsVariables = {
    blogId,
    articleId: alphaArticleId,
    articleAfter: readCursor(readWindows, ['data', 'blog', 'articlesFirst', 'edges', '1', 'cursor']),
    articleBefore: readCursor(readWindows, ['data', 'blog', 'articlesAll', 'edges', '2', 'cursor']),
    commentAfter: readCursor(readWindows, ['data', 'article', 'commentsFirst', 'edges', '1', 'cursor']),
    commentBefore: readCursor(readWindows, ['data', 'article', 'commentsAll', 'edges', '2', 'cursor']),
  };
  const readCursors = await capture('nested connection cursor windows', readCursorsQuery, readCursorsVariables);
  captures.push(readCursors);

  await cleanup('articleDelete cleanup charlie', articleDeleteMutation, charlieArticleId, captures);
  charlieArticleId = null;
  await cleanup('articleDelete cleanup bravo', articleDeleteMutation, bravoArticleId, captures);
  bravoArticleId = null;
  await cleanup('articleDelete cleanup alpha', articleDeleteMutation, alphaArticleId, captures);
  alphaArticleId = null;
  await cleanup('blogDelete cleanup', blogDeleteMutation, blogId, captures);
  blogId = null;
  cleanupCaptured = true;

  const fixture = {
    scenarioId: 'online-store-nested-content-connections',
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes:
      'Live Shopify capture for nested Blog.articles and Article.comments connection windows, reverse ordering, comment query filters, and cursor-page reads. The scenario creates disposable blog/article records through Admin GraphQL, creates REST comments only as setup, records a cold article comments hydrate cassette, stages commentSpam through Admin GraphQL, records nested reads, and deletes the disposable content during cleanup.',
    variables: {
      hydrate: hydrateVariables,
      blogCreate: blogCreateVariables,
      articleCreateAlpha: alphaArticleCreateVariables,
      articleCreateBravo: bravoArticleCreateVariables,
      articleCreateCharlie: charlieArticleCreateVariables,
      commentSpam: commentSpamVariables,
      readWindows: readWindowsVariables,
      readCursors: readCursorsVariables,
    },
    restComments: [alphaComment, bravoComment, charlieComment],
    interactions: captures,
    upstreamCalls: [upstreamCall('OnlineStoreNestedContentConnectionsHydrate', hydrateVariables, hydrate)],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (!cleanupCaptured) {
    const cleanupCaptures: Capture[] = [];
    try {
      await cleanup(
        'articleDelete cleanup charlie after failure',
        articleDeleteMutation,
        charlieArticleId,
        cleanupCaptures,
      );
      await cleanup(
        'articleDelete cleanup bravo after failure',
        articleDeleteMutation,
        bravoArticleId,
        cleanupCaptures,
      );
      await cleanup(
        'articleDelete cleanup alpha after failure',
        articleDeleteMutation,
        alphaArticleId,
        cleanupCaptures,
      );
      await cleanup('blogDelete cleanup after failure', blogDeleteMutation, blogId, cleanupCaptures);
    } catch (error) {
      console.warn(`Cleanup after failure did not complete: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}
