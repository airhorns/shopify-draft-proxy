/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
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
const outputPath = path.join(outputDir, 'online-store-article-media-navigation-follow-through.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const articleCreateMutation = await readFile(
  'config/parity-requests/online-store/online-store-article-media.graphql',
  'utf8',
);

const schemaQuery = `#graphql
  query OnlineStoreArticleMediaNavigationSchema {
    pageType: __type(name: "Page") {
      fields { name }
    }
    articleType: __type(name: "Article") {
      fields { name }
    }
  }
`;

const articleUpdateMutation = `#graphql
  mutation OnlineStoreArticleMediaUpdate($id: ID!, $article: ArticleUpdateInput!) {
    articleUpdate(id: $id, article: $article) {
      article {
        id
        title
        image {
          id
          altText
          url
          width
          height
        }
        metafield(namespace: "online_store_conformance", key: "hero") {
          id
          namespace
          key
          type
          value
          jsonValue
          ownerType
        }
        metafields(first: 5, namespace: "online_store_conformance") {
          nodes {
            id
            namespace
            key
            type
            value
            jsonValue
            ownerType
          }
          pageInfo { hasNextPage hasPreviousPage }
        }
      }
      userErrors { field message }
    }
  }
`;

const articleReadQuery = `#graphql
  query OnlineStoreArticleMediaRead($id: ID!) {
    article(id: $id) {
      id
      title
      image {
        id
        altText
        url
        width
        height
      }
      metafield(namespace: "online_store_conformance", key: "hero") {
        id
        namespace
        key
        type
        value
        jsonValue
        ownerType
      }
      metafields(first: 5, namespace: "online_store_conformance") {
        nodes {
          id
          namespace
          key
          type
          value
          jsonValue
          ownerType
        }
        pageInfo { hasNextPage hasPreviousPage }
      }
    }
  }
`;

const pageCreateMutation = `#graphql
  mutation OnlineStoreNavigationPageCreate($page: PageCreateInput!) {
    pageCreate(page: $page) {
      page { id title handle }
      userErrors { field message }
    }
  }
`;

const menuCreateMutation = `#graphql
  mutation OnlineStoreNavigationMenuCreate($title: String!, $handle: String!, $items: [MenuItemCreateInput!]!) {
    menuCreate(title: $title, handle: $handle, items: $items) {
      menu {
        id
        title
        handle
        isDefault
        items {
          id
          title
          type
          resourceId
          url
          items { id title type resourceId url }
        }
      }
      userErrors { field message }
    }
  }
`;

const menuReadQuery = `#graphql
  query OnlineStoreNavigationMenuRead($id: ID!) {
    menu(id: $id) {
      id
      title
      handle
      isDefault
      items {
        id
        title
        type
        resourceId
        url
        items { id title type resourceId url }
      }
    }
  }
`;

const menusCatalogQuery = `#graphql
  query OnlineStoreNavigationMenusCatalog($query: String!) {
    menus(first: 5, query: $query) {
      nodes {
        id
        title
        handle
        isDefault
      }
      pageInfo { hasNextPage hasPreviousPage }
    }
  }
`;

const menuUpdateMutation = `#graphql
  mutation OnlineStoreNavigationMenuUpdate($id: ID!, $title: String!, $handle: String, $items: [MenuItemUpdateInput!]!) {
    menuUpdate(id: $id, title: $title, handle: $handle, items: $items) {
      menu {
        id
        title
        handle
        isDefault
        items {
          id
          title
          type
          resourceId
          url
          items { id title type resourceId url }
        }
      }
      userErrors { field message }
    }
  }
`;

const menuDeleteMutation = `#graphql
  mutation OnlineStoreNavigationMenuDelete($id: ID!) {
    menuDelete(id: $id) {
      deletedMenuId
      userErrors { field message }
    }
  }
`;

const articleDeleteMutation = `#graphql
  mutation OnlineStoreArticleMediaArticleCleanup($id: ID!) {
    articleDelete(id: $id) {
      deletedArticleId
      userErrors { field message }
    }
  }
`;

