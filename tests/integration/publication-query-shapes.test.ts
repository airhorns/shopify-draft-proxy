import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import type { CollectionRecord, ProductRecord, PublicationRecord } from '../../src/state/types.js';

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

function makeCollection(id: string, title: string, publicationIds: string[]): CollectionRecord {
  return {
    id,
    legacyResourceId: id.split('/').at(-1) ?? null,
    title,
    handle: title.toLowerCase().replace(/\s+/g, '-'),
    publicationIds,
    updatedAt: '2025-01-01T00:00:00.000Z',
    description: null,
    descriptionHtml: null,
    image: null,
    sortOrder: 'MANUAL',
    templateSuffix: null,
    seo: {
      title: null,
      description: null,
    },
    ruleSet: null,
    isSmart: false,
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

    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementationOnce(
        async () =>
          new Response(JSON.stringify(upstreamPublications), {
            status: 200,
            headers: { 'Content-Type': 'application/json' },
          }),
      )
      .mockImplementationOnce(
        async () =>
          new Response(JSON.stringify(upstreamPublications), {
            status: 200,
            headers: { 'Content-Type': 'application/json' },
          }),
      );

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

  it('serves empty channel and publication roots locally in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('empty publication roots should resolve locally in snapshot mode');
    });

    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query EmptyPublicationRoots($id: ID!) {
          publication(id: $id) { id name }
          channel(id: $id) { id name }
          channels(first: 5) {
            nodes { id name }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          publicationsCount { count precision }
          publishedProductsCount(publicationId: $id) { count precision }
        }`,
        variables: { id: 'gid://shopify/Publication/999' },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        publication: null,
        channel: null,
        channels: {
          nodes: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        publicationsCount: {
          count: 0,
          precision: 'EXACT',
        },
        publishedProductsCount: {
          count: 0,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages publication lifecycle mutations and downstream product and collection visibility locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('publication lifecycle should stage locally');
    });

    const basePublication: PublicationRecord = {
      id: 'gid://shopify/Publication/1',
      name: 'Online Store',
    };
    store.upsertBasePublications([basePublication]);
    store.upsertBaseProducts([makeProduct('gid://shopify/Product/1', 'Published Product', [basePublication.id])]);
    store.upsertBaseCollections([
      makeCollection('gid://shopify/Collection/1', 'Published Collection', [basePublication.id]),
    ]);

    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation CreatePublication($input: PublicationInput!) {
          publicationCreate(input: $input) {
            publication {
              id
              name
              autoPublish
              supportsFuturePublishing
              channel { id name }
            }
            userErrors { field message }
          }
        }`,
        variables: { input: { name: 'Codex Sales Channel', autoPublish: true } },
      });

    expect(createResponse.status).toBe(200);
    const createdPublication = createResponse.body.data.publicationCreate.publication;
    expect(createdPublication).toMatchObject({
      name: 'Codex Sales Channel',
      autoPublish: true,
      supportsFuturePublishing: false,
      channel: {
        name: 'Codex Sales Channel',
      },
    });
    expect(createdPublication.id).toMatch(/^gid:\/\/shopify\/Publication\//u);
    expect(createResponse.body.data.publicationCreate.userErrors).toEqual([]);

    const publicationId = createdPublication.id as string;
    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation UpdatePublication($id: ID!, $input: PublicationInput!) {
          publicationUpdate(id: $id, input: $input) {
            publication { id name autoPublish }
            userErrors { field message }
          }
        }`,
        variables: { id: publicationId, input: { name: 'Codex Updated Channel', autoPublish: false } },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.publicationUpdate).toEqual({
      publication: {
        id: publicationId,
        name: 'Codex Updated Channel',
        autoPublish: false,
      },
      userErrors: [],
    });

    const publishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation PublishProduct($id: ID!, $input: [PublicationInput!]!, $publicationId: ID!) {
          publishablePublish(id: $id, input: $input) {
            publishable {
              ... on Product {
                id
                publishedOnPublication(publicationId: $publicationId)
                resourcePublicationsCount { count precision }
              }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          id: 'gid://shopify/Product/1',
          input: [{ publicationId }],
          publicationId,
        },
      });

    expect(publishResponse.status).toBe(200);
    expect(publishResponse.body.data.publishablePublish).toEqual({
      publishable: {
        id: 'gid://shopify/Product/1',
        publishedOnPublication: true,
        resourcePublicationsCount: {
          count: 2,
          precision: 'EXACT',
        },
      },
      userErrors: [],
    });

    const readAfterPublish = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query ReadPublished($publicationId: ID!) {
          publication(id: $publicationId) {
            id
            name
            products(first: 5) { nodes { id title } }
            publishedProductsCount { count precision }
          }
          publicationsCount { count precision }
          publishedProductsCount(publicationId: $publicationId) { count precision }
          product(id: "gid://shopify/Product/1") {
            publishedOnPublication(publicationId: $publicationId)
          }
        }`,
        variables: { publicationId },
      });

    expect(readAfterPublish.status).toBe(200);
    expect(readAfterPublish.body.data).toMatchObject({
      publication: {
        id: publicationId,
        name: 'Codex Updated Channel',
        products: {
          nodes: [
            {
              id: 'gid://shopify/Product/1',
              title: 'Published Product',
            },
          ],
        },
        publishedProductsCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
      publicationsCount: {
        count: 2,
        precision: 'EXACT',
      },
      publishedProductsCount: {
        count: 1,
        precision: 'EXACT',
      },
      product: {
        publishedOnPublication: true,
      },
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `mutation DeletePublication($id: ID!) {
          publicationDelete(id: $id) {
            deletedId
            publication { id name }
            userErrors { field message }
          }
        }`,
        variables: { id: publicationId },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.publicationDelete).toEqual({
      deletedId: publicationId,
      publication: {
        id: publicationId,
        name: 'Codex Updated Channel',
      },
      userErrors: [],
    });

    const readAfterDelete = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query ReadAfterDelete($publicationId: ID!) {
          publication(id: $publicationId) { id name }
          publicationsCount { count precision }
          publishedProductsCount(publicationId: $publicationId) { count precision }
          product(id: "gid://shopify/Product/1") {
            publishedOnPublication(publicationId: $publicationId)
          }
          collection(id: "gid://shopify/Collection/1") {
            publishedOnPublication(publicationId: $publicationId)
          }
        }`,
        variables: { publicationId },
      });

    expect(readAfterDelete.status).toBe(200);
    expect(readAfterDelete.body.data).toEqual({
      publication: null,
      publicationsCount: {
        count: 1,
        precision: 'EXACT',
      },
      publishedProductsCount: {
        count: 0,
        precision: 'EXACT',
      },
      product: {
        publishedOnPublication: false,
      },
      collection: {
        publishedOnPublication: false,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
