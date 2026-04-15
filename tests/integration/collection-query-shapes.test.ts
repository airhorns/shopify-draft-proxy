import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'passthrough',
};

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

    const firstResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
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
});
