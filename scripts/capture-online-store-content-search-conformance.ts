/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
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

type RestComment = {
  id: number;
  body: string;
  article_id: number;
  blog_id: number;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const authHeaders = buildAdminAuthHeaders(adminAccessToken);
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'online-store');
const outputPath = path.join(outputDir, 'online-store-content-search-filters.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: authHeaders,
});

const blogCreateMutation = `#graphql
  mutation BlogCreate($blog: BlogCreateInput!) {
    blogCreate(blog: $blog) {
      blog {
        id
        title
        handle
        createdAt
        updatedAt
        commentPolicy
        templateSuffix
        tags
        articlesCount { count precision }
      }
      userErrors { field message }
    }
  }
`;

const pageCreateMutation = `#graphql
  mutation PageCreate($page: PageCreateInput!) {
    pageCreate(page: $page) {
      page {
        id
        title
        handle
        createdAt
        updatedAt
        body
        bodySummary
        isPublished
        publishedAt
        templateSuffix
      }
      userErrors { field message }
    }
  }
`;

const articleCreateMutation = `#graphql
  mutation ArticleCreate($article: ArticleCreateInput!) {
    articleCreate(article: $article) {
      article {
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
        author { name }
        blog { id title handle }
        commentsCount { count precision }
      }
      userErrors { field message }
    }
  }
`;

const articleDeleteMutation = `#graphql
  mutation ArticleDelete($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message }
    }
  }
`;

const commentApproveMutation = `#graphql
  mutation CommentApprove($id: ID!) {
    commentApprove(id: $id) {
      comment {
        id
        status
        isPublished
        publishedAt
      }
      userErrors { field message code }
    }
  }
`;

const pageDeleteMutation = `#graphql
  mutation PageDelete($id: ID!) {
    pageDelete(id: $id) {
      deletedPageId
      userErrors { field message }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation BlogDelete($id: ID!) {
    blogDelete(id: $id) {
      deletedBlogId
      userErrors { field message }
    }
  }
`;

const baselineQuery = `#graphql
  query OnlineStoreContentSearchSeed($articleSeedQuery: String!, $blogSeedQuery: String!, $pageSeedQuery: String!) {
    articles(first: 5, query: $articleSeedQuery) {
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
    blogs(first: 5, query: $blogSeedQuery) {
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
    pages(first: 5, query: $pageSeedQuery) {
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
  }
`;

const searchQuery = `#graphql
  query OnlineStoreContentSearchFilters(
    $articleTagQuery: String!
    $articleAuthorQuery: String!
    $articleBlogTitleQuery: String!
    $blogTitleQuery: String!
    $pageTitleQuery: String!
    $pagePublishedQuery: String!
    $commentBodyQuery: String!
  ) {
    articlesUnfilteredIdDesc: articles(first: 2, sortKey: ID, reverse: true) {
      nodes {
        id
        title
        isPublished
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    articlesByTag: articles(first: 5, query: $articleTagQuery, sortKey: TITLE) {
      nodes {
        id
        title
        handle
        tags
        isPublished
        author { name }
        blog { id title handle }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    articlesByAuthor: articles(first: 5, query: $articleAuthorQuery, sortKey: TITLE) {
      nodes {
        id
        title
        handle
        tags
        isPublished
        author { name }
        blog { id title handle }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    articlesByBlogTitle: articles(first: 5, query: $articleBlogTitleQuery, sortKey: TITLE, reverse: true) {
      nodes {
        id
        title
        handle
        tags
        isPublished
        author { name }
        blog { id title handle }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    blogsByTitle: blogs(first: 5, query: $blogTitleQuery, sortKey: TITLE) {
      nodes {
        id
        title
        handle
        commentPolicy
        articlesCount { count precision }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    blogsByTitleReverse: blogs(first: 5, query: $blogTitleQuery, sortKey: TITLE, reverse: true) {
      nodes {
        id
        title
        handle
        commentPolicy
        articlesCount { count precision }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    pagesByTitleReverse: pages(first: 5, query: $pageTitleQuery, sortKey: TITLE, reverse: true) {
      nodes {
        id
        title
        handle
        isPublished
        publishedAt
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    pagesByPublishedTitle: pages(first: 5, query: $pagePublishedQuery) {
      nodes {
        id
        title
        handle
        isPublished
        publishedAt
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    commentsByBodyCreatedDesc: comments(first: 5, query: $commentBodyQuery, sortKey: CREATED_AT, reverse: true) {
      nodes {
        id
        body
        status
        isPublished
        createdAt
        article { id title }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
`;

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`Expected object at ${label}.`);
  }
  return value as Record<string, unknown>;
}

