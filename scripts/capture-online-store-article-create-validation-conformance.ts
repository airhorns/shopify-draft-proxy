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
const outputPath = path.join(outputDir, 'online-store-article-create-validation.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const blogCreateMutation = `#graphql
  mutation BlogCreate($blog: BlogCreateInput!) {
    blogCreate(blog: $blog) {
      blog { id title handle }
      userErrors { field message code }
    }
  }
`;

const articleCreateMutation = `#graphql
  mutation ArticleCreate($article: ArticleCreateInput!, $blog: ArticleBlogInput) {
    articleCreate(article: $article, blog: $blog) {
      article {
        id
        title
        handle
        author { name }
        blog { id title handle }
      }
      userErrors { field message code }
    }
  }
`;

const articleDeleteMutation = `#graphql
  mutation ArticleDelete($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation BlogDelete($id: ID!) {
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

function readBlogId(payload: unknown): string {
  const id = readPath(payload, ['data', 'blogCreate', 'blog', 'id']);
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`Blog setup failed: ${JSON.stringify(payload)}`);
  }
  return id;
}

function readArticleId(payload: unknown): string | null {
  const id = readPath(payload, ['data', 'articleCreate', 'article', 'id']);
  return typeof id === 'string' && id.length > 0 ? id : null;
}

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const captures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
const createdArticleIds: string[] = [];
let blogId: string | null = null;

try {
  const blogCreate = await capture(captures, 'blogCreate-setup', blogCreateMutation, {
    blog: {
      title: `HAR 557 Validation Blog ${suffix}`,
    },
  });
  blogId = readBlogId(blogCreate);

  const missingBlogReference = await capture(captures, 'articleCreate-missing-blog-reference', articleCreateMutation, {
    article: {
      title: `HAR 557 Missing Blog ${suffix}`,
      body: '<p>Body</p>',
      author: { name: 'HAR 557 Author' },
    },
  });
  const ambiguousBlog = await capture(captures, 'articleCreate-ambiguous-blog', articleCreateMutation, {
    article: {
      title: `HAR 557 Ambiguous Blog ${suffix}`,
      body: '<p>Body</p>',
      blogId,
      author: { name: 'HAR 557 Author' },
    },
    blog: {
      title: `HAR 557 Inline Blog ${suffix}`,
    },
  });
  const missingAuthor = await capture(captures, 'articleCreate-author-field-required', articleCreateMutation, {
    article: {
      title: `HAR 557 Missing Author ${suffix}`,
      body: '<p>Body</p>',
      blogId,
      author: {},
    },
  });
  const success = await capture(captures, 'articleCreate-success', articleCreateMutation, {
    article: {
      title: `HAR 557 Success ${suffix}`,
      body: '<p>Body</p>',
      blogId,
      author: { name: 'HAR 557 Author' },
    },
  });

  for (const payload of [missingBlogReference, ambiguousBlog, missingAuthor, success]) {
    const articleId = readArticleId(payload);
    if (articleId) {
      createdArticleIds.push(articleId);
    }
  }
} finally {
  for (const id of createdArticleIds.reverse()) {
    await capture(cleanupCaptures, 'articleDelete-cleanup', articleDeleteMutation, { id });
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
      scenarioId: 'online-store/article-create-validation',
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
