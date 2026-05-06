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
const outputPath = path.join(outputDir, 'online-store-content-lifecycle.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const contentCreateMutation = await readFile(
  'config/parity-requests/online-store/online-store-content-create.graphql',
  'utf8',
);
const articleCreateMutation = await readFile(
  'config/parity-requests/online-store/online-store-content-article-create.graphql',
  'utf8',
);
const contentUpdateMutation = await readFile(
  'config/parity-requests/online-store/online-store-content-update.graphql',
  'utf8',
);
const downstreamReadQuery = await readFile(
  'config/parity-requests/online-store/online-store-content-read-after-update.graphql',
  'utf8',
);
const commentUnknownMutation = await readFile(
  'config/parity-requests/online-store/online-store-content-comment-unknown.graphql',
  'utf8',
);
const contentDeleteMutation = await readFile(
  'config/parity-requests/online-store/online-store-content-delete.graphql',
  'utf8',
);

const baselineQuery = `#graphql
  query OnlineStoreContentBaseline($missingArticle: ID!, $missingBlog: ID!, $missingPage: ID!) {
    article(id: $missingArticle) { id title }
    blog(id: $missingBlog) { id title }
    page(id: $missingPage) { id title }
    articles(first: 5) {
      edges {
        cursor
        node {
          id
          title
          handle
          body
          summary
          tags
          isPublished
          publishedAt
          createdAt
          updatedAt
          templateSuffix
          blog { id title handle }
          author { name }
          commentsCount { count precision }
          comments(first: 5) {
            nodes { id body status }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
      }
      nodes { id title handle }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    articleAuthors(first: 5) {
      edges { cursor node { name } }
      nodes { name }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    articleTags(limit: 20)
    blogs(first: 5) {
      edges {
        cursor
        node {
          id
          title
          handle
          commentPolicy
          tags
          templateSuffix
          createdAt
          updatedAt
          articlesCount { count precision }
          articles(first: 5) {
            nodes { id title handle }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
      }
      nodes { id title handle }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    blogsCount { count precision }
    pages(first: 5) {
      edges {
        cursor
        node {
          id
          title
          handle
          body
          bodySummary
          isPublished
          publishedAt
          createdAt
          updatedAt
          templateSuffix
        }
      }
      nodes { id title handle }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    pagesCount { count precision }
    comments(first: 5) {
      edges {
        cursor
        node {
          id
          body
          bodyHtml
          status
          isPublished
          publishedAt
          createdAt
          updatedAt
          ip
          userAgent
          article { id title }
          author { name }
        }
      }
      nodes { id body status }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
`;

const articleDeleteMutation = `#graphql
  mutation OnlineStoreContentLifecycleArticleCleanup($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message }
    }
  }
`;

const pageDeleteMutation = `#graphql
  mutation OnlineStoreContentLifecyclePageCleanup($id: ID!) {
    pageDelete(id: $id) {
      deletedPageId
      userErrors { field message }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation OnlineStoreContentLifecycleBlogCleanup($id: ID!) {
    blogDelete(id: $id) {
      deletedBlogId
      userErrors { field message }
    }
  }
`;

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
    throw new Error(`${label} did not return an ID: ${JSON.stringify(payload)}`);
  }
  return id;
}

