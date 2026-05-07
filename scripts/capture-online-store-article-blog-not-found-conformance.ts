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
const outputPath = path.join(outputDir, 'article-create-update-blog-not-found.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const blogCreateMutation = await readGraphql(
  'config/parity-requests/online-store/online-store-article-update-validation-blog-create.graphql',
);
const articleCreateMutation = await readGraphql(
  'config/parity-requests/online-store/online-store-article-create-validation-article-create.graphql',
);
const articleCreateSetupMutation = await readGraphql(
  'config/parity-requests/online-store/online-store-article-update-validation-article-create.graphql',
);
const articleUpdateMutation = await readGraphql(
  'config/parity-requests/online-store/online-store-article-update-validation-article-update.graphql',
);
const articleReadQuery = await readGraphql(
  'config/parity-requests/online-store/article-create-update-blog-not-found-read.graphql',
);

const articleDeleteMutation = `#graphql
  mutation OnlineStoreArticleBlogNotFoundArticleDelete($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation OnlineStoreArticleBlogNotFoundBlogDelete($id: ID!) {
    blogDelete(id: $id) {
      deletedBlogId
      userErrors { field message code }
    }
  }
`;

async function readGraphql(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

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
    throw new Error(`${label} failed: ${JSON.stringify(payload)}`);
  }
  return id;
}

function assertBlogNotFound(payload: unknown, root: 'articleCreate' | 'articleUpdate'): void {
  const article = readPath(payload, ['data', root, 'article']);
  const errors = readPath(payload, ['data', root, 'userErrors']);
  const expected = JSON.stringify([
    {
      field: ['article'],
      message: 'Must reference an existing blog.',
      code: 'NOT_FOUND',
    },
  ]);
  if (article !== null || JSON.stringify(errors) !== expected) {
    throw new Error(`${root} did not return expected blog NOT_FOUND payload: ${JSON.stringify(payload)}`);
  }
}

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const missingBlogId = 'gid://shopify/Blog/999999999999';
const setupTitle = `Article Blog Not Found Setup ${suffix}`;
const attemptedUpdateTitle = `Article Blog Not Found Should Not Apply ${suffix}`;
const captures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let blogId: string | null = null;
let articleId: string | null = null;

try {
  const blogCreate = await capture(captures, 'blogCreate-setup', blogCreateMutation, {
    blog: {
      title: `Article Blog Not Found Blog ${suffix}`,
    },
  });
  blogId = readRequiredId(blogCreate, ['data', 'blogCreate', 'blog', 'id'], 'Blog setup');

  const articleCreate = await capture(captures, 'articleCreate-setup', articleCreateSetupMutation, {
    article: {
      title: setupTitle,
      body: '<p>Body</p>',
      blogId,
      author: { name: 'Article Blog Not Found Author' },
    },
  });
  articleId = readRequiredId(articleCreate, ['data', 'articleCreate', 'article', 'id'], 'Article setup');

  const badCreate = await capture(captures, 'articleCreate-blog-not-found', articleCreateMutation, {
    article: {
      title: `Article Blog Not Found Create ${suffix}`,
      body: '<p>Body</p>',
      blogId: missingBlogId,
      author: { name: 'Article Blog Not Found Author' },
    },
  });
  assertBlogNotFound(badCreate, 'articleCreate');

  const badUpdate = await capture(captures, 'articleUpdate-blog-not-found', articleUpdateMutation, {
    id: articleId,
    article: {
      title: attemptedUpdateTitle,
      blogId: missingBlogId,
    },
  });
  assertBlogNotFound(badUpdate, 'articleUpdate');

  const readAfterUpdate = await capture(captures, 'articleRead-after-failed-update', articleReadQuery, {
    id: articleId,
  });
  const readTitle = readPath(readAfterUpdate, ['data', 'article', 'title']);
  if (readTitle !== setupTitle) {
    throw new Error(`Failed articleUpdate changed title: ${JSON.stringify(readAfterUpdate)}`);
  }
} finally {
  if (articleId) {
    await capture(cleanupCaptures, 'articleDelete-cleanup', articleDeleteMutation, { id: articleId });
  }
  if (blogId) {
    await capture(cleanupCaptures, 'blogDelete-cleanup', blogDeleteMutation, { id: blogId });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'online-store/article-create-update-blog-not-found',
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
