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
const outputPath = path.join(outputDir, 'online-store-article-update-validation.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const blogCreateMutation = `#graphql
  mutation OnlineStoreArticleUpdateValidationBlogCreate($blog: BlogCreateInput!) {
    blogCreate(blog: $blog) {
      blog { id title handle }
      userErrors { field message code }
    }
  }
`;

const articleCreateMutation = `#graphql
  mutation OnlineStoreArticleUpdateValidationArticleCreate($article: ArticleCreateInput!) {
    articleCreate(article: $article) {
      article {
        id
        title
        handle
        image { altText url }
      }
      userErrors { field message code }
    }
  }
`;

const articleUpdateMutation = `#graphql
  mutation OnlineStoreArticleUpdateValidationArticleUpdate($id: ID!, $article: ArticleUpdateInput!) {
    articleUpdate(id: $id, article: $article) {
      article {
        id
        title
        handle
        image { altText url }
      }
      userErrors { field message code }
    }
  }
`;

const articleDeleteMutation = `#graphql
  mutation OnlineStoreArticleUpdateValidationArticleDelete($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation OnlineStoreArticleUpdateValidationBlogDelete($id: ID!) {
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
let articleId: string | null = null;

try {
  const blogCreate = await capture(captures, 'blogCreate-setup', blogCreateMutation, {
    blog: {
      title: `Article Update Validation Blog ${suffix}`,
    },
  });
  blogId = readRequiredId(blogCreate, ['data', 'blogCreate', 'blog', 'id'], 'Blog setup');

  const articleCreate = await capture(captures, 'articleCreate-setup', articleCreateMutation, {
    article: {
      title: `Article Update Validation ${suffix}`,
      body: '<p>Body</p>',
      blogId,
      author: { name: 'Article Update Validation Author' },
    },
  });
  articleId = readRequiredId(articleCreate, ['data', 'articleCreate', 'article', 'id'], 'Article setup');

  await capture(captures, 'articleUpdate-ambiguous-author', articleUpdateMutation, {
    id: articleId,
    article: {
      author: {
        name: 'Alice',
        userId: 'gid://shopify/StaffMember/1',
      },
    },
  });

  await capture(captures, 'articleUpdate-author-must-exist', articleUpdateMutation, {
    id: articleId,
    article: {
      author: { userId: 'gid://shopify/StaffMember/999999999' },
    },
  });

  await capture(captures, 'articleUpdate-image-required', articleUpdateMutation, {
    id: articleId,
    article: {
      image: { altText: 'Alt only' },
    },
  });
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
      scenarioId: 'online-store/article-update-validation',
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
