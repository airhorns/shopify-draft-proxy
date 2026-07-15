/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient, runStorefrontGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import {
  buildAdminAuthHeaders,
  buildStorefrontRequestHeaders,
  getStoredStorefrontAccessToken,
  getValidConformanceAccessToken,
} from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type StorefrontCapture = {
  name: string;
  method: 'POST';
  apiSurface: 'storefront';
  apiVersion: string;
  path: string;
  endpoint: string;
  authMode: 'storefront-access-token';
  headers: Record<string, string>;
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const storedStorefrontAuth = await getStoredStorefrontAccessToken();
if (storedStorefrontAuth.shop && storedStorefrontAuth.shop !== storeDomain) {
  throw new Error(
    `Stored Storefront token is for ${storedStorefrontAuth.shop}, but SHOPIFY_CONFORMANCE_STORE_DOMAIN is ${storeDomain}. ` +
      'Run `corepack pnpm conformance:grant-storefront-token` for the target store.',
  );
}

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const storefrontOptions = {
  storeOrigin: `https://${storeDomain}`,
  apiVersion,
  storefrontAccessToken: storedStorefrontAuth.storefront_access_token,
};
const storefrontEndpoint = `https://${storeDomain}/api/${apiVersion}/graphql.json`;
const storefrontPath = `/api/${apiVersion}/graphql.json`;
const storefrontRedactedHeaders = Object.fromEntries(
  Object.keys(buildStorefrontRequestHeaders(storedStorefrontAuth.storefront_access_token)).map((name) => [
    name,
    '<redacted:storefront-access-token>',
  ]),
);

const setupDocumentPath = 'config/parity-requests/storefront/storefront-content-setup-admin.graphql';
const articleSetupDocumentPath = 'config/parity-requests/storefront/storefront-content-article-setup-admin.graphql';
const storefrontReadDocumentPath =
  'config/parity-requests/storefront/storefront-content-read-after-admin-setup.graphql';
const menuHydrateDocumentPath = 'config/parity-requests/storefront/storefront-content-menu-hydrate.graphql';
const urlRedirectsEmptyDocumentPath = 'config/parity-requests/storefront/storefront-url-redirects-empty.graphql';

const setupDocument = await readFile(setupDocumentPath, 'utf8');
const articleSetupDocument = await readFile(articleSetupDocumentPath, 'utf8');
const storefrontReadDocument = await readFile(storefrontReadDocumentPath, 'utf8');
const menuHydrateDocument = await readFile(menuHydrateDocumentPath, 'utf8');
const urlRedirectsEmptyDocument = await readFile(urlRedirectsEmptyDocumentPath, 'utf8');

const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const tag = `storefront-content-${suffix}`;
const setupVariables = {
  blog: {
    title: `Storefront Content Blog ${suffix}`,
    handle: `storefront-content-blog-${suffix}`,
  },
  page: {
    title: `Storefront Content Page ${suffix}`,
    handle: `storefront-content-page-${suffix}`,
    body: `<p>Storefront content page body ${suffix}</p>`,
    isPublished: true,
  },
} satisfies Record<string, unknown>;
const articleSetupVariables = {
  article: {
    title: `Storefront Content Article ${suffix}`,
    handle: `storefront-content-article-${suffix}`,
    body: `<p>Storefront content article body ${suffix}</p>`,
    summary: `Storefront content article summary ${suffix}`,
    tags: ['storefront-content', tag],
    author: { name: `Storefront Content Author ${suffix}` },
    isPublished: true,
  },
} satisfies Record<string, unknown>;

const adminCaptures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let articleId: string | null = null;
let pageId: string | null = null;
let blogId: string | null = null;
let storefrontReadCapture: StorefrontCapture | null = null;
let menuHydrateCapture: StorefrontCapture | null = null;
let urlRedirectsEmptyCapture: StorefrontCapture | null = null;

async function captureAdmin(name: string, query: string, variables: Record<string, unknown>): Promise<unknown> {
  const result = await runGraphqlRaw(query, variables);
  const capture = {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
  adminCaptures.push(capture);
  return result.payload;
}

async function captureAdminCleanup(name: string, query: string, variables: Record<string, unknown>): Promise<void> {
  const result = await runGraphqlRaw(query, variables);
  cleanupCaptures.push({
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  });
}

async function storefrontRequest(
  name: string,
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<StorefrontCapture> {
  const result = await runStorefrontGraphqlRequest(storefrontOptions, query, variables);
  return {
    name,
    method: 'POST',
    apiSurface: 'storefront',
    apiVersion,
    path: storefrontPath,
    endpoint: storefrontEndpoint,
    authMode: 'storefront-access-token',
    headers: storefrontRedactedHeaders,
    operationName,
    query,
    variables,
    response: {
      status: result.status,
      body: result.payload,
    },
  };
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  return pathSegments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) return null;
    return (current as Record<string, unknown>)[segment] ?? null;
  }, value);
}

