/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Variables = Record<string, unknown>;

type Interaction = {
  request: { query: string; variables: Variables };
  response: unknown;
  status: number;
};

type UpstreamCall = {
  method: 'POST';
  apiSurface: 'admin';
  apiVersion: string;
  path: string;
  operationName: string;
  query: string;
  variables: Variables;
  response: { status: number; body: unknown };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'online-store');
const outputPath = path.join(outputDir, 'online-store-content-handle-reservations.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readRequest(filename: string): Promise<string> {
  return readFile(path.join('config', 'parity-requests', 'online-store', filename), 'utf8');
}

const pageCreateMutation = await readRequest('online-store-content-handle-page-create.graphql');
const pageUpdateMutation = await readRequest('online-store-content-handle-page-update.graphql');
const blogCreateMutation = await readRequest('online-store-content-handle-blog-create.graphql');
const blogUpdateMutation = await readRequest('online-store-content-handle-blog-update.graphql');
const articleCreateMutation = await readRequest('online-store-content-handle-article-create.graphql');
const articleUpdateMutation = await readRequest('online-store-content-handle-article-update.graphql');
const pageHandleHydrateQuery = await readRequest('online-store-page-handle-reservation-hydrate.graphql');
const blogHandleHydrateQuery = await readRequest('online-store-blog-handle-reservation-hydrate.graphql');
const articleHandleHydrateQuery = await readRequest('online-store-article-handle-reservation-hydrate.graphql');
const pageMutationHydrateQuery = await readRequest('online-store-page-mutation-hydrate.graphql');
const blogMutationHydrateQuery = await readRequest('online-store-blog-mutation-hydrate.graphql');
const articleMutationHydrateQuery = await readRequest('online-store-article-mutation-hydrate.graphql');

const pageDeleteMutation = `#graphql
  mutation OnlineStoreContentHandlePageCleanup($id: ID!) {
    pageDelete(id: $id) {
      deletedPageId
      userErrors { field message code }
    }
  }
`;

const articleDeleteMutation = `#graphql
  mutation OnlineStoreContentHandleArticleCleanup($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation OnlineStoreContentHandleBlogCleanup($id: ID!) {
    blogDelete(id: $id) {
      deletedBlogId
      userErrors { field message code }
    }
  }
`;

function readPath(value: unknown, pathSegments: string[]): unknown {
  return pathSegments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) return null;
    return (current as Record<string, unknown>)[segment] ?? null;
  }, value);
}

function assertGraphqlOk(interaction: Interaction, label: string): void {
  if (interaction.status < 200 || interaction.status >= 300 || readPath(interaction.response, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(interaction, null, 2)}`);
  }
}

function requiredResource(
  interaction: Interaction,
  root: 'pageCreate' | 'blogCreate' | 'articleCreate',
  resource: 'page' | 'blog' | 'article',
  label: string,
): { id: string; handle: string } {
  assertGraphqlOk(interaction, label);
  const userErrors = readPath(interaction.response, ['data', root, 'userErrors']);
  const id = readPath(interaction.response, ['data', root, resource, 'id']);
  const handle = readPath(interaction.response, ['data', root, resource, 'handle']);
  if (!Array.isArray(userErrors) || userErrors.length > 0 || typeof id !== 'string' || typeof handle !== 'string') {
    throw new Error(`${label} did not create ${resource}: ${JSON.stringify(interaction.response, null, 2)}`);
  }
  return { id, handle };
}

function numericId(id: string): string {
  const tail = id.split('/').at(-1)?.split('?')[0];
  if (!tail || !/^\d+$/u.test(tail)) throw new Error(`Expected a numeric Shopify GID tail: ${id}`);
  return tail;
}

function handleQuery(handle: string): string {
  return `handle:${handle}`;
}

function articleHandleQuery(handle: string, blogId: string): string {
  return `${handleQuery(handle)} blog_id:${numericId(blogId)}`;
}

function nextGeneratedHandle(handle: string): string {
  const match = /^(.*?)(\d+)$/u.exec(handle);
  if (!match) return `${handle}-1`;
  const prefix = match[1] ?? '';
  const suffix = match[2];
  if (!suffix) return `${handle}-1`;
  return `${prefix}${BigInt(suffix) + 1n}`;
}

async function interact(query: string, variables: Variables): Promise<Interaction> {
  const result = await runGraphqlRaw(query, variables);
  return {
    request: { query, variables },
    response: result.payload,
    status: result.status,
  };
}

async function captureUpstream(
  calls: UpstreamCall[],
  operationName: string,
  query: string,
  variables: Variables,
): Promise<unknown> {
  const result = await runGraphqlRaw(query, variables);
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${operationName} failed: ${JSON.stringify(result, null, 2)}`);
  }
  calls.push({
    method: 'POST',
    apiSurface: 'admin',
    apiVersion,
    path: `/admin/api/${apiVersion}/graphql.json`,
    operationName,
    query,
    variables,
    response: { status: result.status, body: result.payload },
  });
  return result.payload;
}