const pageDeleteMutation = `#graphql
  mutation OnlineStoreArticleMediaPageCleanup($id: ID!) {
    pageDelete(id: $id) {
      deletedPageId
      userErrors { field message }
    }
  }
`;

const blogDeleteMutation = `#graphql
  mutation OnlineStoreArticleMediaBlogCleanup($id: ID!) {
    blogDelete(id: $id) {
      deletedBlogId
      userErrors { field message }
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
    throw new Error(`${label} did not return an ID: ${JSON.stringify(payload)}`);
  }
  return id;
}

function assertNoTopLevelErrors(payload: unknown, label: string): void {
  if (readPath(payload, ['errors'])) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(payload, null, 2)}`);
  }
}

function hasField(schemaPayload: unknown, typeKey: string, fieldName: string): boolean {
  const fields = readPath(schemaPayload, ['data', typeKey, 'fields']);
  return (
    Array.isArray(fields) &&
    fields.some((field) => {
      return typeof field === 'object' && field !== null && (field as { name?: unknown }).name === fieldName;
    })
  );
}

function readUrlPattern(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  return value.replace(/\/pages\/[^/?#]+/u, '/pages/<page-handle>');
}

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const captures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let blogId: string | null = null;
let articleId: string | null = null;
let pageId: string | null = null;
let menuId: string | null = null;
let schemaEvidence: Record<string, unknown> | null = null;
let articleEvidence: Record<string, unknown> | null = null;
let menuEvidence: Record<string, unknown> | null = null;

try {
  const schema = await capture(captures, 'schema-boundaries', schemaQuery, {});

  const articleCreate = await capture(captures, 'article-create-media-metafield', articleCreateMutation, {
    blog: {
      title: `Online Store Article Media Blog ${suffix}`,
    },
    article: {
      title: `Online Store Article Media ${suffix}`,
      body: '<p>Online store article media body</p>',
      summary: '<p>Online store article media summary</p>',
      author: { name: 'Online Store Media Author' },
      isPublished: true,
      image: {
        altText: 'Online store conformance hero',
        url: 'https://placehold.co/64x64/png',
      },
      metafields: [
        {
          namespace: 'online_store_conformance',
          key: 'hero',
          type: 'single_line_text_field',
          value: 'created hero',
        },
      ],
    },
  });
  assertNoTopLevelErrors(articleCreate, 'article-create-media-metafield');
  blogId = readRequiredId(articleCreate, ['data', 'articleCreate', 'article', 'blog', 'id'], 'articleCreate blog');
  articleId = readRequiredId(articleCreate, ['data', 'articleCreate', 'article', 'id'], 'articleCreate article');

  const articleUpdate = await capture(captures, 'article-update-media-metafield', articleUpdateMutation, {
    id: articleId,
    article: {
      image: {
        altText: 'Online store conformance hero updated',
        url: 'https://placehold.co/80x80/png',
      },
      metafields: [
        {
          namespace: 'online_store_conformance',
          key: 'hero',
          type: 'single_line_text_field',
          value: 'updated hero',
        },
      ],
    },
  });
  assertNoTopLevelErrors(articleUpdate, 'article-update-media-metafield');

  await capture(captures, 'article-read-after-update', articleReadQuery, { id: articleId });

  const pageCreate = await capture(captures, 'navigation-page-create', pageCreateMutation, {
    page: {
      title: `Online Store Navigation Page ${suffix}`,
      body: '<p>Online store navigation page body</p>',
      isPublished: true,
    },
  });
  assertNoTopLevelErrors(pageCreate, 'navigation-page-create');
  pageId = readRequiredId(pageCreate, ['data', 'pageCreate', 'page', 'id'], 'pageCreate');

  const menuHandle = `online-store-navigation-${suffix.toLowerCase()}`;
  const menuCreate = await capture(captures, 'navigation-menu-create', menuCreateMutation, {
    title: `Online Store Navigation ${suffix}`,
    handle: menuHandle,
    items: [
      {
        title: 'Online Store Page Link',
        type: 'PAGE',
        resourceId: pageId,
      },
    ],
  });
  assertNoTopLevelErrors(menuCreate, 'navigation-menu-create');
  menuId = readRequiredId(menuCreate, ['data', 'menuCreate', 'menu', 'id'], 'menuCreate');

  await capture(captures, 'navigation-menu-read-after-create', menuReadQuery, { id: menuId });
  await capture(captures, 'navigation-menus-catalog-by-handle', menusCatalogQuery, {
    query: `handle:${menuHandle}`,
  });

  const menuUpdate = await capture(captures, 'navigation-menu-update', menuUpdateMutation, {
    id: menuId,
    title: `Online Store Navigation Updated ${suffix}`,
    handle: menuHandle,
    items: [
      {
        title: 'Online Store Page Link Updated',
        type: 'PAGE',
        resourceId: pageId,
      },
      {
        title: 'Online Store External Link',
        type: 'HTTP',
        url: 'https://example.com/online-store-conformance',
      },
    ],
  });

  const menuDelete = await capture(captures, 'navigation-menu-delete', menuDeleteMutation, { id: menuId });
  assertNoTopLevelErrors(menuDelete, 'navigation-menu-delete');
  if (readPath(menuDelete, ['data', 'menuDelete', 'deletedMenuId'])) menuId = null;

  const menuReadAfterDelete = await capture(captures, 'navigation-menu-read-after-delete', menuReadQuery, {
    id: readRequiredId(menuDelete, ['data', 'menuDelete', 'deletedMenuId'], 'menuDelete'),
  });

  schemaEvidence = {
    pageHasOnlineStoreUrlField: hasField(schema, 'pageType', 'onlineStoreUrl'),
    articleHasSeoField: hasField(schema, 'articleType', 'seo'),
  };
  const createMenuItem = readPath(menuCreate, ['data', 'menuCreate', 'menu', 'items', '0']);
  const updateFirstItem = readPath(menuUpdate, ['data', 'menuUpdate', 'menu', 'items', '0']);
  const updateSecondItem = readPath(menuUpdate, ['data', 'menuUpdate', 'menu', 'items', '1']);

  articleEvidence = {
    createImageHost: new URL(String(readPath(articleCreate, ['data', 'articleCreate', 'article', 'image', 'url'])))
      .host,
    updateImageHost: new URL(String(readPath(articleUpdate, ['data', 'articleUpdate', 'article', 'image', 'url'])))
      .host,
    updateReadImageMatches:
      readPath(articleUpdate, ['data', 'articleUpdate', 'article', 'image', 'id']) ===
      readPath(captures, ['3', 'response', 'data', 'article', 'image', 'id']),
    updateReadMetafieldMatches:
      readPath(articleUpdate, ['data', 'articleUpdate', 'article', 'metafield', 'value']) ===
      readPath(captures, ['3', 'response', 'data', 'article', 'metafield', 'value']),
  };

  menuEvidence = {
    createPageItemUrlPattern: readUrlPattern(readPath(createMenuItem, ['url'])),
    createPageItemResourceType:
      typeof readPath(createMenuItem, ['resourceId']) === 'string' &&
      String(readPath(createMenuItem, ['resourceId'])).includes('/Page/')
        ? 'Page'
        : null,
    updateFirstItemUrlPattern: readUrlPattern(readPath(updateFirstItem, ['url'])),
    updateSecondItemUrl: readPath(updateSecondItem, ['url']),
    readAfterDelete: readPath(menuReadAfterDelete, ['data', 'menu']),
  };
} finally {
  if (menuId) {
    await capture(cleanupCaptures, 'menuDelete-cleanup', menuDeleteMutation, { id: menuId });
  }
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
      scenarioId: 'online-store-article-media-navigation-follow-through',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      source: 'live-shopify-disposable-capture',
      interactions: captures,
      cleanup: cleanupCaptures,
      upstreamCalls: [],
      schemaEvidence,
      articleEvidence,
      menuEvidence,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
