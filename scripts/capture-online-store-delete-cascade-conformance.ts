/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
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
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: authHeaders,
});

const blogCreateMutation = `#graphql
  mutation OnlineStoreDeleteCascadeBlogCreate($blog: BlogCreateInput!) {
    blogCreate(blog: $blog) {
      blog { id title handle }
      userErrors { field message code }
    }
  }
`;

const articleCreateMutation = `#graphql
  mutation OnlineStoreDeleteCascadeArticleCreate($article: ArticleCreateInput!) {
    articleCreate(article: $article) {
      article { id title handle }
      userErrors { field message code }
    }
  }
`;

const articleCascadeHydrateQuery = `#graphql
  query OnlineStoreArticleDeleteCascadeHydrate($id: ID!) {
    article(id: $id) {
      __typename
      id
      title
      handle
      createdAt
      updatedAt
      blog { id }
      comments(first: 50) {
        nodes {
          __typename
          id
          status
          body
          bodyHtml
          isPublished
          publishedAt
          createdAt
          updatedAt
          article { id }
        }
      }
    }
  }
`;

const blogCascadeHydrateQuery = `#graphql
  query OnlineStoreBlogDeleteCascadeHydrate($id: ID!) {
    blog(id: $id) {
      __typename
      id
      title
      handle
      createdAt
      updatedAt
      commentPolicy
      articles(first: 50) {
        nodes {
          __typename
          id
          title
          handle
          createdAt
          updatedAt
          blog { id }
          comments(first: 50) {
            nodes {
              __typename
              id
              status
              body
              bodyHtml
              isPublished
              publishedAt
              createdAt
              updatedAt
              article { id }
            }
          }
        }
      }
    }
  }
`;

const articleDeleteMutation = `#graphql
  mutation ArticleDeleteCascadesComments($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation BlogDeleteCascadesArticlesAndComments($id: ID!) {
    blogDelete(id: $id) {
      deletedBlogId
      userErrors { field message code }
    }
  }
`;

const articleReadAfterDeleteQuery = `#graphql
  query ArticleDeleteCascadesCommentsRead($articleId: ID!, $commentQuery: String!) {
    article(id: $articleId) { id }
    comments(first: 10, query: $commentQuery) {
      nodes { id article { id } }
    }
  }
`;

const blogReadAfterDeleteQuery = `#graphql
  query BlogDeleteCascadesArticlesAndCommentsRead(
    $blogId: ID!,
    $articleOneId: ID!,
    $articleTwoId: ID!,
    $articleQuery: String!,
    $commentQuery: String!
  ) {
    blog(id: $blogId) { id }
    articleOne: article(id: $articleOneId) { id }
    articleTwo: article(id: $articleTwoId) { id }
    articles(first: 10, query: $articleQuery) { nodes { id } }
    comments(first: 10, query: $commentQuery) { nodes { id article { id } } }
  }
`;