async function capturePagedHydration(
  calls: UpstreamCall[],
  operationName: string,
  query: string,
  root: 'blog' | 'article',
  id: string,
): Promise<void> {
  let after: string | null = null;
  for (let page = 0; page < 20; page += 1) {
    const variables = { id, after };
    const payload = await captureUpstream(calls, operationName, query, variables);
    if (readPath(payload, ['data', root, 'metafields', 'pageInfo', 'hasNextPage']) !== true) return;
    const endCursor = readPath(payload, ['data', root, 'metafields', 'pageInfo', 'endCursor']);
    if (typeof endCursor !== 'string' || endCursor.length === 0) {
      throw new Error(`${operationName} returned hasNextPage without endCursor`);
    }
    after = endCursor;
  }
  throw new Error(`${operationName} exceeded the 20-page safety limit`);
}

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const pageTitle = `Content Handle Reservation ${suffix} Page`;
const blogTitle = `Content Handle Reservation ${suffix} Blog`;
const articleTitle = `Content Handle Reservation ${suffix} Article`;
const author = { name: 'Content Handle Reservation Author' };
const setup: Record<string, Interaction> = {};
const operations: Record<string, Interaction> = {};
const cleanup: Interaction[] = [];
const upstreamCalls: UpstreamCall[] = [];
const pageIds: string[] = [];
const blogIds: string[] = [];
const articleIds: string[] = [];

