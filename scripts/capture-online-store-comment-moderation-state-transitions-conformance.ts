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
const outputFile = path.join(outputDir, 'comment-moderation-state-transitions.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: authHeaders,
});

const blogCreateMutation = `#graphql
  mutation CommentModerationStateBlogCreate($blog: BlogCreateInput!) {
    blogCreate(blog: $blog) {
      blog { id title handle commentPolicy }
      userErrors { field message code }
    }
  }
`;

const articleCreateMutation = `#graphql
  mutation CommentModerationStateArticleCreate($article: ArticleCreateInput!) {
    articleCreate(article: $article) {
      article { id title handle }
      userErrors { field message code }
    }
  }
`;

const articleDeleteMutation = `#graphql
  mutation CommentModerationStateArticleDelete($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation CommentModerationStateBlogDelete($id: ID!) {
    blogDelete(id: $id) {
      deletedBlogId
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

const commentApproveMutation = `#graphql
  mutation CommentModerationStateApprove($id: ID!) {
    commentApprove(id: $id) {
      comment {
        id
        status
        isPublished
        publishedAt
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const commentSpamMutation = `#graphql
  mutation CommentModerationStateSpam($id: ID!) {
    commentSpam(id: $id) {
      comment {
        id
        status
        isPublished
        publishedAt
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const commentNotSpamMutation = `#graphql
  mutation CommentModerationStateNotSpam($id: ID!) {
    commentNotSpam(id: $id) {
      comment {
        id
        status
        isPublished
        publishedAt
      }
      userErrors {
        field
        message
        code
      }
    }
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
      author: { name: 'Comment Moderation Author' },
      isPublished: true,
    },
  });
  return {
    capture: captureResult,
    id: readRequiredId(captureResult.response, ['data', 'articleCreate', 'article', 'id'], 'Article setup'),
  };
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
          author: 'Comment Moderation Fixture',
          email: 'comment-moderation@example.com',
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

async function setupComment(
  blogId: string,
  articleId: string,
  label: string,
): Promise<{ rest: RestComment; id: string }> {
  const rest = await createComment(blogId, articleId, `comment moderation ${label}`);
  return {
    rest,
    id: `gid://shopify/Comment/${rest.id}`,
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

async function main(): Promise<void> {
  const suffix = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const cleanup: Capture[] = [];
  let blogId: string | null = null;
  let articleId: string | null = null;

  try {
    const blog = await createBlog(`Comment Moderation State Blog ${suffix}`);
    blogId = blog.id;
    const article = await createArticle(blogId, `Comment Moderation State Article ${suffix}`);
    articleId = article.id;

    const publishedSetup = await setupComment(blogId, articleId, `published ${suffix}`);
    const approvePublishedSetup = await capture(commentApproveMutation, { id: publishedSetup.id });
    const publishedHydrate = await capture(commentHydrateQuery, { id: publishedSetup.id });
    const approvePublished = await capture(commentApproveMutation, { id: publishedSetup.id });
    const notSpamPublished = await capture(commentNotSpamMutation, { id: publishedSetup.id });

    const spamSetup = await setupComment(blogId, articleId, `spam ${suffix}`);
    const spamSetupTransition = await capture(commentSpamMutation, { id: spamSetup.id });
    const spamHydrate = await capture(commentHydrateQuery, { id: spamSetup.id });
    const spamSpam = await capture(commentSpamMutation, { id: spamSetup.id });
    const approveSpam = await capture(commentApproveMutation, { id: spamSetup.id });

    const unapprovedSetup = await setupComment(blogId, articleId, `unapproved ${suffix}`);
    const unapprovedHydrate = await capture(commentHydrateQuery, { id: unapprovedSetup.id });
    const notSpamUnapproved = await capture(commentNotSpamMutation, { id: unapprovedSetup.id });

    await cleanupArticle(articleId, cleanup);
    articleId = null;
    await cleanupBlog(blogId, cleanup);
    blogId = null;

    await mkdir(outputDir, { recursive: true });
    await writeFile(
      outputFile,
      `${JSON.stringify(
        {
          scenarioId: 'comment-moderation-state-transitions',
          storeDomain,
          apiVersion,
          capturedAt: new Date().toISOString(),
          setup: {
            blogCreate: blog.capture,
            articleCreate: article.capture,
            restComments: [publishedSetup.rest, spamSetup.rest, unapprovedSetup.rest],
            approvePublishedSetup,
            spamSetupTransition,
          },
          variables: {
            publishedId: publishedSetup.id,
            spamId: spamSetup.id,
            unapprovedId: unapprovedSetup.id,
          },
          approvePublished,
          notSpamPublished,
          spamSpam,
          approveSpam,
          notSpamUnapproved,
          cleanup,
          upstreamCalls: [
            upstreamCall('OnlineStoreCommentHydrate', { id: publishedSetup.id }, publishedHydrate),
            upstreamCall('OnlineStoreCommentHydrate', { id: spamSetup.id }, spamHydrate),
            upstreamCall('OnlineStoreCommentHydrate', { id: unapprovedSetup.id }, unapprovedHydrate),
          ],
        },
        null,
        2,
      )}\n`,
      'utf8',
    );
  } finally {
    await cleanupArticle(articleId, cleanup);
    await cleanupBlog(blogId, cleanup);
  }
}

await main();

console.log(`Wrote ${outputFile}`);
