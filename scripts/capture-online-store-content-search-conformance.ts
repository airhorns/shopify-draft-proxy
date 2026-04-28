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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const outputPath = path.join(outputDir, 'online-store-content-search-filters.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
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
    $blogTitleQuery: String!
    $pagePublishedQuery: String!
  ) {
    articlesByTag: articles(first: 5, query: $articleTagQuery) {
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
    articlesByAuthor: articles(first: 5, query: $articleAuthorQuery) {
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
    blogsByTitle: blogs(first: 5, query: $blogTitleQuery) {
      nodes {
        id
        title
        handle
        commentPolicy
        articlesCount { count precision }
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

function assertSearchResult(data: Record<string, unknown>, field: string, expectedTitle: string): string | null {
  const nodes = readNodes(data, field);
  const matchingNode = nodes
    .filter(
      (node): node is Record<string, unknown> => Boolean(node) && typeof node === 'object' && !Array.isArray(node),
    )
    .find((node) => node['title'] === expectedTitle);
  return matchingNode ? null : `${field} did not include ${expectedTitle}; saw ${JSON.stringify(nodes)}`;
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
  expected: { articleTitle: string; blogTitle: string; pageTitle: string },
): Promise<void> {
  let lastMisses: string[] = [];
  for (let attempt = 1; attempt <= 8; attempt += 1) {
    const search = await capture('search-filters', searchQuery, variables);
    const data = readData(search);
    lastMisses = [
      assertSearchResult(data, 'articlesByTag', expected.articleTitle),
      assertSearchResult(data, 'articlesByAuthor', expected.articleTitle),
      assertSearchResult(data, 'blogsByTitle', expected.blogTitle),
      assertSearchResult(data, 'pagesByPublishedTitle', expected.pageTitle),
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

const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const blogTitle = `HAR 393 Search Blog ${suffix}`;
const pageTitle = `HAR 393 Search Page ${suffix}`;
const articleTitle = `HAR 393 Search Article ${suffix}`;
const authorName = `HAR 393 Search Author ${suffix}`;
const tag = `har-393-search-${suffix}`;
const captures: Capture[] = [];
const searchVariables = {
  articleTagQuery: `tag:${tag}`,
  articleAuthorQuery: `author:'${authorName}'`,
  blogTitleQuery: `title:'${blogTitle}'`,
  pagePublishedQuery: `published_status:published title:'${pageTitle}'`,
};
let blogId: string | null = null;
let pageId: string | null = null;
let articleId: string | null = null;
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
      body: '<p>HAR 393 page body for search conformance</p>',
      isPublished: true,
    },
  });
  captures.push(pageCreate);
  pageId = readCreatedId(pageCreate, 'pageCreate', 'page');

  const articleCreate = await capture('articleCreate setup', articleCreateMutation, {
    article: {
      blogId,
      title: articleTitle,
      body: '<p>HAR 393 article body for search conformance</p>',
      summary: '<p>HAR 393 article summary</p>',
      isPublished: true,
      tags: [tag, 'online-store'],
      author: { name: authorName },
    },
  });
  captures.push(articleCreate);
  articleId = readCreatedId(articleCreate, 'articleCreate', 'article');

  await waitForIndexedSearch(searchVariables, { articleTitle, blogTitle, pageTitle });

  captures.push(
    await capture('baseline-catalog-detail-empty', baselineQuery, {
      articleSeedQuery: `tag:${tag}`,
      blogSeedQuery: `title:'${blogTitle}'`,
      pageSeedQuery: `title:'${pageTitle}'`,
    }),
  );

  captures.push(await capture('search-filters', searchQuery, searchVariables));

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
      'HAR-393 live Shopify capture for online-store content fielded search filters. Setup content was created in the dev store, searched after indexing was visible, then deleted in cleanup interactions.',
    interactions: captures,
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (!cleanupCaptured) {
    const cleanupCaptures: Capture[] = [];
    try {
      await cleanup('articleDelete cleanup after failure', articleDeleteMutation, articleId, cleanupCaptures);
      await cleanup('pageDelete cleanup after failure', pageDeleteMutation, pageId, cleanupCaptures);
      await cleanup('blogDelete cleanup after failure', blogDeleteMutation, blogId, cleanupCaptures);
    } catch (error) {
      console.warn(`Cleanup after failure did not complete: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}
