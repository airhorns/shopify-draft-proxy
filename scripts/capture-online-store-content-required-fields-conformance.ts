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
const outputPath = path.join(outputDir, 'online-store-content-required-fields.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const blogCreateMutation = `#graphql
  mutation OnlineStoreContentRequiredFieldsBlogCreate($blog: BlogCreateInput!) {
    blogCreate(blog: $blog) {
      blog { id title handle }
      userErrors { field message code }
    }
  }
`;

const pageCreateMutation = `#graphql
  mutation OnlineStoreContentRequiredFieldsPageCreate($page: PageCreateInput!) {
    pageCreate(page: $page) {
      page { id title handle }
      userErrors { field message code }
    }
  }
`;

const articleCreateMutation = `#graphql
  mutation OnlineStoreContentRequiredFieldsArticleCreate($article: ArticleCreateInput!) {
    articleCreate(article: $article) {
      article { id title handle }
      userErrors { field message code }
    }
  }
`;

const pageCreateMissingTitleMutation = `#graphql
  mutation OnlineStoreContentRequiredFieldsPageCreateMissing {
    pageCreate(page: {}) {
      page { id title handle }
      userErrors { field message code }
    }
  }
`;

const blogCreateMissingTitleMutation = `#graphql
  mutation OnlineStoreContentRequiredFieldsBlogCreateMissing {
    blogCreate(blog: {}) {
      blog { id title handle }
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

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const captures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let blogId: string | null = null;

try {
  const blogCreate = await capture(captures, 'blogCreate-setup', blogCreateMutation, {
    blog: {
      title: `HAR 558 Required Fields Blog ${suffix}`,
    },
  });
  blogId = readBlogId(blogCreate);

  await capture(captures, 'pageCreate-missing-title-schema-error', pageCreateMissingTitleMutation, {});
  await capture(captures, 'pageCreate-blank-title-user-error', pageCreateMutation, {
    page: { title: '' },
  });

  await capture(captures, 'articleCreate-missing-title-schema-error', articleCreateMutation, {
    article: {
      blogId,
      author: { name: 'HAR 558 Author' },
    },
  });
  await capture(captures, 'articleCreate-blank-title-user-error', articleCreateMutation, {
    article: {
      title: '',
      blogId,
      author: { name: 'HAR 558 Author' },
    },
  });

  await capture(captures, 'blogCreate-missing-title-schema-error', blogCreateMissingTitleMutation, {});
  await capture(captures, 'blogCreate-blank-title-user-error', blogCreateMutation, {
    blog: { title: '' },
  });
} finally {
  if (blogId) {
    await capture(cleanupCaptures, 'blogDelete-cleanup', blogDeleteMutation, { id: blogId });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'online-store/content-required-fields',
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
