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
const outputPath = path.join(outputDir, 'online-store-authoritative-content-hydration.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readRequest(filename: string): Promise<string> {
  return readFile(path.join('config', 'parity-requests', 'online-store', filename), 'utf8');
}

const articleCreateMutation = await readRequest('online-store-authoritative-article-create.graphql');
const articleUpdateMutation = await readRequest('online-store-authoritative-article-update.graphql');
const articleMoveMutation = await readRequest('online-store-authoritative-article-move.graphql');
const blogUpdateMutation = await readRequest('online-store-authoritative-blog-update.graphql');
const articleReadQuery = await readRequest('online-store-authoritative-article-read.graphql');
const blogReadQuery = await readRequest('online-store-authoritative-blog-read.graphql');
const articleHydrateQuery = await readRequest('online-store-article-mutation-hydrate.graphql');
const blogHydrateQuery = await readRequest('online-store-blog-mutation-hydrate.graphql');

const setupBlogMutation = `#graphql
  mutation OnlineStoreAuthoritativeSetupBlog($blog: BlogCreateInput!) {
    blogCreate(blog: $blog) {
      blog { id }
      userErrors { field message code }
    }
  }
`;

const setupArticleMutation = `#graphql
  mutation OnlineStoreAuthoritativeSetupArticle($article: ArticleCreateInput!) {
    articleCreate(article: $article) {
      article { id }
      userErrors { field message code }
    }
  }
`;

const articleDeleteMutation = `#graphql
  mutation OnlineStoreAuthoritativeArticleCleanup($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation OnlineStoreAuthoritativeBlogCleanup($id: ID!) {
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

function requiredId(payload: unknown, pathSegments: string[], label: string): string {
  const id = readPath(payload, pathSegments);
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return an ID: ${JSON.stringify(payload)}`);
  }
  return id;
}