function readRequiredString(value: unknown, pathSegments: string[], label: string): string {
  const result = readPath(value, pathSegments);
  if (typeof result !== 'string' || result.length === 0) {
    throw new Error(`${label} did not return a string at ${pathSegments.join('.')}: ${JSON.stringify(value)}`);
  }
  return result;
}

function assertNoTopLevelErrors(payload: unknown, label: string): void {
  const errors = readPath(payload, ['errors']);
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathSegments: string[], label: string): void {
  const userErrors = readPath(payload, pathSegments);
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
}

function storefrontContentVisible(payload: unknown): boolean {
  return (
    typeof readPath(payload, ['data', 'article', 'handle']) === 'string' &&
    typeof readPath(payload, ['data', 'blog', 'handle']) === 'string' &&
    typeof readPath(payload, ['data', 'page', 'handle']) === 'string' &&
    typeof readPath(payload, ['data', 'articles', 'nodes', '0', 'handle']) === 'string' &&
    typeof readPath(payload, ['data', 'blogs', 'nodes', '0', 'handle']) === 'string' &&
    typeof readPath(payload, ['data', 'pages', 'nodes', '0', 'handle']) === 'string'
  );
}

async function waitForStorefrontContent(variables: Record<string, unknown>): Promise<StorefrontCapture> {
  let lastCapture: StorefrontCapture | null = null;
  for (let attempt = 1; attempt <= 30; attempt += 1) {
    lastCapture = await storefrontRequest(
      'storefront-content-read-after-admin-setup',
      'StorefrontContentReadAfterAdminSetup',
      storefrontReadDocument,
      variables,
    );
    assertNoTopLevelErrors(lastCapture.response.body, `storefront content read attempt ${attempt}`);
    if (storefrontContentVisible(lastCapture.response.body)) return lastCapture;
    await delay(2000);
  }
  throw new Error(
    `Storefront content did not become visible after polling: ${JSON.stringify(lastCapture?.response.body, null, 2)}`,
  );
}

const articleDeleteMutation = `#graphql
  mutation StorefrontContentArticleCleanup($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message code }
    }
  }
`;

const pageDeleteMutation = `#graphql
  mutation StorefrontContentPageCleanup($id: ID!) {
    pageDelete(id: $id) {
      deletedPageId
      userErrors { field message code }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation StorefrontContentBlogCleanup($id: ID!) {
    blogDelete(id: $id) {
      deletedBlogId
      userErrors { field message code }
    }
  }
`;

