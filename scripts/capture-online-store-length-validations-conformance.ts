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
const outputPath = path.join(outputDir, 'article-page-blog-length-validations.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const blogCreateMutation = `#graphql
  mutation ArticlePageBlogLengthValidationsBlogCreate($blog: BlogCreateInput!) {
    blogCreate(blog: $blog) {
      blog { id title handle }
      userErrors { field message code }
    }
  }
`;

const blogUpdateMutation = `#graphql
  mutation ArticlePageBlogLengthValidationsBlogUpdate($id: ID!, $blog: BlogUpdateInput!) {
    blogUpdate(id: $id, blog: $blog) {
      blog { id title handle }
      userErrors { field message code }
    }
  }
`;

const pageCreateMutation = `#graphql
  mutation ArticlePageBlogLengthValidationsPageCreate($page: PageCreateInput!) {
    pageCreate(page: $page) {
      page { id title handle body }
      userErrors { field message code }
    }
  }
`;

const pageUpdateMutation = `#graphql
  mutation ArticlePageBlogLengthValidationsPageUpdate($id: ID!, $page: PageUpdateInput!) {
    pageUpdate(id: $id, page: $page) {
      page { id title handle body }
      userErrors { field message code }
    }
  }
`;

const articleCreateMutation = `#graphql
  mutation ArticlePageBlogLengthValidationsArticleCreate($article: ArticleCreateInput!) {
    articleCreate(article: $article) {
      article { id title handle body }
      userErrors { field message code }
    }
  }
`;

const articleUpdateMutation = `#graphql
  mutation ArticlePageBlogLengthValidationsArticleUpdate($id: ID!, $article: ArticleUpdateInput!) {
    articleUpdate(id: $id, article: $article) {
      article { id title handle body }
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation ArticlePageBlogLengthValidationsBlogDelete($id: ID!) {
    blogDelete(id: $id) {
      deletedBlogId
      userErrors { field message code }
    }
  }
`;

const pageDeleteMutation = `#graphql
  mutation ArticlePageBlogLengthValidationsPageDelete($id: ID!) {
    pageDelete(id: $id) {
      deletedPageId
      userErrors { field message code }
    }
  }
`;

const articleDeleteMutation = `#graphql
  mutation ArticlePageBlogLengthValidationsArticleDelete($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
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
const long256 = 'x'.repeat(256);
const long266 = 'x'.repeat(266);
const pageBodyTooBig = 'a'.repeat(512 * 1024);
const articleBodyTooBig = 'a'.repeat(1024 * 1024 + 1);
const captures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let blogId: string | null = null;
let pageId: string | null = null;
let articleId: string | null = null;

try {
  const blogCreate = await capture(captures, 'blogCreate-setup', blogCreateMutation, {
    blog: {
      title: `Length Validation Blog ${suffix}`,
    },
  });
  blogId = readRequiredId(blogCreate, ['data', 'blogCreate', 'blog', 'id'], 'Blog setup');

  await capture(captures, 'blogCreate-title-too-long', blogCreateMutation, {
    blog: { title: long256 },
  });
  await capture(captures, 'blogCreate-handle-too-long', blogCreateMutation, {
    blog: { title: 'Blog', handle: long256 },
  });
  await capture(captures, 'blogCreate-feedburner-schema-error', blogCreateMutation, {
    blog: { title: 'Blog', feedburner: long256 },
  });
  await capture(captures, 'blogUpdate-feedburner-schema-error', blogUpdateMutation, {
    id: blogId,
    blog: { feedburner: long256 },
  });

  await capture(captures, 'pageCreate-title-too-long', pageCreateMutation, {
    page: { title: long256 },
  });
  await capture(captures, 'pageCreate-handle-too-long', pageCreateMutation, {
    page: { title: 'Page', handle: long256 },
  });
  await capture(captures, 'pageCreate-body-too-big', pageCreateMutation, {
    page: { title: 'Page', body: pageBodyTooBig },
  });

  await capture(captures, 'articleCreate-title-too-long', articleCreateMutation, {
    article: { title: long256, blogId, author: { name: 'Length Validation Author' } },
  });
  await capture(captures, 'articleCreate-handle-too-long', articleCreateMutation, {
    article: { title: 'Article', handle: long266, blogId, author: { name: 'Length Validation Author' } },
  });
  await capture(captures, 'articleCreate-body-too-big', articleCreateMutation, {
    article: { title: 'Article', body: articleBodyTooBig, blogId, author: { name: 'Length Validation Author' } },
  });

  const pageCreate = await capture(captures, 'pageCreate-setup', pageCreateMutation, {
    page: { title: `Length Validation Page ${suffix}` },
  });
  pageId = readRequiredId(pageCreate, ['data', 'pageCreate', 'page', 'id'], 'Page setup');

  const articleCreate = await capture(captures, 'articleCreate-setup', articleCreateMutation, {
    article: {
      title: `Length Validation Article ${suffix}`,
      blogId,
      author: { name: 'Length Validation Author' },
    },
  });
  articleId = readRequiredId(articleCreate, ['data', 'articleCreate', 'article', 'id'], 'Article setup');

  await capture(captures, 'blogUpdate-title-too-long', blogUpdateMutation, {
    id: blogId,
    blog: { title: long256 },
  });
  await capture(captures, 'blogUpdate-handle-too-long', blogUpdateMutation, {
    id: blogId,
    blog: { handle: long256 },
  });
  await capture(captures, 'pageUpdate-title-too-long', pageUpdateMutation, {
    id: pageId,
    page: { title: long256 },
  });
  await capture(captures, 'pageUpdate-handle-too-long', pageUpdateMutation, {
    id: pageId,
    page: { handle: long256 },
  });
  await capture(captures, 'pageUpdate-body-too-big', pageUpdateMutation, {
    id: pageId,
    page: { body: pageBodyTooBig },
  });
  await capture(captures, 'articleUpdate-title-too-long', articleUpdateMutation, {
    id: articleId,
    article: { title: long256 },
  });
  await capture(captures, 'articleUpdate-handle-too-long', articleUpdateMutation, {
    id: articleId,
    article: { handle: long266 },
  });
  await capture(captures, 'articleUpdate-body-too-big', articleUpdateMutation, {
    id: articleId,
    article: { body: articleBodyTooBig },
  });
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
      scenarioId: 'online-store/article-page-blog-length-validations',
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