function assertNoTopLevelErrors(payload: unknown, label: string): void {
  if (readPath(payload, ['errors'])) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(payload, null, 2)}`);
  }
}

function upstreamCountCall(operationName: string, query: string, rootField: string, baseline: unknown): unknown {
  return {
    operationName,
    variables: {},
    query,
    response: {
      status: 200,
      body: {
        data: {
          [rootField]: readPath(baseline, ['data', rootField]),
        },
      },
    },
  };
}

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const titlePrefix = `Online Store Content Lifecycle ${suffix}`;
const articleTag = `content-lifecycle-${suffix}`;
const captures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let blogId: string | null = null;
let pageId: string | null = null;
let articleId: string | null = null;
let baselinePayload: unknown = null;

try {
  baselinePayload = await capture(captures, 'baseline-catalog-detail-empty', baselineQuery, {
    missingArticle: 'gid://shopify/Article/999999999999',
    missingBlog: 'gid://shopify/Blog/999999999999',
    missingPage: 'gid://shopify/Page/999999999999',
  });

  const contentCreate = await capture(captures, 'content-create-success', contentCreateMutation, {
    blog: {
      title: `${titlePrefix} Blog`,
      commentPolicy: 'MODERATED',
    },
    page: {
      title: `${titlePrefix} Page`,
      body: '<p>Online store content lifecycle page body</p>',
      isPublished: true,
    },
  });
  assertNoTopLevelErrors(contentCreate, 'content-create-success');
  blogId = readRequiredId(contentCreate, ['data', 'blogCreate', 'blog', 'id'], 'blogCreate');
  pageId = readRequiredId(contentCreate, ['data', 'pageCreate', 'page', 'id'], 'pageCreate');

  const articleCreate = await capture(captures, 'article-create-success', articleCreateMutation, {
    article: {
      blogId,
      title: `${titlePrefix} Article`,
      body: '<p>Online store content lifecycle article body</p>',
      summary: '<p>Lifecycle summary</p>',
      isPublished: true,
      tags: [articleTag, 'online-store'],
      author: { name: 'Online Store Lifecycle Author' },
    },
  });
  assertNoTopLevelErrors(articleCreate, 'article-create-success');
  articleId = readRequiredId(articleCreate, ['data', 'articleCreate', 'article', 'id'], 'articleCreate');

  const contentUpdate = await capture(captures, 'content-update-success', contentUpdateMutation, {
    blogId,
    blog: {
      title: `${titlePrefix} Blog Updated`,
      commentPolicy: 'CLOSED',
    },
    pageId,
    page: {
      title: `${titlePrefix} Page Updated`,
      body: '<p>Updated online store content lifecycle page body</p>',
      isPublished: false,
    },
    articleId,
    article: {
      title: `${titlePrefix} Article Updated`,
      isPublished: false,
      author: { name: 'Online Store Lifecycle Author Updated' },
    },
  });
  assertNoTopLevelErrors(contentUpdate, 'content-update-success');

  await capture(captures, 'downstream-read-after-writes', downstreamReadQuery, {
    blogId,
    pageId,
    articleId,
    articleQuery: articleTag,
  });

  await capture(captures, 'comment-moderation-unknown-id', commentUnknownMutation, {
    id: 'gid://shopify/Comment/999999999999',
  });

  const contentDelete = await capture(captures, 'content-delete-success', contentDeleteMutation, {
    articleId,
    pageId,
    blogId,
  });
  assertNoTopLevelErrors(contentDelete, 'content-delete-success');
  if (readPath(contentDelete, ['data', 'articleDelete', 'deletedArticleId'])) articleId = null;
  if (readPath(contentDelete, ['data', 'pageDelete', 'deletedPageId'])) pageId = null;
  if (readPath(contentDelete, ['data', 'blogDelete', 'deletedBlogId'])) blogId = null;
} finally {
  if (articleId) {
    await capture(cleanupCaptures, 'articleDelete-cleanup', articleDeleteMutation, { id: articleId });
  }
  if (pageId) {
    await capture(cleanupCaptures, 'pageDelete-cleanup', pageDeleteMutation, { id: pageId });
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
      scenarioId: 'online-store-content-lifecycle',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      interactions: captures,
      cleanup: cleanupCaptures,
      upstreamCalls: [
        upstreamCountCall(
          'OnlineStoreBlogsCountHydrate',
          'query OnlineStoreBlogsCountHydrate { blogsCount { count precision } }',
          'blogsCount',
          baselinePayload,
        ),
        upstreamCountCall(
          'OnlineStorePagesCountHydrate',
          'query OnlineStorePagesCountHydrate { pagesCount { count precision } }',
          'pagesCount',
          baselinePayload,
        ),
      ],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