async function capture(query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
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

function numericId(gid: string): string {
  const id = gid.split('/').at(-1);
  if (!id) {
    throw new Error(`Could not read numeric id from ${gid}`);
  }
  return id;
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
          author: 'Delete Cascade Fixture',
          email: 'delete-cascade@example.com',
          ip: '127.0.0.1',
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

async function createBlog(title: string): Promise<{ capture: Capture; id: string }> {
  const captureResult = await capture(blogCreateMutation, {
    blog: {
      title,
      commentPolicy: 'MODERATED',
    },
  });
  return {
    capture: captureResult,
    id: readRequiredId(captureResult.response, ['data', 'blogCreate', 'blog', 'id'], 'Blog setup'),
  };
}

async function createArticle(blogId: string, title: string): Promise<{ capture: Capture; id: string }> {
  const captureResult = await capture(articleCreateMutation, {
    article: {
      title,
      body: `<p>${title}</p>`,
      blogId,
      author: { name: 'Delete Cascade Author' },
      isPublished: true,
    },
  });
  return {
    capture: captureResult,
    id: readRequiredId(captureResult.response, ['data', 'articleCreate', 'article', 'id'], 'Article setup'),
  };
}

function upstreamCall(operationName: string, variables: Record<string, unknown>, captureResult: Capture): unknown {
  return {
    operationName,
    variables,
    response: {
      status: captureResult.status,
      body: captureResult.response,
    },
  };
}

async function cleanupArticle(articleId: string | null, cleanup: Capture[]): Promise<void> {
  if (articleId) {
    cleanup.push(await capture(articleDeleteMutation, { id: articleId }));
  }
}

async function cleanupBlog(blogId: string | null, cleanup: Capture[]): Promise<void> {
  if (blogId) {
    cleanup.push(await capture(blogDeleteMutation, { id: blogId }));
  }
}

async function captureArticleDeleteCascade(suffix: string): Promise<void> {
  const cleanup: Capture[] = [];
  let blogId: string | null = null;
  let articleId: string | null = null;
  let articleDeleted = false;
  let blogDeleted = false;

  try {
    const blog = await createBlog(`Article Delete Cascade Blog ${suffix}`);
    blogId = blog.id;
    const article = await createArticle(blogId, `Article Delete Cascade ${suffix}`);
    articleId = article.id;
    const comment = await createComment(blogId, articleId, `article delete cascade comment ${suffix}`);
    const hydrateVariables = { id: articleId };
    const hydrate = await capture(articleCascadeHydrateQuery, hydrateVariables);
    const articleDelete = await capture(articleDeleteMutation, { id: articleId });
    articleDeleted = true;
    const readAfterDelete = await capture(articleReadAfterDeleteQuery, {
      articleId,
      commentQuery: `"article delete cascade comment ${suffix}"`,
    });

    await mkdir(outputDir, { recursive: true });
    await writeFile(
      path.join(outputDir, 'article-delete-cascades-comments.json'),
      `${JSON.stringify(
        {
          scenarioId: 'article-delete-cascades-comments',
          storeDomain,
          apiVersion,
          capturedAt: new Date().toISOString(),
          setup: {
            blogCreate: blog.capture,
            articleCreate: article.capture,
            restComments: [comment],
          },
          articleDelete,
          readAfterDelete,
          cleanup,
          upstreamCalls: [upstreamCall('OnlineStoreArticleDeleteCascadeHydrate', hydrateVariables, hydrate)],
        },
        null,
        2,
      )}\n`,
      'utf8',
    );
  } finally {
    if (!articleDeleted) {
      await cleanupArticle(articleId, cleanup);
    }
    if (!blogDeleted) {
      await cleanupBlog(blogId, cleanup);
      blogDeleted = true;
    }
  }
}

async function captureBlogDeleteCascade(suffix: string): Promise<void> {
  const cleanup: Capture[] = [];
  let blogId: string | null = null;
  const articleIds: string[] = [];
  let blogDeleted = false;

  try {
    const blog = await createBlog(`Blog Delete Cascade Blog ${suffix}`);
    blogId = blog.id;
    const articleOne = await createArticle(blogId, `Blog Delete Cascade First ${suffix}`);
    const articleTwo = await createArticle(blogId, `Blog Delete Cascade Second ${suffix}`);
    articleIds.push(articleOne.id, articleTwo.id);
    const commentOne = await createComment(blogId, articleOne.id, `blog delete cascade comment one ${suffix}`);
    const commentTwo = await createComment(blogId, articleTwo.id, `blog delete cascade comment two ${suffix}`);
    const hydrateVariables = { id: blogId };
    const hydrate = await capture(blogCascadeHydrateQuery, hydrateVariables);
    const blogDelete = await capture(blogDeleteMutation, { id: blogId });
    blogDeleted = true;
    const readAfterDelete = await capture(blogReadAfterDeleteQuery, {
      blogId,
      articleOneId: articleOne.id,
      articleTwoId: articleTwo.id,
      articleQuery: `"Blog Delete Cascade" AND "${suffix}"`,
      commentQuery: `"blog delete cascade comment" AND "${suffix}"`,
    });

    await mkdir(outputDir, { recursive: true });
    await writeFile(
      path.join(outputDir, 'blog-delete-cascades-articles-and-comments.json'),
      `${JSON.stringify(
        {
          scenarioId: 'blog-delete-cascades-articles-and-comments',
          storeDomain,
          apiVersion,
          capturedAt: new Date().toISOString(),
          setup: {
            blogCreate: blog.capture,
            articleCreate: [articleOne.capture, articleTwo.capture],
            restComments: [commentOne, commentTwo],
          },
          blogDelete,
          readAfterDelete,
          cleanup,
          upstreamCalls: [upstreamCall('OnlineStoreBlogDeleteCascadeHydrate', hydrateVariables, hydrate)],
        },
        null,
        2,
      )}\n`,
      'utf8',
    );
  } finally {
    if (!blogDeleted) {
      for (const articleId of articleIds) {
        await cleanupArticle(articleId, cleanup);
      }
      await cleanupBlog(blogId, cleanup);
    }
  }
}

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

await captureArticleDeleteCascade(suffix);
await captureBlogDeleteCascade(suffix);

console.log(`Wrote ${path.join(outputDir, 'article-delete-cascades-comments.json')}`);
console.log(`Wrote ${path.join(outputDir, 'blog-delete-cascades-articles-and-comments.json')}`);
