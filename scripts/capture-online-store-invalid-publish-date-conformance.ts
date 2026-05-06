/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
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
const outputPath = path.join(outputDir, 'online-store-invalid-publish-date.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const futurePublishDate = '2099-01-01T00:00:00Z';

const blogCreateMutation = `#graphql
  mutation OnlineStoreInvalidPublishDateBlogCreate($blog: BlogCreateInput!) {
    blogCreate(blog: $blog) {
      blog { id title handle }
      userErrors { field message code }
    }
  }
`;

const pageCreateMutation = `#graphql
  mutation OnlineStoreInvalidPublishDatePageCreate($page: PageCreateInput!) {
    pageCreate(page: $page) {
      page { id title isPublished publishedAt }
      userErrors { field message code }
    }
  }
`;

const pageUpdateMutation = `#graphql
  mutation OnlineStoreInvalidPublishDatePageUpdate($id: ID!, $page: PageUpdateInput!) {
    pageUpdate(id: $id, page: $page) {
      page { id title isPublished publishedAt }
      userErrors { field message code }
    }
  }
`;

const articleCreateMutation = `#graphql
  mutation OnlineStoreInvalidPublishDateArticleCreate($article: ArticleCreateInput!) {
    articleCreate(article: $article) {
      article { id title isPublished publishedAt }
      userErrors { field message code }
    }
  }
`;

const articleUpdateMutation = `#graphql
  mutation OnlineStoreInvalidPublishDateArticleUpdate($id: ID!, $article: ArticleUpdateInput!) {
    articleUpdate(id: $id, article: $article) {
      article { id title isPublished publishedAt }
      userErrors { field message code }
    }
  }
`;

const articleDeleteMutation = `#graphql
  mutation OnlineStoreInvalidPublishDateArticleDelete($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message code }
    }
  }
`;

const pageDeleteMutation = `#graphql
  mutation OnlineStoreInvalidPublishDatePageDelete($id: ID!) {
    pageDelete(id: $id) {
      deletedPageId
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation OnlineStoreInvalidPublishDateBlogDelete($id: ID!) {
    blogDelete(id: $id) {
      deletedBlogId
      userErrors { field message code }
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
    throw new Error(`${label} failed: ${JSON.stringify(payload)}`);
  }
  return id;
}

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const captures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let blogId: string | null = null;
let scheduledPageId: string | null = null;
let scheduledArticleId: string | null = null;

try {
  const blogCreate = await capture(captures, 'blogCreate-setup', blogCreateMutation, {
    blog: {
      title: `Invalid Publish Date Blog ${suffix}`,
    },
  });
  blogId = readRequiredId(blogCreate, ['data', 'blogCreate', 'blog', 'id'], 'Blog setup');

  await capture(captures, 'pageCreate-published-future-invalid', pageCreateMutation, {
    page: {
      title: `Invalid Publish Date Page ${suffix}`,
      isPublished: true,
      publishDate: futurePublishDate,
    },
  });

  await capture(captures, 'articleCreate-published-future-invalid', articleCreateMutation, {
    article: {
      title: `Invalid Publish Date Article ${suffix}`,
      body: '<p>Body</p>',
      blogId,
      author: { name: 'Invalid Publish Date Author' },
      isPublished: true,
      publishDate: futurePublishDate,
    },
  });

  const scheduledPage = await capture(captures, 'pageCreate-scheduled-setup', pageCreateMutation, {
    page: {
      title: `Scheduled Publish Date Page ${suffix}`,
      isPublished: false,
      publishDate: futurePublishDate,
    },
  });
  scheduledPageId = readRequiredId(scheduledPage, ['data', 'pageCreate', 'page', 'id'], 'Scheduled page setup');

  await capture(captures, 'pageUpdate-published-future-invalid', pageUpdateMutation, {
    id: scheduledPageId,
    page: {
      isPublished: true,
      publishDate: futurePublishDate,
    },
  });

  const scheduledArticle = await capture(captures, 'articleCreate-scheduled-setup', articleCreateMutation, {
    article: {
      title: `Scheduled Publish Date Article ${suffix}`,
      body: '<p>Body</p>',
      blogId,
      author: { name: 'Invalid Publish Date Author' },
      isPublished: false,
      publishDate: futurePublishDate,
    },
  });
  scheduledArticleId = readRequiredId(
    scheduledArticle,
    ['data', 'articleCreate', 'article', 'id'],
    'Scheduled article setup',
  );

  await capture(captures, 'articleUpdate-published-future-invalid', articleUpdateMutation, {
    id: scheduledArticleId,
    article: {
      isPublished: true,
      publishDate: futurePublishDate,
    },
  });
} finally {
  if (scheduledArticleId) {
    await capture(cleanupCaptures, 'articleDelete-cleanup', articleDeleteMutation, { id: scheduledArticleId });
  }
  if (scheduledPageId) {
    await capture(cleanupCaptures, 'pageDelete-cleanup', pageDeleteMutation, { id: scheduledPageId });
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
      scenarioId: 'online-store/invalid-publish-date',
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
