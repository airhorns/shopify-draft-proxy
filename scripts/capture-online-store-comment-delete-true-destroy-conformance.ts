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
  mutation CommentDeleteTrueDestroyBlogCreate($blog: BlogCreateInput!) {
    blogCreate(blog: $blog) {
      blog { id title handle }
      userErrors { field message code }
    }
  }
`;

const articleCreateMutation = `#graphql
  mutation CommentDeleteTrueDestroyArticleCreate($article: ArticleCreateInput!) {
    articleCreate(article: $article) {
      article { id title handle }
      userErrors { field message code }
    }
  }
`;

const commentApproveMutation = `#graphql
  mutation CommentDeleteTrueDestroyApprove($id: ID!) {
    commentApprove(id: $id) {
      comment { id status }
      userErrors { field message code }
    }
  }
`;

const commentDeleteMutation = `#graphql
  mutation CommentDeleteTrueDestroyDelete($id: ID!) {
    commentDelete(id: $id) {
      deletedCommentId
      userErrors { field message code }
    }
  }
`;

const commentHydrateQuery = `#graphql
  query OnlineStoreCommentHydrate($id: ID!) {
    comment(id: $id) {
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

const commentReadQuery = `#graphql
  query CommentDeleteTrueDestroyRead($articleId: ID!, $commentQuery: String!) {
    comments(first: 10, query: $commentQuery) { nodes { id } }
    article(id: $articleId) {
      comments(first: 10) { nodes { id } }
      commentsCount { count precision }
    }
  }
`;

const directCommentReadQuery = `#graphql
  query CommentDeleteTrueDestroyDirectCommentRead($id: ID!) {
    comment(id: $id) { id status }
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

async function wait(milliseconds: number): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

function readArray(value: unknown, pathSegments: string[]): unknown[] {
  const resolved = readPath(value, pathSegments);
  return Array.isArray(resolved) ? resolved : [];
}

function captureCommentIds(captureResult: Capture, pathSegments: string[]): string[] {
  return readArray(captureResult.response, pathSegments)
    .map((node) => {
      if (typeof node === 'object' && node !== null) {
        const id = (node as Record<string, unknown>)['id'];
        return typeof id === 'string' ? id : null;
      }
      return null;
    })
    .filter((id): id is string => typeof id === 'string');
}

function articleCommentsCount(captureResult: Capture): number | null {
  const count = readPath(captureResult.response, ['data', 'article', 'commentsCount', 'count']);
  return typeof count === 'number' ? count : null;
}

async function waitForConnectionState(
  variables: Record<string, unknown>,
  commentId: string,
  shouldIncludeComment: boolean,
): Promise<Capture> {
  let last: Capture | null = null;
  for (let attempt = 0; attempt < 12; attempt += 1) {
    last = await capture(commentReadQuery, variables);
    const rootIds = captureCommentIds(last, ['data', 'comments', 'nodes']);
    const articleIds = captureCommentIds(last, ['data', 'article', 'comments', 'nodes']);
    const count = articleCommentsCount(last);
    const rootMatches = rootIds.includes(commentId) === shouldIncludeComment;
    const articleMatches = articleIds.includes(commentId) === shouldIncludeComment;
    const countMatches = count === (shouldIncludeComment ? 1 : 0);
    if (rootMatches && articleMatches && countMatches) {
      return last;
    }
    await wait(5000);
  }
  throw new Error(
    `Timed out waiting for comment connection state include=${shouldIncludeComment}: ${JSON.stringify(last?.response)}`,
  );
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

function assertNoUserErrors(captureResult: Capture, pathSegments: string[], label: string): void {
  const errors = readPath(captureResult.response, pathSegments);
  if (Array.isArray(errors) && errors.length === 0) {
    return;
  }
  throw new Error(`${label} returned userErrors: ${JSON.stringify(captureResult.response)}`);
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
          author: 'Comment Delete Fixture',
          email: 'comment-delete@example.com',
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

async function createBlog(title: string): Promise<{ capture: Capture; id: string }> {
  const captureResult = await capture(blogCreateMutation, {
    blog: {
      title,
      commentPolicy: 'MODERATED',
    },
  });
  assertNoUserErrors(captureResult, ['data', 'blogCreate', 'userErrors'], 'Blog setup');
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
      author: { name: 'Comment Delete Author' },
      isPublished: true,
    },
  });
  assertNoUserErrors(captureResult, ['data', 'articleCreate', 'userErrors'], 'Article setup');
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
    cleanup.push(
      await capture(
        `#graphql
          mutation CommentDeleteTrueDestroyArticleCleanup($id: ID!) {
            articleDelete(id: $id) {
              deletedArticleId
              userErrors { field message code }
            }
          }
        `,
        { id: articleId },
      ),
    );
  }
}

async function cleanupBlog(blogId: string | null, cleanup: Capture[]): Promise<void> {
  if (blogId) {
    cleanup.push(
      await capture(
        `#graphql
          mutation CommentDeleteTrueDestroyBlogCleanup($id: ID!) {
            blogDelete(id: $id) {
              deletedBlogId
              userErrors { field message code }
            }
          }
        `,
        { id: blogId },
      ),
    );
  }
}

async function captureCommentDeleteTrueDestroy(suffix: string): Promise<void> {
  const cleanup: Capture[] = [];
  let blogId: string | null = null;
  let articleId: string | null = null;
  let articleDeleted = false;
  let blogDeleted = false;
  let fixture: Record<string, unknown> | null = null;

  try {
    const blog = await createBlog(`Comment Delete True Destroy Blog ${suffix}`);
    blogId = blog.id;
    const article = await createArticle(blogId, `Comment Delete True Destroy ${suffix}`);
    articleId = article.id;
    const comment = await createComment(blogId, articleId, `comment delete true destroy ${suffix}`);
    const commentId = commentGid(comment);
    const readVariables = {
      articleId,
      commentQuery: `"comment delete true destroy ${suffix}"`,
    };

    const approve = await capture(commentApproveMutation, { id: commentId });
    assertNoUserErrors(approve, ['data', 'commentApprove', 'userErrors'], 'Comment approve');
    const commentHydrate = await capture(commentHydrateQuery, { id: commentId });
    const articleHydrate = await capture(articleCascadeHydrateQuery, { id: articleId });
    const readBeforeDelete = await waitForConnectionState(readVariables, commentId, true);
    const deleteResult = await capture(commentDeleteMutation, { id: commentId });
    assertNoUserErrors(deleteResult, ['data', 'commentDelete', 'userErrors'], 'Comment delete');
    const readAfterDelete = await waitForConnectionState(readVariables, commentId, false);
    const directCommentAfterDelete = await capture(directCommentReadQuery, { id: commentId });

    fixture = {
      scenarioId: 'comment-delete-true-destroy',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup: {
        blogCreate: blog.capture,
        articleCreate: article.capture,
        restComments: [comment],
      },
      approve,
      hydrate: {
        comment: commentHydrate,
        article: articleHydrate,
      },
      readBeforeDelete,
      delete: deleteResult,
      readAfterDelete,
      directCommentAfterDelete,
      cleanup,
      upstreamCalls: [
        upstreamCall('OnlineStoreCommentHydrate', { id: commentId }, commentHydrate),
        upstreamCall('OnlineStoreArticleDeleteCascadeHydrate', { id: articleId }, articleHydrate),
      ],
    };
  } finally {
    if (!articleDeleted) {
      await cleanupArticle(articleId, cleanup);
      articleDeleted = true;
    }
    if (!blogDeleted) {
      await cleanupBlog(blogId, cleanup);
      blogDeleted = true;
    }
  }

  if (!fixture) {
    throw new Error('Capture did not complete; no fixture was written.');
  }

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    path.join(outputDir, 'comment-delete-true-destroy.json'),
    `${JSON.stringify(fixture, null, 2)}\n`,
    'utf8',
  );
}

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

await captureCommentDeleteTrueDestroy(suffix);

console.log(`Wrote ${path.join(outputDir, 'comment-delete-true-destroy.json')}`);
