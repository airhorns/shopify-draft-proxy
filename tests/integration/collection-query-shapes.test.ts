import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import type { CollectionRecord, ProductRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'passthrough',
};

function makeCatalogProduct(id: string, title: string, handle: string): ProductRecord {
  const legacyResourceId = id.split('/').at(-1) ?? id;
  return {
    id,
    legacyResourceId,
    title,
    handle,
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-02T00:00:00.000Z',
    vendor: null,
    productType: null,
    tags: [],
    totalInventory: null,
    tracksInventory: null,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: { title: null, description: null },
    category: null,
  };
}

function makeCatalogCollection(
  id: string,
  title: string,
  handle: string,
  updatedAt: string,
  options: { smart?: boolean } = {},
): CollectionRecord {
  const legacyResourceId = id.split('/').at(-1) ?? id;
  const ruleSet =
    options.smart === true
      ? {
          appliedDisjunctively: false,
          rules: [{ column: 'TITLE', relation: 'CONTAINS', condition: title.split(' ')[0] ?? title }],
        }
      : null;
  return {
    id,
    legacyResourceId,
    title,
    handle,
    updatedAt,
    description: `${title} description`,
    descriptionHtml: `<p>${title} description</p>`,
    image: null,
    sortOrder: 'BEST_SELLING',
    templateSuffix: null,
    seo: { title: null, description: null },
    ruleSet,
    isSmart: options.smart === true,
  };
}

