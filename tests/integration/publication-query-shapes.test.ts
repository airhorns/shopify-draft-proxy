import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import type { ProductRecord, PublicationRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function makeProduct(id: string, title: string, publicationIds: string[]): ProductRecord {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? null,
    title,
    handle: title.toLowerCase().replace(/\s+/g, '-'),
    status: 'ACTIVE',
    publicationIds,
    createdAt: '2025-01-01T00:00:00.000Z',
    updatedAt: '2025-01-01T00:00:00.000Z',
    vendor: null,
    productType: null,
    tags: [],
    totalInventory: 0,
    tracksInventory: false,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: {
      title: null,
      description: null,
    },
    category: null,
  };
}

describe('publication query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('replays hydrated publication catalog cursors through the local overlay serializer in live-hybrid mode', async () => {
    const liveHybridConfig: AppConfig = {
      ...config,
      readMode: 'live-hybrid',
    };

    const upstreamPublications = {
      data: {
        publications: {
          edges: [
            {
              cursor: 'opaque-publication-cursor-1',
              node: {
                id: 'gid://shopify/Publication/1',
                name: 'Online Store',
              },
            },
            {
              cursor: 'opaque-publication-cursor-2',
              node: {
                id: 'gid://shopify/Publication/2',
                name: 'Point of Sale',
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'opaque-publication-cursor-1',
            endCursor: 'opaque-publication-cursor-2',
          },
        },
      },
    };

    const fetchSpy = vi.spyOn(globalThis, 'fetch')
      .mockImplementationOnce(async () => new Response(JSON.stringify(upstreamPublications), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }))
      .mockImplementationOnce(async () => new Response(JSON.stringify(upstreamPublications), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }));

    const app = createApp(liveHybridConfig).callback();

    const upstreamResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query PublicationCatalog($first: Int!) { publications(first: $first) { edges { cursor node { id name } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: { first: 10 },
      });

    expect(upstreamResponse.status).toBe(200);
    expect(upstreamResponse.body).toEqual(upstreamPublications);

    const stagedCreate = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation StageProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id } userErrors { field message } } }',
        variables: { product: { title: 'Local publication overlay trigger' } },
      });

    expect(stagedCreate.status).toBe(200);
    expect(stagedCreate.body.data.productCreate.userErrors).toEqual([]);

    const overlayResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query PublicationCatalog($first: Int!) { publications(first: $first) { edges { cursor node { id name } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: { first: 10 },
      });

    expect(overlayResponse.status).toBe(200);
    expect(overlayResponse.body).toEqual({
      data: {
        publications: {
          edges: [
            {
              cursor: 'opaque-publication-cursor-1',
              node: {
                id: 'gid://shopify/Publication/1',
                name: 'Online Store',
              },
            },
            {
              cursor: 'opaque-publication-cursor-2',
              node: {
                id: 'gid://shopify/Publication/2',
                name: 'Point of Sale',
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'opaque-publication-cursor-1',
            endCursor: 'opaque-publication-cursor-2',
          },
        },
      },
    });

    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('serves top-level publications from the effective product publication graph without hitting upstream in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('publications should resolve locally in snapshot mode');
    });

    const alphaPublication: PublicationRecord = {
      id: 'gid://shopify/Publication/1',
      name: 'Online Store',
    };

    store.upsertBasePublications([alphaPublication]);
    store.upsertBaseProducts([
      makeProduct('gid://shopify/Product/1', 'Alpha Product', [alphaPublication.id]),
      makeProduct('gid://shopify/Product/2', 'Beta Product', [alphaPublication.id, 'gid://shopify/Publication/2']),
    ]);

    const app = createApp(config).callback();

    const firstPage = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query PublicationCatalog($first: Int!, $after: String) { publications(first: $first, after: $after) { edges { cursor node { id name } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: { first: 1 },
      });

    expect(firstPage.status).toBe(200);
    expect(firstPage.body).toEqual({
      data: {
        publications: {
          edges: [
            {
              cursor: 'cursor:gid://shopify/Publication/1',
              node: {
                id: 'gid://shopify/Publication/1',
                name: 'Online Store',
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: 'cursor:gid://shopify/Publication/1',
            endCursor: 'cursor:gid://shopify/Publication/1',
          },
        },
      },
    });

    const secondPage = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query PublicationCatalog($first: Int!, $after: String) { publications(first: $first, after: $after) { nodes { id name } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: { first: 10, after: 'cursor:gid://shopify/Publication/1' },
      });

    expect(secondPage.status).toBe(200);
    expect(secondPage.body).toEqual({
      data: {
        publications: {
          nodes: [
            {
              id: 'gid://shopify/Publication/2',
              name: null,
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: true,
            startCursor: 'cursor:gid://shopify/Publication/2',
            endCursor: 'cursor:gid://shopify/Publication/2',
          },
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
