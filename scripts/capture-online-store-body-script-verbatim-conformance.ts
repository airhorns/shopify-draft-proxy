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
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'online-store');
const outputPath = path.join(outputDir, 'online-store-body-script-verbatim.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const blogCreateMutation = `#graphql
  mutation OnlineStoreBodyScriptBlogCreate($blog: BlogCreateInput!) {
    blogCreate(blog: $blog) {
      blog { id title handle }
      userErrors { field message code }
    }
  }
`;

const pageCreateMutation = `#graphql
  mutation OnlineStoreBodyScriptPageCreate($page: PageCreateInput!) {
    pageCreate(page: $page) {
      page { id title body bodySummary }
      userErrors { field message code }
    }
  }
`;

const pageReadQuery = `#graphql
  query OnlineStoreBodyScriptPageRead($id: ID!) {
    page(id: $id) { id body bodySummary }
  }
`;

const articleCreateMutation = `#graphql
  mutation OnlineStoreBodyScriptArticleCreate($article: ArticleCreateInput!) {
    articleCreate(article: $article) {
      article { id title body summary }
      userErrors { field message code }
    }
  }
`;

const articleReadQuery = `#graphql
  query OnlineStoreBodyScriptArticleRead($id: ID!) {
    article(id: $id) { id body summary }
  }
`;

const articleDeleteMutation = `#graphql
  mutation OnlineStoreBodyScriptArticleDelete($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message code }
    }
  }
`;

const pageDeleteMutation = `#graphql
  mutation OnlineStoreBodyScriptPageDelete($id: ID!) {
    pageDelete(id: $id) {
      deletedPageId
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation OnlineStoreBodyScriptBlogDelete($id: ID!) {
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
let pageId: string | null = null;
let articleId: string | null = null;

try {
  const blogCreate = await capture(captures, 'blogCreate-setup', blogCreateMutation, {
    blog: {
      title: `HAR 741 Body Script Blog ${suffix}`,
    },
  });
  blogId = readRequiredId(blogCreate, ['data', 'blogCreate', 'blog', 'id'], 'Blog setup');

  const pageCreate = await capture(captures, 'pageCreate-script-body', pageCreateMutation, {
    page: {
      title: `HAR 741 Body Script Page ${suffix}`,
      body: `<script>window.__har741='${apiVersion}'</script><p onclick="bad" class="safe">Keep page ${apiVersion}</p>`,
    },
  });
  pageId = readRequiredId(pageCreate, ['data', 'pageCreate', 'page', 'id'], 'Page create');

  await capture(captures, 'pageRead-after-create', pageReadQuery, { id: pageId });

  const articleCreate = await capture(captures, 'articleCreate-script-body', articleCreateMutation, {
    article: {
      title: `HAR 741 Body Script Article ${suffix}`,
      blogId,
      author: { name: 'HAR 741 Probe' },
      body: `<p onclick="bad">Keep article ${apiVersion}</p><script>window.__har741_article='${apiVersion}'</script>`,
    },
  });
  articleId = readRequiredId(articleCreate, ['data', 'articleCreate', 'article', 'id'], 'Article create');

  await capture(captures, 'articleRead-after-create', articleReadQuery, { id: articleId });
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
      scenarioId: 'online-store/body-script-verbatim',
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