try {
  setup['pagePrimary'] = await interact(pageCreateMutation, { page: { title: pageTitle } });
  const pagePrimary = requiredResource(setup['pagePrimary'], 'pageCreate', 'page', 'pagePrimary');
  pageIds.push(pagePrimary.id);
  await captureUpstream(upstreamCalls, 'OnlineStorePageHandleReservationHydrate', pageHandleHydrateQuery, {
    query: handleQuery(pagePrimary.handle),
    after: null,
  });
  await captureUpstream(upstreamCalls, 'OnlineStorePageHandleReservationHydrate', pageHandleHydrateQuery, {
    query: handleQuery(nextGeneratedHandle(pagePrimary.handle)),
    after: null,
  });
  operations['pageGeneratedCollision'] = await interact(pageCreateMutation, { page: { title: pageTitle } });
  const pageSecondary = requiredResource(
    operations['pageGeneratedCollision'],
    'pageCreate',
    'page',
    'pageGeneratedCollision',
  );
  pageIds.push(pageSecondary.id);
  operations['pageExplicitCollision'] = await interact(pageCreateMutation, {
    page: { title: `${pageTitle} Explicit`, handle: pagePrimary.handle },
  });
  await captureUpstream(upstreamCalls, 'OnlineStorePageHydrate', pageMutationHydrateQuery, {
    id: pagePrimary.id,
  });
  await captureUpstream(upstreamCalls, 'OnlineStorePageHydrate', pageMutationHydrateQuery, {
    id: pageSecondary.id,
  });
  operations['pageSelfUpdate'] = await interact(pageUpdateMutation, {
    id: pagePrimary.id,
    page: { handle: pagePrimary.handle },
  });
  operations['pageCollisionUpdate'] = await interact(pageUpdateMutation, {
    id: pageSecondary.id,
    page: { handle: pagePrimary.handle },
  });

  setup['blogPrimary'] = await interact(blogCreateMutation, { blog: { title: blogTitle } });
  const blogPrimary = requiredResource(setup['blogPrimary'], 'blogCreate', 'blog', 'blogPrimary');
  blogIds.push(blogPrimary.id);
  await captureUpstream(upstreamCalls, 'OnlineStoreBlogHandleReservationHydrate', blogHandleHydrateQuery, {
    query: handleQuery(blogPrimary.handle),
    after: null,
  });
  await captureUpstream(upstreamCalls, 'OnlineStoreBlogHandleReservationHydrate', blogHandleHydrateQuery, {
    query: handleQuery(nextGeneratedHandle(blogPrimary.handle)),
    after: null,
  });
  await captureUpstream(upstreamCalls, 'OnlineStoreBlogHandleReservationHydrate', blogHandleHydrateQuery, {
    query: handleQuery(nextGeneratedHandle(nextGeneratedHandle(blogPrimary.handle))),
    after: null,
  });
  operations['blogGeneratedCollision'] = await interact(blogCreateMutation, { blog: { title: blogTitle } });
  const blogSecondary = requiredResource(
    operations['blogGeneratedCollision'],
    'blogCreate',
    'blog',
    'blogGeneratedCollision',
  );
  blogIds.push(blogSecondary.id);
  operations['blogExplicitCollision'] = await interact(blogCreateMutation, {
    blog: { title: `${blogTitle} Explicit`, handle: blogPrimary.handle },
  });
  assertGraphqlOk(operations['blogExplicitCollision'], 'blogExplicitCollision');
  const explicitBlogId = readPath(operations['blogExplicitCollision'].response, ['data', 'blogCreate', 'blog', 'id']);
  if (typeof explicitBlogId === 'string') blogIds.push(explicitBlogId);
  await capturePagedHydration(
    upstreamCalls,
    'OnlineStoreBlogMutationHydrate',
    blogMutationHydrateQuery,
    'blog',
    blogPrimary.id,
  );
  await capturePagedHydration(
    upstreamCalls,
    'OnlineStoreBlogMutationHydrate',
    blogMutationHydrateQuery,
    'blog',
    blogSecondary.id,
  );
  operations['blogSelfUpdate'] = await interact(blogUpdateMutation, {
    id: blogPrimary.id,
    blog: { handle: blogPrimary.handle },
  });
  operations['blogCollisionUpdate'] = await interact(blogUpdateMutation, {
    id: blogSecondary.id,
    blog: { handle: blogPrimary.handle },
  });

  setup['articlePrimary'] = await interact(articleCreateMutation, {
    article: { blogId: blogPrimary.id, title: articleTitle, author },
  });
  const articlePrimary = requiredResource(setup['articlePrimary'], 'articleCreate', 'article', 'articlePrimary');
  articleIds.push(articlePrimary.id);
  await captureUpstream(upstreamCalls, 'OnlineStoreArticleHandleReservationHydrate', articleHandleHydrateQuery, {
    query: articleHandleQuery(articlePrimary.handle, blogPrimary.id),
    after: null,
  });
  await captureUpstream(upstreamCalls, 'OnlineStoreArticleHandleReservationHydrate', articleHandleHydrateQuery, {
    query: articleHandleQuery(nextGeneratedHandle(articlePrimary.handle), blogPrimary.id),
    after: null,
  });
  await captureUpstream(upstreamCalls, 'OnlineStoreArticleHandleReservationHydrate', articleHandleHydrateQuery, {
    query: articleHandleQuery(articlePrimary.handle, blogSecondary.id),
    after: null,
  });
  operations['articleGeneratedCollision'] = await interact(articleCreateMutation, {
    article: { blogId: blogPrimary.id, title: articleTitle, author },
  });
  const articleSecondary = requiredResource(
    operations['articleGeneratedCollision'],
    'articleCreate',
    'article',
    'articleGeneratedCollision',
  );
  articleIds.push(articleSecondary.id);
  operations['articleExplicitCollision'] = await interact(articleCreateMutation, {
    article: {
      blogId: blogPrimary.id,
      title: `${articleTitle} Explicit`,
      handle: articlePrimary.handle,
      author,
    },
  });
  await capturePagedHydration(
    upstreamCalls,
    'OnlineStoreArticleMutationHydrate',
    articleMutationHydrateQuery,
    'article',
    articlePrimary.id,
  );
  await capturePagedHydration(
    upstreamCalls,
    'OnlineStoreArticleMutationHydrate',
    articleMutationHydrateQuery,
    'article',
    articleSecondary.id,
  );
  operations['articleSelfUpdate'] = await interact(articleUpdateMutation, {
    id: articlePrimary.id,
    article: { handle: articlePrimary.handle },
  });
  operations['articleCollisionUpdate'] = await interact(articleUpdateMutation, {
    id: articleSecondary.id,
    article: { handle: articlePrimary.handle },
  });
  operations['articleCrossBlogExplicitReuse'] = await interact(articleCreateMutation, {
    article: {
      blogId: blogSecondary.id,
      title: `${articleTitle} Cross Blog`,
      handle: articlePrimary.handle,
      author,
    },
  });
  assertGraphqlOk(operations['articleCrossBlogExplicitReuse'], 'articleCrossBlogExplicitReuse');
  const crossBlogId = readPath(operations['articleCrossBlogExplicitReuse'].response, [
    'data',
    'articleCreate',
    'article',
    'id',
  ]);
  if (typeof crossBlogId === 'string') articleIds.push(crossBlogId);
} finally {
  for (const id of articleIds.reverse()) cleanup.push(await interact(articleDeleteMutation, { id }));
  for (const id of pageIds.reverse()) cleanup.push(await interact(pageDeleteMutation, { id }));
  for (const id of blogIds.reverse()) cleanup.push(await interact(blogDeleteMutation, { id }));
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'online-store-content-handle-reservations',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup,
      operations,
      cleanup,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