describe('collection query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves top-level collection reads from known product memberships in snapshot mode', async () => {
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/1',
        legacyResourceId: '1',
        title: 'Alpha Hat',
        handle: 'alpha-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: ['hat'],
        totalInventory: 9,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
      {
        id: 'gid://shopify/Product/2',
        legacyResourceId: '2',
        title: 'Beta Hat',
        handle: 'beta-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-03T00:00:00.000Z',
        updatedAt: '2024-01-04T00:00:00.000Z',
        vendor: 'ADIDAS',
        productType: 'ACCESSORIES',
        tags: ['hat'],
        totalInventory: 3,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/1', [
      {
        id: 'gid://shopify/Collection/100',
        productId: 'gid://shopify/Product/1',
        title: 'Featured Hats',
        handle: 'featured-hats',
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/2', [
      {
        id: 'gid://shopify/Collection/100',
        productId: 'gid://shopify/Product/2',
        title: 'Featured Hats',
        handle: 'featured-hats',
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { featured: collection(id: $id) { id title handle products(first: 10, sortKey: TITLE) { edges { cursor node { id title handle vendor } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: 'gid://shopify/Collection/100' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        featured: {
          id: 'gid://shopify/Collection/100',
          title: 'Featured Hats',
          handle: 'featured-hats',
          products: {
            edges: [
              {
                cursor: 'cursor:gid://shopify/Product/1',
                node: {
                  id: 'gid://shopify/Product/1',
                  title: 'Alpha Hat',
                  handle: 'alpha-hat',
                  vendor: 'NIKE',
                },
              },
              {
                cursor: 'cursor:gid://shopify/Product/2',
                node: {
                  id: 'gid://shopify/Product/2',
                  title: 'Beta Hat',
                  handle: 'beta-hat',
                  vendor: 'ADIDAS',
                },
              },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: 'cursor:gid://shopify/Product/1',
              endCursor: 'cursor:gid://shopify/Product/2',
            },
          },
        },
      },
    });
  });

  it('serves collectionByIdentifier and collectionByHandle aliases from snapshot state', async () => {
    store.upsertBaseProducts([makeCatalogProduct('gid://shopify/Product/1561', 'Identifier Hat', 'identifier-hat')]);
    store.upsertBaseCollections([
      {
        ...makeCatalogCollection(
          'gid://shopify/Collection/156',
          'Identifier Hats',
          'identifier-hats',
          '2026-04-25T00:00:00.000Z',
        ),
        seo: { title: 'Identifier SEO', description: 'Identifier SEO description' },
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/1561', [
      {
        id: 'gid://shopify/Collection/156',
        productId: 'gid://shopify/Product/1561',
        title: 'Identifier Hats',
        handle: 'identifier-hats',
      },
    ]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('collection identifier snapshot lookup should not hit upstream fetch');
    });

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query Collection(
          $idIdentifier: CollectionIdentifierInput!
          $handleIdentifier: CollectionIdentifierInput!
          $handle: String!
        ) {
          byId: collectionByIdentifier(identifier: $idIdentifier) {
            id
            legacyResourceId
            title
            handle
            descriptionHtml
            seo { title description }
          }
          byIdentifierHandle: collectionByIdentifier(identifier: $handleIdentifier) {
            id
            title
            handle
            products(first: 5) {
              nodes { id title handle }
              pageInfo { hasNextPage hasPreviousPage }
            }
          }
          byHandle: collectionByHandle(handle: $handle) {
            id
            title
            handle
            productsCount { count precision }
          }
          missingById: collectionByIdentifier(identifier: { id: "gid://shopify/Collection/404" }) { id }
          missingByHandle: collectionByHandle(handle: "missing-handle") { id }
          customId: collectionByIdentifier(
            identifier: { customId: { namespace: "custom", key: "external_id", value: "missing" } }
          ) { id }
        }`,
        variables: {
          idIdentifier: { id: 'gid://shopify/Collection/156' },
          handleIdentifier: { handle: 'identifier-hats' },
          handle: 'identifier-hats',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        byId: {
          id: 'gid://shopify/Collection/156',
          legacyResourceId: '156',
          title: 'Identifier Hats',
          handle: 'identifier-hats',
          descriptionHtml: '<p>Identifier Hats description</p>',
          seo: { title: 'Identifier SEO', description: 'Identifier SEO description' },
        },
        byIdentifierHandle: {
          id: 'gid://shopify/Collection/156',
          title: 'Identifier Hats',
          handle: 'identifier-hats',
          products: {
            nodes: [
              {
                id: 'gid://shopify/Product/1561',
                title: 'Identifier Hat',
                handle: 'identifier-hat',
              },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
        byHandle: {
          id: 'gid://shopify/Collection/156',
          title: 'Identifier Hats',
          handle: 'identifier-hats',
          productsCount: {
            count: 1,
            precision: 'EXACT',
          },
        },
        missingById: null,
        missingByHandle: null,
        customId: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('overlays standalone rich collection fields onto nested product collection reads', async () => {
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/501',
        legacyResourceId: '501',
        title: 'Nested Hat',
        handle: 'nested-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: ['hat'],
        totalInventory: 9,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.upsertBaseCollections([
      {
        id: 'gid://shopify/Collection/501',
        legacyResourceId: '501',
        title: 'Nested Rich Hats',
        handle: 'nested-rich-hats',
        updatedAt: '2026-04-20T12:00:00.000Z',
        description: 'Nested rich hats',
        descriptionHtml: '<p>Nested rich hats</p>',
        image: null,
        sortOrder: 'BEST_SELLING',
        templateSuffix: null,
        seo: {
          title: 'Nested SEO',
          description: 'Nested SEO description',
        },
        ruleSet: null,
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/501', [
      {
        id: 'gid://shopify/Collection/501',
        productId: 'gid://shopify/Product/501',
        title: 'Stale Membership Title',
        handle: 'stale-membership-title',
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!, $productId: ID!) { product(id: $productId) { id collections(first: 5) { nodes { id legacyResourceId title handle updatedAt description descriptionHtml productsCount { count precision } hasProduct(id: $id) sortOrder templateSuffix seo { title description } ruleSet { appliedDisjunctively } } } } }',
        variables: {
          id: 'gid://shopify/Product/501',
          productId: 'gid://shopify/Product/501',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.collections.nodes).toEqual([
      {
        id: 'gid://shopify/Collection/501',
        legacyResourceId: '501',
        title: 'Nested Rich Hats',
        handle: 'nested-rich-hats',
        updatedAt: '2026-04-20T12:00:00.000Z',
        description: 'Nested rich hats',
        descriptionHtml: '<p>Nested rich hats</p>',
        productsCount: {
          count: 1,
          precision: 'EXACT',
        },
        hasProduct: true,
        sortOrder: 'BEST_SELLING',
        templateSuffix: null,
        seo: {
          title: 'Nested SEO',
          description: 'Nested SEO description',
        },
        ruleSet: null,
      },
    ]);
  });

  it('sorts collection products by handle in snapshot mode', async () => {
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/11',
        legacyResourceId: '11',
        title: 'Zulu Hat',
        handle: 'zulu-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-05T00:00:00.000Z',
        updatedAt: '2024-01-06T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: ['hat'],
        totalInventory: 9,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
      {
        id: 'gid://shopify/Product/12',
        legacyResourceId: '12',
        title: 'Alpha Hat',
        handle: 'alpha-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-03T00:00:00.000Z',
        updatedAt: '2024-01-04T00:00:00.000Z',
        vendor: 'ADIDAS',
        productType: 'ACCESSORIES',
        tags: ['hat'],
        totalInventory: 3,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/11', [
      {
        id: 'gid://shopify/Collection/200',
        productId: 'gid://shopify/Product/11',
        title: 'Sorted Hats',
        handle: 'sorted-hats',
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/12', [
      {
        id: 'gid://shopify/Collection/200',
        productId: 'gid://shopify/Product/12',
        title: 'Sorted Hats',
        handle: 'sorted-hats',
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { collection(id: $id) { id products(first: 10, sortKey: HANDLE) { nodes { id handle } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: { id: 'gid://shopify/Collection/200' },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.collection.products.nodes).toEqual([
      { id: 'gid://shopify/Product/12', handle: 'alpha-hat' },
      { id: 'gid://shopify/Product/11', handle: 'zulu-hat' },
    ]);
  });

  it('deduplicates top-level collections and paginates them from known memberships', async () => {
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/1',
        legacyResourceId: '1',
        title: 'Alpha Hat',
        handle: 'alpha-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: [],
        totalInventory: 9,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
      {
        id: 'gid://shopify/Product/2',
        legacyResourceId: '2',
        title: 'Beta Shoe',
        handle: 'beta-shoe',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-03T00:00:00.000Z',
        updatedAt: '2024-01-04T00:00:00.000Z',
        vendor: 'ADIDAS',
        productType: 'SHOES',
        tags: [],
        totalInventory: 3,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/1', [
      {
        id: 'gid://shopify/Collection/100',
        productId: 'gid://shopify/Product/1',
        title: 'Featured Hats',
        handle: 'featured-hats',
      },
      {
        id: 'gid://shopify/Collection/200',
        productId: 'gid://shopify/Product/1',
        title: 'Winter',
        handle: 'winter',
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/2', [
      {
        id: 'gid://shopify/Collection/100',
        productId: 'gid://shopify/Product/2',
        title: 'Featured Hats',
        handle: 'featured-hats',
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const firstResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { collections(first: 1) { edges { cursor node { id title handle } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
    });

    expect(firstResponse.status).toBe(200);
    expect(firstResponse.body).toEqual({
      data: {
        collections: {
          edges: [
            {
              cursor: 'cursor:gid://shopify/Collection/100',
              node: {
                id: 'gid://shopify/Collection/100',
                title: 'Featured Hats',
                handle: 'featured-hats',
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: 'cursor:gid://shopify/Collection/100',
            endCursor: 'cursor:gid://shopify/Collection/100',
          },
        },
      },
    });

    const secondResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($after: String!) { collections(first: 10, after: $after) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: { after: 'cursor:gid://shopify/Collection/100' },
      });

    expect(secondResponse.status).toBe(200);
    expect(secondResponse.body).toEqual({
      data: {
        collections: {
          nodes: [{ id: 'gid://shopify/Collection/200', title: 'Winter', handle: 'winter' }],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: true,
            startCursor: 'cursor:gid://shopify/Collection/200',
            endCursor: 'cursor:gid://shopify/Collection/200',
          },
        },
      },
    });
  });

  it('applies top-level collection catalog query filters before pagination', async () => {
    store.upsertBaseProducts([
      makeCatalogProduct('gid://shopify/Product/1', 'Alpha Hat', 'alpha-hat'),
      makeCatalogProduct('gid://shopify/Product/2', 'Beta Shoe', 'beta-shoe'),
    ]);
    store.upsertBaseCollections([
      makeCatalogCollection(
        'gid://shopify/Collection/100',
        'Featured Hats',
        'featured-hats',
        '2024-02-01T00:00:00.000Z',
      ),
      makeCatalogCollection('gid://shopify/Collection/200', 'Smart Shoes', 'smart-shoes', '2024-03-01T00:00:00.000Z', {
        smart: true,
      }),
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/1', [
      {
        id: 'gid://shopify/Collection/100',
        productId: 'gid://shopify/Product/1',
        title: 'Featured Hats',
        handle: 'featured-hats',
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/2', [
      {
        id: 'gid://shopify/Collection/200',
        productId: 'gid://shopify/Product/2',
        title: 'Smart Shoes',
        handle: 'smart-shoes',
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { titleWildcard: collections(first: 10, query: "title:Featured*") { nodes { id title } } defaultText: collections(first: 10, query: "Shoes") { nodes { id title } } customOnly: collections(first: 10, query: "collection_type:custom") { nodes { id title } } smartOnly: collections(first: 10, query: "collection_type:smart") { nodes { id title } } handleMatch: collections(first: 10, query: "handle:smart-shoes") { nodes { id title } } memberMatch: collections(first: 10, query: "product_id:gid://shopify/Product/1") { nodes { id title } } updatedRange: collections(first: 10, query: "updated_at:>=2024-02-15T00:00:00.000Z") { nodes { id title } } unmatched: collections(first: 10, query: "title:Nope*") { nodes { id title } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
    });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      titleWildcard: {
        nodes: [{ id: 'gid://shopify/Collection/100', title: 'Featured Hats' }],
      },
      defaultText: {
        nodes: [{ id: 'gid://shopify/Collection/200', title: 'Smart Shoes' }],
      },
      customOnly: {
        nodes: [{ id: 'gid://shopify/Collection/100', title: 'Featured Hats' }],
      },
      smartOnly: {
        nodes: [{ id: 'gid://shopify/Collection/200', title: 'Smart Shoes' }],
      },
      handleMatch: {
        nodes: [{ id: 'gid://shopify/Collection/200', title: 'Smart Shoes' }],
      },
      memberMatch: {
        nodes: [{ id: 'gid://shopify/Collection/100', title: 'Featured Hats' }],
      },
      updatedRange: {
        nodes: [{ id: 'gid://shopify/Collection/200', title: 'Smart Shoes' }],
      },
      unmatched: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
    });
  });

  it('sorts and cursor-paginates top-level collections after catalog filtering', async () => {
    store.upsertBaseProducts([
      makeCatalogProduct('gid://shopify/Product/1', 'Alpha Hat', 'alpha-hat'),
      makeCatalogProduct('gid://shopify/Product/2', 'Beta Shoe', 'beta-shoe'),
    ]);
    store.upsertBaseCollections([
      makeCatalogCollection('gid://shopify/Collection/300', 'Gamma Hats', 'gamma-hats', '2024-03-01T00:00:00.000Z'),
      makeCatalogCollection('gid://shopify/Collection/100', 'Alpha Hats', 'alpha-hats', '2024-01-01T00:00:00.000Z'),
      makeCatalogCollection('gid://shopify/Collection/200', 'Beta Hats', 'beta-hats', '2024-02-01T00:00:00.000Z'),
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const firstPageResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { updatedDesc: collections(first: 2, query: "title:Hats", sortKey: UPDATED_AT, reverse: true) { edges { cursor node { id title updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } titleDesc: collections(first: 10, sortKey: TITLE, reverse: true) { nodes { id title } } relevanceWithoutQuery: collections(first: 10, sortKey: RELEVANCE) { nodes { id title } } }',
    });

    expect(firstPageResponse.status).toBe(200);
    expect(firstPageResponse.body.data).toEqual({
      updatedDesc: {
        edges: [
          {
            cursor: 'cursor:gid://shopify/Collection/300',
            node: {
              id: 'gid://shopify/Collection/300',
              title: 'Gamma Hats',
              updatedAt: '2024-03-01T00:00:00.000Z',
            },
          },
          {
            cursor: 'cursor:gid://shopify/Collection/200',
            node: {
              id: 'gid://shopify/Collection/200',
              title: 'Beta Hats',
              updatedAt: '2024-02-01T00:00:00.000Z',
            },
          },
        ],
        pageInfo: {
          hasNextPage: true,
          hasPreviousPage: false,
          startCursor: 'cursor:gid://shopify/Collection/300',
          endCursor: 'cursor:gid://shopify/Collection/200',
        },
      },
      titleDesc: {
        nodes: [
          { id: 'gid://shopify/Collection/300', title: 'Gamma Hats' },
          { id: 'gid://shopify/Collection/200', title: 'Beta Hats' },
          { id: 'gid://shopify/Collection/100', title: 'Alpha Hats' },
        ],
      },
      relevanceWithoutQuery: {
        nodes: [
          { id: 'gid://shopify/Collection/100', title: 'Alpha Hats' },
          { id: 'gid://shopify/Collection/200', title: 'Beta Hats' },
          { id: 'gid://shopify/Collection/300', title: 'Gamma Hats' },
        ],
      },
    });

    const secondPageResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($after: String!) { collections(first: 10, after: $after, query: "title:Hats", sortKey: UPDATED_AT, reverse: true) { nodes { id title } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: { after: 'cursor:gid://shopify/Collection/200' },
      });

    expect(secondPageResponse.status).toBe(200);
    expect(secondPageResponse.body.data.collections).toEqual({
      nodes: [{ id: 'gid://shopify/Collection/100', title: 'Alpha Hats' }],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: true,
        startCursor: 'cursor:gid://shopify/Collection/100',
        endCursor: 'cursor:gid://shopify/Collection/100',
      },
    });
  });

  it('applies collection catalog filtering to staged collection and membership changes', async () => {
    store.upsertBaseProducts([
      makeCatalogProduct('gid://shopify/Product/1', 'Alpha Hat', 'alpha-hat'),
      makeCatalogProduct('gid://shopify/Product/2', 'Beta Shoe', 'beta-shoe'),
    ]);
    store.upsertBaseCollections([
      makeCatalogCollection('gid://shopify/Collection/100', 'Base Hats', 'base-hats', '2024-01-01T00:00:00.000Z'),
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/1', [
      {
        id: 'gid://shopify/Collection/100',
        productId: 'gid://shopify/Product/1',
        title: 'Base Hats',
        handle: 'base-hats',
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { collectionCreate(input: { title: "Draft Shoes", handle: "draft-shoes" }) { collection { id title handle } userErrors { field message } } }',
    });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.collectionCreate.userErrors).toEqual([]);
    const createdCollection = createResponse.body.data.collectionCreate.collection as { id: string };

    const addResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation ($id: ID!) { collectionAddProducts(id: $id, productIds: ["gid://shopify/Product/2"]) { collection { id title } userErrors { field message } } }',
        variables: { id: createdCollection.id },
      });

    expect(addResponse.status).toBe(200);
    expect(addResponse.body.data.collectionAddProducts.userErrors).toEqual([]);

    const filteredResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { stagedTitle: collections(first: 10, query: "title:Draft*") { nodes { id title handle } } stagedMembership: collections(first: 10, query: "product_id:gid://shopify/Product/2") { nodes { id title handle } } baseMembership: collections(first: 10, query: "product_id:gid://shopify/Product/1") { nodes { id title handle } } }',
        variables: { id: createdCollection.id },
      });

    expect(filteredResponse.status).toBe(200);
    expect(filteredResponse.body.data).toEqual({
      stagedTitle: {
        nodes: [{ id: createdCollection.id, title: 'Draft Shoes', handle: 'draft-shoes' }],
      },
      stagedMembership: {
        nodes: [{ id: createdCollection.id, title: 'Draft Shoes', handle: 'draft-shoes' }],
      },
      baseMembership: {
        nodes: [{ id: 'gid://shopify/Collection/100', title: 'Base Hats', handle: 'base-hats' }],
      },
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation ($input: CollectionDeleteInput!) { collectionDelete(input: $input) { deletedCollectionId userErrors { field message } } }',
        variables: { input: { id: 'gid://shopify/Collection/100' } },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.collectionDelete.userErrors).toEqual([]);

    const afterDeleteResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { collections(first: 10, query: "product_id:gid://shopify/Product/1") { nodes { id title } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
    });

    expect(afterDeleteResponse.status).toBe(200);
    expect(afterDeleteResponse.body.data.collections).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
  });

  it('serves collection overlay reads in live-hybrid mode after product hydration and a staged mutation', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementationOnce(async () => {
        return new Response(
          JSON.stringify({
            data: {
              product: {
                id: 'gid://shopify/Product/10',
                title: 'Hydrated Hat',
                handle: 'hydrated-hat',
                status: 'ACTIVE',
                vendor: 'NIKE',
                productType: 'ACCESSORIES',
                tags: ['hat'],
                totalInventory: 8,
                tracksInventory: true,
                createdAt: '2024-01-01T00:00:00.000Z',
                updatedAt: '2024-01-02T00:00:00.000Z',
                collections: {
                  nodes: [
                    {
                      id: 'gid://shopify/Collection/900',
                      title: 'Hydrated Collection',
                      handle: 'hydrated-collection',
                    },
                  ],
                },
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        );
      })
      .mockImplementationOnce(async () => {
        return new Response(JSON.stringify({ data: { collection: null } }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query HydrateProduct($id: ID!) { product(id: $id) { id title handle status vendor productType tags totalInventory tracksInventory createdAt updatedAt collections(first: 10) { nodes { id title handle } } } }',
        variables: { id: 'gid://shopify/Product/10' },
      });

    expect(hydrateResponse.status).toBe(200);
    expect(hydrateResponse.body.data.product.collections.nodes).toEqual([
      {
        id: 'gid://shopify/Collection/900',
        title: 'Hydrated Collection',
        handle: 'hydrated-collection',
      },
    ]);

    const stageResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation StageOverlay($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title } userErrors { field message } } }',
        variables: {
          product: {
            id: 'gid://shopify/Product/10',
            title: 'Hydrated Hat Draft',
          },
        },
      });

    expect(stageResponse.status).toBe(200);
    expect(stageResponse.body.data.productUpdate.userErrors).toEqual([]);

    const collectionResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query HydratedCollection($id: ID!) { collection(id: $id) { id title handle products(first: 10) { nodes { id title handle vendor } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: { id: 'gid://shopify/Collection/900' },
      });

    expect(collectionResponse.status).toBe(200);
    expect(collectionResponse.body).toEqual({
      data: {
        collection: {
          id: 'gid://shopify/Collection/900',
          title: 'Hydrated Collection',
          handle: 'hydrated-collection',
          products: {
            nodes: [
              {
                id: 'gid://shopify/Product/10',
                title: 'Hydrated Hat Draft',
                handle: 'hydrated-hat',
                vendor: 'NIKE',
              },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
      },
    });
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('hydrates collection product order from upstream collection reads before applying staged overlays', async () => {
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/1',
        legacyResourceId: '1',
        title: 'Alpha Hat',
        handle: 'alpha-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '1970-01-01T00:00:00.000Z',
        updatedAt: '1970-01-01T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: ['hat'],
        totalInventory: 9,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          data: {
            collection: {
              id: 'gid://shopify/Collection/700',
              title: 'Manual Hats',
              handle: 'manual-hats',
              products: {
                edges: [
                  {
                    cursor: 'upstream-alpha',
                    node: {
                      id: 'gid://shopify/Product/1',
                      title: 'Alpha Hat',
                      handle: 'alpha-hat',
                      tags: ['hat'],
                    },
                  },
                  {
                    cursor: 'upstream-beta',
                    node: {
                      id: 'gid://shopify/Product/2',
                      title: 'Beta Hat',
                      handle: 'beta-hat',
                      tags: ['hat'],
                    },
                  },
                ],
              },
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation StageTitle($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title } userErrors { field message } } }',
        variables: {
          product: {
            id: 'gid://shopify/Product/1',
            title: 'Alpha Hat Draft',
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.productUpdate.userErrors).toEqual([]);

    const collectionResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query OrderedCollection($id: ID!) { collection(id: $id) { id title handle products(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: { id: 'gid://shopify/Collection/700' },
      });

    expect(collectionResponse.status).toBe(200);
    expect(collectionResponse.body).toEqual({
      data: {
        collection: {
          id: 'gid://shopify/Collection/700',
          title: 'Manual Hats',
          handle: 'manual-hats',
          products: {
            nodes: [
              { id: 'gid://shopify/Product/1', title: 'Alpha Hat Draft', handle: 'alpha-hat' },
              { id: 'gid://shopify/Product/2', title: 'Beta Hat', handle: 'beta-hat' },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
      },
    });
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });
});