function assertSuccessfulMutation(payload: unknown, root: string): void {
  const errors = readPath(payload, ['errors']);
  const userErrors = readPath(payload, ['data', root, 'userErrors']);
  if (errors || !Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${root} failed: ${JSON.stringify(payload, null, 2)}`);
  }
}

async function interact(query: string, variables: Variables): Promise<Interaction> {
  const result = await runGraphqlRaw(query, variables);
  return {
    request: { query, variables },
    response: result.payload,
    status: result.status,
  };
}

async function captureHydrationPages(
  calls: UpstreamCall[],
  operationName: string,
  query: string,
  root: 'article' | 'blog',
  id: string,
): Promise<void> {
  let after: string | null = null;
  for (let page = 0; page < 20; page += 1) {
    const variables = { id, after };
    const result = await runGraphqlRaw(query, variables);
    calls.push({
      operationName,
      query,
      variables,
      response: { status: result.status, body: result.payload },
    });
    const resource = readPath(result.payload, ['data', root]);
    if (typeof resource !== 'object' || resource === null) {
      throw new Error(`${operationName} did not return ${root}: ${JSON.stringify(result.payload)}`);
    }
    const hasNextPage = readPath(resource, ['metafields', 'pageInfo', 'hasNextPage']);
    if (hasNextPage !== true) return;
    const endCursor = readPath(resource, ['metafields', 'pageInfo', 'endCursor']);
    if (typeof endCursor !== 'string' || endCursor.length === 0) {
      throw new Error(`${operationName} returned hasNextPage without endCursor`);
    }
    after = endCursor;
  }
  throw new Error(`${operationName} exceeded the 20-page capture safety limit`);
}

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const setup: Record<string, Interaction> = {};
const operations: Record<string, Interaction> = {};
const reads: Record<string, Interaction> = {};
const cleanup: Interaction[] = [];
const upstreamCalls: UpstreamCall[] = [];
const blogIds: string[] = [];
const articleIds: string[] = [];

async function createBlog(key: string, title: string, rich: boolean): Promise<string> {
  const interaction = await interact(setupBlogMutation, {
    blog: {
      title,
      commentPolicy: 'MODERATED',
      templateSuffix: rich ? 'authoritative_blog' : null,
      metafields: rich
        ? [
            {
              namespace: 'authoritative_content',
              key: 'hero',
              type: 'single_line_text_field',
              value: `${key} hero`,
            },
            {
              namespace: 'authoritative_content',
              key: 'secondary',
              type: 'single_line_text_field',
              value: `${key} secondary`,
            },
          ]
        : [],
    },
  });
  setup[key] = interaction;
  assertSuccessfulMutation(interaction.response, 'blogCreate');
  const id = requiredId(interaction.response, ['data', 'blogCreate', 'blog', 'id'], key);
  blogIds.push(id);
  return id;
}

async function createRichArticle(key: string, blogId: string, title: string): Promise<string> {
  const interaction = await interact(setupArticleMutation, {
    article: {
      blogId,
      title,
      body: '<p>Authoritative article body</p>',
      summary: '<p>Authoritative article summary</p>',
      tags: ['authoritative-content', key],
      isPublished: true,
      templateSuffix: 'authoritative_article',
      author: { name: 'Authoritative Content Author' },
      image: {
        altText: 'Authoritative content image',
        url: 'https://placehold.co/96x64/png',
      },
      metafields: [
        {
          namespace: 'authoritative_content',
          key: 'hero',
          type: 'single_line_text_field',
          value: `${key} hero`,
        },
        {
          namespace: 'authoritative_content',
          key: 'secondary',
          type: 'single_line_text_field',
          value: `${key} secondary`,
        },
      ],
    },
  });
  setup[key] = interaction;
  assertSuccessfulMutation(interaction.response, 'articleCreate');
  const id = requiredId(interaction.response, ['data', 'articleCreate', 'article', 'id'], key);
  articleIds.push(id);
  return id;
}

try {
  const createBlogId = await createBlog('createTargetBlog', `Authoritative Create Target ${suffix}`, true);
  const updateArticleBlogId = await createBlog('updateArticleBlog', `Authoritative Update Parent ${suffix}`, false);
  const moveSourceBlogId = await createBlog('moveSourceBlog', `Authoritative Move Source ${suffix}`, false);
  const moveTargetBlogId = await createBlog('moveTargetBlog', `Authoritative Move Target ${suffix}`, true);
  const updateBlogId = await createBlog('updateTargetBlog', `Authoritative Blog Update ${suffix}`, true);
  const updateArticleId = await createRichArticle(
    'updateTargetArticle',
    updateArticleBlogId,
    `Authoritative Article Update ${suffix}`,
  );
  const moveArticleId = await createRichArticle(
    'moveTargetArticle',
    moveSourceBlogId,
    `Authoritative Article Move ${suffix}`,
  );

  await captureHydrationPages(upstreamCalls, 'OnlineStoreBlogMutationHydrate', blogHydrateQuery, 'blog', createBlogId);
  const articleCreate = await interact(articleCreateMutation, {
    article: {
      blogId: createBlogId,
      title: `Authoritative Cold Create ${suffix}`,
      body: '<p>Cold-create body</p>',
      summary: '<p>Cold-create summary</p>',
      tags: ['authoritative-content', 'cold-create'],
      isPublished: true,
      templateSuffix: 'authoritative_create',
      author: { name: 'Authoritative Create Author' },
    },
  });
  operations['articleCreate'] = articleCreate;
  assertSuccessfulMutation(articleCreate.response, 'articleCreate');
  const createdArticleId = requiredId(
    articleCreate.response,
    ['data', 'articleCreate', 'article', 'id'],
    'articleCreate',
  );
  articleIds.push(createdArticleId);
  reads['afterArticleCreate'] = await interact(articleReadQuery, { id: createdArticleId });

  await captureHydrationPages(
    upstreamCalls,
    'OnlineStoreArticleMutationHydrate',
    articleHydrateQuery,
    'article',
    updateArticleId,
  );
  const articleUpdate = await interact(articleUpdateMutation, {
    id: updateArticleId,
    article: { title: `Authoritative Article Narrow Update ${suffix}` },
  });
  operations['articleUpdate'] = articleUpdate;
  assertSuccessfulMutation(articleUpdate.response, 'articleUpdate');
  reads['afterArticleUpdate'] = await interact(articleReadQuery, { id: updateArticleId });

  await captureHydrationPages(
    upstreamCalls,
    'OnlineStoreArticleMutationHydrate',
    articleHydrateQuery,
    'article',
    moveArticleId,
  );
  await captureHydrationPages(
    upstreamCalls,
    'OnlineStoreBlogMutationHydrate',
    blogHydrateQuery,
    'blog',
    moveTargetBlogId,
  );
  const articleMove = await interact(articleMoveMutation, {
    id: moveArticleId,
    article: { blogId: moveTargetBlogId },
  });
  operations['articleMove'] = articleMove;
  assertSuccessfulMutation(articleMove.response, 'articleUpdate');
  reads['afterArticleMove'] = await interact(articleReadQuery, { id: moveArticleId });

  await captureHydrationPages(upstreamCalls, 'OnlineStoreBlogMutationHydrate', blogHydrateQuery, 'blog', updateBlogId);
  const blogUpdate = await interact(blogUpdateMutation, {
    id: updateBlogId,
    blog: { title: `Authoritative Blog Narrow Update ${suffix}` },
  });
  operations['blogUpdate'] = blogUpdate;
  assertSuccessfulMutation(blogUpdate.response, 'blogUpdate');
  reads['afterBlogUpdate'] = await interact(blogReadQuery, { id: updateBlogId });
} finally {
  for (const id of articleIds.reverse()) {
    cleanup.push(await interact(articleDeleteMutation, { id }));
  }
  for (const id of blogIds.reverse()) {
    cleanup.push(await interact(blogDeleteMutation, { id }));
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'online-store-authoritative-content-hydration',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup,
      operations,
      reads,
      cleanup,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