function readString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Expected non-empty string at ${label}.`);
  }
  return value;
}

function readData(capture: Capture): Record<string, unknown> {
  return readObject(readObject(capture.response, `${capture.name}.response`)['data'], `${capture.name}.response.data`);
}

function readNodes(data: Record<string, unknown>, field: string): unknown[] {
  const connection = readObject(data[field], `data.${field}`);
  const nodes = connection['nodes'];
  if (!Array.isArray(nodes)) {
    throw new Error(`Expected nodes array at data.${field}.nodes.`);
  }
  return nodes;
}

function assertNoUserErrors(payload: Record<string, unknown>, label: string): void {
  const userErrors = payload['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function readCreatedId(capture: Capture, mutationName: string, resourceName: string): string {
  const payload = readObject(readData(capture)[mutationName], `${capture.name}.${mutationName}`);
  assertNoUserErrors(payload, `${capture.name}.${mutationName}`);
  const resource = readObject(payload[resourceName], `${capture.name}.${mutationName}.${resourceName}`);
  return readString(resource['id'], `${capture.name}.${mutationName}.${resourceName}.id`);
}

function assertNodeFieldResult(
  data: Record<string, unknown>,
  field: string,
  expectedField: string,
  expectedValue: string,
): string | null {
  const nodes = readNodes(data, field);
  const matchingNode = nodes
    .filter(
      (node): node is Record<string, unknown> => Boolean(node) && typeof node === 'object' && !Array.isArray(node),
    )
    .find((node) => node[expectedField] === expectedValue);
  return matchingNode
    ? null
    : `${field} did not include ${expectedField}=${expectedValue}; saw ${JSON.stringify(nodes)}`;
}

function assertSearchResult(data: Record<string, unknown>, field: string, expectedTitle: string): string | null {
  return assertNodeFieldResult(data, field, 'title', expectedTitle);
}

function assertArticleDefaultIncludes(
  data: Record<string, unknown>,
  expectedTitles: { published: string; draft: string },
): string | null {
  const nodes = readNodes(data, 'articlesUnfilteredIdDesc');
  const titles = nodes
    .filter(
      (node): node is Record<string, unknown> => Boolean(node) && typeof node === 'object' && !Array.isArray(node),
    )
    .map((node) => `${node['title']}:${node['isPublished']}`);
  const hasPublished = titles.includes(`${expectedTitles.published}:true`);
  const hasDraft = titles.includes(`${expectedTitles.draft}:false`);
  return hasPublished && hasDraft
    ? null
    : `articlesUnfilteredIdDesc did not include published+draft titles; saw ${JSON.stringify(nodes)}`;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function capture(label: string, query: string, variables: Record<string, unknown> = {}): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    name: label,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

async function waitForIndexedSearch(
  variables: Record<string, unknown>,
  expected: {
    articleTitle: string;
    draftArticleTitle: string;
    blogTitle: string;
    pageTitle: string;
    commentBody: string;
  },
): Promise<void> {
  let lastMisses: string[] = [];
  for (let attempt = 1; attempt <= 8; attempt += 1) {
    const search = await capture('search-filters', searchQuery, variables);
    const data = readData(search);
    lastMisses = [
      assertArticleDefaultIncludes(data, {
        published: expected.articleTitle,
        draft: expected.draftArticleTitle,
      }),
      assertSearchResult(data, 'articlesByTag', expected.articleTitle),
      assertSearchResult(data, 'articlesByAuthor', expected.articleTitle),
      assertSearchResult(data, 'articlesByBlogTitle', expected.articleTitle),
      assertSearchResult(data, 'blogsByTitle', expected.blogTitle),
      assertSearchResult(data, 'pagesByPublishedTitle', expected.pageTitle),
      assertNodeFieldResult(data, 'commentsByBodyCreatedDesc', 'body', expected.commentBody),
    ].filter((miss): miss is string => typeof miss === 'string');

    if (lastMisses.length === 0) {
      return;
    }

    await sleep(attempt * 1000);
  }

  throw new Error(`Search indexes did not contain the captured content: ${lastMisses.join('; ')}`);
}

async function cleanup(label: string, query: string, id: string | null, captures: Capture[]): Promise<void> {
  if (!id) {
    return;
  }
  captures.push(await capture(label, query, { id }));
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
          author: 'Online Store Search Fixture',
          email: 'online-store-search@example.com',
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

function upstreamCall(operationName: string, variables: Record<string, unknown>, captureResult: Capture): unknown {
  return {
    operationName,
    variables,
    query: captureResult.request.query,
    response: {
      status: captureResult.status,
      body: captureResult.response,
    },
  };
}

const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const blogTitle = `Online Store Search Blog ${suffix}`;
const pageTitle = `Online Store Search Page ${suffix}`;
const articleTitle = `Online Store Search Article ${suffix}`;
const draftArticleTitle = `Online Store Search Draft Article ${suffix}`;
const authorName = `Online Store Search Author ${suffix}`;
const tag = `online-store-search-${suffix}`;
const firstCommentBody = `Online Store Search Comment Alpha ${suffix}`;
const secondCommentBody = `Online Store Search Comment Zulu ${suffix}`;
const captures: Capture[] = [];
const searchVariables = {
  articleTagQuery: `published_status:published tag:${tag}`,
  articleAuthorQuery: `published_status:published author:'${authorName}'`,
  articleBlogTitleQuery: `published_status:published blog_title:'${blogTitle}'`,
  blogTitleQuery: `title:'${blogTitle}'`,
  pageTitleQuery: `title:'${pageTitle}'`,
  pagePublishedQuery: `published_status:published title:'${pageTitle}'`,
  commentBodyQuery: `"Online Store Search Comment" AND "${suffix}"`,
};
let blogId: string | null = null;
let pageId: string | null = null;
let articleId: string | null = null;
let draftArticleId: string | null = null;
let cleanupCaptured = false;

try {
  const blogCreate = await capture('blogCreate setup', blogCreateMutation, {
    blog: {
      title: blogTitle,
      commentPolicy: 'MODERATED',
    },
  });
  captures.push(blogCreate);
  blogId = readCreatedId(blogCreate, 'blogCreate', 'blog');

  const pageCreate = await capture('pageCreate setup', pageCreateMutation, {
    page: {
      title: pageTitle,
      body: '<p>Online-store page body for search conformance</p>',
      isPublished: true,
    },
  });
  captures.push(pageCreate);
  pageId = readCreatedId(pageCreate, 'pageCreate', 'page');

  const articleCreate = await capture('articleCreate setup', articleCreateMutation, {
    article: {
      blogId,
      title: articleTitle,
      body: '<p>Online-store article body for search conformance</p>',
      summary: '<p>Online-store article summary</p>',
      isPublished: true,
      tags: [tag, 'online-store'],
      author: { name: authorName },
    },
  });
  captures.push(articleCreate);
  articleId = readCreatedId(articleCreate, 'articleCreate', 'article');

  const draftArticleCreate = await capture('draft articleCreate setup', articleCreateMutation, {
    article: {
      blogId,
      title: draftArticleTitle,
      body: '<p>Online-store draft article body for search conformance</p>',
      summary: '<p>Online-store draft article summary</p>',
      isPublished: false,
      tags: [tag, 'online-store', 'draft'],
      author: { name: authorName },
    },
  });
  captures.push(draftArticleCreate);
  draftArticleId = readCreatedId(draftArticleCreate, 'articleCreate', 'article');

  const firstComment = await createComment(blogId, articleId, firstCommentBody);
  const secondComment = await createComment(blogId, articleId, secondCommentBody);
  captures.push(await capture('commentApprove setup alpha', commentApproveMutation, { id: commentGid(firstComment) }));
  captures.push(await capture('commentApprove setup zulu', commentApproveMutation, { id: commentGid(secondComment) }));

  await waitForIndexedSearch(searchVariables, {
    articleTitle,
    draftArticleTitle,
    blogTitle,
    pageTitle,
    commentBody: secondCommentBody,
  });

  captures.push(
    await capture('baseline-catalog-detail-empty', baselineQuery, {
      articleSeedQuery: `tag:${tag}`,
      blogSeedQuery: `title:'${blogTitle}'`,
      pageSeedQuery: `title:'${pageTitle}'`,
    }),
  );

  const searchCapture = await capture('search-filters', searchQuery, searchVariables);
  captures.push(searchCapture);

  await cleanup('draft articleDelete cleanup', articleDeleteMutation, draftArticleId, captures);
  draftArticleId = null;
  await cleanup('articleDelete cleanup', articleDeleteMutation, articleId, captures);
  articleId = null;
  await cleanup('pageDelete cleanup', pageDeleteMutation, pageId, captures);
  pageId = null;
  await cleanup('blogDelete cleanup', blogDeleteMutation, blogId, captures);
  blogId = null;
  cleanupCaptured = true;

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes:
      'Live Shopify capture for online-store content fielded search filters, unfiltered article visibility, and connection sort/reverse behavior. Setup content was created in the dev store, searched after indexing was visible, then deleted in cleanup interactions.',
    interactions: captures,
    upstreamCalls: [upstreamCall('OnlineStoreContentSearchFilters', searchVariables, searchCapture)],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (!cleanupCaptured) {
    const cleanupCaptures: Capture[] = [];
    try {
      await cleanup(
        'draft articleDelete cleanup after failure',
        articleDeleteMutation,
        draftArticleId,
        cleanupCaptures,
      );
      await cleanup('articleDelete cleanup after failure', articleDeleteMutation, articleId, cleanupCaptures);
      await cleanup('pageDelete cleanup after failure', pageDeleteMutation, pageId, cleanupCaptures);
      await cleanup('blogDelete cleanup after failure', blogDeleteMutation, blogId, cleanupCaptures);
    } catch (error) {
      console.warn(`Cleanup after failure did not complete: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}