try {
  const adminSetup = await captureAdmin('admin-setup', setupDocument, setupVariables);
  assertNoTopLevelErrors(adminSetup, 'admin setup');
  assertNoUserErrors(adminSetup, ['data', 'setupBlog', 'userErrors'], 'blogCreate');
  assertNoUserErrors(adminSetup, ['data', 'setupPage', 'userErrors'], 'pageCreate');

  blogId = readRequiredString(adminSetup, ['data', 'setupBlog', 'blog', 'id'], 'blogCreate');
  pageId = readRequiredString(adminSetup, ['data', 'setupPage', 'page', 'id'], 'pageCreate');
  const articleSetup = await captureAdmin('admin-article-setup', articleSetupDocument, {
    article: {
      ...(articleSetupVariables.article as Record<string, unknown>),
      blogId,
    },
  });
  assertNoTopLevelErrors(articleSetup, 'admin article setup');
  assertNoUserErrors(articleSetup, ['data', 'setupArticle', 'userErrors'], 'articleCreate');

  articleId = readRequiredString(articleSetup, ['data', 'setupArticle', 'article', 'id'], 'articleCreate');
  const articleHandle = readRequiredString(
    articleSetup,
    ['data', 'setupArticle', 'article', 'handle'],
    'article handle',
  );
  const blogHandle = readRequiredString(
    articleSetup,
    ['data', 'setupArticle', 'article', 'blog', 'handle'],
    'blog handle',
  );
  const pageHandle = readRequiredString(adminSetup, ['data', 'setupPage', 'page', 'handle'], 'page handle');
  const storefrontVariables = {
    articleHandle,
    articleId,
    articleQuery: `Storefront Content Article ${suffix}`,
    blogHandle,
    blogQuery: `Storefront Content Blog ${suffix}`,
    menuHandle: 'main-menu',
    pageHandle,
    pageId,
    pageQuery: `Storefront Content Page ${suffix}`,
  };

  storefrontReadCapture = await waitForStorefrontContent(storefrontVariables);
  menuHydrateCapture = await storefrontRequest(
    'storefront-menu-hydrate',
    'StorefrontMenuHydrate',
    menuHydrateDocument,
    { handle: storefrontVariables.menuHandle },
  );
  assertNoTopLevelErrors(menuHydrateCapture.response.body, 'menu hydrate');

  urlRedirectsEmptyCapture = await storefrontRequest(
    'storefront-url-redirects-empty',
    'StorefrontUrlRedirectsEmpty',
    urlRedirectsEmptyDocument,
    { query: `path:/__shopify-draft-proxy-missing-${suffix}` },
  );
  assertNoTopLevelErrors(urlRedirectsEmptyCapture.response.body, 'urlRedirects empty');
} finally {
  if (articleId !== null) {
    await captureAdminCleanup('articleDelete-cleanup', articleDeleteMutation, { id: articleId });
  }
  if (pageId !== null) {
    await captureAdminCleanup('pageDelete-cleanup', pageDeleteMutation, { id: pageId });
  }
  if (blogId !== null) {
    await captureAdminCleanup('blogDelete-cleanup', blogDeleteMutation, { id: blogId });
  }
}

if (storefrontReadCapture === null || menuHydrateCapture === null || urlRedirectsEmptyCapture === null) {
  throw new Error('Storefront content capture did not complete.');
}

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'storefront');
await mkdir(outputDir, { recursive: true });
const outputPath = path.join(outputDir, 'storefront-content-read-after-admin-setup.json');
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'storefront-content-read-after-admin-setup',
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      apiSurface: 'storefront',
      endpoint: storefrontEndpoint,
      authMode: 'storefront-access-token',
      storefrontToken: {
        id: storedStorefrontAuth.storefront_token_id || '<unknown>',
        title: storedStorefrontAuth.storefront_token_title || '<unknown>',
        accessScopes: storedStorefrontAuth.storefront_access_scopes,
        obtainedAt: storedStorefrontAuth.obtained_at || '<unknown>',
      },
      adminSetup: adminCaptures.find((capture) => capture.name === 'admin-setup'),
      adminArticleSetup: adminCaptures.find((capture) => capture.name === 'admin-article-setup'),
      storefrontRead: storefrontReadCapture,
      menuHydrate: menuHydrateCapture,
      urlRedirectsEmpty: urlRedirectsEmptyCapture,
      cleanup: cleanupCaptures,
      upstreamCalls: [menuHydrateCapture],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
console.log(`Captured authenticated Storefront content status ${storefrontReadCapture.response.status}`);
