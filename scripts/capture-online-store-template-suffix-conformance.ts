/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'online-store');
const outputPath = path.join(outputDir, 'page-blog-article-template-suffix.json');
const requestRoot = path.join('config', 'parity-requests', 'online-store');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const blogCreateMutation = await readFile(
  path.join(requestRoot, 'page-blog-article-template-suffix-blog-create.graphql'),
  'utf8',
);
const pageCreateMutation = await readFile(
  path.join(requestRoot, 'page-blog-article-template-suffix-page-create.graphql'),
  'utf8',
);
const articleCreateMutation = await readFile(
  path.join(requestRoot, 'page-blog-article-template-suffix-article-create.graphql'),
  'utf8',
);
const updateMutation = await readFile(
  path.join(requestRoot, 'page-blog-article-template-suffix-update.graphql'),
  'utf8',
);
const readQuery = await readFile(path.join(requestRoot, 'page-blog-article-template-suffix-read.graphql'), 'utf8');
const deleteMutation = await readFile(
  path.join(requestRoot, 'page-blog-article-template-suffix-delete.graphql'),
  'utf8',
);

async function capture(
  captures: Capture[],
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<unknown> {
  const result = await runGraphqlRaw(query, variables);
  captures.push({
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  });
  return result.payload;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  return pathSegments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) {
      return null;
    }
    return (current as Record<string, unknown>)[segment] ?? null;
  }, value);
}

function readRequiredId(payload: unknown, pathSegments: string[], label: string): string {
  const id = readPath(payload, pathSegments);
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return an ID: ${JSON.stringify(payload, null, 2)}`);
  }
  return id;
}

function assertGraphqlOk(payload: unknown, label: string): void {
  if (readPath(payload, ['errors'])) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(payload, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathSegments: string[], label: string): void {
  const userErrors = readPath(payload, pathSegments);
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
}

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const titlePrefix = `Online Store Template Suffix ${suffix}`;
const captures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let blogId: string | null = null;
let pageId: string | null = null;
let articleId: string | null = null;

try {
  const blogCreate = await capture(captures, 'blog-create-with-template-suffix', blogCreateMutation, {
    blog: {
      title: `${titlePrefix} Blog`,
      templateSuffix: 'blog_custom',
    },
  });
  assertGraphqlOk(blogCreate, 'blog-create-with-template-suffix');
  assertNoUserErrors(blogCreate, ['data', 'blogCreate', 'userErrors'], 'blog-create-with-template-suffix');
  blogId = readRequiredId(blogCreate, ['data', 'blogCreate', 'blog', 'id'], 'blogCreate');

  const pageCreate = await capture(captures, 'page-create-with-template-suffix', pageCreateMutation, {
    page: {
      title: `${titlePrefix} Page`,
      templateSuffix: 'page_custom',
    },
  });
  assertGraphqlOk(pageCreate, 'page-create-with-template-suffix');
  assertNoUserErrors(pageCreate, ['data', 'pageCreate', 'userErrors'], 'page-create-with-template-suffix');
  pageId = readRequiredId(pageCreate, ['data', 'pageCreate', 'page', 'id'], 'pageCreate');

  const articleCreate = await capture(captures, 'article-create-with-template-suffix', articleCreateMutation, {
    article: {
      blogId,
      title: `${titlePrefix} Article`,
      body: '<p>Online store template suffix article body</p>',
      author: { name: 'Online Store Template Author' },
      templateSuffix: 'article_custom',
    },
  });
  assertGraphqlOk(articleCreate, 'article-create-with-template-suffix');
  assertNoUserErrors(articleCreate, ['data', 'articleCreate', 'userErrors'], 'article-create-with-template-suffix');
  articleId = readRequiredId(articleCreate, ['data', 'articleCreate', 'article', 'id'], 'articleCreate');

  const update = await capture(captures, 'page-blog-article-update-template-suffix', updateMutation, {
    blogId,
    blog: { templateSuffix: 'blog_updated' },
    pageId,
    page: { templateSuffix: 'page_updated' },
    articleId,
    article: { templateSuffix: 'article_updated' },
  });
  assertGraphqlOk(update, 'page-blog-article-update-template-suffix');
  assertNoUserErrors(update, ['data', 'blogUpdate', 'userErrors'], 'blogUpdate template suffix');
  assertNoUserErrors(update, ['data', 'pageUpdate', 'userErrors'], 'pageUpdate template suffix');
  assertNoUserErrors(update, ['data', 'articleUpdate', 'userErrors'], 'articleUpdate template suffix');

  const readAfterUpdate = await capture(captures, 'read-after-template-suffix-update', readQuery, {
    blogId,
    pageId,
    articleId,
  });
  assertGraphqlOk(readAfterUpdate, 'read-after-template-suffix-update');

  const clear = await capture(captures, 'page-blog-article-clear-template-suffix', updateMutation, {
    blogId,
    blog: { templateSuffix: '' },
    pageId,
    page: { templateSuffix: null },
    articleId,
    article: { templateSuffix: '' },
  });
  assertGraphqlOk(clear, 'page-blog-article-clear-template-suffix');
  assertNoUserErrors(clear, ['data', 'blogUpdate', 'userErrors'], 'blogUpdate clear template suffix');
  assertNoUserErrors(clear, ['data', 'pageUpdate', 'userErrors'], 'pageUpdate clear template suffix');
  assertNoUserErrors(clear, ['data', 'articleUpdate', 'userErrors'], 'articleUpdate clear template suffix');

  const readAfterClear = await capture(captures, 'read-after-template-suffix-clear', readQuery, {
    blogId,
    pageId,
    articleId,
  });
  assertGraphqlOk(readAfterClear, 'read-after-template-suffix-clear');

  const contentDelete = await capture(captures, 'content-delete-cleanup', deleteMutation, {
    articleId,
    pageId,
    blogId,
  });
  assertGraphqlOk(contentDelete, 'content-delete-cleanup');
  if (readPath(contentDelete, ['data', 'articleDelete', 'deletedArticleId'])) articleId = null;
  if (readPath(contentDelete, ['data', 'pageDelete', 'deletedPageId'])) pageId = null;
  if (readPath(contentDelete, ['data', 'blogDelete', 'deletedBlogId'])) blogId = null;
} finally {
  if (articleId && pageId && blogId) {
    await capture(cleanupCaptures, 'content-delete-cleanup-retry', deleteMutation, {
      articleId,
      pageId,
      blogId,
    });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'online-store-page-blog-article-template-suffix',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      interactions: captures,
      cleanup: cleanupCaptures,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
